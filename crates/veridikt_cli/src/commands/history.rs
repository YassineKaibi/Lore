//! `veridikt history <qname>` (§9.3, D-059): render `git log -L` over the
//! node's subject span — hash, date, author, full message. This is the
//! VCS-derived replacement for the removed `changed:` clause (D-004).

use std::path::Path;
use std::process::Command;

use veridikt_intent::QName;

use crate::commands::project;
use crate::output;

/// One commit touching the subject span, oldest field layout per D-059d.
pub struct Commit {
    pub hash: String,
    pub author: String,
    pub date: String, // ISO-strict author date
    pub message: String,
}

// @veridikt
// name: history
// purpose: "Render the git change history of one node's subject span: the why behind the code, recovered from commit messages"
// because: "Hand-maintained version/changed clauses drift; git already records change intent, so veridikt renders it instead (D-004)"
// because: "history builds the graph without staleness metadata: it reads git itself for the one span it renders (D-068c)"
pub fn run(manifest_path: &Path, qname: &str, json: bool, quiet: bool) -> i32 {
    let p = match project::load(manifest_path) {
        Ok(p) => p,
        Err(code) => return code,
    };

    let (file, start, end, commits) = match collect(&p, manifest_path, qname, quiet) {
        Ok(h) => h,
        Err(e) => {
            eprint!("{e}");
            return 2;
        }
    };
    let file = file.as_path();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output::history_to_json(
                qname, file, start, end, &commits
            ))
            .expect("history JSON serializes")
        );
    } else {
        print!(
            "{}",
            output::render_history(qname, file, start, end, &commits, quiet)
        );
    }
    0
}

/// Build the graph, resolve the node, and render `git log -L` over its
/// subject span. Returns `(file, start, end, commits)` or, on the D-059
/// failure modes (unknown node, git unrunnable, git error), the exact
/// message text `run` prints to stderr — so the `veridikt_history` MCP tool
/// (D-079) reports the same failures as tool errors without re-deriving them.
pub fn collect(
    p: &project::Project,
    manifest_path: &Path,
    qname: &str,
    quiet: bool,
) -> Result<(std::path::PathBuf, u32, u32, Vec<Commit>), String> {
    let graph = project::build_graph(p, manifest_path, false, quiet).graph;

    // D-059a: the argument must name a node; mirror ask's D-053a failure.
    let node = veridikt_graph::exec::lookup(&graph, &QName::from_dotted(qname))
        .map_err(|e| format!("{e}\n"))?;
    let (file, start, end) = (node.loc.file.clone(), node.loc.line, node.loc.end_line);

    // D-059b: -s suppresses the patch; %x1f/%x1e are field/record breaks.
    let root = manifest_path.parent().unwrap_or(Path::new("."));
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            "-s",
            "--date=iso-strict",
            "--format=%H%x1f%an%x1f%ad%x1f%B%x1e",
            &format!("-L{start},{end}:{}", file.display()),
        ])
        .output();
    let out = match out {
        Ok(o) => o,
        // D-059c: no git, no answer — unlike staleness, there is nothing
        // honest to render without the repository.
        Err(e) => {
            return Err(format!(
                "veridikt history needs git, which could not be run: {e}\n"
            ));
        }
    };
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }

    let commits = parse_log(&String::from_utf8_lossy(&out.stdout));
    Ok((file, start, end, commits))
}

/// Split the %x1e-terminated records into commits. An empty log is an
/// honest empty answer (D-059c), not an error.
fn parse_log(stdout: &str) -> Vec<Commit> {
    stdout
        .split('\x1e')
        .filter_map(|record| {
            let mut fields = record.trim_start_matches(['\n', ' ']).split('\x1f');
            let hash = fields.next()?.trim().to_string();
            if hash.is_empty() {
                return None;
            }
            Some(Commit {
                hash,
                author: fields.next()?.to_string(),
                date: fields.next()?.to_string(),
                message: fields.next()?.trim_end_matches('\n').to_string(),
            })
        })
        .collect()
}
