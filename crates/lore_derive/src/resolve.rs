//! The cross-file pass: qnames and D-060d ambiguity, §8.2 call resolution
//! with the drop rule, §8.3 touch validation and dedupe. Recomputed from
//! facts on every run — only per-file extraction is cached (D-064).

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use lore_intent::{Intent, IntentNode, Kind, Origin, QName, Span};

use crate::facts::{CalleeFact, DeclKind, FileFacts, ImportFact, SpanFact};
use crate::lang::Language;
use crate::{
    DeriveResult, DerivedConfidence, DerivedEdge, DerivedEdgeKind, SourceUnit, StateSymbol,
};

impl DerivedEdgeKind {
    /// §6.1 order, for deterministic output sorting.
    fn order(self) -> usize {
        match self {
            DerivedEdgeKind::Affects => 0,
            DerivedEdgeKind::Reads => 1,
            DerivedEdgeKind::Calls => 2,
        }
    }
}

pub(crate) fn resolve(
    extracted: &[(&SourceUnit, Language, FileFacts)],
    states: &[StateSymbol],
    roots: &[String],
) -> DeriveResult {
    // qname per declaration, and the D-060d ambiguity set.
    let qnames: Vec<Vec<QName>> = extracted
        .iter()
        .map(|(file, _, facts)| {
            facts
                .decls
                .iter()
                .map(|d| qualified(&file.module, &d.name))
                .collect()
        })
        .collect();
    let mut counts: HashMap<&QName, usize> = HashMap::new();
    for q in qnames.iter().flatten() {
        *counts.entry(q).or_default() += 1;
    }
    let ambiguous: HashSet<&QName> = counts
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(q, _)| *q)
        .collect();
    let ambiguous_names: usize = counts.values().filter(|n| **n > 1).sum();

    // Lookup tables: file path -> index, per-file name -> unambiguous decl,
    // per-file (class decl, method name) -> decl.
    let file_index: HashMap<&Path, usize> = extracted
        .iter()
        .enumerate()
        .map(|(i, (f, _, _))| (f.path.as_path(), i))
        .collect();
    let by_name: Vec<HashMap<&str, usize>> = extracted
        .iter()
        .enumerate()
        .map(|(i, (_, _, facts))| {
            facts
                .decls
                .iter()
                .enumerate()
                .filter(|(j, _)| !ambiguous.contains(&qnames[i][*j]))
                .map(|(j, d)| (d.name.as_str(), j))
                .collect()
        })
        .collect();
    let methods: Vec<HashMap<(usize, &str), usize>> = extracted
        .iter()
        .map(|(_, _, facts)| {
            facts
                .decls
                .iter()
                .enumerate()
                .filter_map(|(j, d)| d.parent.map(|p| ((p, d.name.as_str()), j)))
                .collect()
        })
        .collect();

    let resolve_import = |i: usize, import: &ImportFact| -> Option<usize> {
        let (file, language, _) = &extracted[i];
        match language {
            Language::Python => python_module_file(import.module(), roots, &file_index),
            Language::TypeScript | Language::Tsx => {
                ts_relative_file(import.module(), &file.path, &file_index)
            }
        }
    };

    // §8.1 nodes: every unambiguous declaration, origin Derived.
    let mut nodes = Vec::new();
    for (i, (file, _, facts)) in extracted.iter().enumerate() {
        for (j, d) in facts.decls.iter().enumerate() {
            if ambiguous.contains(&qnames[i][j]) {
                continue;
            }
            nodes.push(IntentNode {
                qname: qnames[i][j].clone(),
                kind: match d.kind {
                    DeclKind::Function => Kind::Function,
                    DeclKind::Type => Kind::Type,
                },
                origin: Origin::Derived,
                intent: Intent::default(),
                loc: to_span(&file.path, d.span),
            });
        }
    }

    // §8.2 Calls: Exact same-file, Resolved through imports, dropped and
    // counted otherwise (G-7: never guess).
    let mut edges = Vec::new();
    let mut unresolved_calls = 0usize;
    for (i, (file, _, facts)) in extracted.iter().enumerate() {
        for call in &facts.calls {
            let from = call
                .enclosing
                .filter(|e| !ambiguous.contains(&qnames[i][*e]));
            let Some(from) = from else {
                unresolved_calls += 1; // no honest attribution (D-062a)
                continue;
            };
            let target = match &call.callee {
                CalleeFact::Bare(name) => by_name[i]
                    .get(name.as_str())
                    .map(|&j| (i, j, DerivedConfidence::Exact))
                    .or_else(|| {
                        named_import(&facts.imports, name).and_then(|(ii, orig)| {
                            let k = resolve_import(i, &facts.imports[ii])?;
                            by_name[k]
                                .get(orig)
                                .map(|&j| (k, j, DerivedConfidence::Resolved))
                        })
                    }),
                CalleeFact::Attr { obj, name } => {
                    whole_import(&facts.imports, obj).and_then(|ii| {
                        let k = resolve_import(i, &facts.imports[ii])?;
                        by_name[k]
                            .get(name.as_str())
                            .map(|&j| (k, j, DerivedConfidence::Resolved))
                    })
                }
                CalleeFact::Method { class_decl, name } => methods[i]
                    .get(&(*class_decl, name.as_str()))
                    .filter(|&&j| !ambiguous.contains(&qnames[i][j]))
                    .map(|&j| (i, j, DerivedConfidence::Exact)),
                CalleeFact::Opaque => None,
            };
            match target {
                Some((k, j, confidence)) if extracted[k].2.decls[j].kind == DeclKind::Function => {
                    edges.push(DerivedEdge {
                        from: qnames[i][from].clone(),
                        to: qnames[k][j].clone(),
                        kind: DerivedEdgeKind::Calls,
                        confidence,
                        loc: to_span(&file.path, call.span),
                    });
                }
                // resolved to a Type (constructor) or nothing: drop, count
                _ => unresolved_calls += 1,
            }
        }
    }

    // §8.3 touches: validate the import path, dedupe per (function, state,
    // kind) keeping the first occurrence (D-062d). Always Heuristic.
    let mut seen: HashSet<(QName, usize, bool)> = HashSet::new();
    for (i, (file, _, facts)) in extracted.iter().enumerate() {
        for t in &facts.touches {
            let Some(e) = t.enclosing else { continue };
            if ambiguous.contains(&qnames[i][e]) {
                continue;
            }
            if let Some(ii) = t.via_import {
                let resolved = resolve_import(i, &facts.imports[ii])
                    .is_some_and(|k| extracted[k].0.path == states[t.state].file);
                if !resolved {
                    continue;
                }
            }
            let from = qnames[i][e].clone();
            if !seen.insert((from.clone(), t.state, t.write)) {
                continue;
            }
            edges.push(DerivedEdge {
                from,
                to: states[t.state].qname.clone(),
                kind: if t.write {
                    DerivedEdgeKind::Affects
                } else {
                    DerivedEdgeKind::Reads
                },
                confidence: DerivedConfidence::Heuristic,
                loc: to_span(&file.path, t.span),
            });
        }
    }

    nodes.sort_by(|a, b| {
        (&a.loc.file, a.loc.line, &a.qname).cmp(&(&b.loc.file, b.loc.line, &b.qname))
    });
    edges.sort_by(|a, b| {
        (
            &a.loc.file,
            a.loc.line,
            a.loc.col,
            a.kind.order(),
            &a.from,
            &a.to,
        )
            .cmp(&(
                &b.loc.file,
                b.loc.line,
                b.loc.col,
                b.kind.order(),
                &b.from,
                &b.to,
            ))
    });
    let mut scope: Vec<PathBuf> = extracted.iter().map(|(f, _, _)| f.path.clone()).collect();
    scope.sort();

    DeriveResult {
        nodes,
        edges,
        unresolved_calls,
        ambiguous_names,
        scope,
    }
}

