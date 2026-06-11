//! Tree-sitter binder: attaches each scanned block to the declaration that
//! follows it (spec §7.3–§7.4, D-042). One parse per file, reused across
//! blocks. Unbindable blocks are dropped with a finding — never guessed.

use std::path::Path;

use lore_intent::{Finding, Kind, Span};
use tree_sitter::Node;

use crate::{BoundBlock, Lang, RawBlock, Subject};

pub fn bind(
    path: &Path,
    source: &str,
    lang: Lang,
    blocks: Vec<RawBlock>,
) -> (Vec<BoundBlock>, Vec<Finding>) {
    let mut bound = Vec::new();
    let mut findings = Vec::new();
    if blocks.is_empty() {
        return (bound, findings);
    }

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.grammar())
        .expect("pinned grammar must load");
    let tree = parser
        .parse(source, None)
        .expect("tree-sitter always returns a tree");
    let lines: Vec<&str> = source.lines().collect();

    for block in blocks {
        let block_span = block_point_span(path, &block);
        if matches!(
            block.kind.as_ref().map(|k| k.value),
            Some(Kind::Module | Kind::Service | Kind::Workflow)
        ) {
            // Scoping block (D-042): binds to no declaration.
            if block.name.is_none() {
                let kind = block.kind.as_ref().unwrap().value.name();
                findings.push(Finding::new("E0108", block_span, format!(
                    "a scoping block (\"kind: {kind}\") binds to no declaration, so it needs an explicit \"name:\" field")));
                continue;
            }
            bound.push(BoundBlock {
                block,
                subject: None,
            });
            continue;
        }

        // First non-blank line after the block is the subject candidate.
        let mut row = block.end_line as usize; // end_line is 1-based => this is the next row, 0-based
        while row < lines.len() && lines[row].trim().is_empty() {
            row += 1;
        }
        if row >= lines.len() {
            findings.push(Finding::new("E0102", block_span,
                "this @lore block is not followed by a bindable declaration (found end of file); move it directly above a function, class, type, or assignment".into()));
            continue;
        }

        let candidates = nodes_starting_at_row(tree.root_node(), row);
        let Some(first) = candidates.first().copied() else {
            findings.push(Finding::new("E0102", block_span,
                "this @lore block is not followed by a bindable declaration (found nothing parseable); move it directly above a function, class, type, or assignment".into()));
            continue;
        };
        let Some(node) = candidates
            .iter()
            .map(|&n| descend_wrappers(n, lang))
            .find(|n| lang.declaration_kinds().contains(&n.kind()))
            .or_else(|| skip_siblings(first, lang))
        else {
            findings.push(Finding::new("E0102", block_span, format!(
                "this @lore block is not followed by a bindable declaration (found {}); move it directly above a function, class, type, or assignment", first.kind())));
            continue;
        };

        let identifier = subject_identifier(node, source);
        if identifier.is_none() && block.name.is_none() {
            findings.push(Finding::new("E0104", block_span,
                "this declaration has multiple targets; add an explicit \"name:\" field to the @lore block to choose the subject".into()));
            continue;
        }

        let subject = Subject {
            identifier,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            node_kind: node.kind().to_string(),
        };
        bound.push(BoundBlock {
            block,
            subject: Some(subject),
        });
    }
    (bound, findings)
}

fn block_point_span(path: &Path, block: &RawBlock) -> Span {
    Span {
        file: path.to_path_buf(),
        line: block.start_line,
        col: 1,
        end_line: block.end_line,
        end_col: 1,
    }
}

/// Named nodes starting exactly at `row`, shallowest first, along the single
/// descent path that contains the row (the file root never counts). The list
/// is needed because container nodes (e.g. a Python class-body `block`) can
/// start at the same row as the declaration they hold.
fn nodes_starting_at_row(node: Node<'_>, row: usize) -> Vec<Node<'_>> {
    let mut path = Vec::new();
    let mut current = node;
    loop {
        if current.is_named()
            && current.start_position().row == row
            && current.kind() != "module"
            && current.kind() != "program"
        {
            path.push(current);
        }
        let mut cursor = current.walk();
        let next = current
            .named_children(&mut cursor)
            .find(|c| c.start_position().row <= row && row <= c.end_position().row);
        match next {
            Some(child) => current = child,
            None => return path,
        }
    }
}

/// Sibling skip (D-050c): Rust attributes precede the declaration as sibling
/// nodes, so when the block is followed by an `attribute_item` the binder
/// advances along named siblings past consecutive skips and binds the first
/// declaration node it reaches. The subject span excludes the attributes.
fn skip_siblings(node: Node<'_>, lang: Lang) -> Option<Node<'_>> {
    if !lang.sibling_skip_kinds().contains(&node.kind()) {
        return None;
    }
    let mut n = node;
    loop {
        n = n.next_named_sibling()?;
        if lang.sibling_skip_kinds().contains(&n.kind()) {
            continue;
        }
        let d = descend_wrappers(n, lang);
        return lang.declaration_kinds().contains(&d.kind()).then_some(d);
    }
}

/// §7.4 wrapper/skip sets (D-042).
fn descend_wrappers(node: Node<'_>, lang: Lang) -> Node<'_> {
    let mut n = node;
    loop {
        let next = match (lang, n.kind()) {
            (Lang::Python, "decorated_definition") => n.child_by_field_name("definition"),
            (Lang::Python, "expression_statement") => n.named_child(0),
            (Lang::TypeScript | Lang::Tsx, "export_statement") => {
                n.child_by_field_name("declaration")
            }
            _ => None,
        };
        match next {
            Some(inner) => n = inner,
            None => return n,
        }
    }
}

/// Identifier per the D-042 field table. `None` means extraction needs an
/// explicit `name:` (multiple targets/declarators).
fn subject_identifier(node: Node<'_>, source: &str) -> Option<String> {
    let text = |n: Node<'_>| source[n.byte_range()].to_string();
    match node.kind() {
        "assignment" => {
            let left = node.child_by_field_name("left")?;
            (left.kind() == "identifier").then(|| text(left))
        }
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            let declarators: Vec<_> = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "variable_declarator")
                .collect();
            match declarators.as_slice() {
                [single] => single.child_by_field_name("name").map(text),
                _ => None,
            }
        }
        _ => node.child_by_field_name("name").map(text),
    }
}
