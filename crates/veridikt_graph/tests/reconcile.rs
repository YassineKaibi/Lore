//! Boundary tests (G-4) for T7 reconciliation: declared + derived layers +
//! ReconcileInput in -> exact claim statuses and findings out. Table-driven
//! over the §9.1 four-status decision (the T7 exit criterion), including
//! the Heuristic-absence guard (G-7). Unhappy paths first (G-11).

// fixtures read clearest as default-then-assign, one clause per line
#![allow(clippy::field_reassign_with_default)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use veridikt_graph::{
    ClaimStatus, Confidence, DerivedLayer, Edge, EdgeKind, Graph, Layer, ReconcileInput,
    StalenessRecord,
};
use veridikt_intent::{
    Enforcement, Intent, IntentNode, Kind, Origin, QName, Ref, Severity, Span, Spanned,
};

fn sp(file: &str, start: u32, end: u32) -> Span {
    Span {
        file: file.into(),
        line: start,
        col: 1,
        end_line: end,
        end_col: 1,
    }
}

fn r(target: &str, line: u32) -> Spanned<Ref> {
    Spanned {
        value: Ref {
            segments: target.split('.').map(str::to_owned).collect(),
        },
        span: sp("src/f.py", line, line),
    }
}

fn node(qname: &str, kind: Kind, loc: Span, intent: Intent) -> IntentNode {
    IntentNode {
        qname: QName::from_dotted(qname),
        kind,
        origin: Origin::Declared,
        intent,
        loc,
    }
}

fn derived_node(qname: &str, kind: Kind, file: &str, line: u32) -> IntentNode {
    IntentNode {
        qname: QName::from_dotted(qname),
        kind,
        origin: Origin::Derived,
        intent: Intent::default(),
        loc: sp(file, line, line),
    }
}

fn derived_edge(from: &str, to: &str, kind: EdgeKind, confidence: Confidence) -> Edge {
    Edge {
        from: QName::from_dotted(from),
        to: QName::from_dotted(to),
        kind,
        layer: Layer::Derived,
        loc: sp("src/f.py", 4, 4),
        status: None,
        confidence: Some(confidence),
    }
}

fn mods(names: &[&str]) -> Vec<Spanned<String>> {
    names
        .iter()
        .map(|n| Spanned {
            value: n.to_string(),
            span: sp("veridikt.toml", 1, 1),
        })
        .collect()
}

fn purposed(text: &str) -> Intent {
    let mut intent = Intent::default();
    intent.purpose = Some(Spanned {
        value: text.to_string(),
        span: sp("src/t.py", 1, 1),
    });
    intent
}

/// The claimant Payment.f: one ref clause of `clause` kind to `target`.
fn claimant(clause: EdgeKind, target: &str) -> IntentNode {
    let mut intent = Intent::default();
    let refs = vec![r(target, 4)];
    match clause {
        EdgeKind::Affects => intent.affects = refs,
        EdgeKind::Reads => intent.reads = refs,
        EdgeKind::Triggers => intent.triggers = refs,
        EdgeKind::Emits => intent.emits = refs,
        _ => unreachable!("test fixture covers claim clauses only"),
    }
    node("Payment.f", Kind::Function, sp("src/f.py", 3, 6), intent)
}

