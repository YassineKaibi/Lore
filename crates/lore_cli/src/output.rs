//! Output formatting for scan, lint, and ask results: deterministic human
//! text and the `--json` shapes (§10.4: kinds print capitalized; never color
//! JSON).

use lore_annotations::ScanResult;
use lore_graph::exec::{Card, Hit};
use lore_graph::{ClaimStatus, Edge, Graph};
use lore_intent::{Finding, Intent, QName, Severity};

pub fn to_json(r: &ScanResult) -> serde_json::Value {
    let blocks: Vec<serde_json::Value> = r.blocks.iter().map(|b| {
        serde_json::json!({
            "qname": b.qname.to_string(),
            "kind": b.kind.display(),
            "file": b.file.to_string_lossy(),
            "block_span": {"start": b.block_span.0, "end": b.block_span.1},
            "subject": b.subject,
            "subject_span": b.subject_span.map(|(start, end)| serde_json::json!({"start": start, "end": end})),
            "module": b.module,
        })
    }).collect();
    serde_json::json!({
        "lore_version": env!("CARGO_PKG_VERSION"),
        "blocks": blocks,
        "findings": findings_to_json(&r.findings),
    })
}

fn findings_to_json(findings: &[Finding]) -> Vec<serde_json::Value> {
    findings.iter().map(|f| {
        serde_json::json!({
            "code": f.code,
            "severity": match f.severity { Severity::Error => "error", Severity::Warning => "warning" },
            "file": f.span.file.to_string_lossy(),
            "line": f.span.line,
            "message": f.message,
        })
    }).collect()
}

pub fn lint_to_json(graph: &lore_graph::Graph, findings: &[Finding]) -> serde_json::Value {
    let (errors, warnings) = count(findings);
    serde_json::json!({
        "lore_version": env!("CARGO_PKG_VERSION"),
        "nodes": graph.nodes.len(),
        "edges": graph.edge_count(),
        "findings": findings_to_json(findings),
        "summary": {"errors": errors, "warnings": warnings},
    })
}

pub fn render_lint(
    graph: &lore_graph::Graph,
    findings: &[Finding],
    quiet: bool,
    color: bool,
) -> String {
    let mut out = String::new();
    if !quiet {
        let (errors, warnings) = count(findings);
        out.push_str(&format!(
            "lint: {} nodes, {} edges, {} findings ({errors} errors, {warnings} warnings)\n",
            graph.nodes.len(),
            graph.edge_count(),
            findings.len()
        ));
    }
    render_findings(&mut out, findings, color);
    out
}

fn count(findings: &[Finding]) -> (usize, usize) {
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    (errors, findings.len() - errors)
}

pub fn render_human(r: &ScanResult, files_scanned: usize, quiet: bool, color: bool) -> String {
    let mut out = String::new();
    if !quiet {
        out.push_str(&format!(
            "scan: {} blocks in {} files\n",
            r.blocks.len(),
            files_scanned
        ));
        let rows: Vec<(String, &str, String)> = r
            .blocks
            .iter()
            .map(|b| {
                (
                    b.qname.to_string(),
                    b.kind.name(),
                    format!("{}:{}-{}", b.file.display(), b.block_span.0, b.block_span.1),
                )
            })
            .collect();
        let qname_w = rows.iter().map(|(q, _, _)| q.len()).max().unwrap_or(0);
        let kind_w = rows.iter().map(|(_, k, _)| k.len()).max().unwrap_or(0);
        for (qname, kind, loc) in &rows {
            out.push_str(&format!("{qname:<qname_w$}  {kind:<kind_w$}  {loc}\n"));
        }
        if !r.findings.is_empty() {
            out.push('\n');
        }
    }
    render_findings(&mut out, &r.findings, color);
    out
}

// ---- ask (§10.3 human shape, §10.4 JSON) ----

