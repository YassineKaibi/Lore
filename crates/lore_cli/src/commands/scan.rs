//! `lore scan` (§12): manifest-driven file walk, then the lore_annotations
//! pipeline. Exit codes per §10.5.

use std::path::Path;

use lore_annotations::ScanConfig;
use lore_intent::Severity;

use crate::commands::project;
use crate::output;

pub fn run(manifest_path: &Path, json: bool, quiet: bool, no_color: bool) -> i32 {
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let files_scanned = p.sources.len();

    let config = ScanConfig {
        modules: p.manifest.modules.clone(),
    };
    let result = lore_annotations::scan(&config, &p.sources);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::to_json(&result)).expect("scan JSON serializes")
        );
    } else {
        let color = !no_color && std::io::IsTerminal::is_terminal(&std::io::stdout());
        print!(
            "{}",
            output::render_human(&result, files_scanned, quiet, color)
        );
    }

    if result
        .findings
        .iter()
        .any(|f| f.severity == Severity::Error)
    {
        1
    } else {
        0
    }
}