/// Module + host identifier, flat (D-060b) — the binder's §7.5 rule, so the
/// two layers collide (and merge) by construction.
fn qualified(module: &str, name: &str) -> QName {
    let mut segments: Vec<String> = module.split('.').map(str::to_owned).collect();
    segments.push(name.to_string());
    QName(segments)
}

/// The last named import binding `name` wins, like the host languages.
fn named_import<'f>(imports: &'f [ImportFact], name: &str) -> Option<(usize, &'f str)> {
    imports
        .iter()
        .enumerate()
        .rev()
        .find_map(|(ii, imp)| match imp {
            ImportFact::Named {
                name: orig, alias, ..
            } if alias == name => Some((ii, orig.as_str())),
            _ => None,
        })
}

fn whole_import(imports: &[ImportFact], alias: &str) -> Option<usize> {
    imports
        .iter()
        .enumerate()
        .rev()
        .find_map(|(ii, imp)| match imp {
            ImportFact::Whole { alias: a, .. } if a == alias => Some(ii),
            _ => None,
        })
}

/// Python `a.b` -> `<root>/a/b.py` | `<root>/a/b/__init__.py` against the
/// `[project] roots`, first match wins (§8.2).
fn python_module_file(
    module: &str,
    roots: &[String],
    files: &HashMap<&Path, usize>,
) -> Option<usize> {
    for root in roots {
        let mut base = PathBuf::from(root);
        for seg in module.split('.') {
            base.push(seg);
        }
        for candidate in [base.with_extension("py"), base.join("__init__.py")] {
            if let Some(&i) = files.get(normalize(&candidate).as_path()) {
                return Some(i);
            }
        }
    }
    None
}

/// TS relative specifiers only (§8.2): `./`/`../` against the importing
/// file's directory, trying `<p>`, `<p>.ts`, `<p>.tsx`, `<p>.js`,
/// `<p>/index.ts` (D-062c).
fn ts_relative_file(spec: &str, importer: &Path, files: &HashMap<&Path, usize>) -> Option<usize> {
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let base = normalize(&importer.parent().unwrap_or(Path::new("")).join(spec));
    let raw = base.as_os_str().to_string_lossy();
    let candidates = [
        base.clone(),
        PathBuf::from(format!("{raw}.ts")),
        PathBuf::from(format!("{raw}.tsx")),
        PathBuf::from(format!("{raw}.js")),
        base.join("index.ts"),
    ];
    candidates
        .iter()
        .find_map(|c| files.get(c.as_path()).copied())
}

/// Lexical normalization: drop `.`, resolve `..` against earlier segments.
/// Project paths are root-relative, so this cannot escape into surprises.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn to_span(file: &Path, s: SpanFact) -> Span {
    Span {
        file: file.to_path_buf(),
        line: s.line,
        col: s.col,
        end_line: s.end_line,
        end_col: s.end_col,
    }
}
