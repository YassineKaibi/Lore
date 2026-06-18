//! `veridikt mcp` (§12, D-037/D-079): a Model Context Protocol server over the
//! stdio transport — newline-delimited JSON-RPC 2.0 on stdin/stdout, one
//! message per line. Read-only by construction: it exposes exactly the four
//! query/report surfaces (`veridikt_ask`, `veridikt_show`, `veridikt_lint`,
//! `veridikt_history`), each returning the same JSON its `--json` CLI form does,
//! and never mutates the project. Hand-rolled over serde_json — no MCP-SDK
//! dependency (the wire format is a small JSON-RPC loop Veridikt owns outright).

use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};

use veridikt_graph::exec::{self, Answer, Options};
use veridikt_graph::query::{self, QueryExpr};

use crate::commands::{history, lint, project};
use crate::output;

/// Default MCP protocol version (D-079a). The server echoes the client's
/// requested version when present, so a newer client negotiates its own.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// Server-level usage guidance (D-080): the MCP-standard `initialize`
/// `instructions` string, which clients fold into the model's system prompt.
/// It travels with the server to every connected agent — so "when to use
/// veridikt" lives here, not in any one project's instruction file.
const INSTRUCTIONS: &str = "\
Veridikt serves this project's intent graph: a map, reconciled against the actual \
code, of what each construct does, what state it writes (affects) and reads, \
what it calls and reaches, what it depends on, who owns it, and why it \
changed — every answer labeled with how far it can be trusted (declared \
claims as Verified/Unverified/Contradicted/Unverifiable; derived edges as \
Exact/Resolved/Heuristic).

Consult these tools BEFORE reading or grepping source to answer such \
questions: the graph gives a precise, trust-labeled answer faster and more \
reliably than scanning files. Map the question to a tool:
- what does <node> do / what is it for -> veridikt_show(qname)
- what writes / reads <state> -> veridikt_ask query=\"affects(<state>)\" / \"reads(<state>)\"
- what does <fn> affect or read -> veridikt_ask query=\"touches(<fn>)\"
- what calls <fn> / what <fn> calls -> veridikt_ask query=\"callers(<fn>)\" / \"callees(<fn>)\"
- everything <fn> transitively reaches -> veridikt_ask query=\"reaches(<fn>)\"
- what depends on <x> / what does <x> depend on -> veridikt_ask query=\"dependents(<x>)\" / \"depends(<x>)\"
- who owns <area> -> veridikt_ask query=\"owner(\\\"<team>\\\")\"
- why did <node> change -> veridikt_history(qname)
- is anything drifted, stale, or contradicted -> veridikt_lint

Names are qualified (e.g. Payment.charge); a wrong name returns the nearest \
match. All tools are read-only. Fall back to reading source only when the \
graph does not cover the question.";

// @veridikt
// name: mcp
// purpose: "Serve veridikt's four read-only query/report surfaces over MCP stdio so coding agents can read the intent graph (D-037, D-079)"
// because: "The intent graph is exactly the context agents lack; feeding it over MCP is the adoption wedge, and read-only construction keeps the tool honest (G-7)"
pub fn run(manifest_path: &Path) -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // One JSON-RPC message per line (stdio transport: no embedded newlines).
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // EOF / closed pipe ends the session
        };
        if line.trim().is_empty() {
            continue;
        }
        // Notifications (no `id`) produce no response, hence the Option.
        if let Some(response) = handle_line(manifest_path, &line) {
            let s = serde_json::to_string(&response).expect("response serializes");
            if writeln!(out, "{s}").is_err() || out.flush().is_err() {
                break;
            }
        }
    }
    0
}

/// Dispatch one JSON-RPC message. `None` means "no response" — either a
/// notification or an unparseable notification-shaped line. Malformed JSON is
/// a `-32700` error against a null id (D-079a).
fn handle_line(manifest_path: &Path, line: &str) -> Option<Value> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            ));
        }
    };
    let is_notification = msg.get("id").is_none();
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    match method {
        // The `initialized` notification (and any other notification) is
        // acknowledged by silence — JSON-RPC forbids responding to one.
        "notifications/initialized" => None,
        _ if is_notification => None,
        "initialize" => Some(success(id, initialize_result(&params))),
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, tools_list())),
        "tools/call" => Some(tools_call(manifest_path, id, &params)),
        other => Some(error_response(
            id,
            -32601,
            &format!("method not found: {other}"),
        )),
    }
}

