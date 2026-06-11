//! W0207: the CODEOWNERS cross-check (D-010, D-058). The CLI discovers and
//! reads the file; `build` consumes the parsed rules as data — the graph
//! crate never touches the filesystem. Last matching rule wins.

use std::path::PathBuf;

use globset::{GlobBuilder, GlobMatcher};
use lore_intent::Finding;

use crate::{Ctx, OwnedFinding};

/// A parsed CODEOWNERS file: where it was found (for messages) plus its
/// rules in file order (§13, D-058).
#[derive(Debug, Clone)]
pub struct Codeowners {
    pub file: PathBuf,
    pub rules: Vec<CodeownersRule>,
}

/// One CODEOWNERS line: a path pattern and its owner tokens. An empty
/// owner list is an explicitly-unowned path (it never fires W0207).
#[derive(Debug, Clone)]
pub struct CodeownersRule {
    pub pattern: String,
    pub owners: Vec<String>,
}

/// Parse CODEOWNERS text: `#` comments and blank lines are skipped; each
/// remaining line is a pattern followed by whitespace-separated owner
/// tokens (a trailing `#` token starts a comment).
pub fn parse(file: PathBuf, text: &str) -> Codeowners {
    let mut rules = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let Some(pattern) = tokens.next() else {
            continue;
        };
        let owners: Vec<String> = tokens
            .take_while(|t| !t.starts_with('#'))
            .map(str::to_owned)
            .collect();
        rules.push(CodeownersRule {
            pattern: pattern.to_string(),
            owners,
        });
    }
    Codeowners { file, rules }
}

/// D-058c: the v1 pattern subset, translated to path globs. A leading `/`
/// anchors at the project root, otherwise the pattern matches at any depth;
/// a trailing `/` matches everything under the directory; a pattern without
/// a wildcard also matches as a directory prefix. A pattern outside the
/// subset (glob compile failure) skips the rule — a skipped pattern only
/// skips a warning, never invents one (G-7).
fn matchers(pattern: &str) -> Vec<GlobMatcher> {
    let anchored = pattern.starts_with('/');
    let core = pattern.trim_start_matches('/');
    let mut globs = Vec::new();
    if let Some(dir) = core.strip_suffix('/') {
        globs.push(format!("{dir}/**"));
    } else {
        globs.push(core.to_string());
        if !core.contains('*') && !core.contains('?') {
            globs.push(format!("{core}/**"));
        }
    }
    globs
        .into_iter()
        .filter_map(|g| {
            let g = if anchored { g } else { format!("**/{g}") };
            GlobBuilder::new(&g)
                .literal_separator(true)
                .build()
                .ok()
                .map(|g| g.compile_matcher())
        })
        .collect()
}

/// D-058e: a token matches when, after stripping a leading `@`, the whole
/// token or its last `/`-segment equals the declared owner, ASCII-case-
/// insensitively (`@org/payments-team` matches `owner: "payments-team"`).
fn token_matches(token: &str, declared: &str) -> bool {
    let token = token.strip_prefix('@').unwrap_or(token);
    token.eq_ignore_ascii_case(declared)
        || token
            .rsplit('/')
            .next()
            .is_some_and(|seg| seg.eq_ignore_ascii_case(declared))
}

pub(crate) fn check(ctx: &Ctx, codeowners: &Codeowners, findings: &mut Vec<OwnedFinding>) {
    let compiled: Vec<(&CodeownersRule, Vec<GlobMatcher>)> = codeowners
        .rules
        .iter()
        .map(|r| (r, matchers(&r.pattern)))
        .collect();
    for qname in &ctx.order {
        let node = &ctx.nodes[qname];
        let Some(owner) = &node.intent.owner else {
            continue;
        };
        // last matching rule wins (D-058b)
        let winner = compiled
            .iter()
            .rev()
            .find(|(_, ms)| ms.iter().any(|m| m.is_match(&node.loc.file)));
        let Some((rule, _)) = winner else { continue };
        if rule.owners.is_empty() || rule.owners.iter().any(|t| token_matches(t, &owner.value)) {
            continue;
        }
        findings.push(OwnedFinding::new(
            Finding::new(
                "W0207",
                owner.span.clone(),
                format!(
                    "owner \"{}\" on \"{qname}\" disagrees with {}, which maps {} to {}; align the owner clause or CODEOWNERS",
                    owner.value,
                    codeowners.file.display(),
                    node.loc.file.display(),
                    rule.owners.join(" ")
                ),
            ),
            qname,
        ));
    }
}
