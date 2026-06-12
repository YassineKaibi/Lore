//! The intent graph (spec §6, §13): node table, both adjacency maps,
//! resolution, applicability, depends_on surface, hygiene, and strict
//! promotion. Consumes data from lore_annotations/lore_derive — never the
//! crates themselves (§13 dependency direction).

pub mod codeowners;
mod engine;
pub mod exec;
mod hygiene;
mod matrix;
pub mod query;
mod reconcile;
mod resolve;
mod structure;
mod surface;
mod table;
mod util;

pub use codeowners::{Codeowners, CodeownersRule};
pub use engine::{Direction, Hop, Mode, Traversal, Witness};

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use lore_intent::{Finding, IntentNode, Kind, QName, Span, Spanned};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Affects,
    Reads,
    Triggers,
    Emits,
    Handles,
    DependsOn,
    Contains,
    Sequence,
    Calls,
}

impl EdgeKind {
    /// §6.1 order; output surfaces print this form ("Affects", §10.4).
    pub fn name(self) -> &'static str {
        match self {
            EdgeKind::Affects => "Affects",
            EdgeKind::Reads => "Reads",
            EdgeKind::Triggers => "Triggers",
            EdgeKind::Emits => "Emits",
            EdgeKind::Handles => "Handles",
            EdgeKind::DependsOn => "DependsOn",
            EdgeKind::Contains => "Contains",
            EdgeKind::Sequence => "Sequence",
            EdgeKind::Calls => "Calls",
        }
    }

    /// Position in the §6.1 enumeration, for grouped/ordered rendering.
    pub fn order(self) -> usize {
        match self {
            EdgeKind::Affects => 0,
            EdgeKind::Reads => 1,
            EdgeKind::Triggers => 2,
            EdgeKind::Emits => 3,
            EdgeKind::Handles => 4,
            EdgeKind::DependsOn => 5,
            EdgeKind::Contains => 6,
            EdgeKind::Sequence => 7,
            EdgeKind::Calls => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Declared,
    Derived,
}

impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Layer::Declared => "Declared",
            Layer::Derived => "Derived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    Verified,
    Unverified,
    Contradicted,
    Unverifiable,
}

