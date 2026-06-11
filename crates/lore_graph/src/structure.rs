//! Structural edges (§6.1, D-047): Contains from qname containment,
//! Sequence from step declaration order. Both Derived/Exact — structure is
//! fact, not claim.

use lore_intent::{Kind, QName};

use crate::{Confidence, Ctx, Edge, EdgeKind, Layer};

pub(crate) fn derive(ctx: &Ctx) -> Vec<Edge> {
    let mut edges = Vec::new();

    for qname in &ctx.order {
        if let Some(container) = ctx.immediate_container(qname) {
            edges.push(structural(
                &container,
                qname,
                EdgeKind::Contains,
                &ctx.nodes[qname].loc,
            ));
        }
    }

    // Sequence: consecutive steps of the same workflow, in source order
    // (ctx.order preserves it). Edge loc = the successor step's loc.
    let mut last_step: Vec<(QName, QName)> = Vec::new(); // workflow -> last step seen
    for qname in &ctx.order {
        if ctx.nodes[qname].kind != Kind::Step {
            continue;
        }
        let Some(workflow) = ctx.immediate_container(qname) else {
            continue;
        };
        if ctx.nodes[&workflow].kind != Kind::Workflow {
            continue;
        }
        match last_step.iter_mut().find(|(w, _)| *w == workflow) {
            Some((_, prev)) => {
                edges.push(structural(
                    prev,
                    qname,
                    EdgeKind::Sequence,
                    &ctx.nodes[qname].loc,
                ));
                *prev = qname.clone();
            }
            None => last_step.push((workflow, qname.clone())),
        }
    }

    edges
}

fn structural(from: &QName, to: &QName, kind: EdgeKind, loc: &lore_intent::Span) -> Edge {
    Edge {
        from: from.clone(),
        to: to.clone(),
        kind,
        layer: Layer::Derived,
        loc: loc.clone(),
        status: None,
        confidence: Some(Confidence::Exact),
    }
}