/// §10.3: header with query and count, then one result per line:
/// `qname  kind  file:line  [via: edge-kind chain]  [status/confidence]`,
/// per-hop labels in chain order (D-054c). The unknown query appends each
/// unknown string indented under its node (D-052d).
pub fn render_ask(
    graph: &Graph,
    query_text: &str,
    hits: &[Hit],
    with_unknowns: bool,
    quiet: bool,
    color: bool,
) -> String {
    let mut out = String::new();
    if !quiet {
        let n = hits.len();
        let plural = if n == 1 { "result" } else { "results" };
        out.push_str(&format!("{query_text}: {n} {plural}\n"));
    }
    let rows: Vec<(String, &str, String)> = hits
        .iter()
        .map(|h| {
            let node = &graph.nodes[&h.qname];
            (
                h.qname.to_string(),
                node.kind.display(),
                format!("{}:{}", node.loc.file.display(), node.loc.line),
            )
        })
        .collect();
    let qname_w = rows.iter().map(|(q, _, _)| q.len()).max().unwrap_or(0);
    let kind_w = rows.iter().map(|(_, k, _)| k.len()).max().unwrap_or(0);
    for (hit, (qname, kind, loc)) in hits.iter().zip(&rows) {
        out.push_str(&format!("{qname:<qname_w$}  {kind:<kind_w$}  {loc}"));
        if !hit.via.is_empty() {
            let chain: Vec<&str> = hit.via.iter().map(|e| e.kind.name()).collect();
            let labels: Vec<String> = hit.via.iter().map(|e| edge_label(e, color)).collect();
            out.push_str(&format!(
                "  [via: {}]  [{}]",
                chain.join(" -> "),
                labels.join(" -> ")
            ));
        }
        out.push('\n');
        if with_unknowns {
            for u in &graph.nodes[&hit.qname].intent.unknown {
                out.push_str(&format!("  unknown: {}\n", quote(&u.value)));
            }
        }
    }
    let unresolved = unresolved_refs(graph);
    if !quiet && !unresolved.is_empty() {
        // G-7: the answer may be incomplete; say so on every human response.
        out.push_str(&format!(
            "note: {} unresolved ref{} in the graph (run lore lint): {}\n",
            unresolved.len(),
            if unresolved.len() == 1 { "" } else { "s" },
            unresolved.join(", ")
        ));
    }
    out
}

pub fn ask_to_json(
    graph: &Graph,
    query_text: &str,
    hits: &[Hit],
    with_unknowns: bool,
    visited: usize,
    elapsed_ms: u64,
) -> serde_json::Value {
    let results: Vec<serde_json::Value> = hits
        .iter()
        .map(|h| {
            let node = &graph.nodes[&h.qname];
            let via: Vec<serde_json::Value> = h.via.iter().map(edge_to_json).collect();
            let mut r = serde_json::json!({
                "qname": h.qname.to_string(),
                "kind": node.kind.display(),
                "location": {"file": node.loc.file.to_string_lossy(), "line": node.loc.line},
                "via": via,
            });
            if with_unknowns {
                let unknowns: Vec<&str> = node
                    .intent
                    .unknown
                    .iter()
                    .map(|u| u.value.as_str())
                    .collect();
                r["unknown"] = serde_json::json!(unknowns);
            }
            r
        })
        .collect();
    serde_json::json!({
        "lore_version": env!("CARGO_PKG_VERSION"),
        "query": query_text,
        "results": results,
        "unresolved": unresolved_refs(graph),
        "stats": {"nodes_visited": visited, "elapsed_ms": elapsed_ms},
    })
}

/// §10.2 node card: qname, kind, origin, location, every clause verbatim,
/// in/out edges grouped by kind with layer + status/confidence, findings.
pub fn render_card(graph: &Graph, card: &Card, color: bool) -> String {
    let node = &graph.nodes[&card.qname];
    let mut out = format!(
        "{}  {}  {}  {}:{}\n",
        card.qname,
        node.kind.display(),
        origin_name(node),
        node.loc.file.display(),
        node.loc.line
    );
    let clauses = intent_lines(&node.intent);
    if clauses.is_empty() {
        out.push_str("  no declared intent\n");
    }
    for line in clauses {
        out.push_str(&format!("  {line}\n"));
    }
    if !card.edges_out.is_empty() {
        out.push_str("edges out:\n");
        for e in &card.edges_out {
            out.push_str(&format!(
                "  {} -> {}  [{}]\n",
                e.kind.name(),
                e.to,
                edge_label(e, color)
            ));
        }
    }
    if !card.edges_in.is_empty() {
        out.push_str("edges in:\n");
        for e in &card.edges_in {
            out.push_str(&format!(
                "  {} <- {}  [{}]\n",
                e.kind.name(),
                e.from,
                edge_label(e, color)
            ));
        }
    }
    if !card.findings.is_empty() {
        out.push_str("findings:\n");
        let mut text = String::new();
        render_findings(&mut text, &card.findings, color);
        for line in text.lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }
    out
}

