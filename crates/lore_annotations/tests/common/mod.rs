// Shared across several test binaries; each uses a subset.
#![allow(dead_code)]

//! Test plumbing: build the builtin packs as adapters straight from the
//! workspace `packs/` query files, using the dev-dependency grammar crates.
//! The CLI's conformance harness exercises the same packs end to end; these
//! crate-boundary tests keep the per-language assertions close to the binder.

use lore_annotations::{ActivePack, Binder};
use lore_intent::{PackSpec, Span, Tier};

fn span() -> Span {
    Span {
        file: "pack".into(),
        line: 1,
        col: 1,
        end_line: 1,
        end_col: 1,
    }
}

fn spec(
    name: &str,
    exts: &[&str],
    token: &str,
    wrappers: &[&str],
    skips: &[&str],
    bind: &str,
) -> PackSpec {
    PackSpec {
        name: name.into(),
        format: 1,
        tier: Tier::Bind,
        grammar_id: Some(format!("tree_sitter_{name}")),
        extensions: exts.iter().map(|s| s.to_string()).collect(),
        comment_token: token.into(),
        wrappers: wrappers.iter().map(|s| s.to_string()).collect(),
        sibling_skips: skips.iter().map(|s| s.to_string()).collect(),
        value_functions: Vec::new(),
        mutator_methods: Vec::new(),
        mutator_free_functions: Vec::new(),
        whole_alias: lore_intent::WholeAlias::Full,
        imports: Vec::new(),
        bind_scm: Some(bind.into()),
        derive_scm: None,
    }
}

pub fn python_binder() -> Binder {
    let s = spec(
        "python",
        &[".py"],
        "#",
        &["decorated_definition", "expression_statement"],
        &[],
        include_str!("../../../../packs/python/queries/bind.scm"),
    );
    Binder::new(&s, &tree_sitter_python::LANGUAGE.into(), span()).unwrap()
}

pub fn typescript_binder() -> Binder {
    let s = spec(
        "typescript",
        &[".ts"],
        "//",
        &["export_statement"],
        &[],
        include_str!("../../../../packs/typescript/queries/bind.scm"),
    );
    Binder::new(
        &s,
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        span(),
    )
    .unwrap()
}

pub fn rust_binder() -> Binder {
    let s = spec(
        "rust",
        &[".rs"],
        "//",
        &[],
        &["attribute_item"],
        include_str!("../../../../packs/rust/queries/bind.scm"),
    );
    Binder::new(&s, &tree_sitter_rust::LANGUAGE.into(), span()).unwrap()
}

fn pack(exts: &[&str], token: &str, binder: Binder) -> ActivePack {
    ActivePack {
        extensions: exts.iter().map(|s| s.to_string()).collect(),
        comment_token: token.into(),
        binder: Some(binder),
    }
}

/// The full builtin pack set, for `scan()`-level tests (scoping, Rust).
pub fn packs() -> Vec<ActivePack> {
    vec![
        pack(&[".py"], "#", python_binder()),
        pack(
            &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"],
            "//",
            typescript_binder(),
        ),
        pack(&[".rs"], "//", rust_binder()),
    ]
}
