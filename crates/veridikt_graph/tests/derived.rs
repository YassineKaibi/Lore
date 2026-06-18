//! Boundary tests (G-4): declared + derived layers in -> exact merge
//! results, claim statuses (D-063), and traversals over derived edges out.
//! Unhappy paths first (G-11).

// fixtures read clearest as default-then-assign, one clause per line
#![allow(clippy::field_reassign_with_default)]

use std::collections::HashSet;
use std::path::PathBuf;

use veridikt_graph::exec::{Answer, Options, ask};
use veridikt_graph::query::parse;
use veridikt_graph::{
    ClaimStatus, Confidence, DerivedLayer, Edge, EdgeKind, Graph, Layer, ReconcileInput,
};
use veridikt_intent::{Intent, IntentNode, Kind, Origin, QName, Ref, Span, Spanned};

/// This suite exercises the layer merge and traversals with reconciliation
/// inputs withheld: no source text, so §9.1's occurrence test never runs
/// and in-scope unmatched claims stay Unverified. The four-status algorithm
/// has its own suite (tests/reconcile.rs, D-066).
fn build(
    declared: Vec<IntentNode>,
    manifest_modules: &[Spanned<String>],
    codeowners: Option<&veridikt_graph::Codeowners>,
    derived: DerivedLayer,
) -> Graph {
    veridikt_graph::build(
        declared,
        manifest_modules,
        codeowners,
        derived,
        ReconcileInput::empty(),
    )
}

fn sp_in(file: &str, line: u32) -> Span {
    Span {
        file: file.into(),
        line,
        col: 1,
        end_line: line,
        end_col: 1,
    }
}

fn sp(line: u32) -> Span {
    sp_in("src/a.py", line)
}

