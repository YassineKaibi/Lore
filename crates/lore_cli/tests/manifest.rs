use lore_cli::manifest;
use std::path::Path;

fn p(text: &str) -> Result<manifest::Manifest, lore_intent::Finding> {
    manifest::parse(Path::new("lore.toml"), text)
}

#[test]
fn unknown_table_is_e0401() {
    let err = p("[project]\nname = \"x\"\n[deploy]\nregion = \"us\"\n").unwrap_err();
    assert_eq!(err.code, "E0401");
    assert!(err.message.contains("deploy"));
}

#[test]
fn unknown_project_key_is_e0401() {
    let err = p("[project]\nname = \"x\"\nversion = \"1\"\n").unwrap_err();
    assert_eq!(err.code, "E0401");
    assert!(err.message.contains("version"));
}

#[test]
fn unknown_language_is_e0403() {
    let err = p("[project]\nname = \"x\"\nlanguages = [\"cobol\"]\n").unwrap_err();
    assert_eq!(err.code, "E0403");
    assert!(err.message.contains("cobol"));
}

#[test]
fn bad_policy_value_is_e0403() {
    let err = p("[project]\nname = \"x\"\n[policy]\nunknown = \"loud\"\n").unwrap_err();
    assert_eq!(err.code, "E0403");
}

#[test]
fn non_string_module_target_is_e0403() {
    let err = p("[project]\nname = \"x\"\n[modules]\n\"src/**\" = 3\n").unwrap_err();
    assert_eq!(err.code, "E0403");
}

#[test]
fn syntactically_invalid_toml_is_e0403() {
    let err = p("[project\n").unwrap_err();
    assert_eq!(err.code, "E0403");
}

#[test]
fn defaults_apply() {
    let m = p("[project]\nname = \"x\"\nlanguages = [\"python\"]\n").unwrap();
    assert_eq!(m.roots, vec!["src"]);
    assert!(matches!(m.policy.unknown, manifest::PolicyLevel::Warn));
}

#[test]
fn spec_example_manifest_parses_with_order_preserved() {
    // the §11 TOML block verbatim (G-13: spec examples are fixtures)
    let text = "[project]\nname = \"myproject\"\nlanguages = [\"python\", \"typescript\"]\nroots = [\"src\"]\n\n[modules]\n\"src/payments/**\" = \"Payment\"\n\"src/users/**\"    = \"User\"\n\n[policy]\nunknown            = \"warn\"\nstale              = \"warn\"\nundeclared_effects = \"off\"\n\n[lint]\n";
    let m = p(text).unwrap();
    assert_eq!(
        m.modules
            .iter()
            .map(|g| g.module.as_str())
            .collect::<Vec<_>>(),
        ["Payment", "User"]
    );
}

#[test]
fn discover_walks_up_and_reports_absence() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("a/b")).unwrap();
    assert_eq!(manifest::discover(&root.path().join("a/b")), None);
    std::fs::write(root.path().join("lore.toml"), "[project]\nname=\"x\"\n").unwrap();
    assert_eq!(
        manifest::discover(&root.path().join("a/b")),
        Some(root.path().join("lore.toml"))
    );
}
