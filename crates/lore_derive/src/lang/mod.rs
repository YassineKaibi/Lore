//! Per-language derivation (§8.5): each language directory holds the five
//! artifacts — grammar dependency, declaration queries, call-expression
//! queries, import-resolution rules, mutator list — plus a touches query for
//! the §8.3 occurrence scan. This module holds the shared tree-sitter
//! plumbing.

pub(crate) mod python;
pub(crate) mod typescript;

use std::collections::HashMap;
use std::path::Path;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, Tree};

use crate::facts::{DeclFact, DeclKind, SpanFact};

/// Derived-layer languages at T6 (D-014). Go, Java, Rust arrive at T8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    Python,
    TypeScript,
    Tsx,
}

impl Language {
    pub(crate) fn from_path(p: &Path) -> Option<Language> {
        match p.extension()?.to_str()? {
            "py" => Some(Language::Python),
            "tsx" | "jsx" => Some(Language::Tsx),
            "ts" | "js" | "mjs" | "cjs" => Some(Language::TypeScript),
            _ => None,
        }
    }

    /// Cache-key component (D-064) — grammar variants hash differently.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
        }
    }

    pub(crate) fn grammar(self) -> tree_sitter::Language {
        match self {
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    /// Two states with the same identifier never collide across languages:
    /// the bare own-module occurrence form requires the same language family
    /// as the defining file (D-062d).
    pub(crate) fn same_family(self, other: Language) -> bool {
        match self {
            Language::Python => other == Language::Python,
            Language::TypeScript | Language::Tsx => {
                matches!(other, Language::TypeScript | Language::Tsx)
            }
        }
    }
}

pub(crate) fn parse(language: Language, text: &str) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language.grammar())
        .expect("pinned grammar must load");
    parser
        .parse(text, None)
        .expect("tree-sitter always returns a tree")
}

/// One query match, with captures resolved to nodes by capture index —
/// callers tell patterns apart by which captures are present.
pub(crate) struct Match<'t> {
    caps: Vec<(u32, Node<'t>)>,
}

impl<'t> Match<'t> {
    pub(crate) fn cap(&self, index: Option<u32>) -> Option<Node<'t>> {
        let index = index?;
        self.caps.iter().find(|(i, _)| *i == index).map(|(_, n)| *n)
    }
}

/// Collected matches: tree-sitter 0.25 yields them through a streaming
/// iterator, which cannot outlive the cursor — collect once, borrow after.
pub(crate) fn run_query<'t>(query: &Query, root: Node<'t>, src: &[u8]) -> Vec<Match<'t>> {
    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();
    let mut matches = cursor.matches(query, root, src);
    while let Some(m) = matches.next() {
        out.push(Match {
            caps: m.captures.iter().map(|c| (c.index, c.node)).collect(),
        });
    }
    out
}

pub(crate) fn span_fact(n: Node<'_>) -> SpanFact {
    SpanFact {
        line: n.start_position().row as u32 + 1,
        col: n.start_position().column as u32 + 1,
        end_line: n.end_position().row as u32 + 1,
        end_col: n.end_position().column as u32 + 1,
    }
}

/// Build the DeclFact table from a declarations query whose every pattern
/// captures `@decl` and `@name`. Order is source order; parents are the
/// nearest enclosing declaration (a method's class, a nested function's
/// parent function). Returns the facts plus node-id → index for ancestry
/// walks.
pub(crate) fn collect_decls(
    query: &Query,
    root: Node<'_>,
    src: &str,
    kind_of: impl Fn(&str) -> DeclKind,
) -> (Vec<DeclFact>, HashMap<usize, usize>) {
    let decl_cap = query.capture_index_for_name("decl");
    let name_cap = query.capture_index_for_name("name");
    let mut nodes = Vec::new();
    for m in run_query(query, root, src.as_bytes()) {
        let (Some(decl), Some(name)) = (m.cap(decl_cap), m.cap(name_cap)) else {
            continue;
        };
        nodes.push((decl, src[name.byte_range()].to_string()));
    }
    // Queries yield matches in source order, but make it explicit: parents
    // and the resolver both assume it.
    nodes.sort_by_key(|(d, _)| (d.start_byte(), d.end_byte()));
    let index: HashMap<usize, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, (d, _))| (d.id(), i))
        .collect();
    let decls = nodes
        .iter()
        .map(|(decl, name)| DeclFact {
            name: name.clone(),
            kind: kind_of(decl.kind()),
            span: span_fact(*decl),
            parent: nearest_decl(*decl, &index),
        })
        .collect();
    (decls, index)
}

