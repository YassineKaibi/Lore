//! Boundary tests (G-4): query text + graph in -> exact result sets with
//! witness chains out. The fixture includes an event hop and a workflow so
//! the §6.4 traversal matrix is verified literally, including the
//! `affects*` event-hop case (T4 exit criteria). Unhappy paths first (G-11).

// fixtures read clearest as default-then-assign, one clause per line
#![allow(clippy::field_reassign_with_default)]

use lore_graph::exec::{Answer, Hit, Options, ask};
use lore_graph::query::parse;
use lore_graph::{DerivedLayer, EdgeKind, Graph};
use lore_intent::{Intent, IntentNode, Kind, Origin, QName, Ref, Route, Span, Spanned};

/// The query suite exercises declared + structural edges; derived-edge
/// traversal is covered in tests/derived.rs. Empty scope: all Unverifiable.
fn build(
    declared: Vec<IntentNode>,
    manifest_modules: &[Spanned<String>],
    codeowners: Option<&lore_graph::Codeowners>,
) -> Graph {
    lore_graph::build(
        declared,
        manifest_modules,
        codeowners,
        DerivedLayer::empty(),
        lore_graph::ReconcileInput::empty(),
    )
}

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

/// The T4 fixture: a workflow (collect -> verify), a service, and the event
/// hop `charge --Emits--> Settled <--Handles-- audit --Affects--> ledger`.
fn fixture() -> Graph {
    let mut onboarding = Intent::default();
    onboarding.purpose = Some(prose("Onboard a customer", 1));
    onboarding.owner = Some(prose("growth", 2));
    onboarding.depends_on = vec![r("PaymentService", 3)];

    let mut collect = Intent::default();
    collect.triggers = vec![r("PaymentService.charge", 5)];

    let mut verify = Intent::default();
    verify.triggers = vec![r("Payment.book", 8)];

    let mut svc = Intent::default();
    svc.purpose = Some(prose("Payments API", 11));
    svc.owner = Some(prose("payments-team", 12));
    svc.route = Some(spanned(
        Route {
            method: None,
            path: "/payments".into(),
        },
        13,
    ));
    svc.depends_on = vec![r("Payment", 14)];

    let mut charge = Intent::default();
    charge.purpose = Some(prose("Charge a customer", 16));
    charge.owner = Some(prose("payments-team", 17));
    charge.route = Some(spanned(
        Route {
            method: Some(lore_intent::HttpMethod::Post),
            path: "/charge".into(),
        },
        18,
    ));
    charge.reads = vec![r("Payment.balances", 19)];
    charge.emits = vec![r("Payment.Settled", 20)];
    charge.unknown = vec![prose("Concurrent charge + refund untested", 21)];

    let mut payment = Intent::default();
    payment.purpose = Some(prose("Money movement", 23));
    payment.owner = Some(prose("payments-team", 24));

    let mut ledger = Intent::default();
    ledger.purpose = Some(prose("Ledger", 26));
    let mut balances = Intent::default();
    balances.purpose = Some(prose("Balances", 27));
    let mut settled = Intent::default();
    settled.purpose = Some(prose("Funds moved", 28));

    let mut audit = Intent::default();
    audit.on = vec![r("Payment.Settled", 30)];
    audit.affects = vec![r("Payment.ledger", 31)];
    audit.unknown = vec![prose("Audit ordering unverified", 32)];

    let mut book = Intent::default();
    book.triggers = vec![r("Payment.audit", 34)]; // intra-module: W0205, harmless here
    book.reads = vec![r("Payment.balancez", 35)]; // E0306: the unresolved ref

    build(
        vec![
            node("Onboarding", Kind::Workflow, 1, onboarding),
            node("Onboarding.collect", Kind::Step, 4, collect),
            node("Onboarding.verify", Kind::Step, 7, verify),
            node("PaymentService", Kind::Service, 11, svc),
            node("PaymentService.charge", Kind::Function, 16, charge),
            node("Payment", Kind::Module, 23, payment),
            node("Payment.ledger", Kind::State, 26, ledger),
            node("Payment.balances", Kind::State, 27, balances),
            node("Payment.Settled", Kind::Event, 28, settled),
            node("Payment.audit", Kind::Function, 30, audit),
            node("Payment.book", Kind::Function, 34, book),
            node("User.notify", Kind::Function, 2, Intent::default()),
        ],
        &mods(&["User"]),
        None,
    )
}

