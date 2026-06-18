//! Pack-loader boundary tests (spec §8.6, band E041x). Unhappy path first
//! (G-11): every E041x class is produced by a malformed pack, asserting the
//! exact code. Validation is a pure function of a `PackSource`, so these need
//! no filesystem (G-4).

use veridikt_cli::packs::{self, PackSource};
use veridikt_intent::Tier;

/// Build a `PackSource` from parts; fixtures default to the full set so a
/// test isolating one failure does not also trip E0415.
fn source(
    name: &str,
    manifest: &str,
    bind_scm: Option<&str>,
    derive_scm: Option<&str>,
    fixtures: &[&str],
) -> PackSource {
    PackSource {
        name: name.into(),
        manifest_path: format!("packs/{name}/veridikt-lang.toml").into(),
        manifest: manifest.into(),
        bind_scm: bind_scm.map(str::to_owned),
        derive_scm: derive_scm.map(str::to_owned),
        fixture_classes: fixtures.iter().map(|s| s.to_string()).collect(),
    }
}

const SCAN_MANIFEST: &str = r##"
[pack]
name = "toy"
format = 1
tier = "scan"
[scanner]
extensions = [".toy"]
comment_token = "#"
"##;

const DERIVE_MANIFEST: &str = r##"
[pack]
name = "toy"
format = 1
tier = "derive"
[grammar]
source = "builtin"
name = "tree_sitter_python"
[scanner]
extensions = [".toy"]
comment_token = "#"
[derive.mutators]
methods = ["append"]
[[derive.imports.strategy]]
kind = "root_relative"
extensions = [".toy"]
"##;

fn err_code(src: &PackSource) -> &'static str {
    packs::load(src).expect_err("expected a load failure").code
}

#[test]
fn scan_tier_pack_loads() {
    let pack = packs::load(&source("toy", SCAN_MANIFEST, None, None, &["scan"]))
        .expect("scan pack must load");
    assert_eq!(pack.spec.tier, Tier::Scan);
    assert!(pack.grammar.is_none(), "scan tier has no grammar");
    assert_eq!(pack.spec.extensions, vec![".toy".to_string()]);
}

#[test]
fn derive_tier_pack_loads() {
    let pack = packs::load(&source(
        "toy",
        DERIVE_MANIFEST,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind", "derive"],
    ))
    .expect("derive pack must load");
    assert_eq!(pack.spec.tier, Tier::Derive);
    assert!(pack.grammar.is_some());
    assert_eq!(pack.spec.imports.len(), 1);
    assert_eq!(pack.spec.mutator_methods, vec!["append".to_string()]);
}

#[test]
fn e0412_unknown_format_version() {
    let m = SCAN_MANIFEST.replace("format = 1", "format = 2");
    let f = packs::load(&source("toy", &m, None, None, &["scan"])).unwrap_err();
    assert_eq!(f.code, "E0412");
    assert!(f.message.contains("version 2"));
}

#[test]
fn e0410_unparseable_toml() {
    assert_eq!(
        err_code(&source("toy", "not = = toml", None, None, &["scan"])),
        "E0410"
    );
}

#[test]
fn e0410_unknown_top_level_key() {
    let m = format!("{SCAN_MANIFEST}\n[bogus]\nx = 1\n");
    assert_eq!(err_code(&source("toy", &m, None, None, &["scan"])), "E0410");
}

#[test]
fn e0410_unknown_pack_key() {
    let m = SCAN_MANIFEST.replace("tier = \"scan\"", "tier = \"scan\"\nbogus = 1");
    assert_eq!(err_code(&source("toy", &m, None, None, &["scan"])), "E0410");
}

#[test]
fn e0410_name_mismatch() {
    // [pack] name must equal the pack directory name.
    let f = packs::load(&source("other", SCAN_MANIFEST, None, None, &["scan"])).unwrap_err();
    assert_eq!(f.code, "E0410");
    assert!(f.message.contains("directory name"));
}

#[test]
fn e0410_invalid_tier() {
    let m = SCAN_MANIFEST.replace("tier = \"scan\"", "tier = \"frobnicate\"");
    assert_eq!(err_code(&source("toy", &m, None, None, &["scan"])), "E0410");
}

#[test]
fn e0410_grammar_at_scan_tier() {
    // A scan-tier pack declares no grammar (artifact above its tier).
    let m = format!(
        "{SCAN_MANIFEST}\n[grammar]\nsource = \"builtin\"\nname = \"tree_sitter_python\"\n"
    );
    assert_eq!(err_code(&source("toy", &m, None, None, &["scan"])), "E0410");
}

#[test]
fn e0410_bind_tier_missing_grammar() {
    let m = r##"
[pack]
name = "toy"
format = 1
tier = "bind"
[scanner]
extensions = [".toy"]
comment_token = "#"
"##;
    assert_eq!(
        err_code(&source(
            "toy",
            m,
            Some("(x) @subject.function"),
            None,
            &["scan", "bind"]
        )),
        "E0410"
    );
}