/// The claimant's module, carrying the depends_on that makes the
/// cross-module refs legal (D-048b: containers contribute to the effective
/// depends_on) and, when asked, `enforcement: strict`.
fn payment_module(strict: bool, dep_on_user: bool) -> IntentNode {
    let mut intent = purposed("Payments");
    intent.owner = Some(Spanned {
        value: "pay-team".to_string(),
        span: sp("src/f.py", 1, 1),
    });
    if dep_on_user {
        intent.depends_on = vec![r("User", 2)];
    }
    if strict {
        intent.enforcement = Some(Spanned {
            value: Enforcement::Strict,
            span: sp("src/f.py", 1, 1),
        });
    }
    node("Payment", Kind::Module, sp("src/f.py", 1, 1), intent)
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

/// One §9.1 scenario: the claim runs against a target that is in or out of
/// derivation scope, with or without a matching derived edge, and a subject
/// span that does or does not mention the target's host identifier.
fn reconcile_case(
    clause: EdgeKind,
    derived: bool,
    in_scope: bool,
    mention: bool,
) -> (Graph, ClaimStatus) {
    let (target, target_kind) = match clause {
        EdgeKind::Triggers => ("User.g", Kind::Function),
        _ => ("User.s", Kind::State),
    };
    let derived_kind = match clause {
        EdgeKind::Triggers => EdgeKind::Calls,
        k => k,
    };
    let body = if mention {
        "    tgt(1)"
    } else {
        "    other(1)"
    };
    let text = format!("# m\n# dep\ndef f():\n{body}\n    pass\n    end\n# after\n");

    let g = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(clause, target),
            node(
                target,
                target_kind,
                sp("src/t.py", 1, 2),
                purposed("Target"),
            ),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: if derived {
                vec![derived_edge(
                    "Payment.f",
                    target,
                    derived_kind,
                    Confidence::Heuristic,
                )]
            } else {
                vec![]
            },
            scope: if in_scope {
                HashSet::from([PathBuf::from("src/t.py")])
            } else {
                HashSet::new()
            },
        },
        ReconcileInput {
            sources: HashMap::from([(PathBuf::from("src/f.py"), text)]),
            host_identifiers: HashMap::from([(QName::from_dotted(target), "tgt".to_string())]),
            staleness: None,
        },
    );
    let status = status_of(&g, "Payment.f", clause);
    (g, status)
}

// ---- the four-status table (T7 exit criterion), Contradicted first (G-11) ----

#[test]
fn the_four_statuses_for_affects_reads_and_triggers() {
    for clause in [EdgeKind::Affects, EdgeKind::Reads, EdgeKind::Triggers] {
        // zero occurrences of the bound symbol in the subject span: the
        // claim is proven false and surfaces as W0302 at the ref's span
        let (g, status) = reconcile_case(clause, false, true, false);
        assert_eq!(status, ClaimStatus::Contradicted, "{clause:?}");
        assert_eq!(findings(&g), [("W0302", 4)], "{clause:?}");

        // the Heuristic-absence guard (G-7, D-020): the symbol occurs but
        // no derived edge classified — the verdict is withheld, no finding
        let (g, status) = reconcile_case(clause, false, true, true);
        assert_eq!(status, ClaimStatus::Unverified, "{clause:?}");
        assert_eq!(findings(&g), [], "{clause:?}");

        // a matching derived edge settles it before any occurrence test
        let (g, status) = reconcile_case(clause, true, true, false);
        assert_eq!(status, ClaimStatus::Verified, "{clause:?}");
        assert_eq!(findings(&g), [], "{clause:?}");

        // out of scope beats everything — even a span with no occurrence
        // is honestly Unverifiable, never Contradicted
        let (g, status) = reconcile_case(clause, false, false, false);
        assert_eq!(status, ClaimStatus::Unverifiable, "{clause:?}");
        assert_eq!(findings(&g), [], "{clause:?}");
    }
}

#[test]
fn the_w0302_message_names_the_claim_the_node_and_the_symbol() {
    let (g, _) = reconcile_case(EdgeKind::Affects, false, true, false);
    assert_eq!(g.findings[0].code, "W0302");
    assert_eq!(g.findings[0].severity, Severity::Warning);
    assert_eq!(
        g.findings[0].message,
        "contradicted claim: \"affects: User.s\" on \"Payment.f\", whose subject span never mentions \"tgt\"; the code no longer does what the claim says — update or remove the clause"
    );
}

#[test]
fn emits_claims_stay_unverifiable_even_in_scope_with_no_occurrence() {
    // Phase 1 cannot derive event publication (§9.1): never Contradicted
    let g = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(EdgeKind::Emits, "User.E"),
            node("User.E", Kind::Event, sp("src/t.py", 1, 2), purposed("Evt")),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: vec![],
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::from([(
                PathBuf::from("src/f.py"),
                "# m\n# dep\ndef f():\n    other(1)\n    pass\n    end\n".to_string(),
            )]),
            host_identifiers: HashMap::from([(QName::from_dotted("User.E"), "E".to_string())]),
            staleness: None,
        },
    );
    assert_eq!(
        status_of(&g, "Payment.f", EdgeKind::Emits),
        ClaimStatus::Unverifiable
    );
    // no W0302 — the only finding is hygiene: the event has no handler
    assert_eq!(findings(&g), [("W0211", 1)]);
}

// ---- D-066 mechanics: code choice, symbols, spans, withheld verdicts ----

