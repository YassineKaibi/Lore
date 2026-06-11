//! Per-language facts: file extensions and comment tokens (spec §7.4).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Python,
    TypeScript,
    Tsx,
    Rust,
}

impl Lang {
    pub fn from_path(p: &std::path::Path) -> Option<Lang> {
        match p.extension()?.to_str()? {
            "py" => Some(Lang::Python),
            "tsx" | "jsx" => Some(Lang::Tsx),
            "ts" | "js" | "mjs" | "cjs" => Some(Lang::TypeScript),
            "rs" => Some(Lang::Rust),
            _ => None,
        }
    }

    pub fn comment_token(self) -> &'static str {
        match self {
            Lang::Python => "#",
            Lang::TypeScript | Lang::Tsx | Lang::Rust => "//",
        }
    }

    pub fn grammar(self) -> tree_sitter::Language {
        match self {
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    /// §7.4 declaration-node rows, verbatim.
    pub fn declaration_kinds(self) -> &'static [&'static str] {
        match self {
            Lang::Python => &["function_definition", "class_definition", "assignment"],
            Lang::TypeScript | Lang::Tsx => &[
                "function_declaration",
                "class_declaration",
                "method_definition",
                "lexical_declaration",
                "variable_declaration",
                "interface_declaration",
                "type_alias_declaration",
                "enum_declaration",
            ],
            Lang::Rust => &[
                "function_item",
                "struct_item",
                "enum_item",
                "trait_item",
                "static_item",
                "const_item",
                "mod_item",
            ],
        }
    }

    /// Skip nodes that precede the declaration as *siblings* rather than
    /// wrapping it — Rust attributes (D-050c). The binder advances past
    /// consecutive ones to the declaration that follows.
    pub fn sibling_skip_kinds(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["attribute_item"],
            _ => &[],
        }
    }
}