fn nearest_decl(node: Node<'_>, index: &HashMap<usize, usize>) -> Option<usize> {
    let mut n = node;
    while let Some(p) = n.parent() {
        if let Some(&i) = index.get(&p.id()) {
            return Some(i);
        }
        n = p;
    }
    None
}

/// D-062a attribution: the nearest enclosing declaration if it is a derived
/// Function; None at module/class level or inside a value-bound
/// lambda/arrow, where an attributed edge would be a guess (G-7).
pub(crate) fn enclosing_function(
    node: Node<'_>,
    index: &HashMap<usize, usize>,
    decls: &[DeclFact],
    lambda_kinds: &[&str],
) -> Option<usize> {
    let mut n = node;
    while let Some(p) = n.parent() {
        if lambda_kinds.contains(&p.kind()) {
            return None;
        }
        if let Some(&i) = index.get(&p.id()) {
            return (decls[i].kind == DeclKind::Function).then_some(i);
        }
        n = p;
    }
    None
}

pub(crate) fn has_ancestor(node: Node<'_>, kinds: &[&str]) -> bool {
    let mut n = node;
    while let Some(p) = n.parent() {
        if kinds.contains(&p.kind()) {
            return true;
        }
        n = p;
    }
    false
}

/// Is `node` exactly the value of `field` on its parent?
pub(crate) fn is_field_of(node: Node<'_>, parent: Node<'_>, field: &str) -> bool {
    parent
        .child_by_field_name(field)
        .is_some_and(|f| f.id() == node.id())
}

/// State-symbol visibility for one file (D-062d): which local names and
/// `alias.attr` pairs could denote which states, and through which import.
/// A later import binding a name shadows everything before it (own-module
/// visibility included), like the host languages themselves; several states
/// sharing an identifier stay parallel candidates — the resolver keeps only
/// those whose import resolves to the state's defining file.
pub(crate) type BareCandidates = HashMap<String, Vec<(usize, Option<usize>)>>;
pub(crate) type AttrCandidates = HashMap<(String, String), Vec<(usize, usize)>>;

pub(crate) fn candidate_maps(
    file: &crate::SourceUnit,
    states: &[crate::StateSymbol],
    imports: &[crate::facts::ImportFact],
    family: Language,
) -> (BareCandidates, AttrCandidates) {
    use crate::facts::ImportFact;
    let mut bare: BareCandidates = HashMap::new();
    let mut attr: AttrCandidates = HashMap::new();
    for (si, s) in states.iter().enumerate() {
        let same_family = Language::from_path(&s.file).is_some_and(|l| l.same_family(family));
        if same_family && s.module == file.module {
            shadow_push(bare.entry(s.identifier.clone()).or_default(), si, None);
        }
    }
    for (ii, imp) in imports.iter().enumerate() {
        for (si, s) in states.iter().enumerate() {
            match imp {
                ImportFact::Named { name, alias, .. } if *name == s.identifier => {
                    shadow_push(bare.entry(alias.clone()).or_default(), si, Some(ii));
                }
                ImportFact::Whole { alias, .. } => {
                    let entry = attr
                        .entry((alias.clone(), s.identifier.clone()))
                        .or_default();
                    if entry.last().map(|(_, l)| *l) != Some(ii) {
                        entry.clear();
                    }
                    entry.push((si, ii));
                }
                _ => {}
            }
        }
    }
    (bare, attr)
}

fn shadow_push(entry: &mut Vec<(usize, Option<usize>)>, si: usize, via: Option<usize>) {
    if entry.last().map(|(_, l)| *l) != Some(via) {
        entry.clear();
    }
    entry.push((si, via));
}
