//! T2 boundary tests (G-4): clause text in → exact `Intent` out, including
//! spans; every E020x parse diagnostic asserted by code and message shape.
//! Unhappy path first (G-11).

use std::path::PathBuf;

use lore_intent::{
    Enforcement, Finding, HttpMethod, Intent, Ref, Route, Span, Spanned, parse_intent,
};

fn sp(text: &str, line: u32) -> Spanned<String> {
    Spanned {
        value: text.to_string(),
        span: span(line, 1, line, text.len() as u32 + 1),
    }
}

fn span(line: u32, col: u32, end_line: u32, end_col: u32) -> Span {
    Span {
        file: PathBuf::from("f.py"),
        line,
        col,
        end_line,
        end_col,
    }
}

fn parse_one(text: &str) -> (Intent, Vec<Finding>) {
    parse_intent(&[sp(text, 4)])
}

fn prose(value: &str, s: Span) -> Option<Spanned<String>> {
    Some(Spanned {
        value: value.to_string(),
        span: s,
    })
}

fn r(segments: &[&str], s: Span) -> Spanned<Ref> {
    Spanned {
        value: Ref {
            segments: segments.iter().map(|s| s.to_string()).collect(),
        },
        span: s,
    }
}

// ---------------------------------------------------------------- unhappy

#[test]
fn unknown_clause_is_e0202_with_nearest_suggestion() {
    let (intent, findings) = parse_one("afects: Payment.ledger");
    assert_eq!(
        intent,
        Intent::default(),
        "E0202 clause contributes nothing"
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "E0202");
    assert_eq!(
        findings[0].message,
        "unknown clause \"afects\"; did you mean \"affects\"?"
    );
    assert_eq!(findings[0].span, span(4, 1, 4, 7), "span covers the name");
}

#[test]
fn e0202_suggests_across_the_whole_clause_set() {
    for (got, want) in [
        ("purpos", "purpose"),
        ("onwer", "owner"),
        ("dependson", "depends_on"),
        ("emit", "emits"),
        ("enforce", "enforcement"),
    ] {
        let (_, findings) = parse_one(&format!("{got}: \"x\""));
        assert_eq!(findings[0].code, "E0202", "{got}");
        assert!(
            findings[0]
                .message
                .contains(&format!("did you mean \"{want}\"")),
            "{got}: {}",
            findings[0].message
        );
    }
}

#[test]
fn duplicate_singular_clause_is_e0206_and_first_wins() {
    let (intent, findings) =
        parse_intent(&[sp("purpose: \"first\"", 2), sp("purpose: \"second\"", 3)]);
    assert_eq!(intent.purpose.as_ref().unwrap().value, "first");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "E0206");
    assert_eq!(
        findings[0].message,
        "duplicate \"purpose\" clause; \"purpose\" appears at most once per block (first at line 2); remove one"
    );
    assert_eq!(findings[0].span.line, 3, "points at the repeat");
}

#[test]
fn every_singular_clause_reports_e0206_on_repeat() {
    for clause in ["owner: \"a\"", "route: \"/x\"", "enforcement: strict"] {
        let (_, findings) = parse_intent(&[sp(clause, 2), sp(clause, 3)]);
        assert_eq!(findings.len(), 1, "{clause}");
        assert_eq!(findings[0].code, "E0206", "{clause}");
    }
}

#[test]
fn repeatable_clauses_do_not_e0206() {
    let (intent, findings) = parse_intent(&[
        sp("because: \"a\"", 2),
        sp("because: \"b\"", 3),
        sp("unknown: \"c\"", 4),
        sp("unknown: \"d\"", 5),
        sp("assumes: \"e\"", 6),
        sp("assumes: \"f\"", 7),
        sp("affects: A.x", 8),
        sp("affects: B.y", 9),
    ]);
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(intent.because.len(), 2);
    assert_eq!(intent.unknown.len(), 2);
    assert_eq!(intent.assumes.len(), 2);
    assert_eq!(intent.affects.len(), 2);
}

#[test]
fn missing_colon_is_e0207() {
    let (_, findings) = parse_one("purpose \"p\"");
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "malformed clause; expected \":\" after \"purpose\""
    );
}

#[test]
fn line_without_clause_name_is_e0207() {
    let (_, findings) = parse_one(":: weird");
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "malformed clause; a clause line is \"<name>: <value>\""
    );
}

