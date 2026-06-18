//! Boundary tests for `veridikt graph --dot` (D-038, §12). Unhappy path first
//! (G-11). The exit criterion is "renders under `dot -Tsvg` without warnings":
//! we feed the output to the `dot` binary when it is installed.

use std::process::{Command, Stdio};

fn veridikt(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_veridikt"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn without_dot_flag_is_usage_error() {
    let out = veridikt(&["graph"], &fixture("ask_project"));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--dot"));
}

#[test]
fn focus_on_missing_node_is_exit_2_with_nearest() {
    let out = veridikt(
        &["graph", "--dot", "--focus", "Payment.legder"],
        &fixture("ask_project"),
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Payment.ledger"), "{stderr}");
}

#[test]
fn dot_output_is_a_wellformed_digraph() {
    let out = veridikt(&["graph", "--dot"], &fixture("ask_project"));
    assert_eq!(out.status.code(), Some(0));
    let dot = String::from_utf8_lossy(&out.stdout);
    assert!(dot.starts_with("digraph veridikt {"));
    assert!(dot.trim_end().ends_with('}'));
    // Every node is a quoted id with a kind in its label.
    assert!(dot.contains("[label=\"Payment.ledger\\nState\"]"));
}

#[test]
fn focus_with_depth_bounds_the_neighborhood() {
    let full = veridikt(&["graph", "--dot"], &fixture("ask_project"));
    let focused = veridikt(
        &[
            "graph",
            "--dot",
            "--focus",
            "Payment.ledger",
            "--depth",
            "1",
        ],
        &fixture("ask_project"),
    );
    assert_eq!(focused.status.code(), Some(0));
    let full_nodes = String::from_utf8_lossy(&full.stdout)
        .matches("[label=")
        .count();
    let focused_nodes = String::from_utf8_lossy(&focused.stdout)
        .matches("[label=")
        .count();
    // The neighborhood of one node is a strict subset of the whole graph.
    assert!(focused_nodes >= 1 && focused_nodes < full_nodes);
}

#[test]
fn dot_renders_under_graphviz_without_warnings() {
    // Skip cleanly if graphviz is not installed (CI installs it).
    if Command::new("dot").arg("-V").output().is_err() {
        eprintln!("skipping: graphviz `dot` not installed");
        return;
    }
    let out = veridikt(&["graph", "--dot"], &fixture("ask_project"));
    assert_eq!(out.status.code(), Some(0));

    let mut child = Command::new("dot")
        .args(["-Tsvg"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&out.stdout).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "dot failed to render");
    let warnings = String::from_utf8_lossy(&result.stderr);
    assert!(warnings.is_empty(), "dot emitted warnings: {warnings}");
}
