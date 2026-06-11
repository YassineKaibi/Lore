//! `lore lint` (§12), structural subset at T3: scanner/binder findings,
//! clause parsing, then the lore_graph checks — resolution, applicability,
//! depends_on surface, hygiene, strict promotion. Reconciliation and
//! staleness arrive at T7.

use std::path::Path;

use lore_intent::Severity;

use crate::commands::project;
use crate::output;

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

    let (graph, mut findings) = project::build_graph(&p, manifest_path);
    findings.extend(graph.findings.iter().cloned());
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
