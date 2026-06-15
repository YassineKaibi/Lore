//! The generic tree-sitter binder (spec §7.3–§7.4, §8.6.3, D-042/D-044/D-050,
//! D-070). One adapter for every language: the pack supplies `bind.scm` (which
//! marks each §7.4 declaration node with `@subject.function|type|value` and
//! captures its identifier as `@subject.name`) plus the `wrappers` /
//! `sibling_skips` node-kind lists; the binder *mechanics* — same-row search,
//! wrapper descent, sibling skips, the body-included subject span — are engine
//! behavior identical for all packs (D-070g).

use std::collections::HashMap;
use std::path::Path;

use lore_intent::{Finding, Kind, PackSpec, Span};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::{BoundBlock, RawBlock, Subject};

/// The fixed `bind.scm` capture vocabulary (§8.6.3). Any other capture name
/// is `E0411` (an unusable artifact: the adapter would not know what it means).
const SUBJECT_CAPTURES: [&str; 4] = [
    "subject.function",
    "subject.type",
    "subject.value",
    "subject.name",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubjectKind {
    Function,
    Type,
    Value,
}

/// A pack activated for binding: the grammar handle plus the compiled
/// `bind.scm` and the descend/skip node-kind lists. Compiled once at
/// activation (D-070d); reused across every file of the language.
pub struct Binder {
    grammar: Language,
    query: Query,
    func_caps: Vec<u32>,
    type_caps: Vec<u32>,
    value_caps: Vec<u32>,
    name_caps: Vec<u32>,
    wrappers: Vec<String>,
    sibling_skips: Vec<String>,
}

impl Binder {
    /// Compile `bind.scm` against the grammar. A query that does not compile,
    /// or names a capture outside the §8.6.3 vocabulary, is `E0411`.
    pub fn new(pack: &PackSpec, grammar: &Language, span: Span) -> Result<Binder, Finding> {
        let src = pack.bind_scm.as_deref().ok_or_else(|| {
            Finding::new(
                "E0411",
                span.clone(),
                format!(
                    "pack \"{}\" has no bind.scm but binding was requested",
                    pack.name
                ),
            )
        })?;
        let query = Query::new(grammar, src).map_err(|e| {
            Finding::new(
                "E0411",
                span.clone(),
                format!("bind.scm does not compile against the grammar: {e}"),
            )
        })?;
        // Validate the capture vocabulary; classify by kind for fast lookup.
        let (mut func_caps, mut type_caps, mut value_caps, mut name_caps) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (i, name) in query.capture_names().iter().enumerate() {
            if !SUBJECT_CAPTURES.contains(name) {
                return Err(Finding::new(
                    "E0411",
                    span,
                    format!(
                        "bind.scm uses unknown capture \"@{name}\"; the vocabulary is @subject.function, @subject.type, @subject.value, @subject.name (§8.6.3)"
                    ),
                ));
            }
            let i = i as u32;
            match *name {
                "subject.function" => func_caps.push(i),
                "subject.type" => type_caps.push(i),
                "subject.value" => value_caps.push(i),
                "subject.name" => name_caps.push(i),
                _ => unreachable!(),
            }
        }
        Ok(Binder {
            grammar: grammar.clone(),
            query,
            func_caps,
            type_caps,
            value_caps,
            name_caps,
            wrappers: pack.wrappers.clone(),
            sibling_skips: pack.sibling_skips.clone(),
        })
    }

    pub fn bind(
        &self,
        path: &Path,
        source: &str,
        blocks: Vec<RawBlock>,
    ) -> (Vec<BoundBlock>, Vec<Finding>) {
        let mut bound = Vec::new();
        let mut findings = Vec::new();
        if blocks.is_empty() {
            return (bound, findings);
        }

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.grammar)
            .expect("pinned grammar must load");
        let tree = parser
            .parse(source, None)
            .expect("tree-sitter always returns a tree");
        let root = tree.root_node();
        let decls = self.collect_decls(root, source);
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
            let mut row = block.end_line as usize; // 0-based row after the block
            while row < lines.len() && lines[row].trim().is_empty() {
                row += 1;
            }
            if row >= lines.len() {
                findings.push(Finding::new("E0102", block_span,
                    "this @lore block is not followed by a bindable declaration (found end of file); move it directly above a function, class, type, or assignment".into()));
                continue;
            }

            let candidates = nodes_starting_at_row(root, row);
            let Some(first) = candidates.first().copied() else {
                findings.push(Finding::new("E0102", block_span,
                    "this @lore block is not followed by a bindable declaration (found nothing parseable); move it directly above a function, class, type, or assignment".into()));
                continue;
            };
            let Some(node) = candidates
                .iter()
                .map(|&n| self.descend_wrappers(n, &decls))
                .find(|n| decls.contains_key(&n.id()))
                .or_else(|| self.skip_siblings(first, &decls))
            else {
                findings.push(Finding::new("E0102", block_span, format!(
                    "this @lore block is not followed by a bindable declaration (found {}); move it directly above a function, class, type, or assignment", first.kind())));
                continue;
            };

            let identifier = decls[&node.id()].name.clone();
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

    /// Run `bind.scm` and aggregate by subject node: the kind from its
    /// `@subject.*` tag, and the host identifier from its `@subject.name`
    /// captures. Zero distinct name captures (a multi-target catch-all
    /// pattern) or two-or-more (a multi-declarator form) leave the name
    /// `None` -- the binder then demands an explicit `name:` (`E0104`). This
    /// one rule replaces every per-language identifier-field special case
    /// (D-042b) with pack data.
    fn collect_decls<'t>(&self, root: Node<'t>, source: &str) -> HashMap<usize, DeclInfo> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, root, source.as_bytes());
        let mut decls: HashMap<usize, DeclInfo> = HashMap::new();
        while let Some(m) = matches.next() {
            let mut subject: Option<(Node<'t>, SubjectKind)> = None;
            let mut name: Option<Node<'t>> = None;
            for cap in m.captures {
                if self.func_caps.contains(&cap.index) {
                    subject = Some((cap.node, SubjectKind::Function));
                } else if self.type_caps.contains(&cap.index) {
                    subject = Some((cap.node, SubjectKind::Type));
                } else if self.value_caps.contains(&cap.index) {
                    subject = Some((cap.node, SubjectKind::Value));
                } else if self.name_caps.contains(&cap.index) {
                    name = Some(cap.node);
                }
            }
            let Some((node, _kind)) = subject else {
                continue;
            };
            let entry = decls.entry(node.id()).or_insert(DeclInfo {
                name_nodes: Vec::new(),
                name: None,
            });
            if let Some(nm) = name
                && !entry.name_nodes.iter().any(|(id, _)| *id == nm.id())
            {
                entry
                    .name_nodes
                    .push((nm.id(), source[nm.byte_range()].to_string()));
            }
        }
        // Resolve the identifier: exactly one distinct name node => that
        // identifier; zero or many => None (needs an explicit name:).
        for info in decls.values_mut() {
            if let [(_, text)] = info.name_nodes.as_slice() {
                info.name = Some(text.clone());
            }
        }
        decls
    }

    /// §7.4 wrappers (D-042): descend through wrapper nodes to the declaration
    /// they contain. Generic over the pack's `wrappers` list -- the inner
    /// declaration is the first named child that is itself a declaration (in
    /// the match set) or another wrapper.
    fn descend_wrappers<'t>(&self, node: Node<'t>, decls: &HashMap<usize, DeclInfo>) -> Node<'t> {
        let mut n = node;
        loop {
            if !self.wrappers.iter().any(|w| w == n.kind()) {
                return n;
            }
            let mut cursor = n.walk();
            let next = n.named_children(&mut cursor).find(|c| {
                decls.contains_key(&c.id()) || self.wrappers.iter().any(|w| w == c.kind())
            });
            match next {
                Some(inner) => n = inner,
                None => return n,
            }
        }
    }

    /// Sibling skip (D-050c): when the block is followed by a skip node (e.g.
    /// a Rust `attribute_item`), advance along named siblings past consecutive
    /// skips and bind the first declaration that follows. The subject span
    /// excludes the skipped siblings.
    fn skip_siblings<'t>(
        &self,
        node: Node<'t>,
        decls: &HashMap<usize, DeclInfo>,
    ) -> Option<Node<'t>> {
        if !self.sibling_skips.iter().any(|s| s == node.kind()) {
            return None;
        }
        let mut n = node;
        loop {
            n = n.next_named_sibling()?;
            if self.sibling_skips.iter().any(|s| s == n.kind()) {
                continue;
            }
            let d = self.descend_wrappers(n, decls);
            return decls.contains_key(&d.id()).then_some(d);
        }
    }
}