#[test]
fn unquoted_prose_is_e0207() {
    let (intent, findings) = parse_one("purpose: no quotes");
    assert_eq!(intent.purpose, None);
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "\"purpose\" expects a quoted string, e.g. purpose: \"...\""
    );
}

#[test]
fn unterminated_string_is_e0207() {
    let (_, findings) = parse_one("purpose: \"never closes");
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "unterminated string in \"purpose\" clause; add a closing '\"'"
    );
}

#[test]
fn empty_ref_list_is_e0207() {
    let (intent, findings) = parse_one("affects:");
    assert!(intent.affects.is_empty());
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "\"affects\" expects one or more dotted refs like Payment.ledger, separated by commas"
    );
}

#[test]
fn malformed_ref_drops_the_whole_clause() {
    // D-045: no partial ref lists -- a half-parsed claim is a guess (G-7).
    let (intent, findings) = parse_one("reads: Payment.balances, Payment.");
    assert!(intent.reads.is_empty());
    assert_eq!(findings[0].code, "E0207");
}

#[test]
fn trailing_text_after_value_is_e0207() {
    let (intent, findings) = parse_one("enforcement: strict now");
    assert_eq!(intent.enforcement, None);
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "unexpected text after \"enforcement\" clause value: \"now\""
    );
}

#[test]
fn invalid_enforcement_value_is_e0207() {
    let (_, findings) = parse_one("enforcement: maybe");
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "\"enforcement\" must be \"strict\" or \"warn\", got \"maybe\""
    );
}

#[test]
fn invalid_route_method_is_e0207() {
    let (_, findings) = parse_one("route: FETCH \"/x\"");
    assert_eq!(findings[0].code, "E0207");
    assert_eq!(
        findings[0].message,
        "\"route\" expects METHOD \"<path>\" or \"<path>\", where METHOD is GET, POST, PUT, DELETE, or PATCH"
    );
}

#[test]
fn one_bad_clause_does_not_poison_the_rest() {
    let (intent, findings) = parse_intent(&[
        sp("purpose: \"p\"", 2),
        sp("afects: A.x", 3),
        sp("reads: B.y", 4),
    ]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "E0202");
    assert!(intent.purpose.is_some());
    assert_eq!(intent.reads.len(), 1);
}

// ------------------------------------------------------------------ happy

/// T2 exit criterion: clause text → exact `Intent` (including spans) for
/// every clause in the §3.1 set.
#[test]
fn every_clause_parses_to_the_exact_intent() {
    let cases: Vec<(&str, Intent)> = vec![
        (
            "purpose: \"Append-only record\"",
            Intent {
                purpose: prose("Append-only record", span(4, 10, 4, 30)),
                ..Intent::default()
            },
        ),
        (
            "owner: \"payments-team\"",
            Intent {
                owner: prose("payments-team", span(4, 8, 4, 23)),
                ..Intent::default()
            },
        ),
        (
            "because: \"b\"",
            Intent {
                because: vec![Spanned {
                    value: "b".to_string(),
                    span: span(4, 10, 4, 13),
                }],
                ..Intent::default()
            },
        ),
        (
            "unknown: \"u\"",
            Intent {
                unknown: vec![Spanned {
                    value: "u".to_string(),
                    span: span(4, 10, 4, 13),
                }],
                ..Intent::default()
            },
        ),
        (
            "assumes: \"a\"",
            Intent {
                assumes: vec![Spanned {
                    value: "a".to_string(),
                    span: span(4, 10, 4, 13),
                }],
                ..Intent::default()
            },
        ),
        (
            "affects: Payment.ledger",
            Intent {
                affects: vec![r(&["Payment", "ledger"], span(4, 10, 4, 24))],
                ..Intent::default()
            },
        ),
        (
            "reads: Payment.balances",
            Intent {
                reads: vec![r(&["Payment", "balances"], span(4, 8, 4, 24))],
                ..Intent::default()
            },
        ),
        (
            "triggers: User.notify",
            Intent {
                triggers: vec![r(&["User", "notify"], span(4, 11, 4, 22))],
                ..Intent::default()
            },
        ),
        (
            "emits: Payment.PaymentSettled",
            Intent {
                emits: vec![r(&["Payment", "PaymentSettled"], span(4, 8, 4, 30))],
                ..Intent::default()
            },
        ),
        (
            "on: Payment.PaymentSettled",
            Intent {
                on: vec![r(&["Payment", "PaymentSettled"], span(4, 5, 4, 27))],
                ..Intent::default()
            },
        ),
        (
            "depends_on: User, Billing.Gateway",
            Intent {
                depends_on: vec![
                    r(&["User"], span(4, 13, 4, 17)),
                    r(&["Billing", "Gateway"], span(4, 19, 4, 34)),
                ],
                ..Intent::default()
            },
        ),
        (
            "route: POST \"/charge\"",
            Intent {
                route: Some(Spanned {
                    value: Route {
                        method: Some(HttpMethod::Post),
                        path: "/charge".to_string(),
                    },
                    span: span(4, 8, 4, 22),
                }),
                ..Intent::default()
            },
        ),
        (
            "route: \"/payments\"",
            Intent {
                route: Some(Spanned {
                    value: Route {
                        method: None,
                        path: "/payments".to_string(),
                    },
                    span: span(4, 8, 4, 19),
                }),
                ..Intent::default()
            },
        ),
        (
            "enforcement: strict",
            Intent {
                enforcement: Some(Spanned {
                    value: Enforcement::Strict,
                    span: span(4, 14, 4, 20),
                }),
                ..Intent::default()
            },
        ),
        (
            "enforcement: warn",
            Intent {
                enforcement: Some(Spanned {
                    value: Enforcement::Warn,
                    span: span(4, 14, 4, 18),
                }),
                ..Intent::default()
            },
        ),
    ];
    for (text, want) in cases {
        let (got, findings) = parse_one(text);
        assert!(findings.is_empty(), "{text}: {findings:?}");
        assert_eq!(got, want, "{text}");
    }
}

