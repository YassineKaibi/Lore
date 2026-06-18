//! Reconciliation helpers (§9): the symbol-occurrence test feeding the
//! Contradicted branch (D-066), undeclared derived effects W0303 (D-067),
//! and staleness W0301 over CLI-gathered git metadata (D-068). Pure
//! functions of build inputs — no filesystem, no git.

use std::collections::HashSet;

use veridikt_intent::{Finding, Origin, QName};

use crate::{Ctx, Edge, EdgeKind, Layer, OwnedFinding, ReconcileInput, StalenessRecord};

/// t's bound host identifier (D-066c): the binder's extracted subject
/// identifier for annotated nodes (a `name:` override changes the qname,
/// never the matched symbol); the last qname segment for derived-only nodes
/// (D-060b makes that the host identifier by construction). None means the
/// occurrence test cannot run and the claim can never be Contradicted (G-7).
pub(crate) fn host_identifier<'a>(
    ctx: &'a Ctx,
    input: &'a ReconcileInput,
    target: &QName,
) -> Option<&'a str> {
    if let Some(ident) = input.host_identifiers.get(target) {
        return Some(ident);
    }
    let node = ctx.nodes.get(target)?;
    if node.origin == Origin::Derived {
        return node.qname.0.last().map(String::as_str);
    }
    None
}

/// Whether f's subject span mentions `ident` as a token. None when the span
/// text is unavailable (file missing from sources) — the verdict is then
/// withheld, never guessed (D-066c, G-7).
pub(crate) fn span_mentions(
    ctx: &Ctx,
    input: &ReconcileInput,
    f: &QName,
    ident: &str,
) -> Option<bool> {
    let node = ctx.nodes.get(f)?;
    let text = input.sources.get(&node.loc.file)?;
    let start = node.loc.line.max(1) as usize;
    let end = (node.loc.end_line as usize).max(start);
    Some(
        text.lines()
            .skip(start - 1)
            .take(end + 1 - start)
            .any(|line| contains_token(line, ident)),
    )
}

/// D-066d: token match, not substring — the identifier as a maximal
/// `[A-Za-z0-9_]` run in the raw line text. Comments and strings count:
/// any mention withholds the verdict rather than risking a false alarm.
fn contains_token(line: &str, ident: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    !ident.is_empty()
        && line.match_indices(ident).any(|(i, _)| {
            !line[..i].chars().next_back().is_some_and(is_word)
                && !line[i + ident.len()..].chars().next().is_some_and(is_word)
        })
}

/// D-067: one W0303 per derived Affects edge from an annotated node with no
/// declared Affects claim to the same target, of any status. The span is the
/// write site; the message prints the edge's confidence (G-7). The graph
/// always carries the base Warning — `[policy] undeclared_effects` applies
/// at the lint surface.
pub(crate) fn undeclared_effects(ctx: &Ctx, edges: &[Edge], findings: &mut Vec<OwnedFinding>) {
    let declared: HashSet<(&QName, &QName)> = edges
        .iter()
        .filter(|e| e.layer == Layer::Declared && e.kind == EdgeKind::Affects)
        .map(|e| (&e.from, &e.to))
        .collect();
    for e in edges {
        if e.layer != Layer::Derived
            || e.kind != EdgeKind::Affects
            || !ctx.annotated.contains(&e.from)
            || declared.contains(&(&e.from, &e.to))
        {
            continue;
        }
        let confidence = e.confidence.expect("derived edges carry a confidence");
        findings.push(OwnedFinding::new(
            Finding::new(
                "W0303",
                e.loc.clone(),
                format!(
                    "\"{}\" writes \"{}\" here (derived, {}) but its block declares no \"affects: {}\"; add the clause or remove the write",
                    e.from,
                    e.to,
                    confidence.name(),
                    e.to
                ),
            ),
            &e.from,
        ));
    }
}

/// §9.2 via D-068: the CLI gathered the blame metadata; the graph applies
/// the strictly-later comparison so W0301 gets attribution, strict
/// promotion, and show(X) rendering like any graph finding. Ties are not
/// stale.
pub(crate) fn staleness(records: &[StalenessRecord], findings: &mut Vec<OwnedFinding>) {
    for r in records {
        if r.t_subject > r.t_block {
            let hash = &r.subject_commit[..r.subject_commit.len().min(12)];
            findings.push(OwnedFinding::new(
                Finding::new(
                    "W0301",
                    r.span.clone(),
                    format!(
                        "stale intent on \"{}\": the subject changed at {} (commit {hash}), after this block was last touched at {}; re-read the code and refresh the block",
                        r.qname, r.t_subject_iso, r.t_block_iso
                    ),
                ),
                &r.qname,
            ));
        }
    }
}
