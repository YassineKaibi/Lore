//! Boundary tests (G-4): IntentNodes in -> exact findings (code + qname +
//! span) and edges out, in deterministic order. Unhappy paths first (G-11).

// fixtures read clearest as default-then-assign, one clause per line
#![allow(clippy::field_reassign_with_default)]

use lore_graph::{ClaimStatus, Confidence, EdgeKind, Graph, Layer, build};
use lore_intent::{
    Enforcement, Intent, IntentNode, Kind, Origin, QName, Ref, Route, Severity, Span, Spanned,
};

fn sp(line: u32) -> Span {
    Span {
        file: "src/a.py".into(),
        line,
        col: 1,
        end_line: line,
        end_col: 1,
    }
}

fn spanned<T>(value: T, line: u32) -> Spanned<T> {
    Spanned {
        value,
        span: sp(line),
    }
}

fn prose(text: &str, line: u32) -> Spanned<String> {
    spanned(text.to_string(), line)
}

fn r(target: &str, line: u32) -> Spanned<Ref> {
    spanned(
        Ref {
            segments: target.split('.').map(str::to_owned).collect(),
        },
        line,
    )
}

fn node(qname: &str, kind: Kind, line: u32, intent: Intent) -> IntentNode {
    IntentNode {
        qname: QName::from_dotted(qname),
        kind,
        origin: Origin::Declared,
        intent,
        loc: sp(line),
    }
}

fn mods(names: &[&str]) -> Vec<Spanned<String>> {
    names
        .iter()
        .map(|n| Spanned {
            value: n.to_string(),
            span: Span {
                file: "lore.toml".into(),
                line: 1,
                col: 1,
                end_line: 1,
                end_col: 1,
            },
        })
        .collect()
}

/// (code, line) pairs in emitted order — the determinism surface.
fn findings(g: &Graph) -> Vec<(&str, u32)> {
    g.findings.iter().map(|f| (f.code, f.span.line)).collect()
}

fn edges_from<'g>(g: &'g Graph, q: &str) -> Vec<&'g lore_graph::Edge> {
    g.out
        .get(&QName::from_dotted(q))
        .map(|v| v.iter().collect())
        .unwrap_or_default()
}

// ---- node table ----

#[test]
fn duplicate_qname_is_e0305_and_first_declaration_wins() {
    let g = build(
        vec![
            node("Payment.charge", Kind::Function, 3, Intent::default()),
            node("Payment.charge", Kind::Function, 9, Intent::default()),
        ],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("E0305", 9)]);
    assert!(g.findings[0].message.contains("\"Payment.charge\""));
    assert!(g.findings[0].message.contains("src/a.py:3"));
    assert_eq!(g.nodes[&QName::from_dotted("Payment.charge")].loc.line, 3);
}

// ---- resolution ----

#[test]
fn unresolved_ref_is_e0306_naming_the_nearest_qname() {
    let mut intent = Intent::default();
    intent.affects = vec![r("Payment.ledgr", 12)];
    let mut state_intent = Intent::default();
    state_intent.purpose = Some(prose("Ledger", 1));
    state_intent.reads = vec![]; // states never carry refs; explicit for clarity
    let g = build(
        vec![
            node("Payment.ledger", Kind::State, 2, state_intent),
            node("Payment.charge", Kind::Function, 11, intent),
        ],
        &mods(&["Payment"]),
        None,
    );
    // The failed ref cascades honestly: ledger is now orphaned (W0210).
    assert_eq!(findings(&g), [("W0210", 2), ("E0306", 12)]);
    assert_eq!(
        g.findings[1].message,
        "unresolved ref \"Payment.ledgr\" in \"affects\" on \"Payment.charge\"; nearest existing qname is \"Payment.ledger\""
    );
    assert!(edges_from(&g, "Payment.charge").is_empty());
}

