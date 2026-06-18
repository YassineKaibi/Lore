//! `veridikt mcp` (T9, D-037/D-079): boundary tests over the stdio JSON-RPC
//! transport. A session writes newline-delimited requests, closes stdin, and
//! the server's response lines are matched by id. Unhappy paths first (G-11):
//! a malformed request is a protocol error, a bad *question* is a tool error.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// Run one `veridikt mcp` session: write every request line, close stdin (EOF
/// ends the loop), and return the response lines parsed as JSON.
fn mcp_session(dir: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_veridikt"))
        .arg("mcp")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for r in requests {
            writeln!(stdin, "{}", serde_json::to_string(r).unwrap()).unwrap();
        }
        // stdin dropped here -> EOF -> the server's read loop ends.
    }
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Send raw lines (for malformed-JSON cases the helper above can't express).
fn mcp_raw(dir: &Path, lines: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_veridikt"))
        .arg("mcp")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for l in lines {
            writeln!(stdin, "{l}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn call(name: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call",
           "params": {"name": name, "arguments": args}})
}

fn by_id(responses: &[Value], id: i64) -> &Value {
    responses
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap_or_else(|| panic!("no response with id {id} in {responses:?}"))
}

// ---- unhappy paths first (G-11): protocol errors ----

#[test]
fn an_unparseable_line_is_a_minus_32700_against_a_null_id() {
    let r = mcp_raw(&fixture("mcp_project"), &["{not json"]);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0]["id"], Value::Null);
    assert_eq!(r[0]["error"]["code"], -32700);
}

#[test]
fn an_unknown_method_is_a_minus_32601() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[json!({"jsonrpc": "2.0", "id": 1, "method": "frobnicate"})],
    );
    assert_eq!(by_id(&r, 1)["error"]["code"], -32601);
}

#[test]
fn an_unknown_tool_is_a_minus_32602() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[call("veridikt_delete", json!({}))],
    );
    let resp = by_id(&r, 9);
    assert_eq!(resp["error"]["code"], -32602);
    assert!(
        resp["error"]["message"]
            .as_str()
            .unwrap()
            .contains("veridikt_delete"),
        "{resp:?}"
    );
}

#[test]
fn non_object_arguments_is_a_minus_32602() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call",
                 "params": {"name": "veridikt_ask", "arguments": [1, 2, 3]}})],
    );
    assert_eq!(by_id(&r, 9)["error"]["code"], -32602);
}

// ---- unhappy paths: a bad question is a *tool* error, not a protocol one ----

#[test]
fn an_unparseable_query_is_a_tool_error_with_the_session_intact() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[
            call("veridikt_ask", json!({"query": "affects Payment.ledger"})),
            json!({"jsonrpc": "2.0", "id": 10, "method": "ping"}),
        ],
    );
    let resp = by_id(&r, 9);
    assert!(resp.get("error").is_none(), "must not be a protocol error");
    assert_eq!(resp["result"]["isError"], true);
    assert!(
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("expected \"(\""),
        "{resp:?}"
    );
    // The session survived the bad question: ping still answers (D-079d).
    assert!(by_id(&r, 10)["result"].is_object());
}

#[test]
fn show_for_an_unknown_qname_is_a_tool_error_naming_the_nearest() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[call("veridikt_show", json!({"qname": "Payment.charg"}))],
    );
    let resp = by_id(&r, 9);
    assert_eq!(resp["result"]["isError"], true);
    assert!(
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("nearest existing qname is \"Payment.charge\""),
        "{resp:?}"
    );
}

#[test]
fn missing_required_argument_is_a_tool_error() {
    let r = mcp_session(&fixture("mcp_project"), &[call("veridikt_ask", json!({}))]);
    let resp = by_id(&r, 9);
    assert_eq!(resp["result"]["isError"], true);
    assert!(
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("requires a string \"query\""),
        "{resp:?}"
    );
}

// ---- handshake and discovery ----

#[test]
fn notifications_produce_no_response() {
    // initialized is a notification; only the ping (id 5) gets a response.
    let r = mcp_session(
        &fixture("mcp_project"),
        &[
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 5, "method": "ping"}),
        ],
    );
    assert_eq!(r.len(), 1, "exactly one response (the ping): {r:?}");
    assert_eq!(r[0]["id"], json!(5));
    assert_eq!(r[0]["result"], json!({}));
}

#[test]
fn initialize_echoes_protocol_version_and_advertises_tools() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                 "params": {"protocolVersion": "2025-03-26", "capabilities": {}}})],
    );
    let res = &by_id(&r, 1)["result"];
    assert_eq!(res["protocolVersion"], "2025-03-26");
    assert_eq!(res["serverInfo"]["name"], "veridikt");
    assert!(res["capabilities"]["tools"].is_object());
}

#[test]
fn initialize_instructions_tell_agents_when_to_use_the_tools() {
    // D-080: server-level guidance travels to every client via initialize.
    let r = mcp_session(
        &fixture("mcp_project"),
        &[json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                 "params": {"protocolVersion": "2025-06-18", "capabilities": {}}})],
    );
    let instructions = by_id(&r, 1)["result"]["instructions"]
        .as_str()
        .expect("initialize carries an instructions string");
    // it must steer the agent to the tools before reading source, and map
    // question shapes to tool forms.
    assert!(instructions.contains("BEFORE reading"));
    assert!(instructions.contains("veridikt_show") && instructions.contains("veridikt_ask"));
    assert!(instructions.contains("affects(") && instructions.contains("touches("));
}