struct DeclInfo {
    name_nodes: Vec<(usize, String)>,
    name: Option<String>,
}

/// Scan-tier binding (D-070b): no grammar, so nothing binds to a subject.
/// Scoping blocks (`module`/`service`/`workflow`) work in full; every other
/// block MUST carry `name:` (`E0109`) and gets qname = §7.5 module + name with
/// no subject span (never staleness-checked, never `Contradicted`).
pub fn bind_scan_tier(path: &Path, blocks: Vec<RawBlock>) -> (Vec<BoundBlock>, Vec<Finding>) {
    let mut bound = Vec::new();
    let mut findings = Vec::new();
    for block in blocks {
        let kind = block.kind.as_ref().map(|k| k.value);
        let span = block_point_span(path, &block);
        let scoping = matches!(kind, Some(Kind::Module | Kind::Service | Kind::Workflow));
        if block.name.is_none() {
            if scoping {
                let k = kind.unwrap().name();
                findings.push(Finding::new("E0108", span, format!(
                    "a scoping block (\"kind: {k}\") binds to no declaration, so it needs an explicit \"name:\" field")));
            } else {
                findings.push(Finding::new("E0109", span,
                    "this language's pack is scan-tier (no grammar), so a non-scoping @lore block cannot find its subject; add an explicit \"name:\" field (§8.6.2)".into()));
            }
            continue;
        }
        bound.push(BoundBlock {
            block,
            subject: None,
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
/// descent path containing the row (the file root never counts). Container
/// nodes (e.g. a Python class-body `block`) can begin on the same row as the
/// declaration they hold, so the list is needed for the D-044 search.
fn nodes_starting_at_row(root: Node<'_>, row: usize) -> Vec<Node<'_>> {
    let mut path = Vec::new();
    let mut current = root;
    loop {
        if current.is_named() && current.start_position().row == row && current.id() != root.id() {
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