#[test]
fn wrong_kind_ref_is_e0307_naming_both_kinds() {
    let mut intent = Intent::default();
    intent.affects = vec![r("Payment.helper", 5)];
    let g = build(
        vec![
            node("Payment.helper", Kind::Function, 2, Intent::default()),
            node("Payment.charge", Kind::Function, 4, intent),
        ],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("E0307", 5)]);
    assert_eq!(
        g.findings[0].message,
        "\"affects\" must target a state, but \"Payment.helper\" is a function"
    );
    assert!(edges_from(&g, "Payment.charge").is_empty());
}

#[test]
fn refs_in_an_illegal_clause_are_not_resolved() {
    // affects is illegal on a module: E0203 once, and no E0306 for the
    // bogus target — an illegal clause contributes no edges (D-047b).
    let mut intent = Intent::default();
    intent.purpose = Some(prose("Payments", 2));
    intent.owner = Some(prose("payments-team", 3));
    intent.affects = vec![r("Nowhere.x", 4)];
    let g = build(
        vec![node("Payment", Kind::Module, 1, intent)],
        &mods(&[]),
        None,
    );
    assert_eq!(findings(&g), [("E0203", 4)]);
    assert!(edges_from(&g, "Payment").is_empty());
}

// ---- applicability matrix ----

#[test]
fn module_block_missing_purpose_and_owner_is_e0201_twice() {
    let g = build(
        vec![node("Payment", Kind::Module, 1, Intent::default())],
        &mods(&[]),
        None,
    );
    assert_eq!(findings(&g), [("E0201", 1), ("E0201", 1)]);
    // same span and code: message order breaks the tie deterministically
    assert!(g.findings[0].message.contains("\"owner\""));
    assert!(g.findings[1].message.contains("\"purpose\""));
}

#[test]
fn manifest_only_modules_are_exempt_from_required_intent() {
    let g = build(vec![], &mods(&["Payment", "User"]), None);
    assert_eq!(findings(&g), []);
    assert_eq!(g.nodes.len(), 2);
    assert_eq!(g.nodes[&QName::from_dotted("Payment")].kind, Kind::Module);
}

#[test]
fn owner_on_event_is_e0203_with_the_inheritance_message() {
    let mut intent = Intent::default();
    intent.purpose = Some(prose("Settled", 2));
    intent.owner = Some(prose("payments-team", 3));
    let g = build(
        vec![node("Payment.Settled", Kind::Event, 1, intent)],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("E0203", 3)]);
    assert!(
        g.findings[0]
            .message
            .contains("inherited from the owning module")
    );
}

#[test]
fn empty_step_is_e0204() {
    let mut wf = Intent::default();
    wf.purpose = Some(prose("Onboard", 1));
    wf.owner = Some(prose("growth", 1));
    let g = build(
        vec![
            node("Onboarding", Kind::Workflow, 1, wf),
            node("Onboarding.collect", Kind::Step, 4, Intent::default()),
        ],
        &mods(&[]),
        None,
    );
    assert_eq!(findings(&g), [("E0204", 4)]);
    assert!(g.findings[0].message.contains("\"Onboarding.collect\""));
}

#[test]
fn route_on_a_function_outside_a_service_is_e0205() {
    let mut handler = Intent::default();
    handler.route = Some(spanned(
        Route {
            method: Some(lore_intent::HttpMethod::Post),
            path: "/charge".into(),
        },
        6,
    ));
    let g = build(
        vec![node("Payment.charge", Kind::Function, 5, handler)],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("E0205", 6)]);
    assert!(g.findings[0].message.contains("its parent is a module"));
}

#[test]
fn route_on_a_function_under_a_service_is_legal() {
    let mut svc = Intent::default();
    svc.purpose = Some(prose("Payments API", 1));
    svc.owner = Some(prose("payments-team", 2));
    svc.route = Some(spanned(
        Route {
            method: None,
            path: "/payments".into(),
        },
        3,
    ));
    let mut handler = Intent::default();
    handler.route = Some(spanned(
        Route {
            method: Some(lore_intent::HttpMethod::Post),
            path: "/charge".into(),
        },
        6,
    ));
    let g = build(
        vec![
            node("PaymentService", Kind::Service, 1, svc),
            node("PaymentService.charge", Kind::Function, 5, handler),
        ],
        &mods(&[]),
        None,
    );
    assert_eq!(findings(&g), []);
}

