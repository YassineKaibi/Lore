//! The generic derivation extractor (§8.6.3, D-070/D-072/D-073). One adapter
//! for every language: a pack supplies `bind.scm` (declarations), `derive.scm`
//! (the fixed call/import/touch capture vocabulary), `value_functions`, and
//! the mutator list; the *mechanics* here — D-062a attribution, the
//! bare-occurrence validity filter, and the write/read decision — are engine
//! behavior identical for every pack (D-070g). Per-file and cacheable (D-064);
//! everything cross-file lives in resolve.rs.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lore_intent::{ImportStrategy, WholeAlias};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor, Tree};

use crate::facts::{
    CallFact, CalleeFact, DeclFact, DeclKind, FileFacts, ImportFact, ModFact, SpanFact, TouchFact,
};
use crate::{DerivePack, SourceUnit, StateSymbol};

/// A pack compiled for derivation: queries compiled once against the grammar
/// (D-070d), capture indices resolved, and the import strategies / mutators /
/// value-function node kinds carried as data.
pub(crate) struct CompiledPack {
    /// Cache identity: pack name + content hash over both query sources (the
    /// pack-identity component of the D-064 key, per D-070i).
    pub id: String,
    pub strategies: Vec<ImportStrategy>,
    grammar: Language,
    extensions: Vec<String>,
    bind: Query,
    derive: Query,
    value_functions: Vec<String>,
    mutators: HashSet<String>,
    /// How to derive a whole-import's implicit alias (D-076) and the separator
    /// to split the path on when it is `LastSegment`.
    whole_alias: WholeAlias,
    import_sep: String,
    b: BindCaps,
    d: DeriveCaps,
}

struct BindCaps {
    func: Option<u32>,
    typ: Option<u32>,
    name: Option<u32>,
}

struct DeriveCaps {
    call: Option<u32>,
    callee: Option<u32>,
    recv: Option<u32>,
    method: Option<u32>,
    con_var: Option<u32>,
    con_class: Option<u32>,
    imp_source: Option<u32>,
    imp_name: Option<u32>,
    imp_alias: Option<u32>,
    imp_ns: Option<u32>,
    t_symbol: Option<u32>,
    t_acc_obj: Option<u32>,
    t_acc_attr: Option<u32>,
    t_assign: Option<u32>,
    t_aug: Option<u32>,
    t_recv: Option<u32>,
    t_call_fn: Option<u32>,
    mod_name: Option<u32>,
    mod_inline: Option<u32>,
}

