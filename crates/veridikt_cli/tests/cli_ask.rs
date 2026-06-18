//! Boundary tests (G-4): `veridikt ask` command in -> exact stdout (human and
//! --json) and §10.5 exit codes out. Unhappy paths first (G-11).

use std::process::Command;

fn veridikt(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_veridikt"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

// ---- unhappy paths first (G-11) ----

#[test]
fn unparseable_query_is_exit_2_with_the_parse_message() {
    let out = veridikt(&["ask", "affects Payment.ledger"], &fixture("ask_project"));
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("expected \"(\" after \"affects\""),
        "{stderr}"
    );
    assert!(out.stdout.is_empty());
}

#[test]
fn unresolved_argument_is_exit_2_naming_the_nearest_qname() {
    let out = veridikt(&["ask", "affects(Payment.legder)"], &fixture("ask_project"));
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr,
        "\"Payment.legder\" names no node in the graph; nearest existing qname is \"Payment.ledger\"\n"
    );
}

#[test]
fn all_without_max_len_is_a_usage_error() {
    let out = veridikt(
        &["ask", "path(Payment.charge, Payment.ledger)", "--all"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn ask_without_manifest_is_e0402_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = veridikt(&["ask", "unknown"], tmp.path());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0402"));
}

// ---- §10.3 human output ----

#[test]
fn affects_star_renders_the_event_hop_chain_with_per_hop_labels() {
    let out = veridikt(
        &["ask", "affects*(Payment.ledger)", "--no-color"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    // T7: the fixture's pass-bodies never mention the states they claim to
    // touch, so §9.1's occurrence test proves the claims Contradicted
    // (D-066) — and the label rides every hop; emits stays Unverifiable
    // (Phase 1).
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "affects*(Payment.ledger): 2 results\n\
         Payment.audit   Function  src/pay/svc.py:28  [via: Affects]  [Declared/Contradicted]\n\
         Payment.charge  Function  src/pay/svc.py:22  [via: Emits -> Handles -> Affects]  \
         [Declared/Unverifiable -> Declared/Unverifiable -> Declared/Contradicted]\n"
    );
}

#[test]
fn path_renders_the_shortest_witnessed_chain() {
    let out = veridikt(
        &["ask", "path(Payment.charge, Payment.ledger)", "--no-color"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "path(Payment.charge, Payment.ledger): 1 result\n\
         Payment.ledger  State  src/pay/svc.py:4  [via: Emits -> Handles -> Affects]  \
         [Declared/Unverifiable -> Declared/Unverifiable -> Declared/Contradicted]\n"
    );
}

#[test]
fn unknown_query_prints_the_unknown_strings_indented() {
    let out = veridikt(&["ask", "unknown", "--no-color"], &fixture("ask_project"));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "unknown: 1 result\n\
         Payment.charge  Function  src/pay/svc.py:22\n\
         \x20 unknown: \"Concurrent charge + refund untested\"\n"
    );
}

#[test]
fn show_renders_the_full_node_card() {
    let out = veridikt(
        &["ask", "show(Payment.charge)", "--no-color"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    // origin Both: the annotation and the derived layer index the same
    // declaration and merged (§8.1, D-060b)
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Payment.charge  Function  Both  src/pay/svc.py:22\n\
         \x20 purpose: \"Charge a customer\"\n\
         \x20 unknown: \"Concurrent charge + refund untested\"\n\
         \x20 reads: Payment.balances\n\
         \x20 emits: Payment.Settled\n\
         edges out:\n\
         \x20 Reads -> Payment.balances  [Declared/Contradicted]\n\
         \x20 Emits -> Payment.Settled  [Declared/Unverifiable]\n\
         edges in:\n\
         \x20 Contains <- Payment  [Derived/Exact]\n\
         findings:\n\
         \x20 W0302 src/pay/svc.py:19  contradicted claim: \"reads: Payment.balances\" on \
         \"Payment.charge\", whose subject span never mentions \"balances\"; the code no \
         longer does what the claim says — update or remove the clause\n\
         \x20 W0213 src/pay/svc.py:21  function \"Payment.charge\" declares an unknown: \
         \"Concurrent charge + refund untested\"; resolve it and remove the clause once it is answered\n"
    );
}

#[test]
fn show_works_for_a_node_without_declared_intent() {
    // the ambient Payment module has no intent block (D-046)
    let out = veridikt(
        &["ask", "show(Payment)", "--no-color"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("Payment  Module  Declared  veridikt.toml:1\n"),
        "{stdout}"
    );
    assert!(stdout.contains("  no declared intent\n"), "{stdout}");
}

// ---- §10.4 JSON ----

#[test]
fn ask_json_matches_the_schema_exactly() {
    let out = veridikt(
        &["ask", "affects(Payment.ledger)", "--json"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    let mut v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // stats values vary run to run; assert shape, then compare the rest
    let stats = v["stats"].take();
    assert!(stats["nodes_visited"].is_u64());
    assert!(stats["elapsed_ms"].is_u64());
    assert_eq!(
        v,
        serde_json::json!({
            "veridikt_version": "0.2.0",
            "query": "affects(Payment.ledger)",
            "results": [
                {
                    "qname": "Payment.audit",
                    "kind": "Function",
                    "location": {"file": "src/pay/svc.py", "line": 28},
                    "via": [
                        {"from": "Payment.audit", "to": "Payment.ledger",
                         "edge": "Affects", "layer": "Declared", "status": "Contradicted"}
                    ]
                }
            ],
            "unresolved": [],
            "stats": null,
        })
    );
}

#[test]
fn show_json_returns_the_node_card_with_edge_arrays() {
    let out = veridikt(
        &["ask", "show(Payment.audit)", "--json"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["node"]["qname"], "Payment.audit");
    assert_eq!(v["node"]["kind"], "Function");
    assert_eq!(v["node"]["origin"], "Both"); // merged with its derived node (D-060b)
    assert_eq!(
        v["node"]["intent"]["on"],
        serde_json::json!(["Payment.Settled"])
    );
    assert_eq!(
        v["node"]["intent"]["affects"],
        serde_json::json!(["Payment.ledger"])
    );
    assert_eq!(v["node"]["intent"]["purpose"], serde_json::Value::Null);
    let out_kinds: Vec<&str> = v["edges_out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["edge"].as_str().unwrap())
        .collect();
    assert_eq!(out_kinds, ["Affects", "Handles"]);
    // ask/show render the graph's findings unfiltered (D-056c): the
    // pass-body's contradicted affects claim shows up on the card.
    assert_eq!(v["findings"].as_array().unwrap().len(), 1);
    assert_eq!(v["findings"][0]["code"], "W0302");
    assert_eq!(v["findings"][0]["severity"], "warning");
}

#[test]
fn unknown_query_json_carries_the_unknown_strings() {
    let out = veridikt(&["ask", "unknown", "--json"], &fixture("ask_project"));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["results"][0]["unknown"],
        serde_json::json!(["Concurrent charge + refund untested"])
    );
}

// ---- honesty: graph findings never fail ask, unresolved refs surface ----

#[test]
fn ask_answers_on_a_project_with_findings_and_reports_unresolved_refs() {
    // lint_project carries an E0306 typo (Payment.ledgr) and an E0304: lint
    // exits 1 on it, ask still answers (D-053b) and surfaces the unresolved
    // ref on every response (D-053c, G-7).
    let out = veridikt(
        &["ask", "reads(Payment.balances)", "--no-color"],
        &fixture("lint_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "reads(Payment.balances): 1 result\n\
         Payment.charge  Function  src/pay/svc.py:16  [via: Reads]  [Declared/Contradicted]\n\
         note: 1 unresolved ref in the graph (run veridikt lint): Payment.ledgr\n"
    );

    let out = veridikt(
        &["ask", "reads(Payment.balances)", "--json"],
        &fixture("lint_project"),
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["unresolved"], serde_json::json!(["Payment.ledgr"]));

    // --quiet drops the header and the note, keeps the results
    let out = veridikt(
        &["ask", "reads(Payment.balances)", "--no-color", "--quiet"],
        &fixture("lint_project"),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("Payment.charge"), "{stdout}");
    assert!(!stdout.contains("note:"), "{stdout}");
}