/// A strict Payment module wrapping the claimant; same scenario as the
/// Contradicted table row.
fn strict_contradicted_graph(derived_edges: Vec<Edge>, declared_affects: bool) -> Graph {
    let f = if declared_affects {
        claimant(EdgeKind::Affects, "User.s")
    } else {
        let mut intent = Intent::default();
        intent.purpose = Some(Spanned {
            value: "f".to_string(),
            span: sp("src/f.py", 3, 3),
        });
        node("Payment.f", Kind::Function, sp("src/f.py", 3, 6), intent)
    };
    veridikt_graph::build(
        vec![
            // the depends_on is only legal to declare when something uses it
            payment_module(true, declared_affects),
            f,
            node(
                "User.s",
                Kind::State,
                sp("src/t.py", 1, 2),
                purposed("Target"),
            ),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: derived_edges,
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::from([(
                PathBuf::from("src/f.py"),
                "# m\n# dep\ndef f():\n    other(1)\n    pass\n    end\n".to_string(),
            )]),
            host_identifiers: HashMap::from([(QName::from_dotted("User.s"), "tgt".to_string())]),
            staleness: None,
        },
    )
}

#[test]
fn under_strict_the_contradiction_is_e0302_from_birth() {
    // D-066e: the code itself switches (D-019), unlike D-049 promotion
    let g = strict_contradicted_graph(vec![], true);
    assert_eq!(findings(&g), [("E0302", 4)]);
    assert_eq!(g.findings[0].severity, Severity::Error);
    assert_eq!(
        status_of(&g, "Payment.f", EdgeKind::Affects),
        ClaimStatus::Contradicted
    );
}

#[test]
fn token_match_is_not_substring_match() {
    // "tgts(1)" and "x_tgt(1)" contain "tgt" only as a substring: still
    // Contradicted. "a.tgt(1)" mentions it as a token: verdict withheld.
    for (body, expected) in [
        ("    tgts(1)", ClaimStatus::Contradicted),
        ("    x_tgt(1)", ClaimStatus::Contradicted),
        ("    a.tgt(1)", ClaimStatus::Unverified),
    ] {
        let text = format!("# m\n# dep\ndef f():\n{body}\n    pass\n    end\n");
        let g = veridikt_graph::build(
            vec![
                claimant(EdgeKind::Affects, "User.s"),
                node(
                    "User.s",
                    Kind::State,
                    sp("src/t.py", 1, 2),
                    purposed("Target"),
                ),
            ],
            &mods(&["Payment", "User"]),
            None,
            DerivedLayer {
                nodes: vec![],
                edges: vec![],
                scope: HashSet::from([PathBuf::from("src/t.py")]),
            },
            ReconcileInput {
                sources: HashMap::from([(PathBuf::from("src/f.py"), text)]),
                host_identifiers: HashMap::from([(
                    QName::from_dotted("User.s"),
                    "tgt".to_string(),
                )]),
                staleness: None,
            },
        );
        assert_eq!(
            status_of(&g, "Payment.f", EdgeKind::Affects),
            expected,
            "{body}"
        );
    }
}

#[test]
fn an_occurrence_outside_the_subject_span_does_not_count() {
    // "tgt" appears on line 7, past the claimant's 3..=6 span: Contradicted
    let text = "# m\n# dep\ndef f():\n    other(1)\n    pass\n    end\ntgt = 1\n".to_string();
    let g = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(EdgeKind::Affects, "User.s"),
            node(
                "User.s",
                Kind::State,
                sp("src/t.py", 1, 2),
                purposed("Target"),
            ),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: vec![],
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::from([(PathBuf::from("src/f.py"), text)]),
            host_identifiers: HashMap::from([(QName::from_dotted("User.s"), "tgt".to_string())]),
            staleness: None,
        },
    );
    assert_eq!(
        status_of(&g, "Payment.f", EdgeKind::Affects),
        ClaimStatus::Contradicted
    );
}

