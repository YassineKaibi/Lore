// @lore
// kind: module
// name: Cli
// purpose: "Single lore binary: manifest discovery, command wiring, output shaping, §10.5 exit codes"
// owner: "lore-core"
// depends_on: Intent, Annotations, Graph

//! `lore` binary: clap wiring and the §10.5 panic boundary.

mod commands;
mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lore", version)]
struct Cli {
    /// Path to lore.toml (skips discovery)
    #[arg(long, global = true)]
    manifest: Option<PathBuf>,
    /// Print findings only
    #[arg(long, global = true)]
    quiet: bool,
    /// Never emit ANSI color codes
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write a starter lore.toml: detect languages, propose [modules] globs
    Init,
    /// Scanner+binder only: list every annotation block, its subject, qname, kind
    Scan {
        #[arg(long)]
        json: bool,
    },
    /// Structural lint: resolution, required intent, applicability, depends_on
    /// surface, hygiene; exit 1 on error-severity findings
    Lint {
        #[arg(long)]
        json: bool,
        /// Skip staleness checks (no effect until T7 ships them)
        #[arg(long)]
        no_stale: bool,
    },
    /// Answer a §10 query over the intent graph, e.g. 'affects*(Payment.ledger)'
    Ask {
        /// The query text (§10.1 grammar)
        query: String,
        #[arg(long)]
        json: bool,
        /// path(A, B): enumerate all simple paths instead of the shortest
        #[arg(long, requires = "max_len")]
        all: bool,
        /// Bound for --all: maximum path length in edges
        #[arg(long, value_name = "N")]
        max_len: Option<usize>,
    },
    /// Render the git change history of a node's subject span (§9.3)
    History {
        /// The node's qualified name, e.g. Payment.charge
        qname: String,
        #[arg(long)]
        json: bool,
    },
    /// Coverage counts: nodes by kind/origin, declared intent per kind,
    /// unresolved_calls and ambiguous_derived_names (D-065)
    Stats {
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    // §10.5: panics are an internal error, exit 3 — never a raw backtrace.
    std::panic::set_hook(Box::new(|_| {}));
    let code =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(cli))).unwrap_or_else(|_| {
            eprintln!("internal error: this is a bug in lore");
            3
        });
    std::process::exit(code);
}

fn run(cli: Cli) -> i32 {
    match cli.command {
        Command::Init => commands::init::run(),
        Command::Scan { json } => match discover_manifest(&cli) {
            Some(path) => commands::scan::run(&path, json, cli.quiet, cli.no_color),
            None => 2,
        },
        Command::Lint { json, no_stale } => match discover_manifest(&cli) {
            Some(path) => commands::lint::run(&path, json, no_stale, cli.quiet, cli.no_color),
            None => 2,
        },
        Command::Ask {
            ref query,
            json,
            all,
            max_len,
        } => match discover_manifest(&cli) {
            Some(path) => {
                commands::ask::run(&path, query, json, all, max_len, cli.quiet, cli.no_color)
            }
            None => 2,
        },
        Command::History { ref qname, json } => match discover_manifest(&cli) {
            Some(path) => commands::history::run(&path, qname, json, cli.quiet),
            None => 2,
        },
        Command::Stats { json } => match discover_manifest(&cli) {
            Some(path) => commands::stats::run(&path, json, cli.quiet),
            None => 2,
        },
    }
}

/// `--manifest` or walk up from CWD; E0402 on stderr when nothing is found.
fn discover_manifest(cli: &Cli) -> Option<PathBuf> {
    if let Some(p) = &cli.manifest {
        return Some(p.clone());
    }
    let cwd = std::env::current_dir().expect("cwd must exist");
    let found = lore_cli::manifest::discover(&cwd);
    if found.is_none() {
        eprintln!(
            "E0402 no lore.toml found between {} and the filesystem root; run \"lore init\" to create one",
            cwd.display()
        );
    }
    found
}
