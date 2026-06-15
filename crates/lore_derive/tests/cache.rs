//! The extraction cache is invisible (D-064): warm runs return byte-identical
//! results, corruption degrades to a miss, content changes always re-derive.

use lore_derive::{DeriveConfig, DeriveResult, SourceUnit, StateSymbol, derive};
use lore_intent::QName;

mod common;

fn files() -> Vec<SourceUnit> {
    vec![
        SourceUnit {
            path: "src/pay/svc.py".into(),
            text: "ledger = []\n\ndef charge():\n    ledger.append(1)\n".into(),
            module: "Payment".into(),
        },
        SourceUnit {
            path: "src/user/u.py".into(),
            text: "from pay.svc import charge\n\ndef signup():\n    charge()\n".into(),
            module: "User".into(),
        },
    ]
}

fn states() -> Vec<StateSymbol> {
    vec![StateSymbol {
        qname: QName::from_dotted("Payment.ledger"),
        identifier: "ledger".into(),
        file: "src/pay/svc.py".into(),
        module: "Payment".into(),
    }]
}

fn snapshot(r: &DeriveResult) -> (Vec<String>, Vec<String>, usize, usize) {
    (
        r.nodes.iter().map(|n| n.qname.to_string()).collect(),
        r.edges
            .iter()
            .map(|e| {
                format!(
                    "{} -> {} {:?} {:?} {}:{}",
                    e.from,
                    e.to,
                    e.kind,
                    e.confidence,
                    e.loc.file.display(),
                    e.loc.line
                )
            })
            .collect(),
        r.unresolved_calls,
        r.ambiguous_names,
    )
}

#[test]
fn corrupt_cache_entries_degrade_to_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DeriveConfig {
        roots: vec!["src".into()],
        cache_dir: Some(dir.path().to_path_buf()),
        manifests: Vec::new(),
    };
    let cold = snapshot(&derive(&cfg, &common::packs(), &files(), &states()));
    for entry in std::fs::read_dir(dir.path().join("derive")).unwrap() {
        std::fs::write(entry.unwrap().path(), "not json").unwrap();
    }
    let rederived = snapshot(&derive(&cfg, &common::packs(), &files(), &states()));
    assert_eq!(cold, rederived);
}

#[test]
fn warm_runs_return_identical_results() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DeriveConfig {
        roots: vec!["src".into()],
        cache_dir: Some(dir.path().to_path_buf()),
        manifests: Vec::new(),
    };
    let cold = snapshot(&derive(&cfg, &common::packs(), &files(), &states()));
    assert!(
        std::fs::read_dir(dir.path().join("derive"))
            .unwrap()
            .count()
            >= 2,
        "the cache must be populated"
    );
    let warm = snapshot(&derive(&cfg, &common::packs(), &files(), &states()));
    assert_eq!(cold, warm);

    // no cache at all gives the same answer: the cache is invisible
    let uncached = snapshot(&derive(
        &DeriveConfig {
            roots: vec!["src".into()],
            cache_dir: None,
            manifests: Vec::new(),
        },
        &common::packs(),
        &files(),
        &states(),
    ));
    assert_eq!(cold, uncached);
}

#[test]
fn changed_content_misses_the_stale_entry() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = DeriveConfig {
        roots: vec!["src".into()],
        cache_dir: Some(dir.path().to_path_buf()),
        manifests: Vec::new(),
    };
    derive(&cfg, &common::packs(), &files(), &states());

    let mut changed = files();
    changed[1].text = "def signup():\n    pass\n".into(); // the call is gone
    let r = derive(&cfg, &common::packs(), &changed, &states());
    assert!(
        r.edges
            .iter()
            .all(|e| e.from != QName::from_dotted("User.signup")),
        "stale cached facts must not resurrect the deleted call"
    );
}
