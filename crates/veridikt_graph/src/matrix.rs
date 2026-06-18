//! The §3.2 clause applicability and requirement matrix: E0201 missing
//! required intent, E0203 illegal clause for kind, E0204 empty step,
//! E0205 route outside service, W0209 missing recommended purpose.
//! Requirement rows apply only to block-declared nodes (D-046).

use veridikt_intent::{Finding, Intent, IntentNode, Kind, Span};

use crate::{Ctx, OwnedFinding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clause {
    Purpose,
    Owner,
    Because,
    Unknown,
    Assumes,
    Affects,
    Reads,
    Triggers,
    Emits,
    On,
    DependsOn,
    Route,
    Enforcement,
}

impl Clause {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Clause::Purpose => "purpose",
            Clause::Owner => "owner",
            Clause::Because => "because",
            Clause::Unknown => "unknown",
            Clause::Assumes => "assumes",
            Clause::Affects => "affects",
            Clause::Reads => "reads",
            Clause::Triggers => "triggers",
            Clause::Emits => "emits",
            Clause::On => "on",
            Clause::DependsOn => "depends_on",
            Clause::Route => "route",
            Clause::Enforcement => "enforcement",
        }
    }
}

const ALL: [Clause; 13] = [
    Clause::Purpose,
    Clause::Owner,
    Clause::Because,
    Clause::Unknown,
    Clause::Assumes,
    Clause::Affects,
    Clause::Reads,
    Clause::Triggers,
    Clause::Emits,
    Clause::On,
    Clause::DependsOn,
    Clause::Route,
    Clause::Enforcement,
];

enum Req {
    Required,
    Optional,
    Recommended, // W0209 if absent (State purpose only)
    Illegal,     // E0203 if present
    Inherited,   // E0203 if present, with the inheritance message (owner on State/Event)
}

/// The §3.2 table, row by row.
fn req(clause: Clause, kind: Kind) -> Req {
    use Kind::*;
    match clause {
        Clause::Purpose => match kind {
            Module | Service | Workflow | Event | External => Req::Required,
            State => Req::Recommended,
            Step | Type | Function => Req::Optional,
            Error => Req::Illegal,
        },
        Clause::Owner => match kind {
            Module | Service | Workflow | External => Req::Required,
            Function => Req::Optional,
            State | Event => Req::Inherited,
            Step | Type | Error => Req::Illegal,
        },
        Clause::Because => match kind {
            Error => Req::Required,
            _ => Req::Optional,
        },
        Clause::Unknown => match kind {
            Error => Req::Illegal,
            _ => Req::Optional,
        },
        Clause::Assumes => match kind {
            Workflow | Step | Function | External => Req::Optional,
            _ => Req::Illegal,
        },
        Clause::Affects | Clause::Reads | Clause::Triggers | Clause::Emits | Clause::On => {
            match kind {
                Step | Function => Req::Optional,
                _ => Req::Illegal,
            }
        }
        Clause::DependsOn => match kind {
            Module | Service | Workflow => Req::Optional,
            _ => Req::Illegal,
        },
        Clause::Route => match kind {
            Service => Req::Required,
            Function => Req::Optional, // legal only under a Service: E0205, checked separately
            _ => Req::Illegal,
        },
        Clause::Enforcement => match kind {
            Module => Req::Optional,
            _ => Req::Illegal,
        },
    }
}

/// True when the clause may appear on the kind at all. Resolution only
/// processes legal clauses — an illegal clause contributes no edges (D-047b).
pub(crate) fn legal(clause: Clause, kind: Kind) -> bool {
    !matches!(req(clause, kind), Req::Illegal | Req::Inherited)
}

