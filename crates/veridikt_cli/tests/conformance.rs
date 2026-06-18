//! The conformance harness (spec §8.6.4, D-070e, E0415): every embedded
//! builtin pack's fixture suite, run through the *real* scan→bind→derive
//! pipeline and asserted against `expected.json`. This is how a pack protects
//! G-7 in CI — a pack whose queries drift from its fixtures fails here, so it
//! cannot ship.
//!
//! Each pack is loaded from its on-disk directory (`packs::from_dir` →
//! `packs::load`), exactly as an external pack would be, so the fixtures
//! validate the real pack content, not a stand-in. `from_dir` also reads the
//! fixture class directories from disk, so the loader's E0415 gate is checked
//! against reality here (the embedded `fixture_classes` in `builtin.rs` are
//! the trusted assertion this test verifies).
//!
//! Golden files: set `VERIDIKT_BLESS=1` to (re)generate every `expected.json` from
//! the current pipeline output, then review the diff. Without it the harness
//! asserts byte-for-byte (semantic JSON) equality.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use veridikt_annotations::{ActivePack, ScanConfig, ScanResult, SourceFile};
use veridikt_cli::manifest::{self, Manifest};
use veridikt_cli::packs::{self, LoadedPack};
use veridikt_derive::{DeriveConfig, DerivePack, DeriveResult, SourceUnit, StateSymbol};
use veridikt_intent::{ImportStrategy, Tier};

/// The builtin packs and their mandatory conformance classes per tier
/// (§8.6.4). Mirrors `packs::load`'s `required` table; the harness runs every
/// class present, the loader's E0415 guarantees the mandatory ones exist.
const BUILTIN_PACKS: &[&str] = &["python", "typescript", "rust", "go", "java"];

#[test]
fn builtin_packs_pass_their_conformance_suites() {
    let bless = std::env::var_os("VERIDIKT_BLESS").is_some();
    let root = workspace_root();
    let mut ran = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for name in BUILTIN_PACKS {
        let dir = root.join("packs").join(name);
        if !dir.is_dir() {
            continue; // pack not yet shipped (Go/Java land in T8 step 6)
        }
        let source = packs::from_dir(&dir).expect("read builtin pack directory");
        let pack = packs::load(&source).unwrap_or_else(|f| {
            panic!(
                "builtin pack \"{name}\" failed to load: {} {}",
                f.code, f.message
            )
        });

        for class in classes_for(pack.spec.tier) {
            let class_dir = dir.join("fixtures").join(class);
            if !class_dir.is_dir() {
                continue;
            }
            for case in case_dirs(&class_dir) {
                ran += 1;
                let actual = run_case(class, &case, &pack);
                let expected_path = case.join("expected.json");
                if bless {
                    let pretty = serde_json::to_string_pretty(&actual).unwrap();
                    std::fs::write(&expected_path, format!("{pretty}\n")).unwrap();
                    continue;
                }
                let want: Value = serde_json::from_str(
                    &std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
                        panic!(
                            "missing {} — run with VERIDIKT_BLESS=1 to generate",
                            expected_path.display()
                        )
                    }),
                )
                .expect("expected.json is valid JSON");
                if actual != want {
                    mismatches.push(format!(
                        "{}\n  expected: {}\n  actual:   {}",
                        expected_path.display(),
                        serde_json::to_string(&want).unwrap(),
                        serde_json::to_string(&actual).unwrap(),
                    ));
                }
            }
        }
    }

    assert!(ran > 0, "no conformance cases ran — fixtures missing?");
    assert!(
        mismatches.is_empty(),
        "conformance mismatches:\n{}",
        mismatches.join("\n")
    );
}

/// The cumulative class list for a tier (§8.6.2/§8.6.4).
fn classes_for(tier: Tier) -> &'static [&'static str] {
    match tier {
        Tier::Scan => &["scan"],
        Tier::Bind => &["scan", "bind"],
        Tier::Derive => &["scan", "bind", "derive"],
    }
}

/// Run one fixture case through the pipeline up to its class's tier and
/// serialize the class-appropriate view (§8.6.4: scan/bind → block sets +
/// findings; derive → nodes/edges with confidences + drop counters).
fn run_case(class: &str, case: &Path, pack: &LoadedPack) -> Value {
    let m = load_case_manifest(case);
    let active = packs::activate(pack).expect("pack activates");
    let sources = read_sources(case, &active);
    let scan = veridikt_annotations::scan(
        &ScanConfig {
            modules: m.modules.clone(),
        },
        &sources,
        &[active],
    );

    if class == "derive" {
        let result = run_derive(case, pack, &m, &scan, &sources);
        derive_json(&result)
    } else {
        scan_json(&scan)
    }
}

