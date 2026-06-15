// @lore
// kind: module
// name: Derive
// purpose: "Build the derived layer: Function/Type nodes and Calls/Affects/Reads edges extracted from host source, every edge carrying a confidence"
// owner: "lore-core"

//! The derived layer (spec §8): static extraction from host source. Input:
//! the files in derivation scope (D-061) plus the declared state symbols;
//! output: derived nodes and edges with confidence labels, plus the drop
//! counters (G-7: dropped and counted, never guessed). v1 languages: Python
//! and TypeScript (T6); Go, Java, Rust arrive at T8.

mod cache;
mod extract;
mod facts;
mod imports;
mod resolve;

pub use lore_intent::PackSpec;

/// Names of the custom import strategies registered in this crate (D-071b).
/// The named-impl escape hatch: a pack's `kind = "custom", name = "<id>"`
/// must name one of these, validated at pack load (E0414). Each requires its
/// own D-entry; `rust_use_paths` (D-071c) is the first.
pub fn custom_strategy_names() -> &'static [&'static str] {
    &["rust_use_paths"]
}

use std::path::PathBuf;

use lore_intent::{IntentNode, QName, Span};

/// One file in derivation scope: path relative to the project root, content,
/// and the module assigned by §7.5 (D-061 — the CLI computes scope).
pub struct SourceUnit {
    pub path: PathBuf,
    pub text: String,
    pub module: String,
}

/// A declared State node's host binding (§8.3 targets): the qname the touch
/// edges point at, the host identifier the heuristics look for, and where it
/// is defined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSymbol {
    pub qname: QName,
    pub identifier: String,
    pub file: PathBuf,
    pub module: String,
}

pub struct DeriveConfig {
    /// `[project] roots`: import-resolution roots (§8.2, the `root_relative`
    /// strategy's search roots, D-071).
    pub roots: Vec<String>,
    /// `.lore-cache/` location; None disables the cache (D-064).
    pub cache_dir: Option<PathBuf>,
    /// Language-manifest texts (e.g. Go's `go.mod`) keyed by their
    /// project-relative path, for the `manifest_prefix` import strategy
    /// (§8.2 rule 2, D-071). The engine never reads the filesystem (D-058);
    /// the CLI collects these from the manifest files a derive pack's
    /// strategies name. Empty for packs that configure no such strategy.
    pub manifests: Vec<(PathBuf, String)>,
}

/// One activated language pack (D-070): the validated `PackSpec` as data plus
/// the tree-sitter grammar handle, passed in as a *separate* argument so
/// `lore_intent` stays tree-sitter-free (D-070d). `lore_cli` builds these from
/// its loader and the grammar registry.
pub struct DerivePack {
    pub spec: PackSpec,
    pub grammar: tree_sitter::Language,
}

/// Edge kinds the derived layer produces (§8.2, §8.3). A subset of the §6.1
/// set, defined here because the dependency direction forbids reaching into
/// lore_graph (§13); the CLI maps these onto graph edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DerivedEdgeKind {
    Calls,
    Affects,
    Reads,
}

/// §8.4 confidence, mirrored from the §13 contract for the same dependency
/// reason as DerivedEdgeKind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedConfidence {
    Exact,
    Resolved,
    Heuristic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedEdge {
    pub from: QName,
    pub to: QName,
    pub kind: DerivedEdgeKind,
    pub confidence: DerivedConfidence,
    pub loc: Span,
}

pub struct DeriveResult {
    /// Origin Derived, empty intent (§8.1), sorted by (file, line, qname).
    pub nodes: Vec<IntentNode>,
    /// Sorted by (file, line, col, kind, from, to) — deterministic always.
    pub edges: Vec<DerivedEdge>,
    /// §8.2 rule 3 plus the D-062a/b attribution drops: dropped, counted.
    pub unresolved_calls: usize,
    /// Declarations excluded by the D-060d qname-collision rule.
    pub ambiguous_names: usize,
    /// The derivation scope: every file that was derived, sorted.
    pub scope: Vec<PathBuf>,
}

/// The crate boundary (G-4): derivation scope in → derived nodes and edges
/// with confidences out. Files with no supported language are skipped (the
/// CLI filters by language already; this is belt and braces).

// @lore
// purpose: "Derive nodes and confidence-labeled edges from the files in derivation scope"
// because: "Extraction is per-file and cacheable by content; everything cross-file is resolved fresh each run so the cache can never serve a stale edge (D-064)"
// because: "From T8 derivation is pack-driven (D-070): each pack supplies its queries, import strategies, and mutators, so one generic extractor serves every language"
pub fn derive(
    config: &DeriveConfig,
    packs: &[DerivePack],
    files: &[SourceUnit],
    states: &[StateSymbol],
) -> DeriveResult {
    // Compile each derive-tier pack's queries once (D-070d); reuse across files.
    let compiled: Vec<extract::CompiledPack> = packs
        .iter()
        .filter(|p| p.spec.derive_scm.is_some())
        .map(extract::CompiledPack::new)
        .collect();
    let cache = config.cache_dir.as_deref().map(cache::Cache::new);
    let mut extracted = Vec::new();
    for file in files {
        let Some(cp) = compiled.iter().find(|c| c.claims(&file.path)) else {
            continue; // no derive-tier pack for this file's extension
        };
        let key = cache::key(&cp.id, file, &config.roots, states);
        let facts = cache
            .as_ref()
            .and_then(|c| c.load(&key))
            .unwrap_or_else(|| {
                let facts = extract::extract(cp, file, states);
                if let Some(c) = &cache {
                    c.store(&key, &facts);
                }
                facts
            });
        extracted.push((file, cp, facts));
    }
    let manifests: std::collections::HashMap<PathBuf, String> =
        config.manifests.iter().cloned().collect();
    resolve::resolve(&extracted, states, &config.roots, &manifests)
}
