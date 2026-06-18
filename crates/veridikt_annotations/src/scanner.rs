//! Line-based `@veridikt` block scanner over host-language comment tokens
//! (spec §7.1–§7.2, D-013). No tree-sitter here: blocks come out raw,
//! binding is the binder's job.

use std::path::Path;

use veridikt_intent::{Finding, Kind, Span, Spanned};

use crate::RawBlock;

/// Scan a file for `@veridikt` blocks (§7.1–§7.2). Only the comment token is
/// language-specific, supplied by the pack (`[scanner] comment_token`); the
/// scanner itself is one rule for every language (D-070).
pub fn scan_source(
    path: &Path,
    source: &str,
    comment_token: &str,
) -> (Vec<RawBlock>, Vec<Finding>) {
    let token = comment_token;
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks = Vec::new();
    let mut findings = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let Some(content) = comment_content(lines[i], token) else {
            i += 1;
            continue;
        };
        if content.trim_end() != "@veridikt" {
            i += 1;
            continue;
        }
        let prev_is_comment = i > 0 && comment_content(lines[i - 1], token).is_some();
        if prev_is_comment {
            // §7.1: a block is a comment run whose *first* content line is
            // `@veridikt`. A contiguous comment line above it (the classic case:
            // a `///` doc comment with no blank line) means this run starts
            // elsewhere, so the block is not recognized. Dropping it silently
            // is the worst failure for an honesty tool, so warn (W0110,
            // D-085): the user almost always meant a block here.
            findings.push(Finding::new(
                "W0110",
                line_span(path, i, content),
                "this \"@veridikt\" is preceded by another comment line, so it does not begin a comment run and the block is ignored; add a blank line between the preceding comment and \"@veridikt\"".into(),
            ));
            i += 1;
            continue;
        }
        let start = i;
        let mut body: Vec<(usize, &str)> = Vec::new();
        i += 1;
        while i < lines.len() && !lines[i].trim().is_empty() {
            match comment_content(lines[i], token) {
                Some(c) => {
                    body.push((i, c));
                    i += 1;
                }
                None => break,
            }
        }
        blocks.push(parse_block(path, start, &body, &mut findings));
    }
    (blocks, findings)
}

/// Strip the comment token plus at most one following space (§7.1).
fn comment_content<'a>(line: &'a str, token: &str) -> Option<&'a str> {
    let rest = line.trim_start().strip_prefix(token)?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

fn parse_block(
    path: &Path,
    start: usize,
    body: &[(usize, &str)],
    findings: &mut Vec<Finding>,
) -> RawBlock {
    let mut kind: Option<Spanned<Kind>> = None;
    let mut name: Option<Spanned<String>> = None;
    let mut raw_clauses = Vec::new();
    let end = body.last().map_or(start, |(i, _)| *i);
    let mut idx = 0usize;
    while idx < body.len() {
        let (i, content) = body[idx];
        idx += 1;
        let span = line_span(path, i, content);
        let t = content.trim_start();
        if let Some(rest) = t.strip_prefix("kind:") {
            let v = rest.trim();
            if kind.is_some() {
                findings.push(Finding::new(
                    "E0106",
                    span,
                    "duplicate \"kind:\" field; a block declares its kind once".into(),
                ));
            } else if let Some(k) = Kind::parse(v) {
                kind = Some(Spanned { value: k, span });
            } else {
                findings.push(Finding::new("E0106", span, format!(
                    "invalid kind \"{v}\"; valid kinds: module, service, workflow, step, state, event, type, error, function, external")));
            }
        } else if let Some(rest) = t.strip_prefix("name:") {
            let v = rest.trim();
            if name.is_some() {
                findings.push(Finding::new(
                    "E0107",
                    span,
                    "duplicate \"name:\" field; a block declares its name once".into(),
                ));
            } else if is_valid_name(v) {
                name = Some(Spanned {
                    value: v.to_string(),
                    span,
                });
            } else {
                findings.push(Finding::new("E0107", span, format!(
                    "invalid name \"{v}\"; a name is dot-separated identifiers, e.g. \"Payment.ledger\"")));
            }
        } else {
            // Clause line. A quoted string may span lines: reassemble until
            // the quote closes, before the clause parser runs (§7.1–§7.2).
            let mut text = content.to_string();
            let mut span = span;
            let mut open = quote_open(content, false);
            while open && idx < body.len() {
                let (j, cont) = body[idx];
                idx += 1;
                text.push('\n');
                text.push_str(cont);
                span.end_line = j as u32 + 1;
                span.end_col = cont.len() as u32 + 1;
                open = quote_open(cont, true);
            }
            raw_clauses.push(Spanned { value: text, span });
        }
    }
    RawBlock {
        start_line: start as u32 + 1,
        end_line: end as u32 + 1,
        kind,
        name,
        raw_clauses,
    }
}

/// Quote state at end of line, given the state at its start. Inside a string,
/// a backslash escapes the next character (§15 StringLit).
fn quote_open(line: &str, mut open: bool) -> bool {
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if open => {
                chars.next();
            }
            '"' => open = !open,
            _ => {}
        }
    }
    open
}

fn line_span(path: &Path, zero_based_line: usize, content: &str) -> Span {
    let line = zero_based_line as u32 + 1;
    Span {
        file: path.to_path_buf(),
        line,
        col: 1,
        end_line: line,
        end_col: content.len() as u32 + 1,
    }
}

/// `Ident ("." Ident)*` per §7.2's name_field production.
fn is_valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|seg| {
            let mut chars = seg.chars();
            matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}
