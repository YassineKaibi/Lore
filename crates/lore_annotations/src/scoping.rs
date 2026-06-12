//! Module scoping and qname assignment (spec §7.5, D-042). Takes one file's
//! bound blocks and the manifest's module globs; produces qnamed blocks.

use globset::{Glob, GlobMatcher};
use lore_intent::{Finding, Kind, QName, Span};

use crate::{BoundBlock, ModuleGlob, ScannedBlock, SourceFile};

pub(crate) struct CompiledGlobs<'a>(Vec<(GlobMatcher, &'a ModuleGlob)>);

impl<'a> CompiledGlobs<'a> {
    pub(crate) fn compile(globs: &'a [ModuleGlob]) -> Self {
        CompiledGlobs(
            globs
                .iter()
                .filter_map(|g| Glob::new(&g.glob).ok().map(|c| (c.compile_matcher(), g)))
                .collect(),
        )
    }
}

/// Returns the qnamed blocks plus the file's effective module (§7.5: glob
/// mapping, overridden by a top-of-file scoping block) — the module feeds
/// the derivation scope (D-061).
pub(crate) fn scope_file(
    globs: &CompiledGlobs<'_>,
    file: &SourceFile,
    bound: Vec<BoundBlock>,
    findings: &mut Vec<Finding>,
) -> (Vec<ScannedBlock>, Option<String>) {
    let file_span = Span {
        file: file.path.clone(),
        line: 1,
        col: 1,
        end_line: 1,
        end_col: 1,
    };

    // Rule 1: manifest globs, first match winning on overlap (E0103).
    let matches: Vec<&ModuleGlob> = globs
        .0
        .iter()
        .filter(|(m, _)| m.is_match(&file.path))
        .map(|(_, g)| *g)
        .collect();
    if let Some(first) = matches.first()
        && let Some(other) = matches.iter().find(|g| g.module != first.module)
    {
        findings.push(Finding::new("E0103", file_span.clone(), format!(
            "{} is claimed by two modules: \"{}\" (glob \"{}\") and \"{}\" (glob \"{}\"); make the [modules] globs in lore.toml disjoint",
            file.path.display(), first.module, first.glob, other.module, other.glob)));
    }
    let mut module: Option<String> = matches.first().map(|g| g.module.clone());

    let mut out = Vec::new();
    let mut workflow: Option<QName> = None;
    let mut has_orphan_subjects = false;

    for (idx, bb) in bound.into_iter().enumerate() {
        let kind = bb.block.kind.as_ref().map_or(Kind::Function, |k| k.value);
        let block_span = (bb.block.start_line, bb.block.end_line);

        if bb.subject.is_none() {
            // Scoping block (binder guarantees it has a name:).
            let name = bb
                .block
                .name
                .as_ref()
                .expect("binder requires name on scoping blocks")
                .value
                .clone();
            let qname = if idx == 0 {
                // Rule 2: file-scoping block overrides the manifest mapping.
                module = Some(name.clone());
                QName::from_dotted(&name)
            } else {
                qualified(module.as_deref(), &name)
            };
            if kind == Kind::Workflow {
                workflow = Some(qname.clone());
            }
            out.push(ScannedBlock {
                file: file.path.clone(),
                block_span,
                subject: None,
                subject_span: None,
                qname,
                kind,
                module: module.clone(),
                raw_clauses: bb.block.raw_clauses,
            });
            continue;
        }

        let subject = bb.subject.unwrap();
        let name = bb
            .block
            .name
            .as_ref()
            .map(|n| n.value.clone())
            .or_else(|| subject.identifier.clone())
            .expect("binder requires name: when no identifier is extractable");

        let qname = if kind == Kind::Step {
            // Rule 5: a step needs an enclosing workflow block above it.
            let Some(wf) = &workflow else {
                findings.push(Finding::new("E0105",
                    Span { file: file.path.clone(), line: bb.block.start_line, col: 1, end_line: bb.block.end_line, end_col: 1 },
                    format!("step \"{name}\" has no enclosing workflow block in this file; add a \"kind: workflow\" block above it")));
                continue;
            };
            QName::from_dotted(&format!("{wf}.{name}"))
        } else {
            if module.is_none() {
                has_orphan_subjects = true;
            }
            qualified(module.as_deref(), &name)
        };

        out.push(ScannedBlock {
            file: file.path.clone(),
            block_span,
            subject: subject.identifier,
            subject_span: Some((subject.start_line, subject.end_line)),
            qname,
            kind,
            module: module.clone(),
            raw_clauses: bb.block.raw_clauses,
        });
    }

    if has_orphan_subjects {
        // Rule 3: one W0208 per file that has annotated subjects.
        findings.push(Finding::new("W0208", file_span, format!(
            "{} belongs to no module, so its subjects get the \"_orphan\" qname prefix; map it under [modules] in lore.toml or add a top-of-file module block",
            file.path.display())));
    }
    (out, module)
}

fn qualified(module: Option<&str>, name: &str) -> QName {
    QName::from_dotted(&format!("{}.{name}", module.unwrap_or("_orphan")))
}
