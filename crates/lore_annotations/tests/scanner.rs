use lore_annotations::{RawBlock, scan_source};
use lore_intent::{Finding, Kind};
use std::path::Path;

fn scan_py(src: &str) -> (Vec<RawBlock>, Vec<Finding>) {
    scan_source(Path::new("f.py"), src, "#")
}

#[test]
fn plain_comment_run_is_not_a_block() {
    let (blocks, findings) = scan_py("# just a comment\n# another\nx = 1\n");
    assert!(blocks.is_empty() && findings.is_empty());
}

#[test]
fn marker_must_be_first_line_of_the_run() {
    let (blocks, _) = scan_py("# preamble\n# @lore\n# purpose: \"p\"\nx = 1\n");
    assert!(
        blocks.is_empty(),
        "@lore mid-run is not a block start (first content line rule)"
    );
}

#[test]
fn blank_line_ends_block() {
    let (blocks, _) = scan_py("# @lore\n# kind: state\n\n# purpose: \"orphaned\"\nx = 1\n");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].end_line, 2);
    assert_eq!(blocks[0].raw_clauses.len(), 0);
}

#[test]
fn invalid_kind_is_e0106_and_block_still_emitted() {
    let (blocks, findings) = scan_py("# @lore\n# kind: klass\nx = 1\n");
    assert_eq!(findings.iter().filter(|f| f.code == "E0106").count(), 1);
    assert!(findings[0].message.contains("klass"));
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].kind.is_none());
}

#[test]
fn duplicate_kind_is_e0106() {
    let (_, findings) = scan_py("# @lore\n# kind: state\n# kind: event\nx = 1\n");
    assert_eq!(findings.iter().filter(|f| f.code == "E0106").count(), 1);
}

#[test]
fn invalid_name_is_e0107() {
    let (_, findings) = scan_py("# @lore\n# name: 9bad.name\nx = 1\n");
    assert_eq!(findings[0].code, "E0107");
}

#[test]
fn happy_block_with_kind_name_and_clauses() {
    let src =
        "# @lore\n# kind: state\n# name: ledger\n# purpose: \"Append-only record\"\nledger = []\n";
    let (blocks, findings) = scan_py(src);
    assert!(findings.is_empty());
    let b = &blocks[0];
    assert_eq!((b.start_line, b.end_line), (1, 4));
    assert_eq!(b.kind.as_ref().unwrap().value, Kind::State);
    assert_eq!(b.name.as_ref().unwrap().value, "ledger");
    assert_eq!(b.raw_clauses[0].value, "purpose: \"Append-only record\"");
    assert_eq!(b.raw_clauses[0].span.line, 4);
}

#[test]
fn comment_token_strips_at_most_one_space() {
    // "#  x" keeps one leading space after stripping "# " (§7.1)
    let (blocks, _) = scan_py("# @lore\n#  purpose: \"x\"\nx = 1\n");
    assert_eq!(blocks[0].raw_clauses[0].value, " purpose: \"x\"");
}

#[test]
fn multiline_string_is_reassembled_into_one_clause() {
    let src = "# @lore\n# purpose: \"line one\n# line two\"\nx = 1\n";
    let (blocks, findings) = scan_py(src);
    assert!(findings.is_empty());
    let c = &blocks[0].raw_clauses[0];
    assert_eq!(c.value, "purpose: \"line one\nline two\"");
    assert_eq!((c.span.line, c.span.end_line), (2, 3));
    assert_eq!(c.span.end_col, "line two\"".len() as u32 + 1);
}

#[test]
fn kind_line_inside_open_string_is_text_not_a_binding_field() {
    let src = "# @lore\n# purpose: \"spans\n# kind: state\"\nx = 1\n";
    let (blocks, findings) = scan_py(src);
    assert!(findings.is_empty(), "{findings:?}");
    assert!(blocks[0].kind.is_none());
    assert_eq!(blocks[0].raw_clauses.len(), 1);
}

#[test]
fn escaped_quote_does_not_close_a_multiline_string() {
    let src = "# @lore\n# purpose: \"say \\\"hi\n# bye\"\nx = 1\n";
    let (blocks, _) = scan_py(src);
    assert_eq!(blocks[0].raw_clauses.len(), 1);
    assert_eq!(
        blocks[0].raw_clauses[0].value,
        "purpose: \"say \\\"hi\nbye\""
    );
}

#[test]
fn unterminated_string_consumes_the_rest_of_the_block() {
    // The parser reports E0207 on the reassembled clause; the scanner just
    // hands over everything up to the end of the block.
    let src = "# @lore\n# purpose: \"never closed\n# affects: X\nx = 1\n";
    let (blocks, findings) = scan_py(src);
    assert!(findings.is_empty());
    assert_eq!(blocks[0].raw_clauses.len(), 1);
}

#[test]
fn typescript_comment_token() {
    let (blocks, _) = scan_source(
        Path::new("f.ts"),
        "// @lore\n// kind: type\ntype A = string;\n",
        "//",
    );
    assert_eq!(blocks[0].kind.as_ref().unwrap().value, Kind::Type);
}

#[test]
fn indented_blocks_scan_too() {
    let src = "class C:\n    # @lore\n    # purpose: \"method\"\n    def m(self): pass\n";
    let (blocks, _) = scan_py(src);
    assert_eq!((blocks[0].start_line, blocks[0].end_line), (2, 3));
}
