//! `@lore` annotation scanning, binding, and module scoping (spec §7).

mod binder;
mod lang;
mod scanner;
mod scoping;

pub use binder::bind;
pub use lang::Lang;
pub use scanner::scan_source;

/// A scanned-but-unbound block. 1-based inclusive line spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    pub start_line: u32,
    pub end_line: u32,
    pub kind: Option<lore_intent::Spanned<lore_intent::Kind>>, // None => default Function (§7.2)
    pub name: Option<lore_intent::Spanned<String>>,            // dotted ok
    pub raw_clauses: Vec<lore_intent::Spanned<String>>, // one logical clause each (§7.2); feed to lore_intent::parse_intent
}

/// A block bound (or not) to its subject declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundBlock {
    pub block: RawBlock,
    pub subject: Option<Subject>, // None for scoping blocks (D-042)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub identifier: Option<String>, // None when extraction needs name: (multi-target)
    pub start_line: u32,            // subject span, 1-based inclusive, body included
    pub end_line: u32,
    pub node_kind: String, // tree-sitter node kind, for messages
}

pub struct ScanConfig {
    pub modules: Vec<ModuleGlob>, // manifest order matters for E0103 fallback
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGlob {
    pub glob: String,
    pub module: String,
}

pub struct SourceFile {
    pub path: std::path::PathBuf,
    pub text: String,
} // path relative to project root

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedBlock {
    pub file: std::path::PathBuf,
    pub block_span: (u32, u32),
    pub subject: Option<String>, // host identifier as written in source
    pub subject_span: Option<(u32, u32)>,
    pub qname: lore_intent::QName,
    pub kind: lore_intent::Kind,
    pub module: Option<String>, // None => orphan
    pub raw_clauses: Vec<lore_intent::Spanned<String>>,
}

pub struct ScanResult {
    pub blocks: Vec<ScannedBlock>,
    pub findings: Vec<lore_intent::Finding>,
}

/// The crate boundary: scan, bind, and scope a set of source files.
/// Files with unrecognized extensions are skipped. Output is deterministic:
/// blocks by (file, start line), findings by (file, line, code).

// @lore
// purpose: "Scan, bind, and scope a set of source files into qnamed annotation blocks plus findings"
// unknown: "[modules] globs that fail to compile are dropped silently when matching; the manifest layer validates value types but not glob syntax"
pub fn scan(config: &ScanConfig, files: &[SourceFile]) -> ScanResult {
    let globs = scoping::CompiledGlobs::compile(&config.modules);
    let mut blocks = Vec::new();
    let mut findings = Vec::new();
    for file in files {
        let Some(lang) = Lang::from_path(&file.path) else {
            continue;
        };
        let (raw, mut f) = scan_source(&file.path, &file.text, lang);
        let (bound, f2) = bind(&file.path, &file.text, lang, raw);
        f.extend(f2);
        blocks.extend(scoping::scope_file(&globs, file, bound, &mut f));
        findings.extend(f);
    }
    blocks.sort_by(|a, b| (&a.file, a.block_span.0).cmp(&(&b.file, b.block_span.0)));
    findings.sort_by(|a, b| {
        (&a.span.file, a.span.line, a.code).cmp(&(&b.span.file, b.span.line, b.code))
    });
    ScanResult { blocks, findings }
}
