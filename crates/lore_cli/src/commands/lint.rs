//! `lore lint` (§12), structural subset at T3, hardened for CI at T5, full
//! at T7: scanner/binder findings, clause parsing, then the lore_graph
//! checks — resolution with four-status reconciliation, applicability,
//! depends_on surface, hygiene, CODEOWNERS, undeclared effects, staleness,
//! strict promotion — then `[policy]` promotion and `[lint]` overrides
//! (D-056, D-057, D-067, D-068).

use std::path::Path;

use lore_intent::Severity;

use crate::commands::project;
use crate::output;
use lore_cli::manifest::{LintLevel, PolicyLevel, UndeclaredEffects};

// @lore
// name: lint
// purpose: "Project-wide lint: every scanner, parser, graph, and reconciliation finding with §10.5 exit codes"
// because: "Lint is where drift becomes a CI finding instead of a silent decay: contradicted claims and stale blocks fail loudly here (D-019)"
// triggers: Annotations.scan, Intent.parse_intent, Graph.build
pub fn run(manifest_path: &Path, json: bool, no_stale: bool, quiet: bool, no_color: bool) -> i32 {
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };

    // D-068c: lint is the one command that gathers blame metadata.
    let built = project::build_graph(&p, manifest_path, !no_stale, quiet);
    let graph = built.graph;
    let mut findings = built.findings;
    findings.extend(graph.findings.iter().cloned());

    // D-057: [policy] unknown = "error" promotes W0213 (code unchanged,
    // mirroring D-049). Applied here — the policy lives in the manifest,
    // so the graph carries the base Warning. D-068e: [policy] stale does
    // the same for W0301.
    if matches!(p.manifest.policy.unknown, PolicyLevel::Error) {
        for f in findings.iter_mut().filter(|f| f.code == "W0213") {
            f.severity = Severity::Error;
        }
    }
    if matches!(p.manifest.policy.stale, PolicyLevel::Error) {
        for f in findings.iter_mut().filter(|f| f.code == "W0301") {
            f.severity = Severity::Error;
        }
    }

    // D-067b: undeclared effects are off by default — they punish low
    // coverage (D-019). The graph always carries them; ask/show still
    // render them unfiltered (D-056c).
    if matches!(p.manifest.policy.undeclared_effects, UndeclaredEffects::Off) {
        findings.retain(|f| f.code != "W0303");
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