/// `initialize` result (D-079a): advertise the tools capability and echo the
/// client's protocol version when it sent a string one.
fn initialize_result(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "veridikt", "version": env!("CARGO_PKG_VERSION")},
        // D-080: system-prompt-level guidance so every connected agent learns
        // when to reach for these tools, not just that they exist.
        "instructions": INSTRUCTIONS,
    })
}

/// The four read-only tools (D-037/D-079b). Descriptions are written for
/// tool-selection: an agent asking "what writes to the ledger?" must land on
/// `veridikt_ask` with `affects(...)` (the T9 eval).
fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "veridikt_ask",
                "description": "Prefer this over reading or grepping source to trace effects and relationships in this project. Queries the Veridikt intent graph and returns the answer with witness chains and per-hop trust labels (Verified/Unverified/Contradicted/Unverifiable for declared edges, Exact/Resolved/Heuristic for derived). Query forms: affects(State) — what writes a piece of state; reads(State) — what reads it; touches(Fn) — what state a function affects or reads; callers(Fn) — who calls it / callees(Fn) — what it calls / reaches(X) — call reachability; emits(Event)/handlers(Event) — event producers and consumers; depends(X)/dependents(X); path(A, B); owner(\"team\"); unknown — open questions. Example: to answer \"what writes to the ledger?\" pass query = \"affects(Payment.ledger)\".",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "A Veridikt query, e.g. affects(Payment.ledger) or path(A, B)."},
                        "all": {"type": "boolean", "description": "For path(A, B): enumerate all simple paths instead of the shortest. Requires max_len."},
                        "max_len": {"type": "integer", "description": "Bound for all: maximum path length in edges.", "minimum": 1}
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "veridikt_show",
                "description": "Prefer this over reading the source file to understand a single construct: shows the full node card for one qualified name — its kind, origin, every declared intent clause (purpose, because, owner, assumes, unknown, ...), and all incoming/outgoing edges with their layer and trust label. Answers \"what is X for and how does it connect\". Works for derived-only nodes too (the card simply shows no declared intent).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "qname": {"type": "string", "description": "The node's qualified name, e.g. Payment.charge or Payment.ledger."}
                    },
                    "required": ["qname"]
                }
            },
            {
                "name": "veridikt_lint",
                "description": "Prefer this over manually auditing annotations: runs the full Veridikt lint over the project — structural resolution, claim reconciliation (which declared effects are Verified vs Contradicted by the code), staleness, and hygiene — returning every finding with its diagnostic code, severity, and location. Use it to find drift between declared intent and the actual code, or to gauge annotation health. Takes no arguments.",
                "inputSchema": {"type": "object", "properties": {}}
            },
            {
                "name": "veridikt_history",
                "description": "Prefer this over running git log yourself to recover WHY a construct changed: renders the git change history of a node's subject span — the commits (hash, author, date, full message) that touched the code behind a qualified name, straight from commit messages.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "qname": {"type": "string", "description": "The node's qualified name, e.g. Payment.charge."}
                    },
                    "required": ["qname"]
                }
            }
        ]
    })
}

/// Dispatch a `tools/call` (D-079b/d). A bad question (unparseable query, a
/// qname naming no node, git unavailable) becomes a *tool error* — isError on
/// the result, not a JSON-RPC error — so it never tears down the session,
/// mirroring the CLI's exit-2-not-crash discipline (D-053b). Only a malformed
/// request shape (no tool name, non-object arguments, unknown tool) is a
/// protocol error.
fn tools_call(manifest_path: &Path, id: Value, params: &Value) -> Value {
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !args.is_object() {
        return error_response(id, -32602, "tools/call arguments must be an object");
    }
    let result = match params.get("name").and_then(Value::as_str) {
        Some("veridikt_ask") => run_ask(manifest_path, &args),
        Some("veridikt_show") => run_show(manifest_path, &args),
        Some("veridikt_lint") => run_lint(manifest_path),
        Some("veridikt_history") => run_history(manifest_path, &args),
        Some(other) => return error_response(id, -32602, &format!("unknown tool: {other}")),
        None => return error_response(id, -32602, "tools/call requires a tool name"),
    };
    match result {
        Ok(value) => success(id, tool_ok(value)),
        Err(message) => success(id, tool_error(&message)),
    }
}

