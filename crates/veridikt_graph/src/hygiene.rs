//! Hygiene findings (D-026): W0210 orphaned state, W0211 event with
//! emitters but no handlers, W0212 the inverse.

use veridikt_intent::{Finding, Kind};

use crate::{Ctx, Edge, EdgeKind, OwnedFinding};

pub(crate) fn check(ctx: &Ctx, edges: &[Edge], findings: &mut Vec<OwnedFinding>) {
    for qname in &ctx.order {
        let node = &ctx.nodes[qname];
        let inc = |kind: EdgeKind| edges.iter().any(|e| e.kind == kind && e.to == *qname);
        match node.kind {
            Kind::State => {
                if !inc(EdgeKind::Affects) && !inc(EdgeKind::Reads) {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "W0210",
                            node.loc.clone(),
                            format!(
                                "state \"{qname}\" is orphaned: nothing declares affects or reads on it; wire it up or remove it"
                            ),
                        ),
                        qname,
                    ));
                }
            }
            Kind::Event => {
                let emitted = inc(EdgeKind::Emits);
                let handled = inc(EdgeKind::Handles);
                if emitted && !handled {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "W0211",
                            node.loc.clone(),
                            format!(
                                "event \"{qname}\" is emitted but has no handlers; add an \"on: {qname}\" clause to a handler or remove the event"
                            ),
                        ),
                        qname,
                    ));
                }
                if handled && !emitted {
                    findings.push(OwnedFinding::new(
                        Finding::new(
                            "W0212",
                            node.loc.clone(),
                            format!(
                                "event \"{qname}\" has handlers but no emitters; add an \"emits: {qname}\" clause to an emitter or remove the handlers"
                            ),
                        ),
                        qname,
                    ));
                }
            }
            _ => {}
        }
    }
}