impl ClaimStatus {
    pub fn name(self) -> &'static str {
        match self {
            ClaimStatus::Verified => "Verified",
            ClaimStatus::Unverified => "Unverified",
            ClaimStatus::Contradicted => "Contradicted",
            ClaimStatus::Unverifiable => "Unverifiable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Exact,
    Resolved,
    Heuristic,
}

impl Confidence {
    pub fn name(self) -> &'static str {
        match self {
            Confidence::Exact => "Exact",
            Confidence::Resolved => "Resolved",
            Confidence::Heuristic => "Heuristic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: QName,
    pub to: QName,
    pub kind: EdgeKind,
    pub layer: Layer,
    pub loc: Span,
    pub status: Option<ClaimStatus>, // Some iff layer == Declared (claim edges)
    pub confidence: Option<Confidence>, // Some iff layer == Derived
}

pub struct Graph {
    pub nodes: HashMap<QName, IntentNode>,
    pub out: HashMap<QName, Vec<Edge>>,
    pub inc: HashMap<QName, Vec<Edge>>,
    pub findings: Vec<Finding>,
    /// Node -> indices into `findings` attributed to it (D-049's attribution,
    /// public per D-055 so `show(X)` can render per-node findings).
    pub attributions: HashMap<QName, Vec<usize>>,
}

/// The derived layer as data (§13): lore_derive's output mapped onto graph
/// types by the CLI — the graph never depends on the derive crate.

// @lore
// kind: type
// purpose: "The derived layer as build input: extracted nodes, confidence-labeled edges, and the derivation scope that decides Unverifiable"
#[derive(Default)]
pub struct DerivedLayer {
    /// Origin Derived, empty intent (§8.1).
    pub nodes: Vec<IntentNode>,
    /// Layer Derived: Calls/Affects/Reads, each with a confidence (§8.4).
    pub edges: Vec<Edge>,
    /// The §9.1 in-scope test (D-061): claims on targets outside these
    /// files are Unverifiable.
    pub scope: HashSet<PathBuf>,
}

impl DerivedLayer {
    /// No derivation: scope is empty, so §9.1 labels every claim
    /// Unverifiable — the algorithm, not a special case (D-047e).
    pub fn empty() -> DerivedLayer {
        DerivedLayer::default()
    }
}

/// Reconciliation inputs (§9, D-066/D-068): source text and git metadata as
/// data, CLI-supplied — the graph never reads the filesystem or runs git.

// @lore
// kind: type
// purpose: "Reconciliation as build input: source text for the symbol-occurrence test, the binder's host identifiers, and per-block git blame metadata"
// because: "A claim is judged against what the code says now, so the judge needs the source text and the symbol the code would mention — handed in as data to keep the graph pure (D-066)"
#[derive(Default)]
pub struct ReconcileInput {
    /// file -> text, for the §9.1 occurrence test.
    pub sources: HashMap<PathBuf, String>,
    /// Declared nodes' bound host identifiers (binder extraction; a `name:`
    /// override changes the qname, never the matched symbol — D-066c).
    pub host_identifiers: HashMap<QName, String>,
    /// Per-block blame metadata; None = staleness skipped (outside a git
    /// work tree, `--no-stale`, or a non-lint command — D-068).
    pub staleness: Option<Vec<StalenessRecord>>,
}

impl ReconcileInput {
    /// No source text, no symbols, no git: every in-scope unmatched claim
    /// stays Unverified — §9.1 with its inputs withheld, not a special case.
    pub fn empty() -> ReconcileInput {
        ReconcileInput::default()
    }
}

/// One annotation block's blame summary (§9.2, D-068), computed by the CLI.
pub struct StalenessRecord {
    pub qname: QName,
    /// The block's span: where W0301 points.
    pub span: Span,
    /// Max committer-time over block lines, unix seconds.
    pub t_block: i64,
    /// Max committer-time over subject-span lines.
    pub t_subject: i64,
    /// ISO-strict renderings for the message.
    pub t_block_iso: String,
    pub t_subject_iso: String,
    /// Hash of the subject line attaining the max.
    pub subject_commit: String,
}

impl Graph {
    pub fn edge_count(&self) -> usize {
        self.out.values().map(Vec::len).sum()
    }
}

/// A finding plus the node it is attributed to, for `enforcement: strict`
/// promotion (D-049). Internal: stripped before the findings leave the crate.
pub(crate) struct OwnedFinding {
    pub finding: Finding,
    pub node: Option<QName>,
}

impl OwnedFinding {
    pub(crate) fn new(finding: Finding, node: &QName) -> Self {
        OwnedFinding {
            finding,
            node: Some(node.clone()),
        }
    }
}

/// Node table plus the deterministic iteration order, the set of nodes that
/// were declared by an intent block (requirement checks apply only to those,
/// D-046), and the derived qnames rejected by the E0305 collision rule
/// (their derived edges are dropped with them, D-060c).
pub(crate) struct Ctx {
    pub nodes: HashMap<QName, IntentNode>,
    pub order: Vec<QName>,
    pub annotated: HashSet<QName>,
    pub rejected_derived: HashSet<QName>,
}

impl Ctx {
    /// Longest proper qname prefix of kind Module/Service/Workflow (D-047).
    pub(crate) fn immediate_container(&self, q: &QName) -> Option<QName> {
        (1..q.0.len())
            .rev()
            .map(|len| QName(q.0[..len].to_vec()))
            .find(|prefix| {
                self.nodes.get(prefix).is_some_and(|n| {
                    matches!(n.kind, Kind::Module | Kind::Service | Kind::Workflow)
                })
            })
    }

    /// Nearest enclosing Module: the node itself when it is a Module,
    /// otherwise the longest proper prefix of kind Module (D-048e, D-049).
    pub(crate) fn nearest_module(&self, q: &QName) -> Option<QName> {
        if self.nodes.get(q).is_some_and(|n| n.kind == Kind::Module) {
            return Some(q.clone());
        }
        for len in (1..q.0.len()).rev() {
            let prefix = QName(q.0[..len].to_vec());
            if self
                .nodes
                .get(&prefix)
                .is_some_and(|n| n.kind == Kind::Module)
            {
                return Some(prefix);
            }
        }
        None
    }

    /// Whether q's nearest enclosing Module declares `enforcement: strict`
    /// (D-049's attribution rule; D-066e uses it to pick E0302 over W0302).
    pub(crate) fn strict_module(&self, q: &QName) -> bool {
        use lore_intent::Enforcement;
        self.nearest_module(q).is_some_and(|m| {
            self.nodes[&m]
                .intent
                .enforcement
                .as_ref()
                .is_some_and(|e| e.value == Enforcement::Strict)
        })
    }