fn r(target: &str, line: u32) -> Spanned<Ref> {
    Spanned {
        value: Ref {
            segments: target.split('.').map(str::to_owned).collect(),
        },
        span: sp(line),
    }
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

fn derived_node(qname: &str, kind: Kind, file: &str, line: u32) -> IntentNode {
    IntentNode {
        qname: QName::from_dotted(qname),
        kind,
        origin: Origin::Derived,
        intent: Intent::default(),
        loc: sp_in(file, line),
    }
}

fn derived_edge(from: &str, to: &str, kind: EdgeKind, confidence: Confidence) -> Edge {
    Edge {
        from: QName::from_dotted(from),
        to: QName::from_dotted(to),
        kind,
        layer: Layer::Derived,
        loc: sp(1),
        status: None,
        confidence: Some(confidence),
    }
}

fn mods(names: &[&str]) -> Vec<Spanned<String>> {
    names
        .iter()
        .map(|n| Spanned {
            value: n.to_string(),
            span: sp_in("veridikt.toml", 1),
        })
        .collect()
}

fn scope(files: &[&str]) -> HashSet<PathBuf> {
    files.iter().map(PathBuf::from).collect()
}

fn layer(nodes: Vec<IntentNode>, edges: Vec<Edge>, files: &[&str]) -> DerivedLayer {
    DerivedLayer {
        nodes,
        edges,
        scope: scope(files),
    }
}

fn findings(g: &Graph) -> Vec<(&str, u32)> {
    g.findings.iter().map(|f| (f.code, f.span.line)).collect()
}

fn status_of(g: &Graph, from: &str, kind: EdgeKind) -> ClaimStatus {
    g.out[&QName::from_dotted(from)]
        .iter()
        .find(|e| e.kind == kind && e.layer == Layer::Declared)
        .expect("claim edge must exist")
        .status
        .expect("declared edges carry a status")
}

// ---- collisions first (G-11) ----

#[test]
fn derived_node_for_a_different_declaration_is_e0305_and_fully_rejected() {
    // The annotation binds src/a.py:3; the derived node claims the same
    // qname from a different declaration at line 9 (D-060c).
    let g = build(
        vec![node("Payment.charge", Kind::Function, 3, Intent::default())],
        &mods(&["Payment"]),
        None,
        layer(
            vec![derived_node(
                "Payment.charge",
                Kind::Function,
                "src/a.py",
                9,
            )],
            vec![derived_edge(
                "Payment.charge",
                "Payment.charge",
                EdgeKind::Calls,
                Confidence::Exact,
            )],
            &["src/a.py"],
        ),
    );
    assert_eq!(findings(&g), [("E0305", 9)]);
    assert!(
        g.findings[0].message.contains("different declarations")
            || g.findings[0].message.contains("collides")
    );
    // the declared node wins, untouched
    let n = &g.nodes[&QName::from_dotted("Payment.charge")];
    assert_eq!(n.loc.line, 3);
    assert_eq!(n.origin, Origin::Declared);
    // and the rejected qname took its derived edges with it
    assert!(
        g.out
            .get(&QName::from_dotted("Payment.charge"))
            .is_none_or(|v| v
                .iter()
                .all(|e| e.layer != Layer::Derived || e.kind != EdgeKind::Calls))
    );
}

// ---- merging (§8.1, D-060b) ----

#[test]
fn same_declaration_merges_into_origin_both_keeping_declared_intent() {
    let mut intent = Intent::default();
    intent.purpose = Some(Spanned {
        value: "Charge a customer".to_string(),
        span: sp(2),
    });
    let g = build(
        vec![node("Payment.charge", Kind::Function, 3, intent)],
        &mods(&["Payment"]),
        None,
        layer(
            vec![derived_node(
                "Payment.charge",
                Kind::Function,
                "src/a.py",
                3,
            )],
            vec![],
            &["src/a.py"],
        ),
    );
    assert_eq!(findings(&g), []);
    let n = &g.nodes[&QName::from_dotted("Payment.charge")];
    assert_eq!(n.origin, Origin::Both);
    assert_eq!(
        n.intent.purpose.as_ref().unwrap().value,
        "Charge a customer"
    );
}

#[test]
fn declared_refs_resolve_against_derived_only_nodes() {
    // Open world (§7.6, D-017): `triggers: Payment.helper` where helper
    // exists only in the derived layer — no E0306, a normal claim edge.
    let mut intent = Intent::default();
    intent.triggers = vec![r("Payment.helper", 4)];
    let g = build(
        vec![node("User.signup", Kind::Function, 3, intent)],
        &mods(&["Payment", "User"]),
        None,
        layer(
            vec![derived_node(
                "Payment.helper",
                Kind::Function,
                "src/a.py",
                12,
            )],
            vec![],
            &["src/a.py"],
        ),
    );
    assert_eq!(findings(&g), [("E0304", 4)]); // the honest surface check still applies
    assert_eq!(
        status_of(&g, "User.signup", EdgeKind::Triggers),
        ClaimStatus::Unverified // in scope, no derived Calls yet
    );
}

#[test]
fn derived_only_nodes_join_the_table_without_requirement_findings() {
    // a derived-only Function never fires E0201/W0209 (D-046)
    let g = build(
        vec![],
        &mods(&["Payment"]),
        None,
        layer(
            vec![derived_node(
                "Payment.helper",
                Kind::Function,
                "src/a.py",
                7,
            )],
            vec![],
            &["src/a.py"],
        ),
    );
    assert_eq!(findings(&g), []);
    assert_eq!(
        g.nodes[&QName::from_dotted("Payment.helper")].origin,
        Origin::Derived
    );
    // structure still applies: Payment --Contains--> Payment.helper
    let contains = &g.out[&QName::from_dotted("Payment")];
    assert!(
        contains
            .iter()
            .any(|e| e.kind == EdgeKind::Contains && e.to == QName::from_dotted("Payment.helper"))
    );
}

// ---- claim statuses with reconciliation inputs withheld (D-066c) ----

#[test]
fn claim_statuses_withhold_the_verdict_when_no_source_text_is_supplied() {
    let mut charge = Intent::default();
    charge.affects = vec![r("Payment.ledger", 10)]; // matching derived edge -> Verified
    charge.reads = vec![r("Payment.balances", 11)]; // in scope, no derived edge -> Unverified
    charge.triggers = vec![r("User.notify", 12)]; // matching derived Calls -> Verified
    charge.emits = vec![r("Payment.Settled", 13)]; // Phase 1 -> Unverifiable

    let mut audit = Intent::default();
    audit.affects = vec![r("User.log", 20)]; // target outside scope -> Unverifiable

    let g = build(
        vec![
            node("Payment.ledger", Kind::State, 2, Intent::default()),
            node("Payment.balances", Kind::State, 3, Intent::default()),
            node("Payment.Settled", Kind::Event, 4, Intent::default()),
            node("Payment.charge", Kind::Function, 9, charge),
            node("Payment.audit", Kind::Function, 19, audit),
            {
                let mut n = node("User.log", Kind::State, 2, Intent::default());
                n.loc = sp_in("src/user.py", 2); // a file outside derivation scope
                n
            },
            node("User.notify", Kind::Function, 30, Intent::default()),
        ],
        &mods(&["Payment", "User"]),
        None,
        layer(
            vec![],
            vec![
                derived_edge(
                    "Payment.charge",
                    "Payment.ledger",
                    EdgeKind::Affects,
                    Confidence::Heuristic,
                ),
                derived_edge(
                    "Payment.charge",
                    "User.notify",
                    EdgeKind::Calls,
                    Confidence::Resolved,
                ),
            ],
            &["src/a.py"],
        ),
    );
    assert_eq!(
        status_of(&g, "Payment.charge", EdgeKind::Affects),
        ClaimStatus::Verified
    );
    assert_eq!(
        status_of(&g, "Payment.charge", EdgeKind::Reads),
        ClaimStatus::Unverified
    );
    assert_eq!(
        status_of(&g, "Payment.charge", EdgeKind::Triggers),
        ClaimStatus::Verified
    );
    assert_eq!(
        status_of(&g, "Payment.charge", EdgeKind::Emits),
        ClaimStatus::Unverifiable
    );
    assert_eq!(
        status_of(&g, "Payment.audit", EdgeKind::Affects),
        ClaimStatus::Unverifiable
    );
}

// ---- derived edges in checks and traversals ----

#[test]
fn derived_reads_keep_a_state_from_being_orphaned() {
    let mut state = Intent::default();
    state.purpose = Some(Spanned {
        value: "Ledger".to_string(),
        span: sp(1),
    });
    let g = build(
        vec![node("Payment.ledger", Kind::State, 2, state)],
        &mods(&["Payment"]),
        None,
        layer(
            vec![derived_node(
                "Payment.report",
                Kind::Function,
                "src/a.py",
                8,
            )],
            vec![derived_edge(
                "Payment.report",
                "Payment.ledger",
                EdgeKind::Reads,
                Confidence::Heuristic,
            )],
            &["src/a.py"],
        ),
    );
    assert_eq!(findings(&g), []); // no W0210: the derived layer sees a reader
}

#[test]
fn derived_edges_neither_fire_e0304_nor_satisfy_depends_on() {
    // Payment.charge derived-calls User.notify across modules with no
    // depends_on anywhere: a derived fact is not a declared assertion, so
    // no E0304. And User's unused `depends_on: Payment` still fires W0206
    // even though a derived edge runs the other way.
    let mut user_mod = Intent::default();
    user_mod.purpose = Some(Spanned {
        value: "Users".to_string(),
        span: sp(1),
    });
    user_mod.owner = Some(Spanned {
        value: "user-team".to_string(),
        span: sp(1),
    });
    user_mod.depends_on = vec![r("Payment", 5)];
    let g = build(
        vec![{
            let mut n = node("User", Kind::Module, 1, user_mod);
            n.loc = sp_in("src/user.py", 1);
            n
        }],
        &mods(&["Payment", "User"]),
        None,
        layer(
            vec![
                derived_node("Payment.charge", Kind::Function, "src/a.py", 3),
                derived_node("User.notify", Kind::Function, "src/user.py", 3),
            ],
            vec![derived_edge(
                "Payment.charge",
                "User.notify",
                EdgeKind::Calls,
                Confidence::Resolved,
            )],
            &["src/a.py", "src/user.py"],
        ),
    );
    assert_eq!(findings(&g), [("W0206", 5)]);
}

#[test]
fn triggers_and_affects_star_traverse_derived_calls_with_confidence_labels() {
    let mut charge = Intent::default();
    charge.affects = vec![r("Payment.ledger", 10)];
    let g = build(
        vec![
            node("Payment.ledger", Kind::State, 2, Intent::default()),
            node("Payment.charge", Kind::Function, 9, Intent::default()),
        ],
        &mods(&["Payment"]),
        None,
        layer(
            vec![
                derived_node("Payment.writer", Kind::Function, "src/a.py", 20),
                derived_node("Payment.entry", Kind::Function, "src/a.py", 25),
            ],
            vec![
                derived_edge(
                    "Payment.writer",
                    "Payment.ledger",
                    EdgeKind::Affects,
                    Confidence::Heuristic,
                ),
                derived_edge(
                    "Payment.entry",
                    "Payment.writer",
                    EdgeKind::Calls,
                    Confidence::Exact,
                ),
            ],
            &["src/a.py"],
        ),
    );

    // affects(S): the derived writer answers a declared-free question (D-002)
    let answer = ask(
        &g,
        &parse("affects(Payment.ledger)").unwrap(),
        &Options::default(),
    )
    .unwrap();
    let Answer::Hits { hits, .. } = answer else {
        panic!("affects returns hits")
    };
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].qname, QName::from_dotted("Payment.writer"));
    assert_eq!(hits[0].via[0].confidence, Some(Confidence::Heuristic));

    // affects*(S): the transitive set prepends the derived call chain
    let answer = ask(
        &g,
        &parse("affects*(Payment.ledger)").unwrap(),
        &Options::default(),
    )
    .unwrap();
    let Answer::Hits { hits, .. } = answer else {
        panic!("affects* returns hits")
    };
    let qnames: Vec<String> = hits.iter().map(|h| h.qname.to_string()).collect();
    assert_eq!(qnames, ["Payment.entry", "Payment.writer"]);

    // callers(F) walks reverse Calls
    let answer = ask(
        &g,
        &parse("callers(Payment.writer)").unwrap(),
        &Options::default(),
    )
    .unwrap();
    let Answer::Hits { hits, .. } = answer else {
        panic!("callers returns hits")
    };
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].qname, QName::from_dotted("Payment.entry"));
    assert_eq!(hits[0].via[0].layer, Layer::Derived);
}
