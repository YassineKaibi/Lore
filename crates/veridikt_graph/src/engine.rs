//! The two engine primitives (§10.6): `select` over the node table and
//! `traverse` with witness chains. Every §10.1 query form MUST be a
//! composition of these two — new query forms never add engine code paths.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

use veridikt_intent::{IntentNode, QName};

use crate::{Edge, EdgeKind, Graph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

/// One traversable hop: a plain edge kind, or the composite event hop
/// `X --Emits--> Event <--Handles-- Y` yielding X→Y causality (§6.4). The
/// Event node never enters the result set; the hop contributes both
/// constituent edges to the witness chain (D-054b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hop {
    Edge(EdgeKind),
    EventHop,
}

/// Realization of §10.6's `transitive` parameter (D-053d): one hop, the
/// shortest-chain closure, or every simple path of at most `max_len` edges.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Single,
    Transitive,
    AllPaths { max_len: usize },
}

/// A reached node plus its witnessing chain: edges in stored from/to
/// orientation, causal (upstream-first) order (D-054a).

// @veridikt
// kind: type
// purpose: "One traversal result: the node reached and the edge chain that witnesses how"
#[derive(Debug, Clone, PartialEq)]
pub struct Witness {
    pub qname: QName,
    pub via: Vec<Edge>,
}

impl Witness {
    /// A traversal start point: no chain yet.
    pub fn start(qname: QName) -> Self {
        Witness {
            qname,
            via: Vec::new(),
        }
    }
}

pub struct Traversal {
    pub hits: Vec<Witness>,
    /// Distinct nodes visited, for §10.4 `stats.nodes_visited`.
    pub visited: usize,
}

impl Graph {
    /// Primitive 1: predicate scan over the node table. Sorted by qname so
    /// every downstream surface is deterministic.
    pub fn select(&self, pred: impl Fn(&IntentNode) -> bool) -> Vec<QName> {
        let mut hits: Vec<QName> = self
            .nodes
            .values()
            .filter(|n| pred(n))
            .map(|n| n.qname.clone())
            .collect();
        hits.sort();
        hits
    }

    /// Primitive 2: walk edges of the given kinds from the start set. Start
    /// nodes never re-enter the result set; each reached node carries its
    /// shortest witnessing chain (chain length in edges, ties broken by
    /// discovery order — deterministic because adjacency order is).
    pub fn traverse(
        &self,
        start: &[Witness],
        direction: Direction,
        hops: &[Hop],
        mode: Mode,
    ) -> Traversal {
        match mode {
            Mode::AllPaths { max_len } => self.all_paths(start, direction, hops, max_len),
            Mode::Single => self.shortest(start, direction, hops, false),
            Mode::Transitive => self.shortest(start, direction, hops, true),
        }
    }

    /// Uniform-cost search by chain length: an event hop costs its two edges,
    /// so "shortest witnessing chain" (§10.3, D-054a) is exact, not per-hop.
    /// Chains are predecessor links during the search and are materialized
    /// only for hits — the search itself is O(V+E) (§10.7), never O(V·chain).
    fn shortest(
        &self,
        start: &[Witness],
        direction: Direction,
        hops: &[Hop],
        transitive: bool,
    ) -> Traversal {
        struct Entry {
            qname: QName,
            parent: Option<usize>,
            start_idx: usize,
            segment: Vec<Edge>, // the 1–2 edges of the hop that reached qname
            cost: usize,        // total chain edges, prefix included
            depth: usize,       // hops from the start node
        }
        // (cost, seq) min-heap; seq is insertion order, the deterministic tie-break
        let mut heap: BinaryHeap<Reverse<(usize, usize)>> = BinaryHeap::new();
        let mut entries: Vec<Entry> = Vec::new();
        let mut settled: HashSet<QName> = HashSet::new();
        for w in start {
            settled.insert(w.qname.clone());
        }
        for (start_idx, w) in start.iter().enumerate() {
            heap.push(Reverse((w.via.len(), entries.len())));
            entries.push(Entry {
                qname: w.qname.clone(),
                parent: None,
                start_idx,
                segment: Vec::new(),
                cost: w.via.len(),
                depth: 0,
            });
        }

        let mut hit_idxs: Vec<usize> = Vec::new();
        while let Some(Reverse((_, idx))) = heap.pop() {
            let (qname, cost, depth, start_idx) = {
                let e = &entries[idx];
                (e.qname.clone(), e.cost, e.depth, e.start_idx)
            };
            if depth > 0 {
                if !settled.insert(qname.clone()) {
                    continue; // already settled with a chain at most this long
                }
                hit_idxs.push(idx);
                if !transitive {
                    continue;
                }
            }
            for (next, segment) in self.expand(&qname, direction, hops) {
                if settled.contains(&next) {
                    continue;
                }
                let cost = cost + segment.len();
                heap.push(Reverse((cost, entries.len())));
                entries.push(Entry {
                    qname: next,
                    parent: Some(idx),
                    start_idx,
                    segment,
                    cost,
                    depth: depth + 1,
                });
            }
        }

        let hits = hit_idxs
            .into_iter()
            .map(|hit| {
                let mut segments: Vec<&[Edge]> = Vec::new();
                let mut cur = hit;
                while let Some(parent) = entries[cur].parent {
                    segments.push(&entries[cur].segment);
                    cur = parent;
                }
                let prefix = &start[entries[hit].start_idx].via;
                let mut via: Vec<Edge> = Vec::with_capacity(entries[hit].cost);
                match direction {
                    Direction::Forward => {
                        via.extend_from_slice(prefix);
                        for seg in segments.iter().rev() {
                            via.extend_from_slice(seg);
                        }
                    }
                    Direction::Reverse => {
                        for seg in &segments {
                            via.extend_from_slice(seg);
                        }
                        via.extend_from_slice(prefix);
                    }
                }
                Witness {
                    qname: entries[hit].qname.clone(),
                    via,
                }
            })
            .collect();
        Traversal {
            hits,
            visited: settled.len(),
        }
    }