#[test]
fn multi_ref_list_gives_each_ref_its_own_span() {
    let (intent, findings) = parse_one("affects: A.b, C.d");
    assert!(findings.is_empty());
    assert_eq!(
        intent.affects,
        vec![
            r(&["A", "b"], span(4, 10, 4, 13)),
            r(&["C", "d"], span(4, 15, 4, 18)),
        ]
    );
}

#[test]
fn string_escapes_per_d045() {
    // \" and \\ unescape; any other \x stays verbatim.
    let (intent, findings) = parse_one(r#"purpose: "say \"hi\" \n c:\\temp""#);
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(intent.purpose.unwrap().value, "say \"hi\" \\n c:\\temp");
}

#[test]
fn multi_line_string_keeps_newline_and_spans_both_lines() {
    // The scanner reassembles continuation lines with '\n' (§7.2); the
    // parser must keep the newline and produce a two-line span.
    let text = "purpose: \"line one\nline two\"";
    let clause = Spanned {
        value: text.to_string(),
        span: span(4, 1, 5, 10),
    };
    let (intent, findings) = parse_intent(&[clause]);
    assert!(findings.is_empty(), "{findings:?}");
    let p = intent.purpose.unwrap();
    assert_eq!(p.value, "line one\nline two");
    assert_eq!(p.span, span(4, 10, 5, 10));
}

#[test]
fn whitespace_only_clause_lines_are_skipped() {
    let (intent, findings) = parse_intent(&[sp("", 2), sp("   ", 3)]);
    assert_eq!(intent, Intent::default());
    assert!(findings.is_empty());
}

#[test]
fn the_canonical_example_block_parses_in_full() {
    // §19 charge() block, clause for clause.
    let (intent, findings) = parse_intent(&[
        sp("purpose: \"Charge a customer\"", 2),
        sp(
            "because: \"Idempotency key is generated by the caller -- we do not deduplicate here\"",
            3,
        ),
        sp(
            "assumes: \"amount is non-negative and already currency-validated\"",
            4,
        ),
        sp("affects: Payment.ledger", 5),
        sp("reads: Payment.balances", 6),
        sp("emits: Payment.PaymentSettled", 7),
        sp(
            "unknown: \"Behavior under concurrent charge + refund on one account is untested\"",
            8,
        ),
    ]);
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(intent.purpose.unwrap().value, "Charge a customer");
    assert_eq!(intent.because.len(), 1);
    assert_eq!(intent.assumes.len(), 1);
    assert_eq!(intent.unknown.len(), 1);
    assert_eq!(intent.affects[0].value.segments, ["Payment", "ledger"]);
    assert_eq!(intent.reads[0].value.segments, ["Payment", "balances"]);
    assert_eq!(
        intent.emits[0].value.segments,
        ["Payment", "PaymentSettled"]
    );
}