/// `veridikt_ask`: the §10.4 JSON for an arbitrary query, with optional `all` /
/// `max_len` mirroring `path --all --max-len N`.
fn run_ask(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let query_text = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or("veridikt_ask requires a string \"query\" argument")?;
    let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
    let max_len = args
        .get("max_len")
        .and_then(Value::as_u64)
        .map(|n| n as usize);
    ask_query(manifest_path, query_text, all, max_len)
}

/// `veridikt_show`: the §10.2 card, implemented as the `show(<qname>)` query so the
/// card shape is byte-identical to `veridikt ask`.
fn run_show(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let qname = args
        .get("qname")
        .and_then(Value::as_str)
        .ok_or("veridikt_show requires a string \"qname\" argument")?;
    ask_query(manifest_path, &format!("show({qname})"), false, None)
}

/// Shared core of `veridikt_ask` and `veridikt_show`: build the graph, parse, execute,
/// and shape the §10.4 / §10.2 JSON. Parse and execution failures are tool
/// errors (D-079d).
fn ask_query(
    manifest_path: &Path,
    query_text: &str,
    all: bool,
    max_len: Option<usize>,
) -> Result<Value, String> {
    let p = load_project(manifest_path)?;
    let graph = project::build_graph(&p, manifest_path, false, true).graph;

    let parsed = query::parse(query_text).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let answer = exec::ask(&graph, &parsed, &Options { all, max_len })?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let with_unknowns = matches!(parsed.expr, QueryExpr::Unknown { .. });

    Ok(match answer {
        Answer::Hits { hits, visited } => output::ask_to_json(
            &graph,
            query_text,
            &hits,
            with_unknowns,
            visited,
            elapsed_ms,
        ),
        Answer::Card(card) => output::card_to_json(&graph, query_text, &card, elapsed_ms),
    })
}

/// `veridikt_lint`: the §12 lint JSON, full reconciliation with staleness on
/// (no_stale = false, D-079b). Identical policy/override processing as
/// `veridikt lint --json`, via the shared `lint::compute`.
fn run_lint(manifest_path: &Path) -> Result<Value, String> {
    let p = load_project(manifest_path)?;
    let (graph, findings) = lint::compute(&p, manifest_path, false, true);
    Ok(output::lint_to_json(&graph, &findings))
}

/// `veridikt_history`: the §9.3 history JSON. The D-059 failure modes (unknown
/// node, git unrunnable) arrive from `history::collect` as the exact CLI
/// stderr text; trim the trailing newline for the tool-error message.
fn run_history(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let qname = args
        .get("qname")
        .and_then(Value::as_str)
        .ok_or("veridikt_history requires a string \"qname\" argument")?;
    let p = load_project(manifest_path)?;
    let (file, start, end, commits) =
        history::collect(&p, manifest_path, qname, true).map_err(|e| e.trim_end().to_string())?;
    Ok(output::history_to_json(qname, &file, start, end, &commits))
}

/// Load the project fresh per call (D-079e: answers track edits during a
/// long-lived session). `project::load` prints manifest specifics to stderr,
/// which is off the JSON-RPC channel; the tool error carries a short message.
fn load_project(manifest_path: &Path) -> Result<project::Project, String> {
    project::load(manifest_path).map_err(|_| {
        format!(
            "no usable veridikt.toml at {} (see stderr); run veridikt init",
            manifest_path.display()
        )
    })
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A successful tool result (D-079c): the JSON as a text content block plus
/// `structuredContent`, the text byte-for-byte the corresponding `--json`.
fn tool_ok(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).expect("tool JSON serializes");
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": value,
        "isError": false,
    })
}

fn tool_error(message: &str) -> Value {
    json!({
        "content": [{"type": "text", "text": message}],
        "isError": true,
    })
}