pub fn card_to_json(
    graph: &Graph,
    query_text: &str,
    card: &Card,
    elapsed_ms: u64,
) -> serde_json::Value {
    let node = &graph.nodes[&card.qname];
    serde_json::json!({
        "lore_version": env!("CARGO_PKG_VERSION"),
        "query": query_text,
        "node": {
            "qname": card.qname.to_string(),
            "kind": node.kind.display(),
            "origin": origin_name(node),
            "location": {"file": node.loc.file.to_string_lossy(), "line": node.loc.line},
            "intent": intent_to_json(&node.intent),
        },
        "edges_in": card.edges_in.iter().map(edge_to_json).collect::<Vec<_>>(),
        "edges_out": card.edges_out.iter().map(edge_to_json).collect::<Vec<_>>(),
        "findings": findings_to_json(&card.findings),
        "unresolved": unresolved_refs(graph),
        "stats": {"nodes_visited": 1, "elapsed_ms": elapsed_ms},
    })
}

fn origin_name(node: &lore_intent::IntentNode) -> &'static str {
    match node.origin {
        lore_intent::Origin::Declared => "Declared",
        lore_intent::Origin::Derived => "Derived",
        lore_intent::Origin::Both => "Both",
    }
}

/// One edge in the §10.4 via shape: status iff declared, confidence iff
/// derived — exactly one of the two (§6.2).
fn edge_to_json(e: &Edge) -> serde_json::Value {
    let mut v = serde_json::json!({
        "from": e.from.to_string(),
        "to": e.to.to_string(),
        "edge": e.kind.name(),
        "layer": e.layer.name(),
    });
    match (e.status, e.confidence) {
        (Some(s), _) => v["status"] = serde_json::json!(s.name()),
        (_, Some(c)) => v["confidence"] = serde_json::json!(c.name()),
        _ => unreachable!("§6.2: every edge carries exactly one of status/confidence"),
    }
    v
}

/// `Declared/Unverifiable` or `Derived/Exact` — the per-hop trust label
/// (§6.4, D-054c). Claim statuses are colored on a TTY.
fn edge_label(e: &Edge, color: bool) -> String {
    let trust = match (e.status, e.confidence) {
        (Some(s), _) if color => {
            let code = match s {
                ClaimStatus::Verified => "32",     // green
                ClaimStatus::Unverified => "33",   // yellow
                ClaimStatus::Contradicted => "31", // red
                ClaimStatus::Unverifiable => "36", // cyan
            };
            format!("\x1b[{code}m{}\x1b[0m", s.name())
        }
        (Some(s), _) => s.name().to_string(),
        (_, Some(c)) => c.name().to_string(),
        _ => unreachable!("§6.2: every edge carries exactly one of status/confidence"),
    };
    format!("{}/{trust}", e.layer.name())
}

/// The graph's unresolved declared refs: the §10.4 `unresolved` array,
/// derived from E0306 findings (D-047c, D-053c). The message shape is pinned
/// by lore_graph's tests: `unresolved ref "<ref>" in ...`.
fn unresolved_refs(graph: &Graph) -> Vec<String> {
    let mut refs: Vec<String> = graph
        .findings
        .iter()
        .filter(|f| f.code == "E0306")
        .filter_map(|f| {
            let rest = f.message.strip_prefix("unresolved ref \"")?;
            Some(rest.split('"').next()?.to_string())
        })
        .collect();
    refs.sort();
    refs.dedup();
    refs
}