#[test]
fn state_without_purpose_is_w0209_and_error_without_because_is_e0201() {
    let mut touch = Intent::default();
    touch.reads = vec![r("Payment.balances", 9)];
    let g = build(
        vec![
            node("Payment.balances", Kind::State, 2, Intent::default()),
            node("Payment.NoFunds", Kind::Error, 5, Intent::default()),
            node("Payment.charge", Kind::Function, 8, touch),
        ],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("W0209", 2), ("E0201", 5)]);
    assert!(g.findings[1].message.contains("\"because\""));
}

// ---- depends_on surface ----

#[test]
fn cross_module_ref_without_depends_on_is_e0304() {
    let mut charge = Intent::default();
    charge.triggers = vec![r("User.notify", 7)];
    let g = build(
        vec![
            node("User.notify", Kind::Function, 2, Intent::default()),
            node("Payment.charge", Kind::Function, 6, charge),
        ],
        &mods(&["Payment", "User"]),
        None,
    );
    assert_eq!(findings(&g), [("E0304", 7)]);
    assert!(g.findings[0].message.contains("declare depends_on: User"));
    // the claim edge itself still exists — the dependency is what is missing
    assert_eq!(edges_from(&g, "Payment.charge").len(), 1);
}

#[test]
fn depends_on_of_a_container_satisfies_and_counts_as_used() {
    let mut payment = Intent::default();
    payment.purpose = Some(prose("Payments", 1));
    payment.owner = Some(prose("payments-team", 2));
    payment.depends_on = vec![r("User", 3)];
    let mut charge = Intent::default();
    charge.triggers = vec![r("User.notify", 7)];
    let g = build(
        vec![
            node("Payment", Kind::Module, 1, payment),
            node("User.notify", Kind::Function, 2, Intent::default()),
            node("Payment.charge", Kind::Function, 6, charge),
        ],
        &mods(&["User"]),
        None,
    );
    assert_eq!(findings(&g), []); // no E0304, no W0206
}

#[test]
fn depends_on_an_owning_module_satisfies_a_ref_to_a_nested_target() {
    // ref to User.Sub.notify is satisfied by depends_on: User — "M or M's
    // owning module" (D-008, D-048a).
    let mut payment = Intent::default();
    payment.purpose = Some(prose("Payments", 1));
    payment.owner = Some(prose("payments-team", 2));
    payment.depends_on = vec![r("User", 3)];
    let mut sub = Intent::default();
    sub.purpose = Some(prose("Sub", 1));
    sub.owner = Some(prose("users-team", 2));
    let mut charge = Intent::default();
    charge.triggers = vec![r("User.Sub.notify", 7)];
    let g = build(
        vec![
            node("Payment", Kind::Module, 1, payment),
            node("User.Sub", Kind::Module, 1, sub),
            node("User.Sub.notify", Kind::Function, 4, Intent::default()),
            node("Payment.charge", Kind::Function, 6, charge),
        ],
        &mods(&["User"]),
        None,
    );
    assert_eq!(findings(&g), []);
}

#[test]
fn unused_depends_on_is_w0206() {
    let mut payment = Intent::default();
    payment.purpose = Some(prose("Payments", 1));
    payment.owner = Some(prose("payments-team", 2));
    payment.depends_on = vec![r("User", 3)];
    let g = build(
        vec![node("Payment", Kind::Module, 1, payment)],
        &mods(&["User"]),
        None,
    );
    assert_eq!(findings(&g), [("W0206", 3)]);
    assert!(g.findings[0].message.contains("depends_on: User"));
}

