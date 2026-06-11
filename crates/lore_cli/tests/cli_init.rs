use std::process::Command;

fn lore(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn mk_tmp(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let p = dir.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    dir
}

#[test]
fn init_on_bare_python_repo_yields_valid_manifest() {
    let dir = mk_tmp(&[
        ("src/payments/svc.py", "x = 1\n"),
        ("src/users/u.py", "y = 2\n"),
    ]);
    let out = lore(&["init"], dir.path());
    assert_eq!(out.status.code(), Some(0));
    let text = std::fs::read_to_string(dir.path().join("lore.toml")).unwrap();
    let m = lore_cli::manifest::parse(&dir.path().join("lore.toml"), &text).unwrap(); // exit criterion: *valid*
    assert_eq!(m.languages, ["python"]);
    assert_eq!(
        m.modules
            .iter()
            .map(|g| (g.glob.as_str(), g.module.as_str()))
            .collect::<Vec<_>>(),
        [("src/payments/**", "Payments"), ("src/users/**", "Users")]
    );
}

#[test]
fn init_refuses_to_overwrite() {
    let dir = mk_tmp(&[("lore.toml", "[project]\nname=\"x\"\n")]);
    let out = lore(&["init"], dir.path());
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn init_then_scan_round_trips() {
    let dir = mk_tmp(&[("src/pay/svc.py", "# @lore\n# kind: state\nledger = []\n")]);
    lore(&["init"], dir.path());
    let out = lore(&["scan", "--json"], dir.path());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["blocks"][0]["qname"], "Pay.ledger");
}
