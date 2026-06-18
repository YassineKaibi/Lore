//! T6 exit criteria at the binary boundary (G-4): cold-start queries on a
//! repo with zero annotations (D-002, D-060), a Verified claim end to end,
//! and `veridikt stats` with the derivation counters (D-065).

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

// ---- cold start: zero annotations (D-002) ----

#[test]
fn call_graph_queries_answer_on_a_repo_with_zero_annotations() {
    let out = veridikt(
        &["ask", "callers(Payment.charge)", "--no-color"],
        &fixture("derive_calls"),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "callers(Payment.charge): 2 results\n\
         Payment.refund  Function  src/pay/svc.py:5  [via: Calls]  [Derived/Exact]\n\
         User.signup     Function  src/user/u.py:4  [via: Calls]  [Derived/Resolved]\n"
    );
}

#[test]
fn show_renders_a_derived_only_node_card() {
    let out = veridikt(
        &["ask", "show(Payment.charge)", "--no-color"],
        &fixture("derive_calls"),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Payment.charge  Function  Derived  src/pay/svc.py:1\n\
         \x20 no declared intent\n\
         edges in:\n\
         \x20 Contains <- Payment  [Derived/Exact]\n\
         \x20 Calls <- Payment.refund  [Derived/Exact]\n\
         \x20 Calls <- User.signup  [Derived/Resolved]\n"
    );
}

#[test]
fn an_unannotated_repo_lints_clean() {
    // derived-only nodes are exempt from requirement checks (D-046):
    // cold start must not punish the absence of annotations
    let out = veridikt(
        &["lint", "--no-stale", "--no-color"],
        &fixture("derive_calls"),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "lint: 5 nodes, 5 edges, 0 findings (0 errors, 0 warnings)\n"
    );
}

// ---- the trust loop: a claim verified by the derived layer (D-063) ----

#[test]
fn affects_answers_with_a_verified_claim_when_the_write_is_derived() {
    // the only annotations are the state and the claim; the ledger.append
    // in the body produces the matching derived edge -> Verified
    let out = veridikt(
        &["ask", "affects(Payment.ledger)", "--no-color"],
        &fixture("derive_project"),
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "affects(Payment.ledger): 1 result\n\
         Payment.charge  Function  src/pay/svc.py:9  [via: Affects]  [Declared/Verified]\n"
    );
}

// ---- veridikt stats (D-065) ----

#[test]
fn stats_reports_kind_origin_counts_and_the_drop_counters() {
    let out = veridikt(&["stats", "--no-color"], &fixture("derive_project"));
    assert_eq!(out.status.code(), Some(0));
    // the one unresolved call is `ledger.append(user)`: a method call on a
    // non-local object is dropped and counted, never guessed (§8.2, G-7)
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "stats: 6 nodes, 8 edges (1 declared, 7 derived)\n\
         nodes by kind (declared/derived/both, with intent):\n\
         \x20 module    2  (2/0/0, 0 with intent)\n\
         \x20 state     1  (1/0/0, 1 with intent)\n\
         \x20 function  3  (0/2/1, 1 with intent)\n\
         claims by status: 1 (1 verified, 0 unverified, 0 contradicted, 0 unverifiable)\n\
         unresolved_calls: 1\n\
         ambiguous_derived_names: 0\n"
    );
}

#[test]
fn stats_json_field_names_are_pinned() {
    let out = veridikt(&["stats", "--json"], &fixture("derive_project"));
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "veridikt_version": "0.2.0",
            "nodes": {
                "total": 6,
                "by_kind": {
                    "Module":   {"total": 2, "declared": 2, "derived": 0, "both": 0, "with_intent": 0},
                    "State":    {"total": 1, "declared": 1, "derived": 0, "both": 0, "with_intent": 1},
                    "Function": {"total": 3, "declared": 0, "derived": 2, "both": 1, "with_intent": 1},
                },
            },
            "edges": {"total": 8, "declared": 1, "derived": 7},
            // D-069: the claims-by-status breakdown joins at T7
            "claims": {"total": 1, "verified": 1, "unverified": 0, "contradicted": 0, "unverifiable": 0},
            "unresolved_calls": 1,
            "ambiguous_derived_names": 0,
        })
    );
}

#[test]
fn stats_without_manifest_is_e0402_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = veridikt(&["stats"], tmp.path());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0402"));
}

// ---- the cache is an implementation detail (D-064) ----

#[test]
fn a_second_run_over_the_cache_answers_identically() {
    let q = &["ask", "callers(Payment.charge)", "--no-color"];
    let first = veridikt(q, &fixture("derive_calls"));
    let second = veridikt(q, &fixture("derive_calls"));
    assert_eq!(first.stdout, second.stdout);
    assert!(
        fixture("derive_calls")
            .join(".veridikt-cache/derive")
            .is_dir()
    );
}
