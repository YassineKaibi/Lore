use lore_annotations::*;
use lore_intent::{Kind, QName};
use std::path::PathBuf;

fn cfg(globs: &[(&str, &str)]) -> ScanConfig {
    ScanConfig {
        modules: globs
            .iter()
            .map(|(g, m)| ModuleGlob {
                glob: g.to_string(),
                module: m.to_string(),
            })
            .collect(),
    }
}
fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: PathBuf::from(path),
        text: text.to_string(),
    }
}

#[test]
fn overlapping_globs_are_e0103_with_first_glob_winning() {
    let c = cfg(&[("src/pay/**", "Payment"), ("src/**", "Everything")]);
    let r = scan(
        &c,
        &[file(
            "src/pay/svc.py",
            "# @lore\n# kind: state\nledger = []\n",
        )],
    );
    assert_eq!(r.findings[0].code, "E0103");
    assert!(
        r.findings[0].message.contains("Payment") && r.findings[0].message.contains("Everything")
    );
    assert_eq!(r.blocks[0].qname, QName::from_dotted("Payment.ledger"));
}

#[test]
fn orphan_file_is_w0208_with_orphan_qname() {
    let r = scan(
        &cfg(&[]),
        &[file("misc/x.py", "# @lore\ndef f():\n    pass\n")],
    );
    assert_eq!(r.findings[0].code, "W0208");
    assert_eq!(r.blocks[0].qname, QName::from_dotted("_orphan.f"));
    assert_eq!(r.blocks[0].module, None);
}

#[test]
fn step_outside_workflow_is_e0105() {
    let r = scan(
        &cfg(&[("**", "M")]),
        &[file("a.py", "# @lore\n# kind: step\ndef s():\n    pass\n")],
    );
    assert_eq!(r.findings[0].code, "E0105");
    assert!(r.blocks.is_empty());
}

#[test]
fn step_under_top_of_file_workflow_gets_workflow_qname() {
    let src = "# @lore\n# kind: workflow\n# name: Onboarding\n\n# @lore\n# kind: step\ndef collect():\n    pass\n";
    let r = scan(&cfg(&[("**", "M")]), &[file("a.py", src)]);
    assert!(r.findings.is_empty());
    assert_eq!(r.blocks[0].qname, QName::from_dotted("Onboarding"));
    assert_eq!(r.blocks[0].kind, Kind::Workflow);
    assert_eq!(r.blocks[1].qname, QName::from_dotted("Onboarding.collect"));
    assert_eq!(r.blocks[1].kind, Kind::Step);
}

#[test]
fn top_of_file_module_block_overrides_toml_mapping() {
    let src = "# @lore\n# kind: module\n# name: Billing\n\n# @lore\n# kind: state\nledger = []\n";
    let r = scan(&cfg(&[("**", "Payment")]), &[file("src/svc.py", src)]);
    assert_eq!(r.blocks[1].qname, QName::from_dotted("Billing.ledger"));
    assert_eq!(r.blocks[1].module.as_deref(), Some("Billing"));
}

#[test]
fn glob_mapping_and_name_override_compose() {
    let r = scan(
        &cfg(&[("src/pay/**", "Payment")]),
        &[file(
            "src/pay/s.py",
            "# @lore\n# kind: state\n# name: ledger\nLEDGER = []\n",
        )],
    );
    assert_eq!(r.blocks[0].qname, QName::from_dotted("Payment.ledger"));
    assert_eq!(r.blocks[0].subject.as_deref(), Some("LEDGER"));
}

#[test]
fn default_kind_is_function() {
    let r = scan(
        &cfg(&[("**", "M")]),
        &[file("a.py", "# @lore\ndef f():\n    pass\n")],
    );
    assert_eq!(r.blocks[0].kind, Kind::Function);
}

#[test]
fn results_are_sorted_by_file_then_line() {
    let r = scan(
        &cfg(&[("**", "M")]),
        &[
            file("b.py", "# @lore\ndef g():\n    pass\n"),
            file("a.py", "# @lore\ndef f():\n    pass\n"),
        ],
    );
    let files: Vec<_> = r
        .blocks
        .iter()
        .map(|b| b.file.to_str().unwrap().to_string())
        .collect();
    assert_eq!(files, ["a.py", "b.py"]);
}
