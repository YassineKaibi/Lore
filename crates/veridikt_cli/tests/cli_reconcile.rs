//! T7 exit criteria at the binary boundary (G-4): the seeded-drift
//! protocol (10 annotations made false in 10 different ways; lint must
//! flag >= 8 with zero false Contradicted on the 20 true ones — permanent
//! regression armor per the roadmap), staleness over scripted commits
//! (D-068), the W0303 policy surface (D-067), and the stats claims
//! breakdown (D-069).

use std::path::Path;
use std::process::Command;

fn veridikt(args: &[&str], dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_veridikt"))
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

const T1: &str = "2026-01-02T03:04:05+00:00";
const T2: &str = "2026-03-04T05:06:07+00:00";

const PAY_STATE: &str = r#"# @veridikt
# kind: module
# name: Pay
# purpose: "Payments"
# owner: "pay-team"
# depends_on: User

# @veridikt
# kind: state
# purpose: "Append-only ledger"
ledger = []

# @veridikt
# kind: state
# purpose: "Balances per account"
balances = {}

# @veridikt
# kind: event
# name: Settled
# purpose: "Funds moved"
SETTLED = "pay.settled"
"#;

// The lies live here. Each lie block is false in its own way (roadmap T7):
//   refund        — effect removed: affects survives, the write is gone
//   available     — read removed
//   settle_batch  — function gutted, both claims left behind
//   charge_quiet  — call removed: triggers survives, no call
//   hold          — state renamed: affects targets a qname that is gone
//   fees          — stale: the body changes at T2, the block does not
//   archive       — write demoted to a read (HONEST MISS: symbol present)
//   announce      — emits lie (HONEST MISS: unverifiable in Phase 1)
const PAY_SVC: &str = r#"from user.u import notify

# @veridikt
# purpose: "Charge a customer"
# affects: Pay.ledger
# reads: Pay.balances
def charge(user, amount):
    if balances.get(user, 0) >= amount:
        ledger.append((user, amount))

# @veridikt
# purpose: "Refund a customer"
# affects: Pay.ledger
def refund(user, amount):
    return amount

# @veridikt
# purpose: "Check available funds"
# reads: Pay.balances
def available(user):
    return 0

# @veridikt
# purpose: "Settle a batch"
# affects: Pay.ledger
# reads: Pay.balances
def settle_batch(batch):
    pass

# @veridikt
# purpose: "Charge and notify"
# triggers: User.notify
def charge_notify(user, amount):
    ledger.append((user, amount))
    notify(user)

# @veridikt
# purpose: "Quiet charge"
# triggers: User.notify
def charge_quiet(user, amount):
    ledger.append((user, amount))

# @veridikt
# purpose: "Hold funds in escrow"
# affects: Pay.escrow
def hold(user, amount):
    ledger.append((user, amount))

# @veridikt
# purpose: "Compute fees"
def fees(amount):
    return amount * 0.02

# @veridikt
# purpose: "Archive the ledger"
# affects: Pay.ledger
def archive():
    return list(ledger)

# @veridikt
# purpose: "Announce settlement"
# emits: Pay.Settled
def announce():
    pass

# @veridikt
# purpose: "Rebuild caches"
# reads: Pay.balances
def rebuild():
    return load_all()  # balances are reloaded from disk
"#;

// Two more lies: the module's owner contradicts CODEOWNERS, and ... no —
// only the owner lie lives here; everything else in this file is true.
const USER_U: &str = r#"from pay.svc import charge

# @veridikt
# kind: module
# name: User
# purpose: "User accounts"
# owner: "platform-team"
# depends_on: Pay

# @veridikt
# kind: state
# purpose: "Profiles by user id"
profiles = {}

# @veridikt
# purpose: "Send a notification"
def notify(user):
    return user

# @veridikt
# purpose: "Sign a user up"
# triggers: Pay.charge
def signup(user):
    profiles[user] = {}
    charge(user, 1)

# @veridikt
# purpose: "Update a profile"
# affects: User.profiles
def update_profile(user, data):
    profiles.update({user: data})

# @veridikt
# purpose: "Read a profile"
# reads: User.profiles
def get_profile(user):
    return profiles.get(user)

# @veridikt
# purpose: "Deactivate on settlement"
# on: Pay.Settled
# unknown: "Deactivation during an in-flight charge is untested"
def deactivate(user):
    return user
"#;

// The last lie: a depends_on nothing uses (the module block's only
// falsehood); the other seven annotations here are true.
const BILLING_B: &str = r#"# @veridikt
# kind: module
# name: Billing
# purpose: "Invoicing"
# owner: "billing-team"
# depends_on: Pay

# @veridikt
# kind: state
# purpose: "Issued invoices"
invoices = []

# @veridikt
# purpose: "Issue an invoice"
# affects: Billing.invoices
def invoice(user, amount):
    invoices.append((user, amount))

# @veridikt
# purpose: "List invoices"
# reads: Billing.invoices
def list_invoices():
    return list(invoices)

# @veridikt
# purpose: "Total for audit"
def audit_total():
    return 0

# @veridikt
# purpose: "Compute tax"
# assumes: "amount is non-negative"
def tax(amount):
    return amount * 0.2

# @veridikt
# purpose: "Apply a discount"
# because: "Marketing campaign Q3-2026 requires a flat discount"
def discount(amount):
    return amount * 0.9

# @veridikt
# purpose: "Close the books"
def close_books():
    return 0
"#;

/// 30 annotations: 20 true, 10 false in 10 different ways. Two scripted
/// commits: everything at T1, then the fees body alone at T2 (the stale
/// lie).
fn drift_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("veridikt.toml"),
        "[project]\nname = \"drift\"\nlanguages = [\"python\"]\nroots = [\"src\"]\n\n\
         [modules]\n\"src/pay/**\" = \"Pay\"\n\"src/user/**\" = \"User\"\n\"src/billing/**\" = \"Billing\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("CODEOWNERS"),
        "src/pay/ @acme/pay-team\nsrc/user/ @acme/identity-team\nsrc/billing/ @acme/billing-team\n",
    )
    .unwrap();
    for (path, text) in [
        ("src/pay/state.py", PAY_STATE),
        ("src/pay/svc.py", PAY_SVC),
        ("src/user/u.py", USER_U),
        ("src/billing/b.py", BILLING_B),
    ] {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    git(root, T1, &["init", "-q", "-b", "main"]);
    git(root, T1, &["add", "."]);
    git(root, T1, &["commit", "-q", "-m", "drift: seed"]);
    std::fs::write(
        root.join("src/pay/svc.py"),
        PAY_SVC.replace("amount * 0.02", "amount * 0.05"),
    )
    .unwrap();
    git(root, T2, &["commit", "-q", "-am", "pay: fees go up"]);
    tmp
}

