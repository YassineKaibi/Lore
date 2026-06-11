//! Query execution: every §10.1 form composed from the two §10.6 primitives.
//! Failures here are usage errors (exit 2, D-053a) carrying a remedy, never
//! §18 findings — `ask` answers questions, `lint` reports findings.

use lore_intent::{Finding, IntentNode, Kind, QName};

use crate::engine::{Direction, Hop, Mode, Witness};
use crate::query::{Filter, Query, QueryExpr, ScopeKind};
use crate::{Edge, EdgeKind, Graph};

/// `lore ask` flags that reach the engine (`path --all --max-len N`, §12).
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub all: bool,
    pub max_len: Option<usize>,
}

/// One node-set result: the node plus its witnessing chain (empty for scan
/// queries like `owner`/`tagged`/`unknown`).
#[derive(Debug, Clone)]
pub struct Hit {
    pub qname: QName,
    pub via: Vec<Edge>,
}

/// The `show(X)` card payload: the node itself is read from `graph.nodes`;
/// edges come grouped/sorted, findings per `Graph.attributions` (D-055).
#[derive(Debug, Clone)]
pub struct Card {
    pub qname: QName,
    pub edges_out: Vec<Edge>,
    pub edges_in: Vec<Edge>,
    pub findings: Vec<Finding>,
}

#[derive(Debug)]
pub enum Answer {
    Hits { hits: Vec<Hit>, visited: usize },
    Card(Box<Card>),
}

/// `reaches`/`path` traverse everything causal plus structure (§6.4).
const REACH: [Hop; 8] = [
    Hop::Edge(EdgeKind::Contains),
    Hop::Edge(EdgeKind::Sequence),
    Hop::Edge(EdgeKind::Triggers),
    Hop::Edge(EdgeKind::Calls),
    Hop::EventHop,
    Hop::Edge(EdgeKind::Affects),
    Hop::Edge(EdgeKind::Reads),
    Hop::Edge(EdgeKind::DependsOn),
];

/// Transitive extension set for `affects*`/`reads*`/`touches*` (§6.4):
/// call chains and event-hop causality.
const CAUSES: [Hop; 3] = [
    Hop::Edge(EdgeKind::Triggers),
    Hop::Edge(EdgeKind::Calls),
    Hop::EventHop,
];

// @lore
// purpose: "Answer one parsed query against the graph: compose select/traverse, apply filters, return hits with witness chains or the show card"
// because: "Every form must stay sugar over the two §10.6 primitives, so this is composition only: any form needing new engine code is a spec violation"
// unknown: "nodes_visited double-counts nodes seen by more than one composed traversal; revisit if stats start informing optimization"
pub fn ask(graph: &Graph, query: &Query, options: &Options) -> Result<Answer, String> {
    if options.all && !matches!(query.expr, QueryExpr::Path { .. }) {
        return Err("--all applies only to path(A, B)".to_string());
    }
    if options.all && options.max_len.is_none() {
        return Err(
            "--all needs --max-len N to bound the enumeration; add e.g. --max-len 6".to_string(),
        );
    }
    if !query.filters.is_empty()
        && matches!(query.expr, QueryExpr::Show { .. } | QueryExpr::Path { .. })
    {
        return Err("show and path take no trailing filters (D-052f); drop the filter".to_string());
    }

    let (mut hits, visited) = match &query.expr {
        QueryExpr::Affects { arg, star } => effect_sources(graph, arg, EdgeKind::Affects, *star)?,
        QueryExpr::Reads { arg, star } => effect_sources(graph, arg, EdgeKind::Reads, *star)?,
        QueryExpr::Touches { arg, star } => touches(graph, arg, *star)?,
        QueryExpr::Triggers { arg, star } => {
            let start = [resolve(graph, arg)?];
            let mode = if *star {
                Mode::Transitive
            } else {
                Mode::Single
            };
            let t = graph.traverse(
                &start,
                Direction::Reverse,
                &[Hop::Edge(EdgeKind::Triggers), Hop::Edge(EdgeKind::Calls)],
                mode,
            );
            (to_hits(t.hits), t.visited)
        }
        QueryExpr::Emits { arg } => one_hop_reverse(graph, arg, EdgeKind::Emits)?,
        QueryExpr::Handlers { arg } => one_hop_reverse(graph, arg, EdgeKind::Handles)?,
        QueryExpr::Depends { arg, star } => depends(graph, arg, Direction::Forward, *star)?,
        QueryExpr::Dependents { arg, star } => depends(graph, arg, Direction::Reverse, *star)?,
        QueryExpr::Reaches { arg } => {
            let start = [resolve(graph, arg)?];
            let t = graph.traverse(&start, Direction::Forward, &REACH, Mode::Transitive);
            (to_hits(t.hits), t.visited)
        }
        QueryExpr::Path { from, to } => return path(graph, from, to, options),
        QueryExpr::Show { arg } => return show(graph, arg),
        QueryExpr::Tagged(_) => {
            // Phase 1 declares no tags (D-052c): honest empty, not an error.
            (Vec::new(), 0)
        }
        QueryExpr::Owner(team) => {
            let qnames = graph.select(|n| effective_owner(graph, n) == Some(team.as_str()));
            let visited = qnames.len();
            (scan_hits(qnames), visited)
        }
        QueryExpr::Unknown { scope } => {
            let qnames = graph.select(|n| !n.intent.unknown.is_empty());
            let visited = qnames.len();
            let mut hits = scan_hits(qnames);
            if let Some((kind, arg)) = scope {
                let scope_node = scope_node(graph, arg, kind.kind(), kind.name())?;
                hits.retain(|h| is_prefix(&scope_node, &h.qname));
            }
            (hits, visited)
        }
    };

    for filter in &query.filters {
        apply_filter(graph, filter, &mut hits)?;
    }
    hits.sort_by(|a, b| a.qname.cmp(&b.qname));
    Ok(Answer::Hits { hits, visited })
}