/// Build the derive inputs the way `commands::project` does and run the real
/// `veridikt_derive::derive`. State symbols come from the scanned `State` blocks;
/// language manifests (e.g. go.mod) come from any non-source file the pack's
/// `manifest_prefix` strategies name.
fn run_derive(
    case: &Path,
    pack: &LoadedPack,
    m: &Manifest,
    scan: &ScanResult,
    sources: &[SourceFile],
) -> DeriveResult {
    let derive_pack = DerivePack {
        spec: pack.spec.clone(),
        grammar: pack
            .grammar
            .clone()
            .expect("derive-tier pack has a grammar"),
    };
    let modules: HashMap<&Path, &str> = scan
        .file_modules
        .iter()
        .filter_map(|fm| Some((fm.path.as_path(), fm.module.as_deref()?)))
        .collect();
    let units: Vec<SourceUnit> = sources
        .iter()
        .filter_map(|s| {
            Some(SourceUnit {
                path: s.path.clone(),
                text: s.text.clone(),
                module: (*modules.get(s.path.as_path())?).to_string(),
            })
        })
        .collect();
    let states: Vec<StateSymbol> = scan
        .blocks
        .iter()
        .filter(|b| b.kind == veridikt_intent::Kind::State)
        .filter_map(|b| {
            Some(StateSymbol {
                qname: b.qname.clone(),
                identifier: b.subject.clone()?,
                file: b.file.clone(),
                module: b.module.clone()?,
            })
        })
        .collect();
    let config = DeriveConfig {
        roots: m.roots.clone(),
        cache_dir: None,
        manifests: read_manifests(case, &pack.spec.imports),
    };
    veridikt_derive::derive(&config, &[derive_pack], &units, &states)
}

// ---- serialization (the `--json` shapes, §8.6.4) ----

/// Scan/bind view: exact block set + findings (mirrors `output::to_json` minus
/// the version banner — a fixture is stable across releases).
fn scan_json(r: &ScanResult) -> Value {
    let blocks: Vec<Value> = r
        .blocks
        .iter()
        .map(|b| {
            serde_json::json!({
                "qname": b.qname.to_string(),
                "kind": b.kind.display(),
                "file": b.file.to_string_lossy(),
                "block_span": {"start": b.block_span.0, "end": b.block_span.1},
                "subject": b.subject,
                "subject_span": b.subject_span.map(|(s, e)| serde_json::json!({"start": s, "end": e})),
                "module": b.module,
            })
        })
        .collect();
    let findings: Vec<Value> = r
        .findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "code": f.code,
                "severity": match f.severity { veridikt_intent::Severity::Error => "error", veridikt_intent::Severity::Warning => "warning" },
                "file": f.span.file.to_string_lossy(),
                "line": f.span.line,
                "message": f.message,
            })
        })
        .collect();
    serde_json::json!({ "blocks": blocks, "findings": findings })
}

/// Derive view: derived nodes, confidence-labeled edges, and the G-7 drop
/// counters (the contract is as much the absences as the presences).
fn derive_json(r: &DeriveResult) -> Value {
    let nodes: Vec<Value> = r
        .nodes
        .iter()
        .map(|n| serde_json::json!({"qname": n.qname.to_string(), "kind": n.kind.display()}))
        .collect();
    let edges: Vec<Value> = r
        .edges
        .iter()
        .map(|e| {
            serde_json::json!({
                "from": e.from.to_string(),
                "to": e.to.to_string(),
                "kind": edge_kind(e.kind),
                "confidence": confidence(e.confidence),
            })
        })
        .collect();
    serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "unresolved_calls": r.unresolved_calls,
        "ambiguous_names": r.ambiguous_names,
    })
}

fn edge_kind(k: veridikt_derive::DerivedEdgeKind) -> &'static str {
    match k {
        veridikt_derive::DerivedEdgeKind::Calls => "Calls",
        veridikt_derive::DerivedEdgeKind::Affects => "Affects",
        veridikt_derive::DerivedEdgeKind::Reads => "Reads",
    }
}

fn confidence(c: veridikt_derive::DerivedConfidence) -> &'static str {
    match c {
        veridikt_derive::DerivedConfidence::Exact => "Exact",
        veridikt_derive::DerivedConfidence::Resolved => "Resolved",
        veridikt_derive::DerivedConfidence::Heuristic => "Heuristic",
    }
}

// ---- fixture I/O ----

/// Each case carries a `veridikt.toml` giving `[modules]` and `[project] roots` —
/// the case is a self-describing mini-project, so the real §7.5/§8.2 rules run.
fn load_case_manifest(case: &Path) -> Manifest {
    let path = case.join("veridikt.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture case {} needs a veridikt.toml", case.display()));
    manifest::parse(&path, &text).expect("fixture veridikt.toml is valid")
}

/// Read every source file the pack claims under the case dir (recursively),
/// pathed relative to the case dir so spans and module globs are stable.
fn read_sources(case: &Path, pack: &ActivePack) -> Vec<SourceFile> {
    let mut out = Vec::new();
    let mut stack = vec![case.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if pack.claims(&path) {
                let rel = path.strip_prefix(case).unwrap().to_path_buf();
                let text = std::fs::read_to_string(&path).unwrap();
                out.push(SourceFile { path: rel, text });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Language manifests (e.g. go.mod) keyed by their directory-relative path, for
/// the `manifest_prefix` strategy (D-071). Collected from non-source files the
/// pack's strategies name, mirroring `commands::project`.
fn read_manifests(case: &Path, strategies: &[ImportStrategy]) -> Vec<(PathBuf, String)> {
    let names: Vec<&str> = strategies
        .iter()
        .filter_map(|s| match s {
            ImportStrategy::ManifestPrefix { manifest_file, .. } => Some(manifest_file.as_str()),
            _ => None,
        })
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut stack = vec![case.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(fname) = path.file_name().and_then(|n| n.to_str())
                && names.contains(&fname)
            {
                let rel = path.strip_prefix(case).unwrap().to_path_buf();
                out.push((rel, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// Immediate subdirectories of a class dir, sorted — one per case.
fn case_dirs(class_dir: &Path) -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = std::fs::read_dir(class_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    cases.sort();
    cases
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root resolves")
}
