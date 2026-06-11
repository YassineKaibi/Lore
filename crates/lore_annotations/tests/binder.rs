use lore_annotations::{BoundBlock, Lang, bind, scan_source};
use lore_intent::Finding;
use std::path::Path;

fn bind_lang(src: &str, file: &str, lang: Lang) -> (Vec<BoundBlock>, Vec<Finding>) {
    let p = Path::new(file);
    let (blocks, mut findings) = scan_source(p, src, lang);
    let (bound, f2) = bind(p, src, lang, blocks);
    findings.extend(f2);
    (bound, findings)
}
fn bind_py(src: &str) -> (Vec<BoundBlock>, Vec<Finding>) {
    bind_lang(src, "f.py", Lang::Python)
}
fn bind_ts(src: &str) -> (Vec<BoundBlock>, Vec<Finding>) {
    bind_lang(src, "f.ts", Lang::TypeScript)
}

#[test]
fn unbound_block_is_e0102() {
    let (bound, findings) = bind_py("# @lore\n# purpose: \"floating\"\nprint(1)\n");
    assert!(bound.is_empty());
    assert_eq!(findings[0].code, "E0102");
}

#[test]
fn block_at_eof_is_e0102() {
    let (bound, findings) = bind_py("# @lore\n# purpose: \"nothing follows\"\n");
    assert!(bound.is_empty());
    assert_eq!(findings[0].code, "E0102");
}

#[test]
fn multi_target_assignment_without_name_is_e0104() {
    let (bound, findings) = bind_py("# @lore\n# kind: state\na, b = [], []\n");
    assert!(bound.is_empty());
    assert_eq!(findings[0].code, "E0104");
}

#[test]
fn multi_target_assignment_with_name_binds() {
    let (bound, findings) = bind_py("# @lore\n# kind: state\n# name: pair\na, b = [], []\n");
    assert!(findings.is_empty());
    assert!(bound[0].subject.as_ref().unwrap().identifier.is_none());
}

#[test]
fn scoping_block_without_name_is_e0108() {
    let (_, findings) = bind_py("# @lore\n# kind: module\nx = 1\n");
    assert_eq!(findings[0].code, "E0108");
}

#[test]
fn plain_function_binds_with_body_span() {
    let (bound, findings) =
        bind_py("# @lore\n# purpose: \"charge\"\ndef charge(u, a):\n    return a\n");
    assert!(findings.is_empty());
    let s = bound[0].subject.as_ref().unwrap();
    assert_eq!(s.identifier.as_deref(), Some("charge"));
    assert_eq!((s.start_line, s.end_line), (3, 4)); // body included (§2 subject span)
}

#[test]
fn decorated_python_function_binds_through_wrapper() {
    let src = "# @lore\n# purpose: \"handler\"\n@app.route(\"/x\")\ndef handler():\n    pass\n";
    let (bound, findings) = bind_py(src);
    assert!(findings.is_empty());
    assert_eq!(
        bound[0].subject.as_ref().unwrap().identifier.as_deref(),
        Some("handler")
    );
}

#[test]
fn module_level_assignment_binds_through_expression_statement() {
    let (bound, _) = bind_py("# @lore\n# kind: state\nledger = []\n");
    assert_eq!(
        bound[0].subject.as_ref().unwrap().identifier.as_deref(),
        Some("ledger")
    );
}

#[test]
fn blank_lines_between_block_and_subject_are_skipped() {
    let (bound, _) = bind_py("# @lore\n# purpose: \"x\"\n\n\ndef f():\n    pass\n");
    assert_eq!(
        bound[0].subject.as_ref().unwrap().identifier.as_deref(),
        Some("f")
    );
}

#[test]
fn class_and_method_bind() {
    let src = "# @lore\n# kind: type\nclass C:\n    # @lore\n    # purpose: \"m\"\n    def m(self):\n        pass\n";
    let (bound, _) = bind_py(src);
    assert_eq!(bound.len(), 2);
    assert_eq!(
        bound[1].subject.as_ref().unwrap().identifier.as_deref(),
        Some("m")
    );
}

#[test]
fn exported_ts_function_binds_through_export_statement() {
    let (bound, findings) = bind_ts(
        "// @lore\n// purpose: \"charge\"\nexport function charge(a: number) {\n  return a;\n}\n",
    );
    assert!(findings.is_empty());
    assert_eq!(
        bound[0].subject.as_ref().unwrap().identifier.as_deref(),
        Some("charge")
    );
}

#[test]
fn exported_const_interface_type_enum_class_bind() {
    let src = "// @lore\n// kind: state\nexport const ledger: number[] = [];\n\n// @lore\n// kind: type\nexport interface Entry { amount: number }\n\n// @lore\n// kind: type\nexport type Id = string;\n\n// @lore\n// kind: type\nexport enum Status { Open }\n\n// @lore\nexport class Svc {}\n";
    let (bound, findings) = bind_ts(src);
    assert!(findings.is_empty());
    let ids: Vec<_> = bound
        .iter()
        .map(|b| b.subject.as_ref().unwrap().identifier.clone().unwrap())
        .collect();
    assert_eq!(ids, ["ledger", "Entry", "Id", "Status", "Svc"]);
}

#[test]
fn multi_declarator_const_without_name_is_e0104() {
    let (bound, findings) = bind_ts("// @lore\n// kind: state\nconst a = 1, b = 2;\n");
    assert!(bound.is_empty());
    assert_eq!(findings[0].code, "E0104");
}