    /// Every simple path of at most `max_len` edges (D-054d): one Witness per
    /// distinct path per reached node. DFS over deterministic adjacency.
    fn all_paths(
        &self,
        start: &[Witness],
        direction: Direction,
        hops: &[Hop],
        max_len: usize,
    ) -> Traversal {
        let mut hits = Vec::new();
        let mut visited: HashSet<QName> = HashSet::new();
        for w in start {
            visited.insert(w.qname.clone());
            let mut on_path = vec![w.qname.clone()];
            self.dfs(
                w,
                direction,
                hops,
                max_len,
                &mut on_path,
                &mut hits,
                &mut visited,
            );
        }
        Traversal {
            hits,
            visited: visited.len(),
        }
    }

    #[allow(clippy::too_many_arguments)] // recursion state, not a public surface
    fn dfs(
        &self,
        w: &Witness,
        direction: Direction,
        hops: &[Hop],
        max_len: usize,
        on_path: &mut Vec<QName>,
        hits: &mut Vec<Witness>,
        visited: &mut HashSet<QName>,
    ) {
        for (next, segment) in self.expand(&w.qname, direction, hops) {
            if on_path.contains(&next) {
                continue; // simple paths only
            }
            let via = compose(&w.via, segment, direction);
            if via.len() > max_len {
                continue;
            }
            visited.insert(next.clone());
            let reached = Witness {
                qname: next.clone(),
                via,
            };
            hits.push(reached.clone());
            on_path.push(next);
            self.dfs(&reached, direction, hops, max_len, on_path, hits, visited);
            on_path.pop();
        }
    }

    /// One expansion step from `n`: each reachable node with the edge segment
    /// (1 edge, or 2 for the event hop, causal order) that witnesses it.
    fn expand(&self, n: &QName, direction: Direction, hops: &[Hop]) -> Vec<(QName, Vec<Edge>)> {
        let mut out = Vec::new();
        let none: &[Edge] = &[];
        let fwd = self.out.get(n).map(Vec::as_slice).unwrap_or(none);
        let rev = self.inc.get(n).map(Vec::as_slice).unwrap_or(none);
        for hop in hops {
            match (hop, direction) {
                (Hop::Edge(k), Direction::Forward) => {
                    for e in fwd.iter().filter(|e| e.kind == *k) {
                        out.push((e.to.clone(), vec![e.clone()]));
                    }
                }
                (Hop::Edge(k), Direction::Reverse) => {
                    for e in rev.iter().filter(|e| e.kind == *k) {
                        out.push((e.from.clone(), vec![e.clone()]));
                    }
                }
                (Hop::EventHop, Direction::Forward) => {
                    // n --Emits--> E, then every handler Y --Handles--> E
                    for emit in fwd.iter().filter(|e| e.kind == EdgeKind::Emits) {
                        for handle in self
                            .inc
                            .get(&emit.to)
                            .map(Vec::as_slice)
                            .unwrap_or(none)
                            .iter()
                            .filter(|e| e.kind == EdgeKind::Handles)
                        {
                            out.push((handle.from.clone(), vec![emit.clone(), handle.clone()]));
                        }
                    }
                }
                (Hop::EventHop, Direction::Reverse) => {
                    // n --Handles--> E, then every emitter X --Emits--> E
                    for handle in fwd.iter().filter(|e| e.kind == EdgeKind::Handles) {
                        for emit in self
                            .inc
                            .get(&handle.to)
                            .map(Vec::as_slice)
                            .unwrap_or(none)
                            .iter()
                            .filter(|e| e.kind == EdgeKind::Emits)
                        {
                            out.push((emit.from.clone(), vec![emit.clone(), handle.clone()]));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Chains stay in causal edge order (D-054a): forward walks append the new
/// segment, reverse walks prepend it.
fn compose(prefix: &[Edge], segment: Vec<Edge>, direction: Direction) -> Vec<Edge> {
    match direction {
        Direction::Forward => {
            let mut via = prefix.to_vec();
            via.extend(segment);
            via
        }
        Direction::Reverse => {
            let mut via = segment;
            via.extend_from_slice(prefix);
            via
        }
    }
}
