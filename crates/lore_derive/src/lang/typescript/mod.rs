//! TypeScript/TSX derivation (§8.5 artifacts): decls.scm, calls.scm,
//! touches.scm, the relative-import rules, and the mutator list. The TS and
//! TSX grammars are distinct tree-sitter languages, so queries compile once
//! per variant.

use std::collections::HashMap;
use std::sync::OnceLock;

use tree_sitter::{Node, Query};

use crate::facts::{CallFact, CalleeFact, DeclKind, FileFacts, ImportFact, TouchFact};
use crate::lang::{
    self, Language, candidate_maps, enclosing_function, has_ancestor, is_field_of, run_query,
    span_fact,
};
use crate::{SourceUnit, StateSymbol};

/// §8.3 mutator set for TS array/map/set state.
const MUTATORS: [&str; 12] = [
    "push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "set", "delete",
    "clear", "add",
];

/// Value-bound function forms are not derived nodes; calls and touches
/// inside them have no honest attribution and drop (D-062a).
const LAMBDAS: [&str; 3] = [
    "arrow_function",
    "function_expression",
    "generator_function",
];
const IMPORT_STATEMENTS: [&str; 1] = ["import_statement"];

struct Queries {
    decls: Query,
    calls: Query,
    touches: Query,
    imports: Query,
}

fn queries(language: Language) -> &'static Queries {
    static TS: OnceLock<Queries> = OnceLock::new();
    static TSX: OnceLock<Queries> = OnceLock::new();
    let cell = match language {
        Language::Tsx => &TSX,
        _ => &TS,
    };
    cell.get_or_init(|| {
        let grammar = language.grammar();
        let q = |src: &str| Query::new(&grammar, src).expect("query matches pinned grammar");
        Queries {
            decls: q(include_str!("decls.scm")),
            calls: q(include_str!("calls.scm")),
            touches: q(include_str!("touches.scm")),
            imports: q("(import_statement) @i"),
        }
    })
}

pub(crate) fn extract(language: Language, file: &SourceUnit, states: &[StateSymbol]) -> FileFacts {
    let qs = queries(language);
    let tree = lang::parse(language, &file.text);
    let root = tree.root_node();
    let src = file.text.as_str();
    let text = |n: Node<'_>| &src[n.byte_range()];

    let (decls, decl_index) = lang::collect_decls(&qs.decls, root, src, |kind| match kind {
        "function_declaration" | "method_definition" => DeclKind::Function,
        _ => DeclKind::Type,
    });

    let imports = collect_imports(&qs.imports, root, src);

    let mut classes: HashMap<&str, usize> = HashMap::new();
    for (i, d) in decls.iter().enumerate() {
        if d.kind == DeclKind::Type {
            classes.entry(d.name.as_str()).or_insert(i);
        }
    }

    // calls.scm pattern 1: `const x = new Cls()` locals per function.
    let call_cap = qs.calls.capture_index_for_name("call");
    let var_cap = qs.calls.capture_index_for_name("var");
    let cls_cap = qs.calls.capture_index_for_name("cls");
    let matches = run_query(&qs.calls, root, src.as_bytes());
    let mut locals: HashMap<(usize, &str), usize> = HashMap::new();
    for m in &matches {
        let (Some(var), Some(cls)) = (m.cap(var_cap), m.cap(cls_cap)) else {
            continue;
        };
        let Some(enclosing) = enclosing_function(var, &decl_index, &decls, &LAMBDAS) else {
            continue;
        };
        if let Some(&class_decl) = classes.get(text(cls)) {
            locals.insert((enclosing, text(var)), class_decl);
        }
    }

    let mut calls = Vec::new();
    for m in &matches {
        let Some(call) = m.cap(call_cap) else {
            continue;
        };
        let enclosing = enclosing_function(call, &decl_index, &decls, &LAMBDAS);
        let callee = call
            .child_by_field_name("function")
            .map(|f| classify_callee(f, enclosing, &locals, text))
            .unwrap_or(CalleeFact::Opaque);
        calls.push(CallFact {
            callee,
            enclosing,
            span: span_fact(call),
        });
    }

    let touches = collect_touches(qs, root, file, states, &decls, &decl_index, &imports, src);

    FileFacts {
        decls,
        calls,
        imports,
        touches,
    }
}

fn classify_callee<'t>(
    f: Node<'t>,
    enclosing: Option<usize>,
    locals: &HashMap<(usize, &str), usize>,
    text: impl Fn(Node<'t>) -> &'t str,
) -> CalleeFact {
    match f.kind() {
        "identifier" => CalleeFact::Bare(text(f).to_string()),
        "member_expression" => {
            let (Some(obj), Some(prop)) = (
                f.child_by_field_name("object"),
                f.child_by_field_name("property"),
            ) else {
                return CalleeFact::Opaque;
            };
            if obj.kind() != "identifier" {
                return CalleeFact::Opaque; // `this.m()`, chains: drop (D-062c/e)
            }
            if let Some(class_decl) = enclosing.and_then(|e| locals.get(&(e, text(obj)))) {
                return CalleeFact::Method {
                    class_decl: *class_decl,
                    name: text(prop).to_string(),
                };
            }
            CalleeFact::Attr {
                obj: text(obj).to_string(),
                name: text(prop).to_string(),
            }
        }
        _ => CalleeFact::Opaque,
    }
}