#[test]
fn seeded_drift_lint_flags_8_of_10_lies_with_zero_false_contradictions() {
    let repo = drift_repo();
    let out = veridikt(&["lint", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(1)); // the E0306 is error severity
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings: Vec<(&str, &str, u64)> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["code"].as_str().unwrap(),
                f["file"].as_str().unwrap(),
                f["line"].as_u64().unwrap(),
            )
        })
        .collect();
    // 8 of the 10 lie blocks are flagged; archive (write demoted to read:
    // the symbol still occurs, the verdict is withheld) and announce
    // (emits is Unverifiable in Phase 1) are the two honest misses. The
    // only finding on a true annotation is deactivate's declared unknown
    // (W0213) — and crucially zero W0302/E0302 beyond the four lying
    // blocks: a false Contradicted is the one unforgivable output (G-7).
    assert_eq!(
        findings,
        [
            ("W0206", "src/billing/b.py", 6), // lie: unused depends_on
            ("W0302", "src/pay/svc.py", 13),  // lie: refund's write removed
            ("W0302", "src/pay/svc.py", 19),  // lie: available's read removed
            ("W0302", "src/pay/svc.py", 25),  // lie: settle_batch gutted...
            ("W0302", "src/pay/svc.py", 26),  // ...both claims contradicted
            ("W0302", "src/pay/svc.py", 39),  // lie: charge_quiet's call removed
            ("E0306", "src/pay/svc.py", 45),  // lie: escrow state renamed away
            ("W0301", "src/pay/svc.py", 49),  // lie: fees body changed after the block
            ("W0207", "src/user/u.py", 7),    // lie: owner contradicts CODEOWNERS
            ("W0213", "src/user/u.py", 42),   // true: an honest declared unknown
        ]
    );
    assert_eq!(
        v["summary"],
        serde_json::json!({"errors": 1, "warnings": 9})
    );
}