#[test]
fn tool_descriptions_are_imperative_about_preferring_the_graph() {
    // D-080b: passive descriptions don't beat the read-the-source prior.
    let r = mcp_session(
        &fixture("mcp_project"),
        &[json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})],
    );
    for t in by_id(&r, 2)["result"]["tools"].as_array().unwrap() {
        assert!(
            t["description"]
                .as_str()
                .unwrap()
                .contains("Prefer this over"),
            "{} description is not imperative",
            t["name"]
        );
    }
}

#[test]
fn tools_list_advertises_exactly_the_four_read_only_tools() {
    let r = mcp_session(
        &fixture("mcp_project"),
        &[json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})],
    );
    let tools = by_id(&r, 2)["result"]["tools"].as_array().unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "veridikt_ask",
            "veridikt_history",
            "veridikt_lint",
            "veridikt_show"
        ]
    );
    // every tool carries a non-empty description and an object input schema
    for t in tools {
        assert!(t["description"].as_str().unwrap().len() > 30);
        assert_eq!(t["inputSchema"]["type"], "object");
    }
}

// ---- the four tools return the §10.4 JSON, 1:1 with the CLI (D-079c) ----

fn cli_json(dir: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_veridikt"))
        .args(args)
        .current_dir(dir)
        .stderr(Stdio::null())
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn veridikt_ask_structured_content_matches_the_cli_ask_json() {
    let dir = fixture("mcp_project");
    let r = mcp_session(
        &dir,
        &[call(
            "veridikt_ask",
            json!({"query": "affects(Payment.ledger)"}),
        )],
    );
    let sc = &by_id(&r, 9)["result"]["structuredContent"];
    let cli = cli_json(&dir, &["ask", "affects(Payment.ledger)", "--json"]);
    // stats.elapsed_ms is wall-clock; compare everything else.
    assert_eq!(sc["query"], cli["query"]);
    assert_eq!(sc["results"], cli["results"]);
    assert_eq!(sc["unresolved"], cli["unresolved"]);
    // and the text content is the serialized structuredContent (D-079c)
    let text = by_id(&r, 9)["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let reparsed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(&reparsed, sc);
}

#[test]
fn veridikt_show_returns_the_node_card() {
    let dir = fixture("mcp_project");
    let r = mcp_session(
        &dir,
        &[call("veridikt_show", json!({"qname": "Payment.charge"}))],
    );
    let sc = &by_id(&r, 9)["result"]["structuredContent"];
    let cli = cli_json(&dir, &["ask", "show(Payment.charge)", "--json"]);
    assert_eq!(sc["node"], cli["node"]);
    assert_eq!(sc["edges_out"], cli["edges_out"]);
    assert_eq!(sc["edges_in"], cli["edges_in"]);
}

#[test]
fn veridikt_lint_matches_the_cli_lint_json() {
    let dir = fixture("mcp_project");
    let r = mcp_session(&dir, &[call("veridikt_lint", json!({}))]);
    let sc = &by_id(&r, 9)["result"]["structuredContent"];
    // lint JSON is fully deterministic (no timing fields).
    let cli = cli_json(&dir, &["lint", "--json"]);
    assert_eq!(sc, &cli);
    assert_eq!(by_id(&r, 9)["result"]["isError"], false);
}

// ---- veridikt_history against a scripted git repo (real commits) ----

fn git(dir: &Path, date: &str, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Alice",
            "-c",
            "user.email=alice@example.com",
        ])
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn veridikt_history_returns_the_scripted_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("veridikt.toml"),
        "[project]\nname = \"h\"\nlanguages = [\"python\"]\n\n[modules]\n\"src/**\" = \"Payment\"\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let v1 = "# @veridikt\n# purpose: \"Charge a customer\"\ndef charge():\n    return 1\n";
    std::fs::write(root.join("src/svc.py"), v1).unwrap();
    git(
        root,
        "2026-01-02T03:04:05+00:00",
        &["init", "-q", "-b", "main"],
    );
    git(root, "2026-01-02T03:04:05+00:00", &["add", "."]);
    git(
        root,
        "2026-01-02T03:04:05+00:00",
        &["commit", "-q", "-m", "pay: add charge"],
    );

    let r = mcp_session(
        root,
        &[call("veridikt_history", json!({"qname": "Payment.charge"}))],
    );
    let sc = &by_id(&r, 9)["result"]["structuredContent"];
    assert_eq!(sc["qname"], "Payment.charge");
    let commits = sc["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["message"], "pay: add charge");
    assert_eq!(commits[0]["author"], "Alice");
    assert_eq!(commits[0]["hash"].as_str().unwrap().len(), 40);
}

// ---- the T9 eval: the right tool for "what writes to the ledger?" ----

#[test]
fn mcp_eval_what_writes_to_the_ledger() {
    // Replays the tool call the agent selects in EVAL.md and asserts the
    // answer, so the checked-in transcript cannot drift (D-079f).
    let dir = fixture("mcp_project");
    let r = mcp_session(
        &dir,
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            call("veridikt_ask", json!({"query": "affects(Payment.ledger)"})),
        ],
    );

    // The advertisement the agent reasons over names veridikt_ask with affects.
    let ask_desc = by_id(&r, 1)["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "veridikt_ask")
        .unwrap()["description"]
        .as_str()
        .unwrap();
    assert!(ask_desc.contains("affects(State)") && ask_desc.contains("writes"));
    assert!(ask_desc.contains("what writes to the ledger"));

    // The chosen call answers correctly: charge writes the ledger, Verified.
    let results = by_id(&r, 9)["result"]["structuredContent"]["results"]
        .as_array()
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["qname"], "Payment.charge");
    let via = &results[0]["via"][0];
    assert_eq!(via["edge"], "Affects");
    assert_eq!(via["status"], "Verified");
}
