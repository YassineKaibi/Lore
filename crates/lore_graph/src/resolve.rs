//! Declared-ref resolution (§6.3): E0306 unresolved ref with the nearest
//! existing qname, E0307 wrong-kind ref naming both kinds, W0205
//! intra-module triggers (D-007). Failed refs produce no edge (D-047b).

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

pub(crate) fn resolve(ctx: &Ctx, findings: &mut Vec<OwnedFinding>) -> Vec<Edge> {
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
                edges.push(Edge {
                    from: qname.clone(),
                    to: target,
                    kind: edge_kind,
                    layer: Layer::Declared,
                    loc: r.span.clone(),
                    // §9.1 with an empty derivation scope (no lore_derive
                    // until T6): every target is outside scope (D-047e).
                    status: Some(ClaimStatus::Unverifiable),
                    confidence: None,
                });
            }
        }
    }
    edges
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
