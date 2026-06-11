//! `lore history` against a fixture repo with scripted commits (T5 exit
//! criterion; D-059). Unhappy paths first (G-11).

use std::path::Path;
use std::process::Command;

fn lore(args: &[&str], dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lore"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

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

/// A git repo with two scripted commits: the second edits a line inside
/// the charge subject span, so `-L` reports both.
fn scripted_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("lore.toml"),
        "[project]\nname = \"hist\"\nlanguages = [\"python\"]\n\n[modules]\n\"src/**\" = \"Payment\"\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let v1 = "# @lore\n# purpose: \"Charge a customer\"\ndef charge():\n    return 1\n";
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
    let v2 = v1.replace("return 1", "return 2");
    std::fs::write(root.join("src/svc.py"), v2).unwrap();
    git(
        root,
        "2026-02-03T04:05:06+00:00",
        &[
            "commit",
            "-q",
            "-am",
            "pay: charge returns 2\n\nthe ledger needs the new amount",
        ],
    );
    tmp
}

#[test]
fn history_for_an_unknown_qname_is_exit_2_with_the_nearest() {
    let repo = scripted_repo();
    let out = lore(&["history", "Payment.charg"], repo.path());
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "\"Payment.charg\" names no node in the graph; nearest existing qname is \"Payment.charge\"\n"
    );
}

#[test]
fn history_outside_a_git_work_tree_is_exit_2() {
    // same project, never `git init`ed (D-059c: no git, no answer)
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("lore.toml"),
        "[project]\nname = \"hist\"\nlanguages = [\"python\"]\n\n[modules]\n\"src/**\" = \"Payment\"\n",
    )
    .unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src/svc.py"),
        "# @lore\n# purpose: \"Charge\"\ndef charge():\n    return 1\n",
    )
    .unwrap();
    let out = lore(&["history", "Payment.charge"], tmp.path());
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not a git repository"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn history_renders_the_scripted_commits_newest_first() {
    let repo = scripted_repo();
    let out = lore(&["history", "Payment.charge"], repo.path());
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    // subject span is the def + body (lines 3-4)
    assert_eq!(
        lines[0],
        "history for Payment.charge  src/svc.py:3-4: 2 commits"
    );
    // 12-char hash, ISO-strict author date, author; message indented 4
    assert!(
        lines[1].ends_with("  2026-02-03T04:05:06Z  Alice")
            && lines[1].len() == 12 + 2 + 20 + 2 + 5,
        "line: {:?}",
        lines[1]
    );
    assert_eq!(lines[2], "    pay: charge returns 2");
    assert_eq!(lines[3], "");
    assert_eq!(lines[4], "    the ledger needs the new amount");
    assert!(lines[5].ends_with("  2026-01-02T03:04:05Z  Alice"));
    assert_eq!(lines[6], "    pay: add charge");
    assert_eq!(lines.len(), 7);
}

#[test]
fn history_json_carries_full_hashes_and_messages() {
    let repo = scripted_repo();
    let out = lore(&["history", "Payment.charge", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["qname"], "Payment.charge");
    assert_eq!(
        v["location"],
        serde_json::json!({"file": "src/svc.py", "line": 3})
    );
    assert_eq!(v["span"], serde_json::json!({"start": 3, "end": 4}));
    let commits = v["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0]["author"], "Alice");
    assert_eq!(commits[0]["date"], "2026-02-03T04:05:06Z");
    assert_eq!(
        commits[0]["message"],
        "pay: charge returns 2\n\nthe ledger needs the new amount"
    );
    assert_eq!(commits[1]["message"], "pay: add charge");
    assert_eq!(commits[0]["hash"].as_str().unwrap().len(), 40);
}

#[test]
fn history_quiet_drops_the_header() {
    let repo = scripted_repo();
    let out = lore(&["history", "Payment.charge", "--quiet"], repo.path());
    assert_eq!(out.status.code(), Some(0));
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("history for"),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
