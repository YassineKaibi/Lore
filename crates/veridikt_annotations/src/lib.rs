//! `@veridikt` annotation scanning, binding, and module scoping (spec §7).

mod binder;
mod scanner;
mod scoping;

pub use binder::{Binder, bind_scan_tier};
pub use scanner::scan_source;

/// A language pack activated for scanning/binding (D-070d): the extensions it
/// claims, its comment token, and a compiled `Binder` (None at the `scan`
/// tier — no grammar). The CLI builds these from validated packs and passes
/// them to `scan`; `veridikt_annotations` never reads pack files or the grammar
/// registry itself.
pub struct ActivePack {
    pub extensions: Vec<String>,
    pub comment_token: String,
    pub binder: Option<Binder>,
}

impl ActivePack {
    pub fn claims(&self, path: &std::path::Path) -> bool {
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => {
                let dotted = format!(".{ext}");
                self.extensions.iter().any(|e| e == &dotted)
            }
            None => false,
        }
    }
}

/// A scanned-but-unbound block. 1-based inclusive line spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    pub start_line: u32,
    pub end_line: u32,
    pub kind: Option<veridikt_intent::Spanned<veridikt_intent::Kind>>, // None => default Function (§7.2)
    pub name: Option<veridikt_intent::Spanned<String>>,                // dotted ok
    pub raw_clauses: Vec<veridikt_intent::Spanned<String>>, // one logical clause each (§7.2); feed to veridikt_intent::parse_intent
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
    pub qname: veridikt_intent::QName,
    pub kind: veridikt_intent::Kind,
    pub module: Option<String>, // None => orphan
    pub raw_clauses: Vec<veridikt_intent::Spanned<String>>,
}

/// One scanned file's effective module per §7.5: glob mapping overridden by
/// a top-of-file scoping block; None for orphans. Exposed so the CLI can
/// compute the derivation scope without re-implementing §7.5 (D-061).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileModule {
    pub path: std::path::PathBuf,
    pub module: Option<String>,
}

pub struct ScanResult {
    pub blocks: Vec<ScannedBlock>,
    pub findings: Vec<veridikt_intent::Finding>,
    pub file_modules: Vec<FileModule>,
}

/// The crate boundary: scan, bind, and scope a set of source files.
/// Files with unrecognized extensions are skipped. Output is deterministic:
/// blocks by (file, start line), findings by (file, line, code).

// @veridikt
// purpose: "Scan, bind, and scope a set of source files into qnamed annotation blocks, per-file module assignments, and findings"
// because: "The per-file module assignment is exposed so the CLI can build the derivation scope without re-implementing the scoping rules (D-061)"
// because: "From T8 the pipeline is pack-driven (D-070): each file's pack supplies the comment token and binder, so one generic adapter scans Python, TypeScript, Rust, Go, and Java"
pub fn scan(config: &ScanConfig, files: &[SourceFile], packs: &[ActivePack]) -> ScanResult {
    let globs = scoping::CompiledGlobs::compile(&config.modules);
    let mut blocks = Vec::new();
    let mut findings = Vec::new();
    let mut file_modules = Vec::new();
    for file in files {
        let Some(pack) = packs.iter().find(|p| p.claims(&file.path)) else {
            continue;
        };
        let (raw, mut f) = scan_source(&file.path, &file.text, &pack.comment_token);
        let (bound, f2) = match &pack.binder {
            Some(b) => b.bind(&file.path, &file.text, raw),
            None => bind_scan_tier(&file.path, raw),
        };
        f.extend(f2);
        let (scoped, module) = scoping::scope_file(&globs, file, bound, &mut f);
        blocks.extend(scoped);
        file_modules.push(FileModule {
            path: file.path.clone(),
            module,
        });
        findings.extend(f);
    }
    blocks.sort_by(|a, b| (&a.file, a.block_span.0).cmp(&(&b.file, b.block_span.0)));
    findings.sort_by(|a, b| {
        (&a.span.file, a.span.line, a.code).cmp(&(&b.span.file, b.span.line, b.code))
    });
    file_modules.sort_by(|a, b| a.path.cmp(&b.path));
    ScanResult {
        blocks,
        findings,
        file_modules,
    }
}