    /// Every prefix of t (t included) naming a Module/Service/External node
    /// — the qnames whose presence in a depends_on satisfies a ref to t (D-048a).
    pub(crate) fn owner_chain(&self, t: &QName) -> Vec<QName> {
        (1..=t.0.len())
            .map(|len| QName(t.0[..len].to_vec()))
            .filter(|p| {
                self.nodes.get(p).is_some_and(|n| {
                    matches!(n.kind, Kind::Module | Kind::Service | Kind::External)
                })
            })
            .collect()
    }
}

pub(crate) fn is_prefix_of(p: &QName, q: &QName) -> bool {
    p.0.len() <= q.0.len() && q.0[..p.0.len()] == p.0[..]
}

/// Build the graph from both layers: node table with declared/derived
/// merging (E0305, D-060), ambient manifest modules (D-046), applicability
/// matrix (§3.2), resolution with the full §9.1 claim statuses (§6.3,
/// D-066), structural edges (D-047), depends_on surface (D-048), hygiene
/// (W0210–W0212), undeclared effects (W0303, D-067), staleness (W0301,
/// D-068), and strict promotion (D-049). Findings come out sorted by
/// (file, line, col, code, message) — deterministic, always.

// @lore
// purpose: "Build the intent graph from both layers: node table with merging, structural and derived edges, resolution with the four-status reconciliation, and the lint findings"
// because: "Contradicted needs the symbol-occurrence test, not derived-edge absence: a Heuristic edge that failed to classify must never read as proof the code changed (G-7, D-066)"
pub fn build(
    declared: Vec<IntentNode>,
    manifest_modules: &[Spanned<String>],
    codeowners: Option<&Codeowners>,
    derived: DerivedLayer,
    reconcile: ReconcileInput,
) -> Graph {
    let mut findings: Vec<OwnedFinding> = Vec::new();
    let ctx = table::build(declared, manifest_modules, derived.nodes, &mut findings);

    // A rejected derived node takes every derived edge touching its qname
    // with it: the qname now names a different declaration (D-060c).
    let derived_edges: Vec<Edge> = derived
        .edges
        .into_iter()
        .filter(|e| {
            !ctx.rejected_derived.contains(&e.from) && !ctx.rejected_derived.contains(&e.to)
        })
        .collect();

    matrix::check(&ctx, &mut findings);
    let mut edges = resolve::resolve(
        &ctx,
        &reconcile,
        &derived_edges,
        &derived.scope,
        &mut findings,
    );
    edges.extend(structure::derive(&ctx));
    edges.extend(derived_edges);
    surface::check(&ctx, &edges, &mut findings);
    hygiene::check(&ctx, &edges, &mut findings);
    if let Some(co) = codeowners {
        codeowners::check(&ctx, co, &mut findings);
    }
    surface_unknowns(&ctx, &mut findings);
    reconcile::undeclared_effects(&ctx, &edges, &mut findings);
    if let Some(records) = &reconcile.staleness {
        reconcile::staleness(records, &mut findings);
    }

    promote_strict(&ctx, &mut findings);

    findings.sort_by(|a, b| {
        let (a, b) = (&a.finding, &b.finding);
        (&a.span.file, a.span.line, a.span.col, a.code, &a.message).cmp(&(
            &b.span.file,
            b.span.line,
            b.span.col,
            b.code,
            &b.message,
        ))
    });
    let mut out: Vec<Finding> = Vec::with_capacity(findings.len());
    let mut attributions: HashMap<QName, Vec<usize>> = HashMap::new();
    for (i, f) in findings.into_iter().enumerate() {
        if let Some(node) = f.node {
            attributions.entry(node).or_default().push(i);
        }
        out.push(f.finding);
    }

    let mut fwd: HashMap<QName, Vec<Edge>> = HashMap::new();
    let mut rev: HashMap<QName, Vec<Edge>> = HashMap::new();
    for e in edges {
        rev.entry(e.to.clone()).or_default().push(e.clone());
        fwd.entry(e.from.clone()).or_default().push(e);
    }

    Graph {
        nodes: ctx.nodes,
        out: fwd,
        inc: rev,
        findings: out,
        attributions,
    }
}

/// D-057: every declared unknown becomes a W0213, attributed to its node —
/// so strict promotion applies here and `show(X)` renders it. Promotion to
/// error under `[policy] unknown = "error"` happens at the lint surface,
/// where the manifest lives; the graph always carries the base Warning.
fn surface_unknowns(ctx: &Ctx, findings: &mut Vec<OwnedFinding>) {
    for qname in &ctx.order {
        let node = &ctx.nodes[qname];
        for u in &node.intent.unknown {
            findings.push(OwnedFinding::new(
                Finding::new(
                    "W0213",
                    u.span.clone(),
                    format!(
                        "{} \"{qname}\" declares an unknown: \"{}\"; resolve it and remove the clause once it is answered",
                        node.kind.name(),
                        u.value
                    ),
                ),
                qname,
            ));
        }
    }
}

/// D-049: W findings from bands 02x/03x attributed to a node whose nearest
/// module declares `enforcement: strict` become errors; the code stays.
fn promote_strict(ctx: &Ctx, findings: &mut [OwnedFinding]) {
    use lore_intent::Severity;
    for f in findings.iter_mut() {
        let code = f.finding.code;
        if !(code.starts_with("W02") || code.starts_with("W03")) {
            continue;
        }
        let Some(node) = &f.node else { continue };
        if ctx.strict_module(node) {
            f.finding.severity = Severity::Error;
        }
    }
}