/// Presence and the span of the first occurrence, per clause.
fn occurrence(intent: &Intent, clause: Clause) -> Option<&Span> {
    fn first<T>(v: &[veridikt_intent::Spanned<T>]) -> Option<&Span> {
        v.first().map(|s| &s.span)
    }
    match clause {
        Clause::Purpose => intent.purpose.as_ref().map(|s| &s.span),
        Clause::Owner => intent.owner.as_ref().map(|s| &s.span),
        Clause::Because => first(&intent.because),
        Clause::Unknown => first(&intent.unknown),
        Clause::Assumes => first(&intent.assumes),
        Clause::Affects => first(&intent.affects),
        Clause::Reads => first(&intent.reads),
        Clause::Triggers => first(&intent.triggers),
        Clause::Emits => first(&intent.emits),
        Clause::On => first(&intent.on),
        Clause::DependsOn => first(&intent.depends_on),
        Clause::Route => intent.route.as_ref().map(|s| &s.span),
        Clause::Enforcement => intent.enforcement.as_ref().map(|s| &s.span),
    }
}

pub(crate) fn check(ctx: &Ctx, findings: &mut Vec<OwnedFinding>) {
    for qname in &ctx.order {
        if !ctx.annotated.contains(qname) {
            continue; // D-046: nodes without an intent block carry no requirements
        }
        let node = &ctx.nodes[qname];
        for clause in ALL {
            check_clause(ctx, node, clause, findings);
        }

        // A step must declare at least one of triggers, emits, on (§3.2).
        if node.kind == Kind::Step
            && node.intent.triggers.is_empty()
            && node.intent.emits.is_empty()
            && node.intent.on.is_empty()
        {
            findings.push(OwnedFinding::new(
                Finding::new(
                    "E0204",
                    node.loc.clone(),
                    format!(
                        "step \"{qname}\" declares none of \"triggers\", \"emits\", \"on\"; a step must declare at least one"
                    ),
                ),
                qname,
            ));
        }

        // route on a function is legal only when its parent is a Service (§3.2).
        if node.kind == Kind::Function
            && let Some(route) = &node.intent.route
        {
            let parent_kind = ctx.immediate_container(qname).map(|c| ctx.nodes[&c].kind);
            if parent_kind != Some(Kind::Service) {
                let parent = match parent_kind {
                    Some(k) => format!("its parent is a {}", k.name()),
                    None => "it has no parent".to_string(),
                };
                findings.push(OwnedFinding::new(
                    Finding::new(
                        "E0205",
                        route.span.clone(),
                        format!(
                            "\"route\" on function \"{qname}\" is only legal inside a service; {parent}; move the function under a service or remove the route"
                        ),
                    ),
                    qname,
                ));
            }
        }
    }
}

fn check_clause(ctx: &Ctx, node: &IntentNode, clause: Clause, findings: &mut Vec<OwnedFinding>) {
    let _ = ctx;
    let present = occurrence(&node.intent, clause);
    let qname = &node.qname;
    let kind = node.kind.name();
    let name = clause.name();
    match (req(clause, node.kind), present) {
        (Req::Required, None) => findings.push(OwnedFinding::new(
            Finding::new(
                "E0201",
                node.loc.clone(),
                format!(
                    "{kind} \"{qname}\" is missing required \"{name}\"; add a {name} clause to its @veridikt block"
                ),
            ),
            qname,
        )),
        (Req::Recommended, None) => findings.push(OwnedFinding::new(
            Finding::new(
                "W0209",
                node.loc.clone(),
                format!(
                    "{kind} \"{qname}\" has no \"{name}\"; declaring one is recommended"
                ),
            ),
            qname,
        )),
        (Req::Illegal, Some(span)) => findings.push(OwnedFinding::new(
            Finding::new(
                "E0203",
                span.clone(),
                format!("\"{name}\" is not a legal clause on a {kind}; remove it from \"{qname}\""),
            ),
            qname,
        )),
        (Req::Inherited, Some(span)) => findings.push(OwnedFinding::new(
            Finding::new(
                "E0203",
                span.clone(),
                format!(
                    "\"{name}\" on a {kind} is inherited from the owning module and must not be declared locally; remove it from \"{qname}\""
                ),
            ),
            qname,
        )),
        _ => {}
    }
}
