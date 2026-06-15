//! Shared project loading for scan and lint: manifest parsing plus the
//! source walk over the manifest's active languages. Paths stay relative to
//! the manifest dir (they feed the §7.5 module globs).

use std::path::{Path, PathBuf};

use lore_annotations::{ActivePack, ScanConfig, SourceFile};
use lore_cli::manifest::{self, Manifest};
use lore_cli::packs;
use lore_intent::{Finding, IntentNode, Origin, Span, Spanned};

/// Directories never scanned (build output, VCS, caches, dot-dirs).
const SKIP_DIRS: [&str; 4] = [".git", "target", "node_modules", ".lore-cache"];

pub struct Project {
    pub manifest: Manifest,
    pub sources: Vec<SourceFile>,
    /// The packs activated for this project (D-070): scanning/binding adapters
    /// for the languages named in `[project] languages`.
    pub packs: Vec<ActivePack>,
    /// The derive-tier packs (§8.6.2): `PackSpec` + grammar handle, passed to
    /// lore_derive as data (D-070d). Derivation scope is exactly the files
    /// these claim — the pack tier drives it, not a hardcoded extension list.
    pub derive_packs: Vec<lore_derive::DerivePack>,
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

    // Validate the embedded builtin packs and activate those named in
    // [project] languages (D-070d). A builtin pack should never fail
    // validation (the conformance harness enforces that in CI), but if one
    // does we report it and drop the language rather than guess.
    let (loaded, pack_findings) = packs::load_all(&packs::builtin::sources());
    for f in &pack_findings {
        eprintln!(
            "{} {}:{}  {}",
            f.code,
            f.span.file.display(),
            f.span.line,
            f.message
        );
    }
    let mut active_packs = Vec::new();
    let mut derive_packs = Vec::new();
    for lang in &m.languages {
        match loaded.iter().find(|p| p.spec.name == *lang) {
            Some(p) => {
                match packs::activate(p) {
                    Ok(ap) => active_packs.push(ap),
                    Err(f) => eprintln!("{} {}  {}", f.code, f.span.file.display(), f.message),
                }
                // Derive-tier packs also feed the derived layer (D-070d): pass
                // the spec as data plus the grammar handle as a separate arg.
                if let (lore_intent::Tier::Derive, Some(grammar)) = (p.spec.tier, &p.grammar) {
                    derive_packs.push(lore_derive::DerivePack {
                        spec: p.spec.clone(),
                        grammar: grammar.clone(),
                    });
                }
            }
            None => eprintln!("note: language \"{lang}\" has no pack yet; skipping its files"),
        }
    }

    let root = manifest_path.parent().unwrap_or(Path::new("."));
    let mut paths = Vec::new();
    collect_sources(root, root, &active_packs, &mut paths);
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
        packs: active_packs,
        derive_packs,
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
/// ambient Module nodes (D-046), the derived layer comes from lore_derive
/// over the D-061 scope, and reconciliation gets its inputs as data
/// (D-066). Only lint pays the git cost: `check_stale` gathers the §9.2
/// blame metadata (D-068c) and is false for every other command.

// @lore
// purpose: "Run the scan -> derive -> reconcile pipeline once and return the graph plus the findings and drop counters every command surfaces"
// because: "lint, ask, stats, and graph all need the same built graph; one pipeline here keeps a single place for the D-066 reconciliation inputs"
// triggers: Annotations.scan, Intent.parse_intent, Graph.build
pub fn build_graph(p: &Project, manifest_path: &Path, check_stale: bool, quiet: bool) -> Built {
    let config = ScanConfig {
        modules: p.manifest.modules.clone(),
    };
    let result = lore_annotations::scan(&config, &p.sources, &p.packs);

    let (derived, unresolved_calls, ambiguous_derived_names) =
        derive_layer(p, manifest_path, &result);

    // D-066b/c: the occurrence test's inputs. The first block wins a
    // duplicate qname, matching the node table's first-declaration rule.
    let mut host_identifiers: std::collections::HashMap<lore_intent::QName, String> =
        std::collections::HashMap::new();
    for block in &result.blocks {
        if let Some(subject) = &block.subject {
            host_identifiers
                .entry(block.qname.clone())
                .or_insert_with(|| subject.clone());
        }
    }
    let root = manifest_path.parent().unwrap_or(Path::new("."));
    let reconcile = lore_graph::ReconcileInput {
        sources: p
            .sources
            .iter()
            .map(|s| (s.path.clone(), s.text.clone()))
            .collect(),
        host_identifiers,
        staleness: if check_stale {
            super::stale::gather(root, &result.blocks, quiet)
        } else {
            None
        },
    };

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
        graph: lore_graph::build(
            nodes,
            &manifest_modules,
            codeowners.as_ref(),
            derived,
            reconcile,
        ),
        findings,
        unresolved_calls,
        ambiguous_derived_names,
    }
}

/// Collect the language-manifest texts the derive packs' `manifest_prefix`
/// strategies name (e.g. Go's `go.mod`), keyed by project-relative path
/// (§8.2 rule 2, D-071). The engine never reads the filesystem (D-058), so the
/// CLI gathers these; empty when no pack configures the strategy.
fn collect_manifests(root: &Path, packs: &[lore_derive::DerivePack]) -> Vec<(PathBuf, String)> {
    use lore_intent::ImportStrategy;
    let mut names: Vec<&str> = packs
        .iter()
        .flat_map(|p| &p.spec.imports)
        .filter_map(|s| match s {
            ImportStrategy::ManifestPrefix { manifest_file, .. } => Some(manifest_file.as_str()),
            _ => None,
        })
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !(SKIP_DIRS.contains(&name.as_ref())
                    || name.starts_with('.')
                    || is_pack_fixtures(&dir, &name))
                {
                    stack.push(path);
                }
            } else if names.contains(&name.as_ref())
                && let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(rel) = path.strip_prefix(root)
            {
                found.push((rel.to_path_buf(), text));
            }
        }
    }
    found.sort();
    found
}

/// Whether a derive-tier pack claims `path` (§8.6.2): derivation scope is the
/// union of every derive pack's claimed extensions (D-070b — tier drives it).
fn derive_capable(packs: &[lore_derive::DerivePack], path: &Path) -> bool {
    let name = path.to_string_lossy();
    packs
        .iter()
        .any(|p| p.spec.extensions.iter().any(|e| name.ends_with(e.as_str())))
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
        .filter(|s| derive_capable(&p.derive_packs, &s.path))
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

    let root = manifest_path.parent().unwrap_or(Path::new("."));
    let config = lore_derive::DeriveConfig {
        roots: p.manifest.roots.clone(),
        cache_dir: Some(root.join(".lore-cache")),
        manifests: collect_manifests(root, &p.derive_packs),
    };
    let result = lore_derive::derive(&config, &p.derive_packs, &units, &states);

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

/// A language pack's `fixtures/` directory (a `fixtures` dir beside a
/// `lore-lang.toml`) holds deliberately-malformed conformance inputs, not
/// project source, so the walk skips it (§8.6.4, D-075).
fn is_pack_fixtures(parent: &Path, name: &str) -> bool {
    name == "fixtures" && parent.join("lore-lang.toml").is_file()
}

fn collect_sources(root: &Path, dir: &Path, packs: &[ActivePack], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref())
                || name.starts_with('.')
                || is_pack_fixtures(dir, &name)
            {
                continue;
            }
            collect_sources(root, &path, packs, out);
        } else if packs.iter().any(|p| p.claims(&path)) {
            out.push(
                path.strip_prefix(root)
                    .expect("walk stays under root")
                    .to_path_buf(),
            );
        }
    }
}