#[test]
fn the_matched_symbol_is_the_bound_identifier_not_the_qname_segment() {
    // `name: s` renamed the node to User.s but the declaration is `tgt`:
    // the span mentioning "s" proves nothing; mentioning "tgt" withholds
    let text_with_qname_segment = "# m\n# dep\ndef f():\n    s = 1\n    pass\n    end\n";
    let g = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(EdgeKind::Affects, "User.s"),
            node(
                "User.s",
                Kind::State,
                sp("src/t.py", 1, 2),
                purposed("Target"),
            ),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: vec![],
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::from([(
                PathBuf::from("src/f.py"),
                text_with_qname_segment.to_string(),
            )]),
            host_identifiers: HashMap::from([(QName::from_dotted("User.s"), "tgt".to_string())]),
            staleness: None,
        },
    );
    assert_eq!(
        status_of(&g, "Payment.f", EdgeKind::Affects),
        ClaimStatus::Contradicted
    );
}

#[test]
fn no_bound_symbol_or_no_source_text_withholds_the_verdict() {
    // host identifier unknown (extraction failed) -> never Contradicted
    let g_no_ident = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(EdgeKind::Affects, "User.s"),
            node(
                "User.s",
                Kind::State,
                sp("src/t.py", 1, 2),
                purposed("Target"),
            ),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: vec![],
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::from([(
                PathBuf::from("src/f.py"),
                "# m\n# dep\ndef f():\n    other(1)\n    pass\n    end\n".to_string(),
            )]),
            host_identifiers: HashMap::new(), // no entry for User.s
            staleness: None,
        },
    );
    assert_eq!(
        status_of(&g_no_ident, "Payment.f", EdgeKind::Affects),
        ClaimStatus::Unverified
    );
    assert_eq!(findings(&g_no_ident), []);

    // claimant's file missing from sources -> never Contradicted
    let g_no_text = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(EdgeKind::Affects, "User.s"),
            node(
                "User.s",
                Kind::State,
                sp("src/t.py", 1, 2),
                purposed("Target"),
            ),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![],
            edges: vec![],
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::new(),
            host_identifiers: HashMap::from([(QName::from_dotted("User.s"), "tgt".to_string())]),
            staleness: None,
        },
    );
    assert_eq!(
        status_of(&g_no_text, "Payment.f", EdgeKind::Affects),
        ClaimStatus::Unverified
    );
    assert_eq!(findings(&g_no_text), []);
}

#[test]
fn a_derived_only_target_uses_its_last_qname_segment_as_the_symbol() {
    // D-066c: derived qnames end in the host identifier by construction
    let g = veridikt_graph::build(
        vec![
            payment_module(false, true),
            claimant(EdgeKind::Triggers, "User.tgt"),
        ],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![derived_node("User.tgt", Kind::Function, "src/t.py", 5)],
            edges: vec![],
            scope: HashSet::from([PathBuf::from("src/t.py")]),
        },
        ReconcileInput {
            sources: HashMap::from([(
                PathBuf::from("src/f.py"),
                "# m\n# dep\ndef f():\n    other(1)\n    pass\n    end\n".to_string(),
            )]),
            host_identifiers: HashMap::new(),
            staleness: None,
        },
    );
    assert_eq!(
        status_of(&g, "Payment.f", EdgeKind::Triggers),
        ClaimStatus::Contradicted
    );
    assert_eq!(findings(&g), [("W0302", 4)]);
}

// ---- W0303 undeclared effects (D-067) ----

#[test]
fn a_derived_write_from_an_annotated_function_without_a_declaration_is_w0303() {
    let g = strict_contradicted_graph(
        vec![derived_edge(
            "Payment.f",
            "User.s",
            EdgeKind::Affects,
            Confidence::Heuristic,
        )],
        false, // f is annotated (purpose) but declares no affects
    );
    assert_eq!(findings(&g), [("W0303", 4)]);
    assert_eq!(
        g.findings[0].message,
        "\"Payment.f\" writes \"User.s\" here (derived, Heuristic) but its block declares no \"affects: User.s\"; add the clause or remove the write"
    );
    // strict module: band-03x W finding promoted to Error (D-049)
    assert_eq!(g.findings[0].severity, Severity::Error);
}

#[test]
fn a_declared_affects_of_any_status_suppresses_w0303() {
    // the claim is even Verified here (matching derived edge): no W0303
    let g = strict_contradicted_graph(
        vec![derived_edge(
            "Payment.f",
            "User.s",
            EdgeKind::Affects,
            Confidence::Heuristic,
        )],
        true,
    );
    assert_eq!(findings(&g), []);
    assert_eq!(
        status_of(&g, "Payment.f", EdgeKind::Affects),
        ClaimStatus::Verified
    );
}