fn run(g: &Graph, query: &str) -> Vec<Hit> {
    run_with(g, query, &Options::default())
}

fn run_with(g: &Graph, query: &str, options: &Options) -> Vec<Hit> {
    match ask(g, &parse(query).expect("query parses"), options).expect("query executes") {
        Answer::Hits { hits, .. } => hits,
        Answer::Card(_) => panic!("expected hits, got a card"),
    }
}

fn qnames(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|h| h.qname.to_string()).collect()
}

fn chain(hit: &Hit) -> Vec<(EdgeKind, String, String)> {
    hit.via
        .iter()
        .map(|e| (e.kind, e.from.to_string(), e.to.to_string()))
        .collect()
}

// ---- unhappy paths first (G-11) ----

#[test]
fn unknown_query_form_suggests_the_nearest() {
    assert_eq!(
        parse("affcts(Payment.ledger)").unwrap_err(),
        "unknown query form \"affcts\"; did you mean \"affects\"?"
    );
}

#[test]
fn star_on_a_non_transitive_form_is_rejected() {
    let err = parse("emits*(Payment.Settled)").unwrap_err();
    assert!(err.contains("\"emits\" does not take \"*\""), "{err}");
    assert_eq!(
        parse("reaches*(Payment)").unwrap_err(),
        "\"reaches\" is always transitive; drop the \"*\""
    );
}

#[test]
fn malformed_queries_name_what_was_expected() {
    let err = parse("affects Payment.ledger").unwrap_err();
    assert!(err.contains("expected \"(\" after \"affects\""), "{err}");
    let err = parse("owner(team)").unwrap_err();
    assert!(err.contains("expected a quoted string"), "{err}");
    let err = parse("owner(\"team)").unwrap_err();
    assert!(err.contains("unterminated string"), "{err}");
    let err = parse("affects(Payment.ledger) kind(banana)").unwrap_err();
    assert!(err.contains("\"banana\" is not a kind"), "{err}");
    let err = parse("affects(Payment.ledger) in workflow(W)").unwrap_err();
    assert!(
        err.contains("\"in\" filters take \"module\" or \"service\""),
        "{err}"
    );
}

