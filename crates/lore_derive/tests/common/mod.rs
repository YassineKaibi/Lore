//! Shared pack construction for the boundary tests. The derive adapter is
//! pack-driven (D-070): these build the Python and TypeScript `DerivePack`s
//! from the real on-disk pack files (queries via `include_str!`) plus the
//! pinned grammar handle, exactly as `lore_cli`'s loader would — so the tests
//! exercise the shipping packs, not a stand-in.

use lore_derive::DerivePack;
use lore_intent::{ImportStrategy, PackSpec, Tier, WholeAlias};

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Both builtin derive-tier packs; `derive()` selects per file by extension.
pub fn packs() -> Vec<DerivePack> {
    vec![python_pack(), typescript_pack()]
}

fn python_pack() -> DerivePack {
    DerivePack {
        spec: PackSpec {
            name: "python".into(),
            format: 1,
            tier: Tier::Derive,
            grammar_id: Some("tree_sitter_python".into()),
            extensions: strings(&[".py"]),
            comment_token: "#".into(),
            wrappers: strings(&["decorated_definition", "expression_statement"]),
            sibling_skips: Vec::new(),
            value_functions: strings(&["lambda"]),
            mutator_methods: strings(&[
                "append",
                "extend",
                "insert",
                "remove",
                "pop",
                "clear",
                "update",
                "setdefault",
                "sort",
                "reverse",
                "add",
                "discard",
            ]),
            mutator_free_functions: Vec::new(),
            whole_alias: WholeAlias::Full,
            imports: vec![ImportStrategy::RootRelative {
                separator: ".".into(),
                extensions: strings(&[".py"]),
                init_files: strings(&["__init__.py"]),
            }],
            bind_scm: Some(include_str!("../../../../packs/python/queries/bind.scm").into()),
            derive_scm: Some(include_str!("../../../../packs/python/queries/derive.scm").into()),
        },
        grammar: tree_sitter_python::LANGUAGE.into(),
    }
}

fn typescript_pack() -> DerivePack {
    DerivePack {
        spec: PackSpec {
            name: "typescript".into(),
            format: 1,
            tier: Tier::Derive,
            grammar_id: Some("tree_sitter_typescript".into()),
            extensions: strings(&[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]),
            comment_token: "//".into(),
            wrappers: strings(&["export_statement"]),
            sibling_skips: Vec::new(),
            value_functions: strings(&[
                "arrow_function",
                "function_expression",
                "generator_function",
            ]),
            mutator_methods: strings(&[
                "push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "set",
                "delete", "clear", "add",
            ]),
            mutator_free_functions: Vec::new(),
            whole_alias: WholeAlias::Full,
            imports: vec![ImportStrategy::Relative {
                extensions: strings(&[".ts", ".tsx", ".js"]),
                index_files: strings(&["index.ts"]),
            }],
            bind_scm: Some(include_str!("../../../../packs/typescript/queries/bind.scm").into()),
            derive_scm: Some(
                include_str!("../../../../packs/typescript/queries/derive.scm").into(),
            ),
        },
        grammar: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    }
}