#[test]
fn intra_module_triggers_is_w0205_and_module_local_refs_need_no_depends_on() {
    let mut charge = Intent::default();
    charge.triggers = vec![r("Payment.audit", 7)];
    let g = build(
        vec![
            node("Payment.audit", Kind::Function, 2, Intent::default()),
            node("Payment.charge", Kind::Function, 6, charge),
        ],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("W0205", 7)]); // and no E0304
}

// ---- hygiene ----

#[test]
fn event_hygiene_w0211_w0212_and_silent_when_unused() {
    let mut settled = Intent::default();
    settled.purpose = Some(prose("Settled", 2));
    let mut refunded = Intent::default();
    refunded.purpose = Some(prose("Refunded", 4));
    let mut unused = Intent::default();
    unused.purpose = Some(prose("Unused", 6));
    let mut emitter = Intent::default();
    emitter.emits = vec![r("Payment.Settled", 9)];
    let mut handler = Intent::default();
    handler.on = vec![r("Payment.Refunded", 12)];
    let g = build(
        vec![
            node("Payment.Settled", Kind::Event, 1, settled),
            node("Payment.Refunded", Kind::Event, 3, refunded),
            node("Payment.Unused", Kind::Event, 5, unused),
            node("Payment.charge", Kind::Function, 8, emitter),
            node("Payment.book", Kind::Function, 11, handler),
        ],
        &mods(&["Payment"]),
        None,
    );
    // D-026: W0211 needs an emitter, W0212 a handler; a dead event gets neither.
    assert_eq!(findings(&g), [("W0211", 1), ("W0212", 3)]);
}

// ---- edges ----

#[test]
fn claim_edges_are_declared_and_unverifiable_until_derivation_exists() {
    let mut ledger = Intent::default();
    ledger.purpose = Some(prose("Ledger", 2));
    let mut charge = Intent::default();
    charge.affects = vec![r("Payment.ledger", 7)];
    let g = build(
        vec![
            node("Payment.ledger", Kind::State, 1, ledger),
            node("Payment.charge", Kind::Function, 6, charge),
        ],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), []);
    let edges = edges_from(&g, "Payment.charge");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].kind, EdgeKind::Affects);
    assert_eq!(edges[0].layer, Layer::Declared);
    assert_eq!(edges[0].status, Some(ClaimStatus::Unverifiable));
    assert_eq!(edges[0].confidence, None);
    assert_eq!(edges[0].loc.line, 7);
    // both adjacency directions hold the edge (§10.7)
    assert_eq!(g.inc[&QName::from_dotted("Payment.ledger")].len(), 2); // Contains + Affects
}

#[test]
fn contains_and_sequence_edges_are_derived_exact() {
    let mut wf = Intent::default();
    wf.purpose = Some(prose("Onboard", 1));
    wf.owner = Some(prose("growth", 2));
    wf.depends_on = vec![r("Payment", 3)];
    let mut s1 = Intent::default();
    s1.triggers = vec![r("Payment.charge", 5)];
    let mut s2 = Intent::default();
    s2.triggers = vec![r("Payment.charge", 8)];
    let g = build(
        vec![
            node("Onboarding", Kind::Workflow, 1, wf),
            node("Onboarding.collect", Kind::Step, 4, s1),
            node("Onboarding.verify", Kind::Step, 7, s2),
            node("Payment.charge", Kind::Function, 2, Intent::default()),
        ],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), []);
    let wf_edges = edges_from(&g, "Onboarding");
    let contains: Vec<&str> = wf_edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains)
        .map(|e| e.to.0.last().unwrap().as_str())
        .collect();
    assert_eq!(contains, ["collect", "verify"]);
    assert!(
        wf_edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .all(|e| e.layer == Layer::Derived && e.confidence == Some(Confidence::Exact))
    );
    let seq: Vec<_> = edges_from(&g, "Onboarding.collect")
        .into_iter()
        .filter(|e| e.kind == EdgeKind::Sequence)
        .collect();
    assert_eq!(seq.len(), 1);
    assert_eq!(seq[0].to, QName::from_dotted("Onboarding.verify"));
    assert_eq!(seq[0].confidence, Some(Confidence::Exact));
    // ambient module contains the function it maps
    assert_eq!(
        edges_from(&g, "Payment")
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count(),
        1
    );
}

