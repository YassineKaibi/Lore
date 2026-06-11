//! Rust scanner+binder row (§7.4, D-050), tested at the crate boundary.
//! Fixtures are single-line string literals on purpose: the Lore repo
//! dogfoods itself (G-12), and a raw multi-line literal containing a line
//! that starts with `// @lore` would be scanned as a real block.

use lore_annotations::*;
use lore_intent::QName;
use std::path::PathBuf;

fn scan_rs(src: &str) -> ScanResult {
    let config = ScanConfig {
        modules: vec![ModuleGlob {
            glob: "**".into(),
            module: "M".into(),
        }],
    };
    scan(
        &config,
        &[SourceFile {
            path: PathBuf::from("src/lib.rs"),
            text: src.to_string(),
        }],
    )
}

#[test]
fn non_declaration_after_block_is_e0102_naming_the_node() {
    let r = scan_rs("// @lore\nuse std::fmt;\n");
    assert_eq!(r.findings[0].code, "E0102");
    assert!(r.findings[0].message.contains("use_declaration"));
    assert!(r.blocks.is_empty());
}

#[test]
fn attribute_items_are_skipped_to_the_declaration() {
    let src = "// @lore\n// kind: type\n#[derive(Debug, Clone)]\n#[repr(C)]\npub struct Span { line: u32 }\n";
    let r = scan_rs(src);
    assert!(r.findings.is_empty());
    assert_eq!(r.blocks[0].qname, QName::from_dotted("M.Span"));
    assert_eq!(r.blocks[0].subject.as_deref(), Some("Span"));
    // subject span starts at the struct, attributes excluded (D-050c)
    assert_eq!(r.blocks[0].subject_span, Some((5, 5)));
}

#[test]
fn rust_declaration_kinds_bind_by_their_name_field() {
    let src = "// @lore\npub fn scan() {}\n\n// @lore\n// kind: state\nstatic CACHE: u32 = 0;\n\n// @lore\nconst LIMIT: u32 = 8;\n\n// @lore\n// kind: type\nenum Layer { A, B }\n\n// @lore\ntrait Bind {}\n\n// @lore\nmod scanner;\n";
    let r = scan_rs(src);
    assert!(r.findings.is_empty());
    let names: Vec<String> = r.blocks.iter().map(|b| b.qname.to_string()).collect();
    assert_eq!(
        names,
        [
            "M.scan",
            "M.CACHE",
            "M.LIMIT",
            "M.Layer",
            "M.Bind",
            "M.scanner"
        ]
    );
}

#[test]
fn method_inside_an_impl_block_binds() {
    let src = "struct S;\n\nimpl S {\n    // @lore\n    pub fn bind(&self) {}\n}\n";
    let r = scan_rs(src);
    assert!(r.findings.is_empty());
    assert_eq!(r.blocks[0].qname, QName::from_dotted("M.bind"));
    assert_eq!(r.blocks[0].subject_span, Some((5, 5)));
}

#[test]
fn doc_comments_above_the_block_do_not_break_recognition() {
    // The dogfooding pattern (D-050d): docs, blank line, @lore block, item.
    let src = "/// Renders things.\n\n// @lore\n// purpose: \"Render\"\npub fn render() {}\n";
    let r = scan_rs(src);
    assert!(r.findings.is_empty());
    assert_eq!(r.blocks[0].qname, QName::from_dotted("M.render"));
}
