use std::process::Command;

fn veridikt(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_veridikt"))
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
    let out = veridikt(&["init"], dir.path());
    assert_eq!(out.status.code(), Some(0));
    let text = std::fs::read_to_string(dir.path().join("veridikt.toml")).unwrap();
    let m = veridikt_cli::manifest::parse(&dir.path().join("veridikt.toml"), &text).unwrap(); // exit criterion: *valid*
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
fn init_covers_root_level_source_files_with_disjoint_glob() {
    // src/main.rs + src/config.rs live directly under the root: D-081 adds a
    // trailing `src/*` glob (named for the project) so they are scoped, not
    // orphaned. It is disjoint from the per-directory `src/users/**` glob.
    let dir = tempfile::Builder::new().prefix("paysvc").tempdir().unwrap();
    for (path, content) in [
        ("src/main.rs", "fn main() {}\n"),
        ("src/config.rs", "pub const X: u8 = 1;\n"),
        ("src/users/u.rs", "pub fn u() {}\n"),
    ] {
        let p = dir.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let out = veridikt(&["init"], dir.path());
    assert_eq!(out.status.code(), Some(0));
    let text = std::fs::read_to_string(dir.path().join("veridikt.toml")).unwrap();
    let m = veridikt_cli::manifest::parse(&dir.path().join("veridikt.toml"), &text).unwrap();
    let project = pascal_case_of_dir(dir.path());
    assert_eq!(
        m.modules
            .iter()
            .map(|g| (g.glob.as_str(), g.module.as_str()))
            .collect::<Vec<_>>(),
        [("src/users/**", "Users"), ("src/*", project.as_str())]
    );

    // G-4 boundary: init's real contract is a manifest that lints clean. The
    // `src/*` catch-all must be disjoint from `src/users/**` under the module
    // matcher (D-082) -- so `veridikt lint` on the unedited output reports no
    // E0103. (Asserting only the glob strings, as above, missed the overlap
    // bug D-082 fixes.)
    let lint = veridikt(&["lint"], dir.path());
    let stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(
        !stdout.contains("E0103"),
        "init output must lint clean, got:\n{stdout}"
    );
    assert_eq!(lint.status.code(), Some(0));
}

/// `veridikt init` names the root-files module after the project (its directory).
fn pascal_case_of_dir(dir: &std::path::Path) -> String {
    dir.file_name()
        .unwrap()
        .to_string_lossy()
        .split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().unwrap().to_uppercase().collect::<String>() + c.as_str()
        })
        .collect()
}

#[test]
fn init_refuses_to_overwrite() {
    let dir = mk_tmp(&[("veridikt.toml", "[project]\nname=\"x\"\n")]);
    let out = veridikt(&["init"], dir.path());
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn init_then_scan_round_trips() {
    let dir = mk_tmp(&[(
        "src/pay/svc.py",
        "# @veridikt\n# kind: state\nledger = []\n",
    )]);
    veridikt(&["init"], dir.path());
    let out = veridikt(&["scan", "--json"], dir.path());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["blocks"][0]["qname"], "Pay.ledger");
}
