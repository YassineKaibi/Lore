//! Python derivation (§8.5 artifacts): decls.scm, calls.scm, touches.scm,
//! the import rules, and the mutator list. Extraction only — resolution is
//! cross-file and lives in resolve.rs.

use std::collections::HashMap;
use std::sync::OnceLock;

use tree_sitter::{Node, Query};

use crate::facts::{CallFact, CalleeFact, DeclKind, FileFacts, ImportFact, TouchFact};
use crate::lang::{
    self, Language, candidate_maps, enclosing_function, has_ancestor, is_field_of, run_query,
    span_fact,
};
use crate::{SourceUnit, StateSymbol};

/// §8.3 mutator set for Python list/dict/set state.
const MUTATORS: [&str; 12] = [
    "append",
    "extend",
    "insert",
    "remove",
    "pop",
    "clear",
    "update",
    "setdefault",
    "sort",
    "reverse",
    "add",
    "discard",
];

const LAMBDAS: [&str; 1] = ["lambda"];
const IMPORT_STATEMENTS: [&str; 2] = ["import_statement", "import_from_statement"];

fn query(cell: &'static OnceLock<Query>, source: &str) -> &'static Query {
    cell.get_or_init(|| {
        Query::new(&Language::Python.grammar(), source).expect("query matches pinned grammar")
    })
}

fn decls_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    query(&Q, include_str!("decls.scm"))
}

fn calls_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    query(&Q, include_str!("calls.scm"))
}

fn touches_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    query(&Q, include_str!("touches.scm"))
}

fn imports_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    query(&Q, "(import_statement) @i (import_from_statement) @i")
}

