//! The grammar registry: pack `[grammar] name` → statically linked
//! tree-sitter handle (spec §8.6.1, D-070d). This is the *only* place a
//! grammar id becomes a `tree_sitter::Language`; the grammar crates are
//! dependencies of `veridikt_cli` alone. An unknown id is `E0413`.

/// Resolve a builtin grammar id to its handle. `None` => `E0413`.
pub fn lookup(id: &str) -> Option<tree_sitter::Language> {
    Some(match id {
        "tree_sitter_python" => tree_sitter_python::LANGUAGE.into(),
        "tree_sitter_typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tree_sitter_tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "tree_sitter_rust" => tree_sitter_rust::LANGUAGE.into(),
        "tree_sitter_go" => tree_sitter_go::LANGUAGE.into(),
        "tree_sitter_java" => tree_sitter_java::LANGUAGE.into(),
        _ => return None,
    })
}

/// Known ids, for the `E0413` message. Grows with the registry above.
pub const KNOWN: &[&str] = &[
    "tree_sitter_python",
    "tree_sitter_typescript",
    "tree_sitter_tsx",
    "tree_sitter_rust",
    "tree_sitter_go",
    "tree_sitter_java",
];