/// Import rules, v1 (§8.2, D-062c): named imports `{ n [as a] }` and
/// namespace imports `* as m`, relative specifiers only (the resolver
/// enforces `./`/`../`). Default imports drop.
fn collect_imports(query: &Query, root: Node<'_>, src: &str) -> Vec<ImportFact> {
    let text = |n: Node<'_>| src[n.byte_range()].to_string();
    let mut out = Vec::new();
    for m in run_query(query, root, src.as_bytes()) {
        let Some(stmt) = m.cap(Some(0)) else { continue };
        let Some(module) = stmt
            .child_by_field_name("source")
            .and_then(|s| s.named_child(0))
            .map(text)
        else {
            continue;
        };
        let mut cursor = stmt.walk();
        let Some(clause) = stmt
            .named_children(&mut cursor)
            .find(|c| c.kind() == "import_clause")
        else {
            continue; // side-effect import: nothing bound
        };
        let mut clause_cursor = clause.walk();
        for item in clause.named_children(&mut clause_cursor) {
            match item.kind() {
                "namespace_import" => {
                    let mut c = item.walk();
                    if let Some(alias) = item
                        .named_children(&mut c)
                        .find(|n| n.kind() == "identifier")
                    {
                        out.push(ImportFact::Whole {
                            module: module.clone(),
                            alias: text(alias),
                        });
                    }
                }
                "named_imports" => {
                    let mut c = item.walk();
                    for spec in item
                        .named_children(&mut c)
                        .filter(|n| n.kind() == "import_specifier")
                    {
                        let Some(name) = spec.child_by_field_name("name") else {
                            continue;
                        };
                        let alias = spec.child_by_field_name("alias").unwrap_or(name);
                        out.push(ImportFact::Named {
                            module: module.clone(),
                            name: text(name),
                            alias: text(alias),
                        });
                    }
                }
                _ => {} // default import: drops (D-062c)
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)] // extraction state, not a public surface
fn collect_touches(
    qs: &Queries,
    root: Node<'_>,
    file: &SourceUnit,
    states: &[StateSymbol],
    decls: &[crate::facts::DeclFact],
    decl_index: &HashMap<usize, usize>,
    imports: &[ImportFact],
    src: &str,
) -> Vec<TouchFact> {
    let (bare, attr) = candidate_maps(file, states, imports, Language::TypeScript);
    if bare.is_empty() && attr.is_empty() {
        return Vec::new();
    }

    let id_cap = qs.touches.capture_index_for_name("id");
    let access_cap = qs.touches.capture_index_for_name("access");
    let obj_cap = qs.touches.capture_index_for_name("obj");
    let attr_cap = qs.touches.capture_index_for_name("attr");
    let text = |n: Node<'_>| &src[n.byte_range()];

    let mut out = Vec::new();
    for m in run_query(&qs.touches, root, src.as_bytes()) {
        let (site, candidates): (Node<'_>, Vec<(usize, Option<usize>)>) =
            if let Some(id) = m.cap(id_cap) {
                let Some(c) = bare.get(text(id)) else {
                    continue;
                };
                if !valid_bare_site(id) {
                    continue;
                }
                (id, c.clone())
            } else if let Some(access) = m.cap(access_cap) {
                let (Some(obj), Some(at)) = (m.cap(obj_cap), m.cap(attr_cap)) else {
                    continue;
                };
                let key = (text(obj).to_string(), text(at).to_string());
                let Some(c) = attr.get(&key) else { continue };
                (access, c.iter().map(|&(si, ii)| (si, Some(ii))).collect())
            } else {
                continue;
            };
        if has_ancestor(site, &IMPORT_STATEMENTS) {
            continue;
        }
        let write = is_write(site, src);
        let enclosing = enclosing_function(site, decl_index, decls, &LAMBDAS);
        for (state, via_import) in candidates {
            out.push(TouchFact {
                state,
                write,
                enclosing,
                via_import,
                span: span_fact(site),
            });
        }
    }
    out
}

/// A bare identifier counts as an occurrence unless it is the name being
/// declared (function/class/variable declarator) or a member of some other
/// value (the property side never matches `identifier`, but the object side
/// of someone else's member chain can).
fn valid_bare_site(id: Node<'_>) -> bool {
    let Some(parent) = id.parent() else {
        return true;
    };
    match parent.kind() {
        "function_declaration"
        | "class_declaration"
        | "method_definition"
        | "enum_declaration"
        | "variable_declarator" => !is_field_of(id, parent, "name"),
        _ => true,
    }
}

/// §8.3 mutator pattern set for TS: assignment / augmented assignment to
/// the symbol, or a mutator-list method call on it. Everything else reads.
fn is_write(site: Node<'_>, src: &str) -> bool {
    let Some(parent) = site.parent() else {
        return false;
    };
    match parent.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            is_field_of(site, parent, "left")
        }
        "member_expression" => {
            if !is_field_of(site, parent, "object") {
                return false;
            }
            let method = parent
                .child_by_field_name("property")
                .map(|a| &src[a.byte_range()]);
            let called = parent.parent().is_some_and(|gp| {
                gp.kind() == "call_expression" && is_field_of(parent, gp, "function")
            });
            called && method.is_some_and(|m| MUTATORS.contains(&m))
        }
        _ => false,
    }
}
