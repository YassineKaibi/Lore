//! `lore stats` (§12, D-065): coverage counts — nodes by kind and origin,
//! declared-intent coverage per kind, edges by layer, the claims-by-status
//! breakdown (D-069), and the derivation drop counters.

use std::path::Path;

use lore_graph::{ClaimStatus, Layer};
use lore_intent::{Intent, Kind, Origin};

use crate::commands::project;

/// §4 order: the deterministic row order for every output surface.
const KINDS: [Kind; 10] = [
    Kind::Module,
    Kind::Service,
    Kind::Workflow,
    Kind::Step,
    Kind::State,
    Kind::Event,
    Kind::Type,
    Kind::Error,
    Kind::Function,
    Kind::External,
];

#[derive(Default)]
struct KindRow {
    declared: usize,
    derived: usize,
    both: usize,
    with_intent: usize,
}

impl KindRow {
    fn total(&self) -> usize {
        self.declared + self.derived + self.both
    }
}

// @lore
// name: stats
// purpose: "Coverage counts over the graph: nodes by kind and origin, declared-intent share per kind, edges by layer, claims by status, derivation drop counters"
// because: "The drop counters are the honesty surface for the derived layer: every guess lore refused to make is visible here, not silently absent (G-7)"
// triggers: Graph.build
pub fn run(manifest_path: &Path, json: bool, quiet: bool) -> i32 {
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let built = project::build_graph(&p, manifest_path, false, quiet);
    let graph = &built.graph;

    let mut rows: Vec<(Kind, KindRow)> = KINDS.iter().map(|k| (*k, KindRow::default())).collect();
    for node in graph.nodes.values() {
        let row = &mut rows
            .iter_mut()
            .find(|(k, _)| *k == node.kind)
            .expect("KINDS covers every kind")
            .1;
        match node.origin {
            Origin::Declared => row.declared += 1,
            Origin::Derived => row.derived += 1,
            Origin::Both => row.both += 1,
        }
        if node.intent != Intent::default() {
            row.with_intent += 1;
        }
    }
    let declared_edges = graph
        .out
        .values()
        .flatten()
        .filter(|e| e.layer == Layer::Declared)
        .count();
    let derived_edges = graph.edge_count() - declared_edges;

    // D-069: every declared edge carrying a ClaimStatus, by §6.2 order.
    let mut claims = [0usize; 4];
    for e in graph.out.values().flatten() {
        if let Some(status) = e.status {
            claims[match status {
                ClaimStatus::Verified => 0,
                ClaimStatus::Unverified => 1,
                ClaimStatus::Contradicted => 2,
                ClaimStatus::Unverifiable => 3,
            }] += 1;
        }
    }
    let claims_total: usize = claims.iter().sum();

    if json {
        let by_kind: serde_json::Map<String, serde_json::Value> = rows
            .iter()
            .filter(|(_, r)| r.total() > 0)
            .map(|(k, r)| {
                (
                    k.display().to_string(),
                    serde_json::json!({
                        "total": r.total(),
                        "declared": r.declared,
                        "derived": r.derived,
                        "both": r.both,
                        "with_intent": r.with_intent,
                    }),
                )
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "lore_version": env!("CARGO_PKG_VERSION"),
                "nodes": {"total": graph.nodes.len(), "by_kind": by_kind},
                "edges": {"total": graph.edge_count(), "declared": declared_edges, "derived": derived_edges},
                "claims": {
                    "total": claims_total,
                    "verified": claims[0],
                    "unverified": claims[1],
                    "contradicted": claims[2],
                    "unverifiable": claims[3],
                },
                "unresolved_calls": built.unresolved_calls,
                "ambiguous_derived_names": built.ambiguous_derived_names,
            }))
            .expect("stats JSON serializes")
        );
        return 0;
    }

    let mut out = String::new();
    if !quiet {
        out.push_str(&format!(
            "stats: {} nodes, {} edges ({declared_edges} declared, {derived_edges} derived)\n",
            graph.nodes.len(),
            graph.edge_count(),
        ));
    }
    out.push_str("nodes by kind (declared/derived/both, with intent):\n");
    let name_w = rows
        .iter()
        .filter(|(_, r)| r.total() > 0)
        .map(|(k, _)| k.name().len())
        .max()
        .unwrap_or(0);
    for (kind, row) in rows.iter().filter(|(_, r)| r.total() > 0) {
        out.push_str(&format!(
            "  {:<name_w$}  {}  ({}/{}/{}, {} with intent)\n",
            kind.name(),
            row.total(),
            row.declared,
            row.derived,
            row.both,
            row.with_intent,
        ));
    }
    out.push_str(&format!(
        "claims by status: {claims_total} ({} verified, {} unverified, {} contradicted, {} unverifiable)\n",
        claims[0], claims[1], claims[2], claims[3],
    ));
    out.push_str(&format!("unresolved_calls: {}\n", built.unresolved_calls));
    out.push_str(&format!(
        "ambiguous_derived_names: {}\n",
        built.ambiguous_derived_names
    ));
    print!("{out}");
    0
}