/// `affects(S)` / `reads(S)`: one effect edge into S; with `*`, prepend
/// reverse call/event-hop chains from the direct sources (§6.4).
fn effect_sources(
    graph: &Graph,
    arg: &QName,
    kind: EdgeKind,
    star: bool,
) -> Result<(Vec<Hit>, usize), String> {
    let start = [resolve(graph, arg)?];
    let direct = graph.traverse(&start, Direction::Reverse, &[Hop::Edge(kind)], Mode::Single);
    if !star {
        return Ok((to_hits(direct.hits), direct.visited));
    }
    let upstream = graph.traverse(&direct.hits, Direction::Reverse, &CAUSES, Mode::Transitive);
    Ok((
        union(direct.hits, upstream.hits, arg),
        direct.visited + upstream.visited,
    ))
}

/// `touches(F)`: effect edges out of F; with `*`, also out of everything F
/// transitively calls/triggers/causes via events (§6.4).
fn touches(graph: &Graph, arg: &QName, star: bool) -> Result<(Vec<Hit>, usize), String> {
    let start = [resolve(graph, arg)?];
    let effects = [Hop::Edge(EdgeKind::Affects), Hop::Edge(EdgeKind::Reads)];
    let direct = graph.traverse(&start, Direction::Forward, &effects, Mode::Single);
    if !star {
        return Ok((to_hits(direct.hits), direct.visited));
    }
    let downstream = graph.traverse(&start, Direction::Forward, &CAUSES, Mode::Transitive);
    let more = graph.traverse(&downstream.hits, Direction::Forward, &effects, Mode::Single);
    Ok((
        union(direct.hits, more.hits, arg),
        direct.visited + downstream.visited + more.visited,
    ))
}

fn one_hop_reverse(
    graph: &Graph,
    arg: &QName,
    kind: EdgeKind,
) -> Result<(Vec<Hit>, usize), String> {
    let start = [resolve(graph, arg)?];
    let t = graph.traverse(&start, Direction::Reverse, &[Hop::Edge(kind)], Mode::Single);
    Ok((to_hits(t.hits), t.visited))
}

fn depends(
    graph: &Graph,
    arg: &QName,
    direction: Direction,
    star: bool,
) -> Result<(Vec<Hit>, usize), String> {
    let start = [resolve(graph, arg)?];
    let mode = if star { Mode::Transitive } else { Mode::Single };
    let t = graph.traverse(&start, direction, &[Hop::Edge(EdgeKind::DependsOn)], mode);
    Ok((to_hits(t.hits), t.visited))
}

/// `path(A, B)`: shortest witnessed path, or with `--all --max-len N` one
/// result per simple path of at most N edges, qname = B (D-054d).
fn path(graph: &Graph, from: &QName, to: &QName, options: &Options) -> Result<Answer, String> {
    let start = [resolve(graph, from)?];
    resolve(graph, to)?;
    let mode = match (options.all, options.max_len) {
        (true, Some(max_len)) => Mode::AllPaths { max_len },
        _ => Mode::Transitive,
    };
    let t = graph.traverse(&start, Direction::Forward, &REACH, mode);
    let hits: Vec<Hit> = t
        .hits
        .into_iter()
        .filter(|w| &w.qname == to)
        .map(|w| Hit {
            qname: w.qname,
            via: w.via,
        })
        .collect();
    Ok(Answer::Hits {
        hits,
        visited: t.visited,
    })
}

