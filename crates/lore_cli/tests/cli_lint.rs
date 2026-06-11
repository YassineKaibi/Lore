use std::process::Command;

fn lore(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lore"))
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

#[test]
fn lint_json_emits_exact_findings_and_exit_1() {
    let out = lore(&["lint", "--json"], &fixture("lint_project"));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings: Vec<(&str, &str, u64)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["code"].as_str().unwrap(),
                f["file"].as_str().unwrap(),
                f["line"].as_u64().unwrap(),
            )
        })
        .collect();
    // sorted by (file, line, code): W0210 ledger (orphaned by the typo'd
    // affects), W0209 balances, E0306 the typo, E0304 the undeclared dep.
    assert_eq!(
        findings,
        [
            ("W0210", "src/pay/svc.py", 4),
            ("W0209", "src/pay/svc.py", 9),
            ("E0306", "src/pay/svc.py", 13),
            ("E0304", "src/pay/svc.py", 15),
        ]
    );
    let e0306 = &v["findings"][2]["message"];
    assert_eq!(
        e0306,
        "unresolved ref \"Payment.ledgr\" in \"affects\" on \"Payment.charge\"; nearest existing qname is \"Payment.ledger\""
    );
    assert_eq!(
        v["summary"],
        serde_json::json!({"errors": 2, "warnings": 2})
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn lint_clean_project_is_exit_0_with_header() {
    let out = lore(&["lint", "--no-color"], &fixture("lint_clean"));
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    // App (ambient), App.count, App.bump; Contains x2 + Affects
    assert_eq!(
        stdout,
        "lint: 3 nodes, 3 edges, 0 findings (0 errors, 0 warnings)\n"
    );
}

#[test]
fn lint_human_output_is_deterministic_and_quiet_drops_the_header() {
    let a = lore(&["lint", "--no-color"], &fixture("lint_project"));
    let b = lore(&["lint", "--no-color"], &fixture("lint_project"));
    assert_eq!(a.stdout, b.stdout);
    let quiet = lore(&["lint", "--no-color", "--quiet"], &fixture("lint_project"));
    let stdout = String::from_utf8_lossy(&quiet.stdout);
    assert!(stdout.starts_with("W0210 "));
    assert!(!stdout.contains("lint:"));
}

#[test]
fn lint_without_manifest_is_e0402_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = lore(&["lint"], tmp.path());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0402"));
}

// ---- T5 CI hardening: [policy] unknown, [lint] overrides, W0207 ----

#[test]
fn policy_unknown_error_promotes_w0213_and_fails_lint() {
    let out = lore(&["lint", "--json"], &fixture("policy_unknown"));
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    // D-057: severity promoted, code unchanged
    assert_eq!(findings[0]["code"], "W0213");
    assert_eq!(findings[0]["severity"], "error");
    assert!(
        findings[0]["message"]
            .as_str()
            .unwrap()
            .contains("\"Concurrency untested\"")
    );
    assert_eq!(v["summary"], serde_json::json!({"errors": 1, "warnings": 0}));
}

#[test]
fn lint_off_overrides_suppress_findings_and_the_exit_surface() {
    // the fixture state would carry W0209 + W0210; both are off (D-056b)
    let out = lore(&["lint", "--no-color"], &fixture("lint_off"));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "lint: 2 nodes, 1 edges, 0 findings (0 errors, 0 warnings)\n"
    );
}

#[test]
fn codeowners_disagreement_is_w0207() {
    let out = lore(&["lint", "--json"], &fixture("codeowners_project"));
    assert_eq!(out.status.code(), Some(0)); // a warning, not an error
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["code"], "W0207");
    assert_eq!(
        findings[0]["message"],
        "owner \"payments-team\" on \"Payment.charge\" disagrees with CODEOWNERS, which maps src/svc.py to @acme/platform; align the owner clause or CODEOWNERS"
    );
}
