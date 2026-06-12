//! Declared-ref resolution (§6.3): E0306 unresolved ref with the nearest
//! existing qname, E0307 wrong-kind ref naming both kinds, W0205
//! intra-module triggers (D-007). Failed refs produce no edge (D-047b).
//! Claim statuses follow §9.1 minus the T7 Contradicted branch (D-063).

use std::collections::HashSet;
use std::path::PathBuf;

use lore_intent::{Finding, Kind, QName, Ref, Spanned};

use crate::matrix::{self, Clause};
use crate::{ClaimStatus, Ctx, Edge, EdgeKind, Layer, OwnedFinding};

/// The six ref clauses, their edge kinds, and required target kinds (§6.3).
const REF_CLAUSES: [(Clause, EdgeKind, &[Kind], &str); 6] = [
    (
        Clause::Affects,
        EdgeKind::Affects,
        &[Kind::State],
        "a state",
    ),
    (Clause::Reads, EdgeKind::Reads, &[Kind::State], "a state"),
    (
        Clause::Triggers,
        EdgeKind::Triggers,
        &[Kind::Function],
        "a function",
    ),
    (Clause::Emits, EdgeKind::Emits, &[Kind::Event], "an event"),
    (Clause::On, EdgeKind::Handles, &[Kind::Event], "an event"),
    (
        Clause::DependsOn,
        EdgeKind::DependsOn,
        &[Kind::Module, Kind::Service, Kind::External],
        "a module, service, or external",
    ),
];

pub(crate) fn resolve(
    ctx: &Ctx,
    derived_edges: &[Edge],
    scope: &HashSet<PathBuf>,
    findings: &mut Vec<OwnedFinding>,
) -> Vec<Edge> {
    let index: HashSet<(&QName, &QName, EdgeKind)> = derived_edges
        .iter()
        .map(|e| (&e.from, &e.to, e.kind))
        .collect();
    let mut edges = Vec::new();
    for qname in &ctx.order {
        let node = &ctx.nodes[qname];
        for (clause, edge_kind, target_kinds, kind_text) in REF_CLAUSES {
            if !matrix::legal(clause, node.kind) {
                continue; // E0203 already reported; an illegal clause contributes no edges
            }
            let refs = refs_of(node, clause);
            for r in refs {
                let target = QName(r.value.segments.clone());
                let Some(target_node) = ctx.nodes.get(&target) else {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "E0306",
                            r.span.clone(),
                            format!(
                                "unresolved ref \"{target}\" in \"{}\" on \"{qname}\"; nearest existing qname is \"{}\"",
                                clause.name(),
                                nearest_qname(ctx, &target)
                            ),
                        ),
                        qname,
                    ));
                    continue;
                };
                if !target_kinds.contains(&target_node.kind) {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "E0307",
                            r.span.clone(),
                            format!(
                                "\"{}\" must target {kind_text}, but \"{target}\" is a {}",
                                clause.name(),
                                target_node.kind.name()
                            ),
                        ),
                        qname,
                    ));
                    continue;
                }
                if clause == Clause::Triggers
                    && let (Some(m), Some(t)) =
                        (ctx.nearest_module(qname), ctx.nearest_module(&target))
                    && m == t
                {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "W0205",
                            r.span.clone(),
                            format!(
                                "\"triggers: {target}\" on \"{qname}\" targets its own module; intra-module call edges are derived, so this claim is redundant"
                            ),
                        ),
                        qname,
                    ));
                }
                let status = claim_status(edge_kind, qname, &target, target_node, scope, &index);
                edges.push(Edge {
                    from: qname.clone(),
                    to: target,
                    kind: edge_kind,
                    layer: Layer::Declared,
                    loc: r.span.clone(),
                    status: Some(status),
                    confidence: None,
                });
            }
        }
    }
    edges
}

/// §9.1 minus its Contradicted branch (D-063): outside derivation scope →
/// Unverifiable; matching derived edge (same-kind for Affects/Reads, Calls
/// for Triggers) → Verified; otherwise Unverified. Emits/Handles/DependsOn
/// stay Unverifiable in Phase 1.
fn claim_status(
    edge_kind: EdgeKind,
    from: &QName,
    to: &QName,
    target_node: &lore_intent::IntentNode,
    scope: &HashSet<PathBuf>,
    index: &HashSet<(&QName, &QName, EdgeKind)>,
) -> ClaimStatus {
    match edge_kind {
        EdgeKind::Affects | EdgeKind::Reads | EdgeKind::Triggers => {
            if !scope.contains(&target_node.loc.file) {
                return ClaimStatus::Unverifiable;
            }
            let derived_kind = match edge_kind {
                EdgeKind::Triggers => EdgeKind::Calls,
                k => k,
            };
            if index.contains(&(from, to, derived_kind)) {
                ClaimStatus::Verified
            } else {
                ClaimStatus::Unverified
            }
        }
        _ => ClaimStatus::Unverifiable,
    }
}

fn refs_of(node: &lore_intent::IntentNode, clause: Clause) -> &[Spanned<Ref>] {
    match clause {
        Clause::Affects => &node.intent.affects,
        Clause::Reads => &node.intent.reads,
        Clause::Triggers => &node.intent.triggers,
        Clause::Emits => &node.intent.emits,
        Clause::On => &node.intent.on,
        Clause::DependsOn => &node.intent.depends_on,
        _ => unreachable!("REF_CLAUSES holds only ref clauses"),
    }
}

/// Nearest existing qname by edit distance over the dotted form; ties go to
/// the lexically smaller qname so the message is deterministic.
fn nearest_qname(ctx: &Ctx, missing: &QName) -> String {
    crate::util::nearest(&missing.to_string(), ctx.order.iter().map(QName::to_string))
        .expect("the node table always holds at least the referencing node")
}