#[test]
fn drift_repo_stats_breaks_claims_down_by_status() {
    let repo = drift_repo();
    let out = veridikt(&["stats", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // 20 claim edges: 8 verified (charge x2, charge_notify, signup,
    // update_profile, get_profile, invoice, list_invoices), 2 unverified
    // (archive, rebuild), 5 contradicted (refund, available,
    // settle_batch x2, charge_quiet), 5 unverifiable (emits, on, and the
    // three depends_on). hold's claim resolved to nothing — no edge.
    assert_eq!(
        v["claims"],
        serde_json::json!({
            "total": 20,
            "verified": 8,
            "unverified": 2,
            "contradicted": 5,
            "unverifiable": 5,
        })
    );
}

// ---- staleness mechanics (§9.2, D-068) ----

fn stale_repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("veridikt.toml"),
        "[project]\nname = \"stale\"\nlanguages = [\"python\"]\n\n[modules]\n\"src/**\" = \"App\"\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let v1 = "# @veridikt\n# purpose: \"Charge\"\ndef charge():\n    return 1\n";
    std::fs::write(root.join("src/m.py"), v1).unwrap();
    git(root, T1, &["init", "-q", "-b", "main"]);
    git(root, T1, &["add", "."]);
    git(root, T1, &["commit", "-q", "-m", "app: charge"]);
    std::fs::write(root.join("src/m.py"), v1.replace("return 1", "return 2")).unwrap();
    git(root, T2, &["commit", "-q", "-am", "app: charge returns 2"]);
    tmp
}

#[test]
fn a_subject_changed_after_its_block_is_w0301_with_both_timestamps() {
    let repo = stale_repo();
    let out = veridikt(&["lint", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(0)); // a warning, not an error
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["code"], "W0301");
    assert_eq!(findings[0]["severity"], "warning");
    assert_eq!(findings[0]["line"], 1); // the block's span
    let msg = findings[0]["message"].as_str().unwrap();
    // both timestamps in iso-strict, rendered in the commit's own zone
    assert!(msg.contains("2026-03-04T05:06:07+00:00"), "{msg}");
    assert!(msg.contains("2026-01-02T03:04:05+00:00"), "{msg}");
    assert!(msg.contains("(commit "), "{msg}");
}

#[test]
fn policy_stale_error_promotes_w0301_and_fails_lint() {
    let repo = stale_repo();
    let manifest = repo.path().join("veridikt.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, format!("{text}\n[policy]\nstale = \"error\"\n")).unwrap();
    let out = veridikt(&["lint", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // D-068e: severity promoted at the lint surface, code unchanged
    assert_eq!(v["findings"][0]["code"], "W0301");
    assert_eq!(v["findings"][0]["severity"], "error");
}

#[test]
fn no_stale_skips_the_check_entirely() {
    let repo = stale_repo();
    let out = veridikt(&["lint", "--no-stale", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["findings"], serde_json::json!([]));
}

#[test]
fn outside_a_git_work_tree_lint_skips_staleness_with_one_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("veridikt.toml"),
        "[project]\nname = \"nogit\"\nlanguages = [\"python\"]\n\n[modules]\n\"src/**\" = \"App\"\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/m.py"),
        "# @veridikt\n# purpose: \"Charge\"\ndef charge():\n    return 1\n",
    )
    .unwrap();

    let out = veridikt(&["lint", "--json"], root);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["findings"], serde_json::json!([]));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not a git work tree; staleness check (W0301) skipped"),
        "{stderr}"
    );

    // --quiet drops the notice (D-068c)
    let out = veridikt(&["lint", "--json", "--quiet"], root);
    assert!(String::from_utf8_lossy(&out.stderr).is_empty());
}

// ---- W0303 behind [policy] undeclared_effects (D-019, D-067) ----

fn undeclared_effect_project(policy: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(
        root.join("veridikt.toml"),
        format!(
            "[project]\nname = \"fx\"\nlanguages = [\"python\"]\n\n[modules]\n\"src/**\" = \"App\"\n{policy}"
        ),
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    // bump is annotated and writes count without declaring it
    std::fs::write(
        root.join("src/m.py"),
        "# @veridikt\n# kind: state\n# purpose: \"Counter\"\ncount = 0\n\n\
         # @veridikt\n# purpose: \"Bump\"\ndef bump():\n    count += 1\n",
    )
    .unwrap();
    tmp
}

#[test]
fn undeclared_effects_warn_surfaces_w0303_at_the_write_site() {
    let repo = undeclared_effect_project("\n[policy]\nundeclared_effects = \"warn\"\n");
    let out = veridikt(&["lint", "--no-stale", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(0)); // a warning, not an error
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = v["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["code"], "W0303");
    assert_eq!(findings[0]["line"], 9); // the write site, not the block
    assert_eq!(
        findings[0]["message"],
        "\"App.bump\" writes \"App.count\" here (derived, Heuristic) but its block declares no \"affects: App.count\"; add the clause or remove the write"
    );
}

#[test]
fn undeclared_effects_default_off_hides_w0303_from_lint_but_not_from_show() {
    let repo = undeclared_effect_project("");
    let out = veridikt(&["lint", "--no-stale", "--json"], repo.path());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["findings"], serde_json::json!([]));

    // D-067b/D-056c: the graph still carries it; show renders unfiltered
    let out = veridikt(&["ask", "show(App.bump)", "--json"], repo.path());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["findings"][0]["code"], "W0303");
}