// ---- enforcement: strict ----

#[test]
fn strict_module_promotes_w_findings_to_errors_with_the_code_unchanged() {
    let mut payment = Intent::default();
    payment.purpose = Some(prose("Payments", 1));
    payment.owner = Some(prose("payments-team", 2));
    payment.enforcement = Some(spanned(Enforcement::Strict, 3));
    payment.depends_on = vec![r("User", 4)]; // unused -> W0206, promoted
    let g = build(
        vec![node("Payment", Kind::Module, 1, payment)],
        &mods(&["User"]),
        None,
    );
    assert_eq!(findings(&g), [("W0206", 4)]);
    assert_eq!(g.findings[0].severity, Severity::Error);
}

#[test]
fn strict_is_not_inherited_by_nested_modules() {
    let mut outer = Intent::default();
    outer.purpose = Some(prose("Payments", 1));
    outer.owner = Some(prose("payments-team", 2));
    outer.enforcement = Some(spanned(Enforcement::Strict, 3));
    let mut inner = Intent::default();
    inner.purpose = Some(prose("Refunds", 5));
    inner.owner = Some(prose("payments-team", 6));
    let mut state = Intent::default();
    state.purpose = Some(prose("Queue", 8));
    let g = build(
        vec![
            node("Payment", Kind::Module, 1, outer),
            node("Payment.Refunds", Kind::Module, 5, inner),
            node("Payment.Refunds.queue", Kind::State, 8, state), // orphaned -> W0210
        ],
        &mods(&[]),
        None,
    );
    assert_eq!(findings(&g), [("W0210", 8)]);
    assert_eq!(g.findings[0].severity, Severity::Warning); // nearest module is not strict
}

// ---- determinism ----

#[test]
fn findings_come_out_sorted_by_file_line_col_code() {
    let mut charge = Intent::default();
    charge.affects = vec![r("Payment.ledgr", 9)];
    charge.triggers = vec![r("User.notify", 3)];
    let g = build(
        vec![
            node("User.notify", Kind::Function, 1, Intent::default()),
            node("Payment.ledger", Kind::State, 5, Intent::default()),
            node("Payment.charge", Kind::Function, 8, charge),
        ],
        &mods(&["Payment", "User"]),
        None,
    );
    let a = findings(&g);
    let reordered = build(
        vec![
            node("Payment.charge", Kind::Function, 8, {
                let mut i = Intent::default();
                i.affects = vec![r("Payment.ledgr", 9)];
                i.triggers = vec![r("User.notify", 3)];
                i
            }),
            node("Payment.ledger", Kind::State, 5, Intent::default()),
            node("User.notify", Kind::Function, 1, Intent::default()),
        ],
        &mods(&["User", "Payment"]),
        None,
    );
    assert_eq!(a, findings(&reordered)); // input order never changes the output
    assert_eq!(a, [("E0304", 3), ("W0209", 5), ("W0210", 5), ("E0306", 9)]);
}

// ---- W0213: declared unknowns (D-057) ----

#[test]
fn unknown_clauses_surface_as_w0213_per_occurrence() {
    let mut intent = Intent::default();
    intent.unknown = vec![
        prose("Concurrency untested", 4),
        prose("Retry behavior unclear", 5),
    ];
    let g = build(
        vec![node("Payment.charge", Kind::Function, 6, intent)],
        &mods(&["Payment"]),
        None,
    );
    assert_eq!(findings(&g), [("W0213", 4), ("W0213", 5)]);
    assert_eq!(g.findings[0].severity, Severity::Warning);
    assert_eq!(
        g.findings[0].message,
        "function \"Payment.charge\" declares an unknown: \"Concurrency untested\"; resolve it and remove the clause once it is answered"
    );
    // attributed to the node so show(X) renders it (D-055, D-057)
    assert_eq!(
        g.attributions[&QName::from_dotted("Payment.charge")],
        [0, 1]
    );
}