#[test]
fn unresolved_query_argument_names_the_nearest_qname_and_creates_no_answer() {
    let g = fixture();
    let err = ask(
        &g,
        &parse("affects(Payment.ledgr)").unwrap(),
        &Options::default(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        "\"Payment.ledgr\" names no node in the graph; nearest existing qname is \"Payment.ledger\""
    );
}

#[test]
fn wrong_kind_filter_argument_names_both_kinds() {
    let g = fixture();
    let err = ask(
        &g,
        &parse("affects(Payment.ledger) in module(PaymentService)").unwrap(),
        &Options::default(),
    )
    .unwrap_err();
    assert_eq!(
        err,
        "in module(PaymentService): \"PaymentService\" is a service, not a module"
    );
}

#[test]
fn show_takes_no_filters_and_all_needs_path_and_max_len() {
    let g = fixture();
    let err = ask(
        &g,
        &parse("show(Payment) in module(Payment)").unwrap(),
        &Options::default(),
    )
    .unwrap_err();
    assert!(
        err.contains("show and path take no trailing filters"),
        "{err}"
    );

    let opts = Options {
        all: true,
        max_len: Some(4),
    };
    let err = ask(&g, &parse("affects(Payment.ledger)").unwrap(), &opts).unwrap_err();
    assert_eq!(err, "--all applies only to path(A, B)");

    let opts = Options {
        all: true,
        max_len: None,
    };
    let err = ask(
        &g,
        &parse("path(Onboarding.collect, Payment.ledger)").unwrap(),
        &opts,
    )
    .unwrap_err();
    assert!(err.contains("--all needs --max-len"), "{err}");
}

// ---- the §6.4 traversal matrix, literally ----

#[test]
fn affects_is_one_reverse_effect_hop() {
    let g = fixture();
    let hits = run(&g, "affects(Payment.ledger)");
    assert_eq!(qnames(&hits), ["Payment.audit"]);
    assert_eq!(
        chain(&hits[0]),
        [(
            EdgeKind::Affects,
            "Payment.audit".into(),
            "Payment.ledger".into()
        )]
    );
}

#[test]
fn affects_star_prepends_reverse_call_and_event_hop_chains() {
    let g = fixture();
    let hits = run(&g, "affects*(Payment.ledger)");
    assert_eq!(
        qnames(&hits),
        [
            "Onboarding.collect",
            "Onboarding.verify",
            "Payment.audit",
            "Payment.book",
            "PaymentService.charge",
        ]
    );
    // The event-hop case: charge reaches the ledger only through
    // Emits -> Settled <- Handles (audit), then audit's Affects (D-054b).
    let charge = &hits[4];
    assert_eq!(
        chain(charge),
        [
            (
                EdgeKind::Emits,
                "PaymentService.charge".into(),
                "Payment.Settled".into()
            ),
            (
                EdgeKind::Handles,
                "Payment.audit".into(),
                "Payment.Settled".into()
            ),
            (
                EdgeKind::Affects,
                "Payment.audit".into(),
                "Payment.ledger".into()
            ),
        ]
    );
    // collect's chain extends charge's by its Triggers edge, causal order
    assert_eq!(hits[0].via.len(), 4);
    assert_eq!(hits[0].via[0].kind, EdgeKind::Triggers);
}

#[test]
fn reads_and_reads_star() {
    let g = fixture();
    assert_eq!(
        qnames(&run(&g, "reads(Payment.balances)")),
        ["PaymentService.charge"]
    );
    assert_eq!(
        qnames(&run(&g, "reads*(Payment.balances)")),
        ["Onboarding.collect", "PaymentService.charge"]
    );
}

#[test]
fn touches_is_forward_effects_and_star_appends_call_chains() {
    let g = fixture();
    let hits = run(&g, "touches(PaymentService.charge)");
    assert_eq!(qnames(&hits), ["Payment.balances"]);
    assert_eq!(hits[0].via[0].kind, EdgeKind::Reads);

    let hits = run(&g, "touches*(PaymentService.charge)");
    assert_eq!(qnames(&hits), ["Payment.balances", "Payment.ledger"]);
    let ledger = &hits[1];
    assert_eq!(
        ledger.via.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [EdgeKind::Emits, EdgeKind::Handles, EdgeKind::Affects]
    );

    // from the workflow step, the whole chain is witnessed
    assert_eq!(
        qnames(&run(&g, "touches*(Onboarding.collect)")),
        ["Payment.balances", "Payment.ledger"]
    );
}

#[test]
fn triggers_is_reverse_calls_only_and_never_the_event_hop() {
    let g = fixture();
    assert_eq!(
        qnames(&run(&g, "triggers(PaymentService.charge)")),
        ["Onboarding.collect"]
    );
    // transitive over Triggers/Calls only: charge causes audit via the event
    // hop, but `triggers*` must not see it (§6.4)
    assert_eq!(
        qnames(&run(&g, "triggers*(Payment.audit)")),
        ["Onboarding.verify", "Payment.book"]
    );
}

#[test]
fn emits_and_handlers_are_one_reverse_hop() {
    let g = fixture();
    assert_eq!(
        qnames(&run(&g, "emits(Payment.Settled)")),
        ["PaymentService.charge"]
    );
    assert_eq!(
        qnames(&run(&g, "handlers(Payment.Settled)")),
        ["Payment.audit"]
    );
}

#[test]
fn depends_and_dependents_traverse_depends_on() {
    let g = fixture();
    assert_eq!(qnames(&run(&g, "depends(Onboarding)")), ["PaymentService"]);
    assert_eq!(
        qnames(&run(&g, "depends*(Onboarding)")),
        ["Payment", "PaymentService"]
    );
    assert_eq!(qnames(&run(&g, "dependents(Payment)")), ["PaymentService"]);
    assert_eq!(
        qnames(&run(&g, "dependents*(Payment)")),
        ["Onboarding", "PaymentService"]
    );
}

#[test]
fn reaches_traverses_structure_calls_effects_and_the_event_hop() {
    let g = fixture();
    let hits = run(&g, "reaches(Onboarding.collect)");
    assert_eq!(
        qnames(&hits),
        [
            "Onboarding.verify",
            "Payment.audit",
            "Payment.balances",
            "Payment.book",
            "Payment.ledger",
            "PaymentService.charge",
        ]
    );
    // the Event node is not a result: §6.4 traverses the hop, not bare Emits
    assert!(!qnames(&hits).contains(&"Payment.Settled".to_string()));
}

#[test]
fn path_returns_the_shortest_witnessed_chain() {
    let g = fixture();
    let hits = run(&g, "path(PaymentService.charge, Payment.ledger)");
    assert_eq!(qnames(&hits), ["Payment.ledger"]);
    assert_eq!(
        hits[0].via.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [EdgeKind::Emits, EdgeKind::Handles, EdgeKind::Affects]
    );
}

#[test]
fn path_all_enumerates_simple_paths_bounded_by_max_len() {
    let g = fixture();
    let opts = Options {
        all: true,
        max_len: Some(4),
    };
    let hits = run_with(&g, "path(Onboarding.collect, Payment.ledger)", &opts);
    assert_eq!(hits.len(), 2);
    let mut firsts: Vec<EdgeKind> = hits.iter().map(|h| h.via[0].kind).collect();
    firsts.sort_by_key(|k| k.order());
    assert_eq!(firsts, [EdgeKind::Triggers, EdgeKind::Sequence]);
    assert!(hits.iter().all(|h| h.via.len() == 4));

    let opts = Options {
        all: true,
        max_len: Some(3),
    };
    assert!(run_with(&g, "path(Onboarding.collect, Payment.ledger)", &opts).is_empty());
}

// ---- scans and filters ----

#[test]
fn owner_matches_declared_and_inherited_owners() {
    let g = fixture();
    // ledger/balances/Settled inherit from module Payment (§3.2, D-052b)
    assert_eq!(
        qnames(&run(&g, "owner(\"payments-team\")")),
        [
            "Payment",
            "Payment.Settled",
            "Payment.balances",
            "Payment.ledger",
            "PaymentService",
            "PaymentService.charge",
        ]
    );
    assert_eq!(qnames(&run(&g, "owner(\"growth\")")), ["Onboarding"]);
}

#[test]
fn tagged_is_honestly_empty_in_phase_1() {
    let g = fixture();
    assert_eq!(run(&g, "tagged(\"user-identity\")").len(), 0);
}

#[test]
fn unknown_returns_carriers_project_wide_and_scoped() {
    let g = fixture();
    assert_eq!(
        qnames(&run(&g, "unknown")),
        ["Payment.audit", "PaymentService.charge"]
    );
    assert_eq!(
        qnames(&run(&g, "unknown in service(PaymentService)")),
        ["PaymentService.charge"]
    );
    assert_eq!(
        qnames(&run(&g, "unknown in module(Payment)")),
        ["Payment.audit"]
    );
    assert_eq!(run(&g, "unknown in workflow(Onboarding)").len(), 0);
}

#[test]
fn filters_intersect_the_result_set() {
    let g = fixture();
    assert_eq!(
        qnames(&run(&g, "affects*(Payment.ledger) in module(Payment)")),
        ["Payment.audit", "Payment.book"]
    );
    assert_eq!(
        qnames(&run(&g, "affects*(Payment.ledger) kind(step)")),
        ["Onboarding.collect", "Onboarding.verify"]
    );
    assert_eq!(
        qnames(&run(
            &g,
            "affects*(Payment.ledger) owned_by(\"payments-team\")"
        )),
        ["PaymentService.charge"]
    );
    assert_eq!(
        qnames(&run(
            &g,
            "affects*(Payment.ledger) in module(Payment) kind(step)"
        )),
        Vec::<String>::new()
    );
}

// ---- show ----

#[test]
fn show_returns_the_card_with_grouped_edges_and_attributed_findings() {
    let g = fixture();
    let card = match ask(
        &g,
        &parse("show(PaymentService.charge)").unwrap(),
        &Options::default(),
    )
    .unwrap()
    {
        Answer::Card(c) => c,
        Answer::Hits { .. } => panic!("expected a card"),
    };
    // §6.1 kind order within each direction
    assert_eq!(
        card.edges_out.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [EdgeKind::Reads, EdgeKind::Emits]
    );
    assert_eq!(
        card.edges_in.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [EdgeKind::Triggers, EdgeKind::Contains]
    );
    // the charge node's unknown clause surfaces as W0213 on its card (D-057)
    assert_eq!(
        card.findings.iter().map(|f| f.code).collect::<Vec<_>>(),
        ["W0213"]
    );

    // Payment.book carries W0205 (intra-module triggers) and E0306 (typo),
    // surfaced on the card per Graph.attributions (D-055)
    let card = match ask(
        &g,
        &parse("show(Payment.book)").unwrap(),
        &Options::default(),
    )
    .unwrap()
    {
        Answer::Card(c) => c,
        Answer::Hits { .. } => panic!("expected a card"),
    };
    let codes: Vec<&str> = card.findings.iter().map(|f| f.code).collect();
    assert_eq!(codes, ["W0205", "E0306"]);
}

// ---- §10.7 performance contract ----

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "the §10.7 50ms budget is for the shipped (release) build; CI runs this test with --release"
)]
fn queries_over_a_5000_node_graph_answer_under_50ms() {
    // 50 modules x (98 functions + 1 state) + 50 ambient modules = 5,000
    // nodes, shaped like a real call graph: a 10-ary trigger tree inside
    // each module (f_i triggers f_{10i+1}..f_{10i+10}), the deepest leaf
    // writing the module's state, and M0's leaf dispatching to every other
    // module's root.
    let mut nodes = Vec::new();
    let mut names = Vec::new();
    for m in 0..50 {
        names.push(format!("M{m}"));
        for f in 0..98 {
            let mut triggers: Vec<Spanned<Ref>> = (10 * f + 1..=10 * f + 10)
                .filter(|c| *c <= 97)
                .map(|c| r(&format!("M{m}.f{c}"), 1))
                .collect();
            let mut intent = Intent::default();
            if f == 97 {
                intent.affects = vec![r(&format!("M{m}.s"), 1)];
                if m == 0 {
                    triggers = (1..50).map(|k| r(&format!("M{k}.f0"), 1)).collect();
                }
            }
            intent.triggers = triggers;
            nodes.push(node(&format!("M{m}.f{f}"), Kind::Function, 1, intent));
        }
        let mut s = Intent::default();
        s.purpose = Some(prose("State", 1));
        nodes.push(node(&format!("M{m}.s"), Kind::State, 1, s));
    }
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let g = build(nodes, &mods(&name_refs), None);
    assert_eq!(g.nodes.len(), 5000);

    for query in ["reaches(M0.f0)", "affects*(M49.s)", "show(M25.f50)"] {
        let parsed = parse(query).unwrap();
        let started = std::time::Instant::now();
        let answer = ask(&g, &parsed, &Options::default()).unwrap();
        let elapsed = started.elapsed();
        match answer {
            Answer::Hits { hits, .. } => assert!(!hits.is_empty()),
            Answer::Card(_) => {}
        }
        assert!(
            elapsed.as_millis() < 50,
            "{query} took {elapsed:?}, over the §10.7 50ms budget"
        );
    }
}