fn show(graph: &Graph, arg: &QName) -> Result<Answer, String> {
    resolve(graph, arg)?;
    // grouped by kind in §6.1 order, then by the far endpoint (§10.2)
    let mut edges_out = graph.out.get(arg).cloned().unwrap_or_default();
    edges_out.sort_by(|a, b| (a.kind.order(), &a.to).cmp(&(b.kind.order(), &b.to)));
    let mut edges_in = graph.inc.get(arg).cloned().unwrap_or_default();
    edges_in.sort_by(|a, b| (a.kind.order(), &a.from).cmp(&(b.kind.order(), &b.from)));
    let findings = graph
        .attributions
        .get(arg)
        .map(|idxs| idxs.iter().map(|&i| graph.findings[i].clone()).collect())
        .unwrap_or_default();
    Ok(Answer::Card(Box::new(Card {
        qname: arg.clone(),
        edges_out,
        edges_in,
        findings,
    })))
}

/// Resolve a CLI-provided qname to its node, with the D-053a message on
/// failure — shared with `lore history` (D-059a), which mirrors ask's
/// exit-2 behavior for an argument naming no node.
pub fn lookup<'g>(graph: &'g Graph, arg: &QName) -> Result<&'g IntentNode, String> {
    resolve(graph, arg).map(|w| &graph.nodes[&w.qname])
}

/// A query argument must name a node; the message mirrors E0306 (D-053a).
fn resolve(graph: &Graph, arg: &QName) -> Result<Witness, String> {
    if graph.nodes.contains_key(arg) {
        return Ok(Witness::start(arg.clone()));
    }
    Err(
        match crate::util::nearest(&arg.to_string(), graph.nodes.keys().map(QName::to_string)) {
            Some(nearest) => format!(
                "\"{arg}\" names no node in the graph; nearest existing qname is \"{nearest}\""
            ),
            None => {
                format!("\"{arg}\" names no node: the graph is empty; annotate something first")
            }
        },
    )
}

/// A filter/scope argument must name a node of the right kind (D-053a).
fn scope_node(graph: &Graph, arg: &QName, kind: Kind, kindword: &str) -> Result<QName, String> {
    let w = resolve(graph, arg)?;
    let actual = graph.nodes[&w.qname].kind;
    if actual != kind {
        return Err(format!(
            "in {kindword}({arg}): \"{arg}\" is a {}, not a {kindword}",
            actual.name()
        ));
    }
    Ok(w.qname)
}

fn apply_filter(graph: &Graph, filter: &Filter, hits: &mut Vec<Hit>) -> Result<(), String> {
    match filter {
        Filter::InModule(m) => {
            let scope = scope_node(graph, m, Kind::Module, ScopeKind::Module.name())?;
            hits.retain(|h| is_prefix(&scope, &h.qname));
        }
        Filter::InService(s) => {
            let scope = scope_node(graph, s, Kind::Service, ScopeKind::Service.name())?;
            hits.retain(|h| is_prefix(&scope, &h.qname));
        }
        Filter::OwnedBy(team) => {
            hits.retain(|h| effective_owner(graph, &graph.nodes[&h.qname]) == Some(team.as_str()));
        }
        Filter::Kind(k) => hits.retain(|h| graph.nodes[&h.qname].kind == *k),
    }
    Ok(())
}

/// Declared owner, or for State/Event (whose owner is inherited, §3.2) the
/// nearest enclosing Module's declared owner (D-052b).
fn effective_owner<'g>(graph: &'g Graph, node: &'g IntentNode) -> Option<&'g str> {
    if let Some(o) = &node.intent.owner {
        return Some(&o.value);
    }
    if !matches!(node.kind, Kind::State | Kind::Event) {
        return None;
    }
    let q = &node.qname;
    (1..q.0.len()).rev().find_map(|len| {
        let prefix = QName(q.0[..len].to_vec());
        let n = graph.nodes.get(&prefix)?;
        if n.kind == Kind::Module {
            n.intent.owner.as_ref().map(|o| o.value.as_str())
        } else {
            None
        }
    })
}

fn is_prefix(p: &QName, q: &QName) -> bool {
    p.0.len() <= q.0.len() && q.0[..p.0.len()] == p.0[..]
}

fn to_hits(witnesses: Vec<Witness>) -> Vec<Hit> {
    witnesses
        .into_iter()
        .map(|w| Hit {
            qname: w.qname,
            via: w.via,
        })
        .collect()
}

fn scan_hits(qnames: Vec<QName>) -> Vec<Hit> {
    qnames
        .into_iter()
        .map(|qname| Hit {
            qname,
            via: Vec::new(),
        })
        .collect()
}

/// Union two witness sets, dedup by qname keeping the shorter chain
/// (D-052e), and drop the query argument itself.
fn union(a: Vec<Witness>, b: Vec<Witness>, arg: &QName) -> Vec<Hit> {
    let mut merged: Vec<Witness> = Vec::new();
    for w in a.into_iter().chain(b) {
        if &w.qname == arg {
            continue;
        }
        match merged.iter_mut().find(|m| m.qname == w.qname) {
            Some(m) => {
                if w.via.len() < m.via.len() {
                    *m = w;
                }
            }
            None => merged.push(w),
        }
    }
    to_hits(merged)
}