#[test]
fn unannotated_functions_and_derived_reads_never_fire_w0303() {
    // an unannotated (derived-only) writer is never penalized (D-019), and
    // a derived Read with no declaration is not an undeclared *effect*
    let g = veridikt_graph::build(
        vec![node(
            "User.s",
            Kind::State,
            sp("src/t.py", 1, 2),
            purposed("Target"),
        )],
        &mods(&["Payment", "User"]),
        None,
        DerivedLayer {
            nodes: vec![derived_node("Payment.w", Kind::Function, "src/f.py", 3)],
            edges: vec![
                derived_edge(
                    "Payment.w",
                    "User.s",
                    EdgeKind::Affects,
                    Confidence::Heuristic,
                ),
                derived_edge(
                    "Payment.w",
                    "User.s",
                    EdgeKind::Reads,
                    Confidence::Heuristic,
                ),
            ],
            scope: HashSet::from([PathBuf::from("src/t.py"), PathBuf::from("src/f.py")]),
        },
        ReconcileInput::empty(),
    );
    assert_eq!(findings(&g), []);
}

// ---- W0301 staleness (§9.2, D-068) ----

fn stale_record(t_block: i64, t_subject: i64) -> StalenessRecord {
    StalenessRecord {
        qname: QName::from_dotted("Payment.f"),
        span: sp("src/f.py", 1, 2),
        t_block,
        t_subject,
        t_block_iso: "2026-01-01T10:00:00+00:00".to_string(),
        t_subject_iso: "2026-02-02T10:00:00+00:00".to_string(),
        subject_commit: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
    }
}

fn staleness_graph(staleness: Option<Vec<StalenessRecord>>) -> Graph {
    let mut intent = Intent::default();
    intent.purpose = Some(Spanned {
        value: "f".to_string(),
        span: sp("src/f.py", 1, 1),
    });
    veridikt_graph::build(
        vec![node(
            "Payment.f",
            Kind::Function,
            sp("src/f.py", 3, 6),
            intent,
        )],
        &mods(&["Payment"]),
        None,
        DerivedLayer::empty(),
        ReconcileInput {
            sources: HashMap::new(),
            host_identifiers: HashMap::new(),
            staleness,
        },
    )
}

#[test]
fn a_subject_committed_after_its_block_is_w0301_with_both_timestamps() {
    let g = staleness_graph(Some(vec![stale_record(100, 200)]));
    assert_eq!(findings(&g), [("W0301", 1)]); // the block's span
    assert_eq!(g.findings[0].severity, Severity::Warning);
    assert_eq!(
        g.findings[0].message,
        "stale intent on \"Payment.f\": the subject changed at 2026-02-02T10:00:00+00:00 (commit abcdef123456), after this block was last touched at 2026-01-01T10:00:00+00:00; re-read the code and refresh the block"
    );
}

#[test]
fn ties_and_older_subjects_are_not_stale_and_none_skips_the_check() {
    assert_eq!(
        findings(&staleness_graph(Some(vec![stale_record(200, 200)]))),
        []
    );
    assert_eq!(
        findings(&staleness_graph(Some(vec![stale_record(200, 100)]))),
        []
    );
    assert_eq!(findings(&staleness_graph(None)), []);
}

#[test]
fn strict_promotes_w0301_to_error_with_the_code_unchanged() {
    let mut module = purposed("Payments");
    module.owner = Some(Spanned {
        value: "pay-team".to_string(),
        span: sp("src/f.py", 1, 1),
    });
    module.enforcement = Some(Spanned {
        value: Enforcement::Strict,
        span: sp("src/f.py", 1, 1),
    });
    let mut intent = Intent::default();
    intent.purpose = Some(Spanned {
        value: "f".to_string(),
        span: sp("src/f.py", 3, 3),
    });
    let g = veridikt_graph::build(
        vec![
            node("Payment", Kind::Module, sp("src/f.py", 1, 1), module),
            node("Payment.f", Kind::Function, sp("src/f.py", 3, 6), intent),
        ],
        &mods(&["Payment"]),
        None,
        DerivedLayer::empty(),
        ReconcileInput {
            sources: HashMap::new(),
            host_identifiers: HashMap::new(),
            staleness: Some(vec![stale_record(100, 200)]),
        },
    );
    assert_eq!(findings(&g), [("W0301", 1)]);
    assert_eq!(g.findings[0].severity, Severity::Error);
}