#[test]
fn strict_module_promotes_w0213_to_error() {
    let mut module = Intent::default();
    module.purpose = Some(prose("Money", 1));
    module.owner = Some(prose("payments-team", 2));
    module.enforcement = Some(spanned(Enforcement::Strict, 3));
    let mut f = Intent::default();
    f.unknown = vec![prose("Idempotency unverified", 8)];
    let g = build(
        vec![
            node("Payment", Kind::Module, 1, module),
            node("Payment.charge", Kind::Function, 9, f),
        ],
        &mods(&[]),
        None,
    );
    assert_eq!(findings(&g), [("W0213", 8)]);
    assert_eq!(g.findings[0].severity, Severity::Error); // D-049 band 02x
}

// ---- W0207: CODEOWNERS cross-check (D-010, D-058) ----

fn owner_node(owner: &str) -> IntentNode {
    let mut intent = Intent::default();
    intent.owner = Some(prose(owner, 2));
    node("Payment.charge", Kind::Function, 3, intent)
}

fn co(lines: &str) -> lore_graph::Codeowners {
    lore_graph::codeowners::parse(".github/CODEOWNERS".into(), lines)
}

#[test]
fn codeowners_mismatch_is_w0207_on_the_owner_clause() {
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&co("* @acme/platform\n")),
    );
    assert_eq!(findings(&g), [("W0207", 2)]);
    assert_eq!(
        g.findings[0].message,
        "owner \"payments-team\" on \"Payment.charge\" disagrees with .github/CODEOWNERS, which maps src/a.py to @acme/platform; align the owner clause or CODEOWNERS"
    );
}

#[test]
fn codeowners_org_team_token_matches_case_insensitively() {
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&co("* @acme/Payments-Team\n")),
    );
    assert_eq!(findings(&g), []);
}

#[test]
fn codeowners_last_matching_rule_wins() {
    // file is src/a.py (test fixture span); the later, more specific rule wins
    let mismatch_last = co("* @acme/payments-team\n/src/ @acme/platform\n");
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&mismatch_last),
    );
    assert_eq!(findings(&g), [("W0207", 2)]);

    let match_last = co("* @acme/platform\n/src/ @acme/payments-team\n");
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&match_last),
    );
    assert_eq!(findings(&g), []);
}

#[test]
fn codeowners_rule_without_owners_never_fires() {
    // an explicitly-unowned path contradicts nothing (D-058e)
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&co("/src/\n")),
    );
    assert_eq!(findings(&g), []);
}

#[test]
fn codeowners_unanchored_pattern_matches_at_any_depth_anchored_does_not() {
    // file is src/a.py; "a.py" floats, "/a.py" is root-anchored
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&co("a.py @acme/platform\n")),
    );
    assert_eq!(findings(&g), [("W0207", 2)]);

    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&co("/a.py @acme/platform\n")),
    );
    assert_eq!(findings(&g), []);
}

#[test]
fn codeowners_comments_and_blanks_are_skipped() {
    let g = build(
        vec![owner_node("payments-team")],
        &mods(&["Payment"]),
        Some(&co("# owners\n\n* @acme/platform # platform owns all\n")),
    );
    assert_eq!(findings(&g), [("W0207", 2)]);
}

#[test]
fn nodes_without_a_declared_owner_are_not_checked() {
    let g = build(
        vec![node("Payment.charge", Kind::Function, 3, Intent::default())],
        &mods(&["Payment"]),
        Some(&co("* @acme/platform\n")),
    );
    assert_eq!(findings(&g), []);
}
