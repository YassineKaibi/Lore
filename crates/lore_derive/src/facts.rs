//! Per-file extraction facts: the cacheable unit (D-064). Everything in here
//! is a pure function of one file's content (plus the state-symbol
//! descriptors, which are part of the cache key); anything cross-file lives
//! in resolve.rs and is recomputed every run.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DeclKind {
    Function,
    Type,
}

/// 1-based, like every Span in the §13 contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SpanFact {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DeclFact {
    pub name: String,
    pub kind: DeclKind,
    pub span: SpanFact,
    /// Enclosing declaration (a method's class, a nested function's parent).
    pub parent: Option<usize>,
}

/// Callee shapes the v1 resolver understands (§8.2, D-062). Anything else
/// is not extracted; its call expression still counts as a CallFact with an
/// unresolvable callee so the drop is counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CalleeFact {
    /// `f(...)` — same-file declaration or named import.
    Bare(String),
    /// `m.f(...)` — whole-module / namespace import alias.
    Attr { obj: String, name: String },
    /// `x.m(...)` where x was constructed from a same-file class in the
    /// same function (D-062e); class is a decl index.
    Method { class_decl: usize, name: String },
    /// A call expression whose callee matches no v1 shape: always dropped,
    /// always counted (§8.2 rule 3).
    Opaque,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CallFact {
    pub callee: CalleeFact,
    /// Index of the nearest enclosing derived-function decl; None when the
    /// call sits at module/class level or inside a lambda/arrow (D-062a).
    pub enclosing: Option<usize>,
    pub span: SpanFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ImportFact {
    /// Python `import m [as a]`, TS `import * as a from "m"`.
    Whole { module: String, alias: String },
    /// Python `from m import n [as a]`, TS `import { n [as a] } from "m"`.
    Named {
        module: String,
        name: String,
        alias: String,
    },
}

impl ImportFact {
    pub(crate) fn module(&self) -> &str {
        match self {
            ImportFact::Whole { module, .. } | ImportFact::Named { module, .. } => module,
        }
    }
}

/// One occurrence of a state symbol (§8.3), classified Write/Read at
/// extraction time — classification is positional, not cross-file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TouchFact {
    /// Index into the state-symbol list (part of the cache key, so stable).
    pub state: usize,
    pub write: bool,
    pub enclosing: Option<usize>,
    /// The import the symbol is visible through; None means a bare
    /// own-module occurrence (validity checked at extraction, D-062d).
    pub via_import: Option<usize>,
    pub span: SpanFact,
}

/// A module declaration (`mod x;` / `mod x { }`), for the `rust_use_paths`
/// resolver's crate module tree (§8.6.3, D-078). `inline` is true for a
/// body-bearing `mod x { }` (items live in the same file) and false for an
/// external `mod x;` (items live in a sibling file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModFact {
    pub name: String,
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct FileFacts {
    pub decls: Vec<DeclFact>,
    pub calls: Vec<CallFact>,
    pub imports: Vec<ImportFact>,
    pub touches: Vec<TouchFact>,
    /// Module declarations (D-078). `#[serde(default)]` so cache entries
    /// written before this field degrade to a miss, not a hard error (D-064).
    #[serde(default)]
    pub mods: Vec<ModFact>,
}
