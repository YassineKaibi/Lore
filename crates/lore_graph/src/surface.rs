//! The depends_on surface (D-008, D-048): undeclared cross-module use is
//! E0304, a declared-but-unused dependency is W0206. Both directions linted
//! so dependency growth stays deliberate.

use lore_intent::{Finding, QName};

use crate::{Ctx, Edge, EdgeKind, Layer, OwnedFinding, is_prefix_of};

/// The edges whose refs count as "use" of a dependency: declared clause
/// refs only (D-048c/d) — a derived Affects/Reads/Calls is a fact about the
/// code, not a declared assertion, so it neither fires E0304 nor satisfies
/// a depends_on entry.
fn is_use(e: &Edge) -> bool {
    e.layer == Layer::Declared
        && matches!(
            e.kind,
            EdgeKind::Affects
                | EdgeKind::Reads
                | EdgeKind::Triggers
                | EdgeKind::Emits
                | EdgeKind::Handles
        )
}

pub(crate) fn check(ctx: &Ctx, edges: &[Edge], findings: &mut Vec<OwnedFinding>) {
    // E0304: a non-local clause ref needs the target's owner chain to
    // intersect the source's effective depends_on (D-048c).
    for edge in edges.iter().filter(|e| is_use(e)) {
        let chain = ctx.owner_chain(&edge.to);
        if chain.is_empty() {
            continue; // orphan target: there is no module to declare
        }
        if chain.iter().any(|p| is_prefix_of(p, &edge.from)) {
            continue; // module-local ref
        }
        let effective = effective_depends_on(ctx, &edge.from);
        if !chain.iter().any(|p| effective.contains(p)) {
            let owner = chain.last().expect("chain is non-empty");
            findings.push(OwnedFinding::new(
                Finding::new(
                    "E0304",
                    edge.loc.clone(),
                    format!(
                        "\"{}\" references \"{}\" but \"{owner}\" is not in its effective depends_on; declare depends_on: {owner} on its module, service, or workflow",
                        edge.from, edge.to
                    ),
                ),
                &edge.from,
            ));
        }
    }

    // W0206: a depends_on entry on C is used iff some clause ref from C or a
    // node contained in C resolves to a target whose owner chain holds it
    // (D-048d). Unresolved entries were already E0306 — not double-reported.
    for qname in &ctx.order {
        for dep in &ctx.nodes[qname].intent.depends_on {
            let dep_q = QName(dep.value.segments.clone());
            if !ctx.nodes.contains_key(&dep_q) {
                continue;
            }
            let used = edges.iter().any(|e| {
                is_use(e) && is_prefix_of(qname, &e.from) && ctx.owner_chain(&e.to).contains(&dep_q)
            });
            if !used {
                findings.push(OwnedFinding::new(
                    Finding::new(
                        "W0206",
                        dep.span.clone(),
                        format!(
                            "\"{qname}\" declares depends_on: {dep_q} but no clause in it references {dep_q}; remove the entry or add the missing reference",
                        ),
                    ),
                    qname,
                ));
            }
        }
    }
}

/// C's own depends_on plus that of every container (D-048b).
fn effective_depends_on(ctx: &Ctx, q: &QName) -> Vec<QName> {
    let mut deps = Vec::new();
    for len in (1..=q.0.len()).rev() {
        let prefix = QName(q.0[..len].to_vec());
        if let Some(node) = ctx.nodes.get(&prefix) {
            deps.extend(
                node.intent
                    .depends_on
                    .iter()
                    .map(|d| QName(d.value.segments.clone())),
            );
        }
    }
    deps
}
