//! `lore ask "<query>"` (§10, T4): parse the query, build the graph from
//! the declared layer, execute over the §10.6 primitives, render §10.3
//! human output or the §10.4 JSON. Failures are usage errors (D-053).

use std::path::Path;
use std::time::Instant;

use lore_graph::exec::{self, Answer, Options};
use lore_graph::query::{self, QueryExpr};

use crate::commands::project;
use crate::output;

// @lore
// name: ask
// purpose: "Answer one intent-graph query with witness chains, exit 2 on a bad question, exit 0 on any honest answer"
// because: "Graph findings never fail ask (D-053b): lint owns the CI surface, ask answers questions even on a project with findings"
// triggers: Graph.build
pub fn run(
    manifest_path: &Path,
    query_text: &str,
    json: bool,
    all: bool,
    max_len: Option<usize>,
    quiet: bool,
    no_color: bool,
) -> i32 {
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (graph, _scan_findings) = project::build_graph(&p, manifest_path);

    let parsed = match query::parse(query_text) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    let started = Instant::now();
    let answer = match exec::ask(&graph, &parsed, &Options { all, max_len }) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let color = !no_color && std::io::IsTerminal::is_terminal(&std::io::stdout());
    let with_unknowns = matches!(parsed.expr, QueryExpr::Unknown { .. });
    match answer {
        Answer::Hits { hits, visited } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output::ask_to_json(
                        &graph,
                        query_text,
                        &hits,
                        with_unknowns,
                        visited,
                        elapsed_ms,
                    ))
                    .expect("ask JSON serializes")
                );
            } else {
                print!(
                    "{}",
                    output::render_ask(&graph, query_text, &hits, with_unknowns, quiet, color)
                );
            }
        }
        Answer::Card(card) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output::card_to_json(
                        &graph, query_text, &card, elapsed_ms,
                    ))
                    .expect("show JSON serializes")
                );
            } else {
                print!("{}", output::render_card(&graph, &card, color));
            }
        }
    }
    0
}
