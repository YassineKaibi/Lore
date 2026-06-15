//! `lore graph --dot [--focus <qname> --depth N]` (§12, D-038): export the
//! intent graph as Graphviz DOT. Output is deterministic (nodes and edges
//! sorted by qname) and renders under `dot -Tsvg` without warnings: every
//! node and edge endpoint is a quoted id, labels carry the kind plus an
//! edge's layer trust (claim status or derived confidence).

use std::collections::BTreeSet;
use std::path::Path;

use lore_graph::Graph;
use lore_intent::QName;

use crate::commands::project;

// @lore
// name: graph
// purpose: "Emit the intent graph as Graphviz DOT, optionally a depth-bounded neighborhood around one node"
// because: "DOT is the lingua franca for graph rendering; --focus/--depth keep large graphs legible (D-038)"
pub fn run(
    manifest_path: &Path,
    dot: bool,
    focus: Option<&str>,
    depth: Option<usize>,
    quiet: bool,
) -> i32 {
    if !dot {
        eprintln!("lore graph supports only --dot output in v1; pass --dot");
        return 2;
    }
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let graph = project::build_graph(&p, manifest_path, false, quiet).graph;

    let included: BTreeSet<QName> = match focus {
        None => graph.nodes.keys().cloned().collect(),
        Some(f) => {
            let fq = QName::from_dotted(f);
            if !graph.nodes.contains_key(&fq) {
                match lore_graph::nearest_qname(f, &graph) {
                    Some(n) => eprintln!(
                        "no node named \"{f}\" in the graph; nearest existing qname is \"{n}\""
                    ),
                    None => eprintln!("no node named \"{f}\" in the graph"),
                }
                return 2;
            }
            neighborhood(&graph, &fq, depth.unwrap_or(1))
        }
    };

    print!("{}", to_dot(&graph, &included));
    0
}

/// Undirected BFS from `focus` out to `depth` edges, over both adjacency maps:
/// the focus node's neighborhood regardless of edge direction (D-038's intent
/// is "what is near X", not a one-way reachability set).
fn neighborhood(graph: &Graph, focus: &QName, depth: usize) -> BTreeSet<QName> {
    let mut seen = BTreeSet::new();
    seen.insert(focus.clone());
    let mut frontier = vec![focus.clone()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for q in &frontier {
            for e in graph.out.get(q).into_iter().flatten() {
                if seen.insert(e.to.clone()) {
                    next.push(e.to.clone());
                }
            }
            for e in graph.inc.get(q).into_iter().flatten() {
                if seen.insert(e.from.clone()) {
                    next.push(e.from.clone());
                }
            }
        }
        frontier = next;
    }
    seen
}

fn to_dot(graph: &Graph, included: &BTreeSet<QName>) -> String {
    let mut out = String::from("digraph lore {\n  rankdir=LR;\n  node [shape=box];\n");

    // Nodes, sorted by qname. Label is "qname\nKind" (D-022's node card, terse).
    for qname in included {
        let kind = graph
            .nodes
            .get(qname)
            .map(|n| n.kind.display())
            .unwrap_or("?");
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\\n{}\"];\n",
            esc(&qname.to_string()),
            esc(&qname.to_string()),
            kind
        ));
    }

    // Edges among the included nodes, sorted; the edge label carries the kind
    // and the layer's trust (claim status for declared, confidence for
    // derived) so a rendered graph never presents a guess as a fact (G-7).
    let mut edges: Vec<(String, String, String)> = Vec::new();
    for from in included {
        for e in graph.out.get(from).into_iter().flatten() {
            if !included.contains(&e.to) {
                continue;
            }
            let trust = e
                .status
                .map(|s| s.name())
                .or_else(|| e.confidence.map(|c| c.name()));
            let label = match trust {
                Some(t) => format!("{} ({})", e.kind.name(), t),
                None => e.kind.name().to_string(),
            };
            edges.push((from.to_string(), e.to.to_string(), label));
        }
    }
    edges.sort();
    for (from, to, label) in edges {
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            esc(&from),
            esc(&to),
            esc(&label)
        ));
    }

    out.push_str("}\n");
    out
}

/// Escape a DOT double-quoted string: backslash and quote only (qnames and
/// kind names are otherwise DOT-safe).
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