#[test]
fn e0410_derive_table_at_bind_tier() {
    let m = r##"
[pack]
name = "toy"
format = 1
tier = "bind"
[grammar]
source = "builtin"
name = "tree_sitter_python"
[scanner]
extensions = [".toy"]
comment_token = "#"
[derive.mutators]
methods = ["append"]
"##;
    assert_eq!(
        err_code(&source(
            "toy",
            m,
            Some("(x) @subject.function"),
            None,
            &["scan", "bind"]
        )),
        "E0410"
    );
}

#[test]
fn e0410_derive_scm_present_at_bind_tier() {
    // queries/derive.scm is an artifact above tier "bind" (D-070b).
    let m = r##"
[pack]
name = "toy"
format = 1
tier = "bind"
[grammar]
source = "builtin"
name = "tree_sitter_python"
[scanner]
extensions = [".toy"]
comment_token = "#"
"##;
    let f = packs::load(&source(
        "toy",
        m,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0410");
}

#[test]
fn e0411_bind_tier_missing_bind_scm() {
    let m = r##"
[pack]
name = "toy"
format = 1
tier = "bind"
[grammar]
source = "builtin"
name = "tree_sitter_python"
[scanner]
extensions = [".toy"]
comment_token = "#"
"##;
    // No bind.scm file -> unusable artifact.
    let f = packs::load(&source("toy", m, None, None, &["scan", "bind"])).unwrap_err();
    assert_eq!(f.code, "E0411");
    assert!(f.message.contains("bind.scm"));
}

#[test]
fn e0413_wasm_grammar_reserved() {
    let m = r##"
[pack]
name = "toy"
format = 1
tier = "bind"
[grammar]
source = "wasm"
path = "toy.wasm"
[scanner]
extensions = [".toy"]
comment_token = "#"
"##;
    let f = packs::load(&source(
        "toy",
        m,
        Some("(x) @subject.function"),
        None,
        &["scan", "bind"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0413");
    assert!(f.message.contains("wasm"));
}

#[test]
fn e0413_unknown_builtin_grammar() {
    let m = DERIVE_MANIFEST.replace("tree_sitter_python", "tree_sitter_cobol");
    let f = packs::load(&source(
        "toy",
        &m,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind", "derive"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0413");
    assert!(f.message.contains("tree_sitter_cobol"));
}

#[test]
fn e0414_unknown_strategy_kind() {
    let m = DERIVE_MANIFEST.replace("kind = \"root_relative\"", "kind = \"telepathy\"");
    let f = packs::load(&source(
        "toy",
        &m,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind", "derive"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0414");
    assert!(f.message.contains("telepathy"));
}

#[test]
fn e0414_unknown_custom_strategy_name() {
    let m = DERIVE_MANIFEST.replace(
        "kind = \"root_relative\"\nextensions = [\".toy\"]",
        "kind = \"custom\"\nname = \"made_up\"",
    );
    let f = packs::load(&source(
        "toy",
        &m,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind", "derive"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0414");
    assert!(f.message.contains("made_up"));
}

#[test]
fn e0410_derive_tier_missing_strategies() {
    let m = r##"
[pack]
name = "toy"
format = 1
tier = "derive"
[grammar]
source = "builtin"
name = "tree_sitter_python"
[scanner]
extensions = [".toy"]
comment_token = "#"
"##;
    let f = packs::load(&source(
        "toy",
        m,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind", "derive"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0410");
    assert!(f.message.contains("strategy"));
}

#[test]
fn e0415_missing_mandatory_fixture_class() {
    // A derive-tier pack with only the scan + bind classes is incomplete.
    let f = packs::load(&source(
        "toy",
        DERIVE_MANIFEST,
        Some("(x) @subject.function"),
        Some("(y) @call"),
        &["scan", "bind"],
    ))
    .unwrap_err();
    assert_eq!(f.code, "E0415");
    assert!(f.message.contains("derive"));
}

#[test]
fn e0410_two_packs_claim_one_extension() {
    let a = source("toy", SCAN_MANIFEST, None, None, &["scan"]);
    // A second scan pack named "two" also claiming ".toy".
    let m2 = SCAN_MANIFEST.replace("name = \"toy\"", "name = \"two\"");
    let b = source("two", &m2, None, None, &["scan"]);
    let (loaded, findings) = packs::load_all(&[a, b]);
    assert_eq!(loaded.len(), 2, "both packs are individually valid");
    assert!(
        findings
            .iter()
            .any(|f| f.code == "E0410" && f.message.contains(".toy")),
        "extension collision must be E0410: {findings:?}"
    );
}
