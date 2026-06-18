use std::path::Path;
use veridikt_cli::manifest;

fn p(text: &str) -> Result<manifest::Manifest, veridikt_intent::Finding> {
    manifest::parse(Path::new("veridikt.toml"), text)
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
fn malformed_module_glob_is_e0403() {
    // An unclosed alternate is invalid glob syntax; it must fail loudly, not
    // be silently dropped at match time (§8.6 Annotations.scan resolution).
    let err = p("[project]\nname = \"x\"\n[modules]\n\"src/{a,b\" = \"M\"\n").unwrap_err();
    assert_eq!(err.code, "E0403");
    assert!(err.message.contains("glob"));
}

#[test]
fn syntactically_invalid_toml_is_e0403() {
    let err = p("[project\n").unwrap_err();
    assert_eq!(err.code, "E0403");
}

#[test]
fn lint_override_of_an_e_code_is_e0401() {
    // D-056a: E findings can never be silenced
    let err = p("[project]\nname = \"x\"\n[lint]\n\"E0306\" = \"off\"\n").unwrap_err();
    assert_eq!(err.code, "E0401");
    assert!(err.message.contains("E0306"));
    assert!(err.message.contains("W-codes"));
}

#[test]
fn lint_override_of_a_typoed_w_code_is_e0401() {
    // D-056a: a typo'd code must fail loudly, not silently fail to suppress
    let err = p("[project]\nname = \"x\"\n[lint]\n\"W0260\" = \"off\"\n").unwrap_err();
    assert_eq!(err.code, "E0401");
    assert!(err.message.contains("W0260"));
}

#[test]
fn lint_override_with_a_bad_level_is_e0403() {
    let err = p("[project]\nname = \"x\"\n[lint]\n\"W0206\" = \"error\"\n").unwrap_err();
    assert_eq!(err.code, "E0403");
    assert!(err.message.contains("\"warn\" or \"off\""));
}

#[test]
fn lint_overrides_parse() {
    let m =
        p("[project]\nname = \"x\"\n[lint]\n\"W0206\" = \"off\"\n\"W0209\" = \"warn\"\n").unwrap();
    assert_eq!(
        m.lint_overrides,
        [
            ("W0206".to_string(), manifest::LintLevel::Off),
            ("W0209".to_string(), manifest::LintLevel::Warn),
        ]
    );
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
    std::fs::write(root.path().join("veridikt.toml"), "[project]\nname=\"x\"\n").unwrap();
    assert_eq!(
        manifest::discover(&root.path().join("a/b")),
        Some(root.path().join("veridikt.toml"))
    );
}