/// Every clause verbatim (§10.2), reconstructed in §3.1 clause order.
fn intent_lines(intent: &Intent) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(p) = &intent.purpose {
        lines.push(format!("purpose: {}", quote(&p.value)));
    }
    if let Some(o) = &intent.owner {
        lines.push(format!("owner: {}", quote(&o.value)));
    }
    for b in &intent.because {
        lines.push(format!("because: {}", quote(&b.value)));
    }
    for u in &intent.unknown {
        lines.push(format!("unknown: {}", quote(&u.value)));
    }
    for a in &intent.assumes {
        lines.push(format!("assumes: {}", quote(&a.value)));
    }
    for (name, refs) in [
        ("affects", &intent.affects),
        ("reads", &intent.reads),
        ("triggers", &intent.triggers),
        ("emits", &intent.emits),
        ("on", &intent.on),
        ("depends_on", &intent.depends_on),
    ] {
        if !refs.is_empty() {
            let joined: Vec<String> = refs
                .iter()
                .map(|r| QName(r.value.segments.clone()).to_string())
                .collect();
            lines.push(format!("{name}: {}", joined.join(", ")));
        }
    }
    if let Some(r) = &intent.route {
        let method = r.value.method.map(method_name);
        lines.push(match method {
            Some(m) => format!("route: {m} {}", quote(&r.value.path)),
            None => format!("route: {}", quote(&r.value.path)),
        });
    }
    if let Some(e) = &intent.enforcement {
        let level = match e.value {
            lore_intent::Enforcement::Strict => "strict",
            lore_intent::Enforcement::Warn => "warn",
        };
        lines.push(format!("enforcement: {level}"));
    }
    lines
}

fn intent_to_json(intent: &Intent) -> serde_json::Value {
    let prose = |v: &[lore_intent::Spanned<String>]| -> Vec<String> {
        v.iter().map(|s| s.value.clone()).collect()
    };
    let refs = |v: &[lore_intent::Spanned<lore_intent::Ref>]| -> Vec<String> {
        v.iter()
            .map(|r| QName(r.value.segments.clone()).to_string())
            .collect()
    };
    serde_json::json!({
        "purpose": intent.purpose.as_ref().map(|s| s.value.clone()),
        "owner": intent.owner.as_ref().map(|s| s.value.clone()),
        "because": prose(&intent.because),
        "unknown": prose(&intent.unknown),
        "assumes": prose(&intent.assumes),
        "affects": refs(&intent.affects),
        "reads": refs(&intent.reads),
        "triggers": refs(&intent.triggers),
        "emits": refs(&intent.emits),
        "on": refs(&intent.on),
        "depends_on": refs(&intent.depends_on),
        "route": intent.route.as_ref().map(|r| match r.value.method.map(method_name) {
            Some(m) => format!("{m} {}", r.value.path),
            None => r.value.path.clone(),
        }),
        "enforcement": intent.enforcement.as_ref().map(|e| match e.value {
            lore_intent::Enforcement::Strict => "strict",
            lore_intent::Enforcement::Warn => "warn",
        }),
    })
}

fn method_name(m: lore_intent::HttpMethod) -> &'static str {
    match m {
        lore_intent::HttpMethod::Get => "GET",
        lore_intent::HttpMethod::Post => "POST",
        lore_intent::HttpMethod::Put => "PUT",
        lore_intent::HttpMethod::Delete => "DELETE",
        lore_intent::HttpMethod::Patch => "PATCH",
    }
}

/// Re-quote a prose value with D-045b escapes inverted.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn render_findings(out: &mut String, findings: &[Finding], color: bool) {
    for f in findings {
        let code = if color {
            match f.severity {
                Severity::Error => format!("\x1b[31m{}\x1b[0m", f.code),
                Severity::Warning => format!("\x1b[33m{}\x1b[0m", f.code),
            }
        } else {
            f.code.to_string()
        };
        out.push_str(&format!(
            "{code} {}:{}  {}\n",
            f.span.file.display(),
            f.span.line,
            f.message
        ));
    }
}
