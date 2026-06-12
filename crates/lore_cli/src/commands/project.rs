//! Shared project loading for scan and lint: manifest parsing plus the
//! source walk over the manifest's active languages. Paths stay relative to
//! the manifest dir (they feed the §7.5 module globs).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lore_annotations::{Lang, ScanConfig, SourceFile};
use lore_cli::manifest::{self, Manifest};
use lore_intent::{Finding, IntentNode, Origin, Span, Spanned};

/// Directories never scanned (build output, VCS, caches, dot-dirs).
const SKIP_DIRS: [&str; 4] = [".git", "target", "node_modules", ".lore-cache"];

/// Languages with a §7.4 row activated: Python+TypeScript at T1, Rust at T3
/// (D-050, dogfooding). Go and Java arrive at T8.
const ACTIVE_LANGUAGES: [&str; 3] = ["python", "typescript", "rust"];

pub struct Project {
    pub manifest: Manifest,
    pub sources: Vec<SourceFile>,
}

/// Load manifest + sources, reporting manifest problems on stderr.
/// Err carries the §10.5 exit code.
pub fn load(manifest_path: &Path) -> Result<Project, i32> {
    let text = match std::fs::read_to_string(manifest_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("E0402 cannot read {}: {e}", manifest_path.display());
            return Err(2);
        }
    };
    let m = match manifest::parse(manifest_path, &text) {
        Ok(m) => m,
        Err(f) => {
            eprintln!(
                "{} {}:{}  {}",
                f.code,
                f.span.file.display(),
                f.span.line,
                f.message
            );
            return Err(2);
        }
    };

    let mut active = BTreeSet::new();
    for lang in &m.languages {
        if ACTIVE_LANGUAGES.contains(&lang.as_str()) {
            active.insert(lang.as_str());
        } else {
            eprintln!("note: language \"{lang}\" has no scanner yet; skipping its files");
        }
    }

    let root = manifest_path.parent().unwrap_or(Path::new("."));
    let mut paths = Vec::new();
    collect_sources(root, root, &active, &mut paths);
    paths.sort();

    let sources: Vec<SourceFile> = paths
        .into_iter()
        .filter_map(|rel| {
            std::fs::read_to_string(root.join(&rel))
                .ok()
                .map(|text| SourceFile { path: rel, text })
        })
        .collect();

    Ok(Project {
        manifest: m,
        sources,
    })
}

/// Everything the commands need from one pipeline run: the graph, the
/// scanner/parser findings that precede it (lint reports them; ask does not
/// — D-053b), and the derivation drop counters for `lore stats` (D-065).
pub struct Built {
    pub graph: lore_graph::Graph,
    pub findings: Vec<Finding>,
    pub unresolved_calls: usize,
    pub ambiguous_derived_names: usize,
}

/// Scan + parse + derive + graph-build, shared by lint, ask, and stats:
/// blocks become declared IntentNodes (D-046a), `[modules]` names become
/// ambient Module nodes (D-046), and the derived layer comes from
/// lore_derive over the D-061 scope.
pub fn build_graph(p: &Project, manifest_path: &Path) -> Built {
    let config = ScanConfig {
        modules: p.manifest.modules.clone(),
    };
    let result = lore_annotations::scan(&config, &p.sources);

    let (derived, unresolved_calls, ambiguous_derived_names) =
        derive_layer(p, manifest_path, &result);

    let mut findings = result.findings;
    let mut nodes = Vec::new();
    for block in &result.blocks {
        let (intent, parse_findings) = lore_intent::parse_intent(&block.raw_clauses);
        findings.extend(parse_findings);
        let (start, end) = block.subject_span.unwrap_or(block.block_span);
        nodes.push(IntentNode {
            qname: block.qname.clone(),
            kind: block.kind,
            origin: Origin::Declared,
            intent,
            loc: Span {
                file: block.file.clone(),
                line: start,
                col: 1,
                end_line: end,
                end_col: 1,
            },
        });
    }

    // Ambient Module nodes from [modules] (D-046), deduped in manifest order.
    // The span file stays root-relative like every source path, so output
    // surfaces print "lore.toml:1", not an absolute path.
    let manifest_file: PathBuf = manifest_path
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_path.to_path_buf());
    let mut manifest_modules: Vec<Spanned<String>> = Vec::new();
    for glob in &p.manifest.modules {
        if manifest_modules.iter().any(|m| m.value == glob.module) {
            continue;
        }
        manifest_modules.push(Spanned {
            value: glob.module.clone(),
            span: Span {
                file: manifest_file.clone(),
                line: 1,
                col: 1,
                end_line: 1,
                end_col: 1,
            },
        });
    }

    let codeowners = discover_codeowners(manifest_path.parent().unwrap_or(Path::new(".")));
    Built {
        graph: lore_graph::build(nodes, &manifest_modules, codeowners.as_ref(), derived),
        findings,
        unresolved_calls,
        ambiguous_derived_names,
    }
}

