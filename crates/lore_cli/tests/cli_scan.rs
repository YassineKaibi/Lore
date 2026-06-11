use std::process::Command;

fn lore(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scan_project")
}

#[test]
fn scan_json_emits_exact_block_set() {
    let out = lore(&["scan", "--json"], &fixture());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["lore_version"], "0.2.0");
    let qnames: Vec<&str> = v["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["qname"].as_str().unwrap())
        .collect();
    // sorted by (file, line): misc/orphan.py < src/pay/svc.py < src/pay/web.ts
    assert_eq!(
        qnames,
        [
            "_orphan.helper",
            "Payment.ledger",
            "Payment.PaymentSettled",
            "Payment.charge",
            "Payment.render"
        ]
    );
    // one block in full: file, line span, subject identifier, qname, kind (T1 exit criterion)
    assert_eq!(
        v["blocks"][1],
        serde_json::json!({
            "qname": "Payment.ledger",
            "kind": "State",
            "file": "src/pay/svc.py",
            "block_span": {"start": 1, "end": 4},
            "subject": "ledger",
            "subject_span": {"start": 5, "end": 5},
            "module": "Payment"
        })
    );
    let codes: Vec<&str> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["W0208", "E0102"]);
    assert_eq!(out.status.code(), Some(1)); // E0102 present => error severity => exit 1
}

#[test]
fn missing_manifest_is_e0402_exit_2() {
    let tmp = tempfile::tempdir().unwrap();
    let out = lore(&["scan"], tmp.path());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("E0402"));
}

#[test]
fn human_output_is_deterministic() {
    let a = lore(&["scan", "--no-color"], &fixture());
    let b = lore(&["scan", "--no-color"], &fixture());
    assert_eq!(a.stdout, b.stdout);
    assert!(String::from_utf8_lossy(&a.stdout).starts_with("scan: "));
}
