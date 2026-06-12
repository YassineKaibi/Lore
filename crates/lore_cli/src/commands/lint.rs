//! `lore lint` (§12), structural subset at T3, hardened for CI at T5:
//! scanner/binder findings, clause parsing, then the lore_graph checks —
//! resolution, applicability, depends_on surface, hygiene, CODEOWNERS,
//! strict promotion — then `[policy]` promotion and `[lint]` overrides
//! (D-056, D-057). Reconciliation and staleness arrive at T7.

use std::path::Path;

use lore_intent::Severity;

use crate::commands::project;
use crate::output;
use lore_cli::manifest::{LintLevel, PolicyLevel};

// @lore
// name: lint
// purpose: "Project-wide structural lint: every scanner, parser, and graph finding with §10.5 exit codes"
// triggers: Annotations.scan, Intent.parse_intent, Graph.build
pub fn run(manifest_path: &Path, json: bool, no_stale: bool, quiet: bool, no_color: bool) -> i32 {
    let _ = no_stale; // staleness checks land at T7 (§9.2); the flag is accepted now per §12
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };

    let built = project::build_graph(&p, manifest_path);
    let graph = built.graph;
    let mut findings = built.findings;
    findings.extend(graph.findings.iter().cloned());

    // D-057: [policy] unknown = "error" promotes W0213 (code unchanged,
    // mirroring D-049). Applied here — the policy lives in the manifest,
    // so the graph carries the base Warning.
    if matches!(p.manifest.policy.unknown, PolicyLevel::Error) {
        for f in findings.iter_mut().filter(|f| f.code == "W0213") {
            f.severity = Severity::Error;
        }
    }

    // D-056: [lint] overrides, after promotion. "off" suppresses the code
    // everywhere, including promoted instances; "warn" restates the default.
    let off: Vec<&str> = p
        .manifest
        .lint_overrides
        .iter()
        .filter(|(_, level)| *level == LintLevel::Off)
        .map(|(code, _)| code.as_str())
        .collect();
    findings.retain(|f| !off.contains(&f.code));

    findings.sort_by(|a, b| {
        (&a.span.file, a.span.line, a.span.col, a.code, &a.message).cmp(&(
            &b.span.file,
            b.span.line,
            b.span.col,
            b.code,
            &b.message,
        ))
    });

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::lint_to_json(&graph, &findings))
                .expect("lint JSON serializes")
        );
    } else {
        let color = !no_color && std::io::IsTerminal::is_terminal(&std::io::stdout());
        print!("{}", output::render_lint(&graph, &findings, quiet, color));
    }

    if findings.iter().any(|f| f.severity == Severity::Error) {
        1
    } else {
        0
    }
}
