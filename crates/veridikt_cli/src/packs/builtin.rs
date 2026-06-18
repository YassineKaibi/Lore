//! The builtin packs, embedded at build time (spec §8.6, D-070d). Manifest +
//! query files ride in via `include_str!`; fixtures are read from disk by the
//! conformance harness in CI, not needed at runtime (D-070e), so the embedded
//! `fixture_classes` assert the classes the harness verifies.
//!
//! `veridikt_cli` is the only crate that names grammar crates; `veridikt_annotations`
//! and `veridikt_derive` receive the grammar handle from the loader (D-070d).

use super::PackSource;

/// Path to a builtin pack file, relative to this source file's directory
/// (`crates/veridikt_cli/src/packs/`), reaching the workspace-root `packs/` dir.
macro_rules! pack_file {
    ($name:literal, $rel:literal) => {
        include_str!(concat!("../../../../packs/", $name, "/", $rel))
    };
}

/// Build the embedded builtin `PackSource`s. The `fixture_classes` here are a
/// trusted assertion verified by the conformance harness (D-070e); the runtime
/// loader does not read fixtures from disk for embedded packs.
pub fn sources() -> Vec<PackSource> {
    vec![
        PackSource {
            name: "python".into(),
            manifest_path: "packs/python/veridikt-lang.toml".into(),
            manifest: pack_file!("python", "veridikt-lang.toml").into(),
            bind_scm: Some(pack_file!("python", "queries/bind.scm").into()),
            derive_scm: Some(pack_file!("python", "queries/derive.scm").into()),
            fixture_classes: vec!["scan".into(), "bind".into(), "derive".into()],
        },
        PackSource {
            name: "typescript".into(),
            manifest_path: "packs/typescript/veridikt-lang.toml".into(),
            manifest: pack_file!("typescript", "veridikt-lang.toml").into(),
            bind_scm: Some(pack_file!("typescript", "queries/bind.scm").into()),
            derive_scm: Some(pack_file!("typescript", "queries/derive.scm").into()),
            fixture_classes: vec!["scan".into(), "bind".into(), "derive".into()],
        },
        PackSource {
            name: "rust".into(),
            manifest_path: "packs/rust/veridikt-lang.toml".into(),
            manifest: pack_file!("rust", "veridikt-lang.toml").into(),
            bind_scm: Some(pack_file!("rust", "queries/bind.scm").into()),
            derive_scm: Some(pack_file!("rust", "queries/derive.scm").into()),
            fixture_classes: vec!["scan".into(), "bind".into(), "derive".into()],
        },
        PackSource {
            name: "java".into(),
            manifest_path: "packs/java/veridikt-lang.toml".into(),
            manifest: pack_file!("java", "veridikt-lang.toml").into(),
            bind_scm: Some(pack_file!("java", "queries/bind.scm").into()),
            derive_scm: Some(pack_file!("java", "queries/derive.scm").into()),
            fixture_classes: vec!["scan".into(), "bind".into(), "derive".into()],
        },
        PackSource {
            name: "go".into(),
            manifest_path: "packs/go/veridikt-lang.toml".into(),
            manifest: pack_file!("go", "veridikt-lang.toml").into(),
            bind_scm: Some(pack_file!("go", "queries/bind.scm").into()),
            derive_scm: Some(pack_file!("go", "queries/derive.scm").into()),
            fixture_classes: vec!["scan".into(), "bind".into(), "derive".into()],
        },
    ]
}