impl CompiledPack {
    pub(crate) fn new(pack: &DerivePack) -> CompiledPack {
        let bind_src = pack
            .spec
            .bind_scm
            .as_deref()
            .expect("derive-tier pack has bind.scm (loader guarantees it)");
        let derive_src = pack
            .spec
            .derive_scm
            .as_deref()
            .expect("derive-tier pack has derive.scm (loader guarantees it)");
        let bind =
            Query::new(&pack.grammar, bind_src).expect("bind.scm matches the pinned grammar");
        let derive =
            Query::new(&pack.grammar, derive_src).expect("derive.scm matches the pinned grammar");
        let idx = |q: &Query, n: &str| q.capture_index_for_name(n);
        let b = BindCaps {
            func: idx(&bind, "subject.function"),
            typ: idx(&bind, "subject.type"),
            name: idx(&bind, "subject.name"),
        };
        let d = DeriveCaps {
            call: idx(&derive, "call"),
            callee: idx(&derive, "call.callee"),
            recv: idx(&derive, "call.receiver"),
            method: idx(&derive, "call.method"),
            con_var: idx(&derive, "call.construct.var"),
            con_class: idx(&derive, "call.construct.class"),
            imp_source: idx(&derive, "import.source"),
            imp_name: idx(&derive, "import.name"),
            imp_alias: idx(&derive, "import.alias"),
            imp_ns: idx(&derive, "import.namespace"),
            t_symbol: idx(&derive, "touch.symbol"),
            t_acc_obj: idx(&derive, "touch.access.obj"),
            t_acc_attr: idx(&derive, "touch.access.attr"),
            t_assign: idx(&derive, "touch.assign_lhs"),
            t_aug: idx(&derive, "touch.aug_assign_lhs"),
            t_recv: idx(&derive, "touch.receiver"),
            t_call_fn: idx(&derive, "touch.call_function"),
            mod_name: idx(&derive, "module.name"),
            mod_inline: idx(&derive, "module.inline"),
        };
        // The separator for D-076 last-segment aliasing: a path strategy's
        // own separator (root_relative's `.`) or `/` for the path-shaped ones.
        let import_sep = pack
            .spec
            .imports
            .iter()
            .find_map(|s| match s {
                ImportStrategy::RootRelative { separator, .. } => Some(separator.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "/".to_string());
        CompiledPack {
            id: format!(
                "{}:{:016x}",
                pack.spec.name,
                fnv1a64(bind_src.as_bytes()) ^ fnv1a64(derive_src.as_bytes())
            ),
            strategies: pack.spec.imports.clone(),
            grammar: pack.grammar.clone(),
            extensions: pack.spec.extensions.clone(),
            bind,
            derive,
            value_functions: pack.spec.value_functions.clone(),
            mutators: pack.spec.mutator_methods.iter().cloned().collect(),
            whole_alias: pack.spec.whole_alias,
            import_sep,
            b,
            d,
        }
    }

    /// Does this pack claim `path` by extension (§8.6.1)?
    pub(crate) fn claims(&self, path: &Path) -> bool {
        let name = path.to_string_lossy();
        self.extensions.iter().any(|e| name.ends_with(e.as_str()))
    }

    /// State of a different language never collides on a bare own-module
    /// occurrence (D-062d): same-family means the state's defining file has an
    /// extension this pack claims.
    fn same_family(&self, file: &Path) -> bool {
        self.claims(file)
    }
}

pub(crate) fn extract(cp: &CompiledPack, file: &SourceUnit, states: &[StateSymbol]) -> FileFacts {
    let tree = parse(&cp.grammar, &file.text);
    let root = tree.root_node();
    let src = file.text.as_str();
    let text = |n: Node<'_>| &src[n.byte_range()];

    let (decls, decl_index, name_nodes) = collect_decls(cp, root, src);
    let derive_matches = run_query(&cp.derive, root, src.as_bytes());

    // Same-file class table for the D-062e local-construction rule.
    let mut classes: HashMap<&str, usize> = HashMap::new();
    for (i, d) in decls.iter().enumerate() {
        if d.kind == DeclKind::Type {
            classes.entry(d.name.as_str()).or_insert(i);
        }
    }

    // `var -> same-file class` per enclosing function, from @call.construct.*.
    let mut locals: HashMap<(usize, &str), usize> = HashMap::new();
    for m in &derive_matches {
        let (Some(var), Some(cls)) = (m.cap(cp.d.con_var), m.cap(cp.d.con_class)) else {
            continue;
        };
        let Some(enc) = enclosing_function(var, &decl_index, &decls, &cp.value_functions) else {
            continue; // module-level constructions feed module-level calls, which drop
        };
        if let Some(&class_decl) = classes.get(text(cls)) {
            locals.insert((enc, text(var)), class_decl);
        }
    }

    let calls = collect_calls(cp, &derive_matches, &decl_index, &decls, &locals, text);
    let imports = collect_imports(cp, &derive_matches, text);
    let mods = collect_mods(cp, &derive_matches, text);
    let touches = collect_touches(
        cp,
        &derive_matches,
        file,
        states,
        &decls,
        &decl_index,
        &imports,
        &name_nodes,
        text,
    );

    FileFacts {
        decls,
        calls,
        imports,
        touches,
        mods,
    }
}

/// Declarations from `bind.scm` (D-070g unifies decl extraction with binding):
/// `@subject.function`/`@subject.type` nodes become Function/Type decls; value
/// forms derive no node (D-060a). Returns the decls, node-id -> decl index, and
/// the set of *all* `@subject.name` node ids (the bare-occurrence validity
/// filter excludes them, D-073).
fn collect_decls(
    cp: &CompiledPack,
    root: Node<'_>,
    src: &str,
) -> (Vec<DeclFact>, HashMap<usize, usize>, HashSet<usize>) {
    let mut name_nodes = HashSet::new();
    let mut nodes: Vec<(Node<'_>, String, DeclKind)> = Vec::new();
    for m in run_query(&cp.bind, root, src.as_bytes()) {
        if let Some(name) = m.cap(cp.b.name) {
            name_nodes.insert(name.id());
        }
        let (node, kind) = if let Some(n) = m.cap(cp.b.func) {
            (n, DeclKind::Function)
        } else if let Some(n) = m.cap(cp.b.typ) {
            (n, DeclKind::Type)
        } else {
            continue; // value-binding form: no derived node (D-060a)
        };
        let Some(name) = m.cap(cp.b.name) else {
            continue; // multi-target form (no single name): no derived node
        };
        nodes.push((node, src[name.byte_range()].to_string(), kind));
    }
    nodes.sort_by_key(|(n, _, _)| (n.start_byte(), n.end_byte()));
    let index: HashMap<usize, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, (n, _, _))| (n.id(), i))
        .collect();
    let decls = nodes
        .iter()
        .map(|(n, name, kind)| DeclFact {
            name: name.clone(),
            kind: *kind,
            span: span_fact(*n),
            parent: nearest_decl(*n, &index),
        })
        .collect();
    (decls, index, name_nodes)
}

/// §8.2 call sites: one CallFact per `@call` node (the captures of every
/// pattern matching that node are unioned), classified into the callee shapes
/// the resolver understands (D-072). Deterministic by source position.
fn collect_calls<'t>(
    cp: &CompiledPack,
    matches: &[Match<'t>],
    decl_index: &HashMap<usize, usize>,
    decls: &[DeclFact],
    locals: &HashMap<(usize, &str), usize>,
    text: impl Fn(Node<'t>) -> &'t str,
) -> Vec<CallFact> {
    struct Group<'t> {
        node: Node<'t>,
        callee: Option<Node<'t>>,
        recv: Option<Node<'t>>,
        method: Option<Node<'t>>,
    }
    let mut groups: HashMap<usize, Group<'t>> = HashMap::new();
    for m in matches {
        let Some(call) = m.cap(cp.d.call) else {
            continue;
        };
        let g = groups.entry(call.id()).or_insert(Group {
            node: call,
            callee: None,
            recv: None,
            method: None,
        });
        g.callee = g.callee.or(m.cap(cp.d.callee));
        g.recv = g.recv.or(m.cap(cp.d.recv));
        g.method = g.method.or(m.cap(cp.d.method));
    }
    let mut groups: Vec<Group<'t>> = groups.into_values().collect();
    groups.sort_by_key(|g| (g.node.start_byte(), g.node.end_byte()));

    groups
        .into_iter()
        .map(|g| {
            let enclosing = enclosing_function(g.node, decl_index, decls, &cp.value_functions);
            let callee = match (g.recv, g.method) {
                (Some(recv), Some(method)) => {
                    if let Some(&class_decl) = enclosing.and_then(|e| locals.get(&(e, text(recv))))
                    {
                        CalleeFact::Method {
                            class_decl,
                            name: text(method).to_string(),
                        }
                    } else {
                        CalleeFact::Attr {
                            obj: text(recv).to_string(),
                            name: text(method).to_string(),
                        }
                    }
                }
                _ => match g.callee {
                    Some(c) => CalleeFact::Bare(text(c).to_string()),
                    None => CalleeFact::Opaque,
                },
            };
            CallFact {
                callee,
                enclosing,
                span: span_fact(g.node),
            }
        })
        .collect()
}

/// Import forms from the `@import.*` captures (D-062c). `@import.name` =>
/// Named (alias defaults to the name); else `@import.namespace` or a bare
/// `@import.source` => Whole (a plain `import a.b` whole binding is harmless —
/// no single-identifier callee/access can address its dotted alias). Document
/// order, so the resolver's "last binding wins" shadowing holds.
fn collect_imports<'t>(
    cp: &CompiledPack,
    matches: &[Match<'t>],
    text: impl Fn(Node<'t>) -> &'t str,
) -> Vec<ImportFact> {
    let mut out: Vec<(usize, ImportFact)> = Vec::new();
    for m in matches {
        let Some(source) = m.cap(cp.d.imp_source) else {
            continue;
        };
        let module = text(source).to_string();
        let fact = if let Some(name) = m.cap(cp.d.imp_name) {
            ImportFact::Named {
                module,
                name: text(name).to_string(),
                alias: m
                    .cap(cp.d.imp_alias)
                    .map(|a| text(a).to_string())
                    .unwrap_or_else(|| text(name).to_string()),
            }
        } else {
            // Explicit namespace wins; else the implicit alias is the whole
            // source (Python/TS, `Full`) or its path tail (Go/Java, D-076).
            let alias = m
                .cap(cp.d.imp_ns)
                .map(|n| text(n).to_string())
                .unwrap_or_else(|| match cp.whole_alias {
                    WholeAlias::Full => module.clone(),
                    WholeAlias::LastSegment => module
                        .rsplit(cp.import_sep.as_str())
                        .next()
                        .unwrap_or(module.as_str())
                        .to_string(),
                });
            ImportFact::Whole { module, alias }
        };
        out.push((source.start_byte(), fact));
    }
    out.sort_by_key(|(b, _)| *b);
    out.into_iter().map(|(_, f)| f).collect()
}

/// Module declarations for the `rust_use_paths` crate tree (D-078): one
/// `ModFact` per `@module.name`, `inline` iff the pattern also captured a
/// `@module.inline` body. Document order (irrelevant: the tree keys by name).
fn collect_mods<'t>(
    cp: &CompiledPack,
    matches: &[Match<'t>],
    text: impl Fn(Node<'t>) -> &'t str,
) -> Vec<ModFact> {
    let mut by_node: HashMap<usize, (String, bool)> = HashMap::new();
    for m in matches {
        let Some(name) = m.cap(cp.d.mod_name) else {
            continue;
        };
        let entry = by_node
            .entry(name.id())
            .or_insert_with(|| (text(name).to_string(), false));
        if m.cap(cp.d.mod_inline).is_some() {
            entry.1 = true;
        }
    }
    by_node
        .into_values()
        .map(|(name, inline)| ModFact { name, inline })
        .collect()
}

/// §8.3 occurrence scan (D-073 hybrid): the query supplies occurrence shapes
/// (`@touch.symbol`, `@touch.access.*`) and write markers
/// (`@touch.assign_lhs`/`@touch.aug_assign_lhs`/`@touch.receiver` +
/// `@touch.call_function`); the engine does the validity filter, the
/// mutator-list application, and the write/read decision.
#[allow(clippy::too_many_arguments)] // extraction state, not a public surface
fn collect_touches<'t>(
    cp: &CompiledPack,
    matches: &[Match<'t>],
    file: &SourceUnit,
    states: &[StateSymbol],
    decls: &[DeclFact],
    decl_index: &HashMap<usize, usize>,
    imports: &[ImportFact],
    name_nodes: &HashSet<usize>,
    text: impl Fn(Node<'t>) -> &'t str,
) -> Vec<TouchFact> {
    let (bare, attr) = candidate_maps(cp, file, states, imports);
    if bare.is_empty() && attr.is_empty() {
        return Vec::new();
    }

    // Write markers and the validity-exclusion sets, in one pass.
    let mut assign: HashSet<usize> = HashSet::new();
    let mut aug: HashSet<usize> = HashSet::new();
    let mut receiver_method: HashMap<usize, &str> = HashMap::new();
    let mut exclude: HashSet<usize> = HashSet::new(); // member/property/import positions
    for m in matches {
        if let Some(n) = m.cap(cp.d.t_assign) {
            assign.insert(n.id());
        }
        if let Some(n) = m.cap(cp.d.t_aug) {
            aug.insert(n.id());
        }
        if let (Some(r), Some(f)) = (m.cap(cp.d.t_recv), m.cap(cp.d.t_call_fn)) {
            receiver_method.insert(r.id(), text(f));
        }
        if let Some(n) = m.cap(cp.d.t_acc_attr) {
            exclude.insert(n.id());
        }
        if let Some(n) = m.cap(cp.d.method) {
            exclude.insert(n.id());
        }
        for cap in [cp.d.imp_source, cp.d.imp_name, cp.d.imp_alias, cp.d.imp_ns] {
            if let Some(n) = m.cap(cap) {
                exclude.insert(n.id());
            }
        }
    }

    // Occurrence sites, document order (the resolver's per-(fn,state,kind)
    // dedupe keeps the first, D-062d).
    enum Site<'t> {
        Bare(Node<'t>),
        Access(Node<'t>, String, String),
    }
    let mut sites: Vec<(usize, Site<'t>)> = Vec::new();
    for m in matches {
        if let Some(id) = m.cap(cp.d.t_symbol) {
            sites.push((id.start_byte(), Site::Bare(id)));
        } else if let (Some(obj), Some(at)) = (m.cap(cp.d.t_acc_obj), m.cap(cp.d.t_acc_attr)) {
            // the access node is the parent of the obj/attr pair
            let access = at.parent().unwrap_or(at);
            sites.push((
                access.start_byte(),
                Site::Access(access, text(obj).to_string(), text(at).to_string()),
            ));
        }
    }
    sites.sort_by_key(|(b, _)| *b);

    let is_write = |node: Node<'_>| {
        assign.contains(&node.id())
            || aug.contains(&node.id())
            || receiver_method
                .get(&node.id())
                .is_some_and(|mth| cp.mutators.contains(*mth))
    };

    let mut out = Vec::new();
    for (_, site) in sites {
        let (node, candidates): (Node<'_>, Vec<(usize, Option<usize>)>) = match site {
            Site::Bare(id) => {
                let valid = !(exclude.contains(&id.id()) || name_nodes.contains(&id.id()))
                    || assign.contains(&id.id())
                    || aug.contains(&id.id());
                if !valid {
                    continue;
                }
                let Some(c) = bare.get(text(id)) else {
                    continue;
                };
                (id, c.clone())
            }
            Site::Access(access, obj, at) => {
                let Some(c) = attr.get(&(obj, at)) else {
                    continue;
                };
                (access, c.iter().map(|&(si, ii)| (si, Some(ii))).collect())
            }
        };
        let write = is_write(node);
        let enclosing = enclosing_function(node, decl_index, decls, &cp.value_functions);
        for (state, via_import) in candidates {
            out.push(TouchFact {
                state,
                write,
                enclosing,
                via_import,
                span: span_fact(node),
            });
        }
    }
    out
}

/// State-symbol visibility for one file (D-062d), ported verbatim from the
/// pre-T8 per-language code — parameterized only by the pack's claimed
/// extensions for the same-family test.
type BareCandidates = HashMap<String, Vec<(usize, Option<usize>)>>;
type AttrCandidates = HashMap<(String, String), Vec<(usize, usize)>>;

fn candidate_maps(
    cp: &CompiledPack,
    file: &SourceUnit,
    states: &[StateSymbol],
    imports: &[ImportFact],
) -> (BareCandidates, AttrCandidates) {
    let mut bare: BareCandidates = HashMap::new();
    let mut attr: AttrCandidates = HashMap::new();
    for (si, s) in states.iter().enumerate() {
        if cp.same_family(&s.file) && s.module == file.module {
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

// ---- tree-sitter plumbing (was lang/mod.rs) ----

fn parse(grammar: &Language, text: &str) -> Tree {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(grammar).expect("pinned grammar loads");
    parser
        .parse(text, None)
        .expect("tree-sitter returns a tree")
}

/// One query match, captures resolved by capture index.
pub(crate) struct Match<'t> {
    caps: Vec<(u32, Node<'t>)>,
}

impl<'t> Match<'t> {
    fn cap(&self, index: Option<u32>) -> Option<Node<'t>> {
        let index = index?;
        self.caps.iter().find(|(i, _)| *i == index).map(|(_, n)| *n)
    }
}

fn run_query<'t>(query: &Query, root: Node<'t>, src: &[u8]) -> Vec<Match<'t>> {
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

fn span_fact(n: Node<'_>) -> SpanFact {
    SpanFact {
        line: n.start_position().row as u32 + 1,
        col: n.start_position().column as u32 + 1,
        end_line: n.end_position().row as u32 + 1,
        end_col: n.end_position().column as u32 + 1,
    }
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
/// Function; None at module/class level or inside a value-bound function
/// (lambda/arrow), where an attributed edge would be a guess (G-7, D-074).
fn enclosing_function(
    node: Node<'_>,
    index: &HashMap<usize, usize>,
    decls: &[DeclFact],
    value_functions: &[String],
) -> Option<usize> {
    let mut n = node;
    while let Some(p) = n.parent() {
        if value_functions.iter().any(|k| k == p.kind()) {
            return None;
        }
        if let Some(&i) = index.get(&p.id()) {
            return (decls[i].kind == DeclKind::Function).then_some(i);
        }
        n = p;
    }
    None
}

/// FNV-1a 64 over the query sources, for the pack-identity cache component
/// (D-070i). Stable across runs; collisions only cost a re-extraction.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