/// Languages with a §8 derived layer at T6: Python and TypeScript (D-014).
/// Go, Java, Rust derive at T8 — until then their files stay outside the
/// derivation scope and claims about them are honestly Unverifiable.
fn derived_lang(lang: Lang) -> bool {
    matches!(lang, Lang::Python | Lang::TypeScript | Lang::Tsx)
}

/// Run lore_derive over the derivation scope (D-061: files of supported
/// languages that §7.5 assigns a module) and map its output onto graph
/// types. Returns the layer plus the D-065 stats counters.
fn derive_layer(
    p: &Project,
    manifest_path: &Path,
    scan: &lore_annotations::ScanResult,
) -> (lore_graph::DerivedLayer, usize, usize) {
    let modules: std::collections::HashMap<&Path, &str> = scan
        .file_modules
        .iter()
        .filter_map(|fm| Some((fm.path.as_path(), fm.module.as_deref()?)))
        .collect();
    let units: Vec<lore_derive::SourceUnit> = p
        .sources
        .iter()
        .filter(|s| Lang::from_path(&s.path).is_some_and(derived_lang))
        .filter_map(|s| {
            Some(lore_derive::SourceUnit {
                path: s.path.clone(),
                text: s.text.clone(),
                module: (*modules.get(s.path.as_path())?).to_string(),
            })
        })
        .collect();

    // §8.3 targets: every annotated State with an extractable host
    // identifier and a module. The derive crate matches occurrences against
    // these; the qname is what the touch edges point at.
    let states: Vec<lore_derive::StateSymbol> = scan
        .blocks
        .iter()
        .filter(|b| b.kind == lore_intent::Kind::State)
        .filter_map(|b| {
            Some(lore_derive::StateSymbol {
                qname: b.qname.clone(),
                identifier: b.subject.clone()?,
                file: b.file.clone(),
                module: b.module.clone()?,
            })
        })
        .collect();

    let config = lore_derive::DeriveConfig {
        roots: p.manifest.roots.clone(),
        cache_dir: Some(
            manifest_path
                .parent()
                .unwrap_or(Path::new("."))
                .join(".lore-cache"),
        ),
    };
    let result = lore_derive::derive(&config, &units, &states);

    let edges = result.edges.into_iter().map(graph_edge).collect();
    let layer = lore_graph::DerivedLayer {
        nodes: result.nodes,
        edges,
        scope: result.scope.into_iter().collect(),
    };
    (layer, result.unresolved_calls, result.ambiguous_names)
}

/// lore_derive cannot name lore_graph types (§13 dependency direction), so
/// the CLI is where its edges become graph edges.
fn graph_edge(e: lore_derive::DerivedEdge) -> lore_graph::Edge {
    use lore_derive::{DerivedConfidence, DerivedEdgeKind};
    use lore_graph::{Confidence, Edge, EdgeKind, Layer};
    Edge {
        from: e.from,
        to: e.to,
        kind: match e.kind {
            DerivedEdgeKind::Calls => EdgeKind::Calls,
            DerivedEdgeKind::Affects => EdgeKind::Affects,
            DerivedEdgeKind::Reads => EdgeKind::Reads,
        },
        layer: Layer::Derived,
        loc: e.loc,
        status: None,
        confidence: Some(match e.confidence {
            DerivedConfidence::Exact => Confidence::Exact,
            DerivedConfidence::Resolved => Confidence::Resolved,
            DerivedConfidence::Heuristic => Confidence::Heuristic,
        }),
    }
}

/// D-058a: first existing of .github/CODEOWNERS, CODEOWNERS,
/// docs/CODEOWNERS (GitHub's search order), parsed for the W0207
/// cross-check. The stored path stays root-relative for messages.
fn discover_codeowners(root: &Path) -> Option<lore_graph::Codeowners> {
    [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"]
        .iter()
        .find_map(|rel| {
            let text = std::fs::read_to_string(root.join(rel)).ok()?;
            Some(lore_graph::codeowners::parse(PathBuf::from(rel), &text))
        })
}

fn collect_sources(root: &Path, dir: &Path, active: &BTreeSet<&str>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            collect_sources(root, &path, active, out);
        } else if let Some(lang) = Lang::from_path(&path) {
            let lang_name = match lang {
                Lang::Python => "python",
                Lang::TypeScript | Lang::Tsx => "typescript",
                Lang::Rust => "rust",
            };
            if active.contains(lang_name) {
                out.push(
                    path.strip_prefix(root)
                        .expect("walk stays under root")
                        .to_path_buf(),
                );
            }
        }
    }
}