pub(crate) fn extract(file: &SourceUnit, states: &[StateSymbol]) -> FileFacts {
    let tree = lang::parse(Language::Python, &file.text);
    let root = tree.root_node();
    let src = file.text.as_str();
    let text = |n: Node<'_>| &src[n.byte_range()];

    let (decls, decl_index) = lang::collect_decls(decls_query(), root, src, |kind| match kind {
        "class_definition" => DeclKind::Type,
        _ => DeclKind::Function,
    });

    let imports = collect_imports(root, src);

    // Same-file class table for the D-062e local-construction rule.
    let mut classes: HashMap<&str, usize> = HashMap::new();
    for (i, d) in decls.iter().enumerate() {
        if d.kind == DeclKind::Type {
            classes.entry(d.name.as_str()).or_insert(i);
        }
    }

    // calls.scm pattern 1: `x = Cls()` locals, keyed by enclosing function.
    let q = calls_query();
    let call_cap = q.capture_index_for_name("call");
    let var_cap = q.capture_index_for_name("var");
    let cls_cap = q.capture_index_for_name("cls");
    let matches = run_query(q, root, src.as_bytes());
    let mut locals: HashMap<(usize, &str), usize> = HashMap::new();
    for m in &matches {
        let (Some(var), Some(cls)) = (m.cap(var_cap), m.cap(cls_cap)) else {
            continue;
        };
        let Some(enclosing) = enclosing_function(var, &decl_index, &decls, &LAMBDAS) else {
            continue; // module-level constructions feed module-level calls, which drop anyway
        };
        if let Some(&class_decl) = classes.get(text(cls)) {
            locals.insert((enclosing, text(var)), class_decl);
        }
    }

    let mut calls = Vec::new();
    for m in &matches {
        let Some(call) = m.cap(call_cap) else {
            continue; // the @construct pattern
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

    let touches = collect_touches(root, file, states, &decls, &decl_index, &imports, src);

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
        "attribute" => {
            let (Some(obj), Some(attr)) = (
                f.child_by_field_name("object"),
                f.child_by_field_name("attribute"),
            ) else {
                return CalleeFact::Opaque;
            };
            if obj.kind() != "identifier" {
                return CalleeFact::Opaque; // dotted deeper than alias.name (D-062c)
            }
            if let Some(class_decl) = enclosing.and_then(|e| locals.get(&(e, text(obj)))) {
                return CalleeFact::Method {
                    class_decl: *class_decl,
                    name: text(attr).to_string(),
                };
            }
            CalleeFact::Attr {
                obj: text(obj).to_string(),
                name: text(attr).to_string(),
            }
        }
        _ => CalleeFact::Opaque,
    }
}

/// Import rules, v1 (§8.2, D-062c): `import m [as a]` (plain form only when
/// single-segment — `import a.b` binds `a`, unusable for `alias.name(...)`)
/// and `from m import n [as a]`. Relative imports and wildcards drop.
fn collect_imports(root: Node<'_>, src: &str) -> Vec<ImportFact> {
    let text = |n: Node<'_>| src[n.byte_range()].to_string();
    let mut out = Vec::new();
    for m in run_query(imports_query(), root, src.as_bytes()) {
        let Some(stmt) = m.cap(Some(0)) else { continue };
        let mut cursor = stmt.walk();
        match stmt.kind() {
            "import_statement" => {
                for name in stmt.children_by_field_name("name", &mut cursor) {
                    match name.kind() {
                        "dotted_name" if !text(name).contains('.') => {
                            out.push(ImportFact::Whole {
                                module: text(name),
                                alias: text(name),
                            });
                        }
                        "aliased_import" => {
                            let (Some(module), Some(alias)) = (
                                name.child_by_field_name("name"),
                                name.child_by_field_name("alias"),
                            ) else {
                                continue;
                            };
                            out.push(ImportFact::Whole {
                                module: text(module),
                                alias: text(alias),
                            });
                        }
                        _ => {}
                    }
                }
            }
            "import_from_statement" => {
                let Some(module) = stmt.child_by_field_name("module_name") else {
                    continue;
                };
                if module.kind() != "dotted_name" {
                    continue; // relative imports drop (D-062c)
                }
                for name in stmt.children_by_field_name("name", &mut cursor) {
                    match name.kind() {
                        "dotted_name" => out.push(ImportFact::Named {
                            module: text(module),
                            name: text(name),
                            alias: text(name),
                        }),
                        "aliased_import" => {
                            let (Some(n), Some(alias)) = (
                                name.child_by_field_name("name"),
                                name.child_by_field_name("alias"),
                            ) else {
                                continue;
                            };
                            out.push(ImportFact::Named {
                                module: text(module),
                                name: text(n),
                                alias: text(alias),
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_touches(
    root: Node<'_>,
    file: &SourceUnit,
    states: &[StateSymbol],
    decls: &[crate::facts::DeclFact],
    decl_index: &HashMap<usize, usize>,
    imports: &[ImportFact],
    src: &str,
) -> Vec<TouchFact> {
    let (bare, attr) = candidate_maps(file, states, imports, Language::Python);
    if bare.is_empty() && attr.is_empty() {
        return Vec::new();
    }

    let q = touches_query();
    let id_cap = q.capture_index_for_name("id");
    let access_cap = q.capture_index_for_name("access");
    let obj_cap = q.capture_index_for_name("obj");
    let attr_cap = q.capture_index_for_name("attr");
    let text = |n: Node<'_>| &src[n.byte_range()];

    let mut out = Vec::new();
    for m in run_query(q, root, src.as_bytes()) {
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
/// declared, the member of some other value, or a keyword-argument name.
fn valid_bare_site(id: Node<'_>) -> bool {
    let Some(parent) = id.parent() else {
        return true;
    };
    match parent.kind() {
        "function_definition" | "class_definition" | "keyword_argument" => {
            !is_field_of(id, parent, "name")
        }
        "attribute" => !is_field_of(id, parent, "attribute"),
        _ => true,
    }
}

/// §8.3 mutator pattern set: assignment / augmented assignment to the
/// symbol, or a mutator-list method call on it. Everything else is Read.
fn is_write(site: Node<'_>, src: &str) -> bool {
    let Some(parent) = site.parent() else {
        return false;
    };
    match parent.kind() {
        "assignment" | "augmented_assignment" => is_field_of(site, parent, "left"),
        "attribute" => {
            if !is_field_of(site, parent, "object") {
                return false;
            }
            let method = parent
                .child_by_field_name("attribute")
                .map(|a| &src[a.byte_range()]);
            let called = parent
                .parent()
                .is_some_and(|gp| gp.kind() == "call" && is_field_of(parent, gp, "function"));
            called && method.is_some_and(|m| MUTATORS.contains(&m))
        }
        _ => false,
    }
}
