//! `lore.toml` discovery, parsing, and validation (spec §11, D-043).
//! Keys are validated manually — not via `deny_unknown_fields` — so that
//! `E0401` can name the exact offending key.

use std::path::{Path, PathBuf};

use lore_intent::{Finding, Span};
use toml::Value;

#[derive(Debug)]
pub struct Manifest {
    pub name: String,
    pub languages: Vec<String>, // validated against the five §7.4 names
    pub roots: Vec<String>,     // default ["src"]
    pub modules: Vec<lore_annotations::ModuleGlob>, // TOML order preserved
    pub policy: Policy,
    pub lint_overrides: Vec<(String, LintLevel)>, // W-code -> level (D-056)
}

#[derive(Debug)]
pub struct Policy {
    pub unknown: PolicyLevel,
    pub stale: PolicyLevel,
    pub undeclared_effects: UndeclaredEffects,
}
#[derive(Debug)]
pub enum PolicyLevel {
    Warn,
    Error,
}
#[derive(Debug)]
pub enum UndeclaredEffects {
    Off,
    Warn,
}

/// `[lint]` override level (D-056): "warn" restates the default, "off"
/// suppresses the code from lint output and the exit-code computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLevel {
    Warn,
    Off,
}

pub const LANGUAGES: [&str; 5] = ["python", "typescript", "go", "java", "rust"];

/// The W-codes of the §18 registry: the legal `[lint]` key space (D-056a).
/// Grows with §18 in the same PR (G-5).
pub const W_CODES: [&str; 12] = [
    "W0205", "W0206", "W0207", "W0208", "W0209", "W0210", "W0211", "W0212", "W0213", "W0301",
    "W0302", "W0303",
];

const VALID_KEYS: &str = "valid keys: [project] name/languages/roots, [modules] \"<glob>\" = \"<Module>\", [policy] unknown/stale/undeclared_effects, [lint] \"<code>\" = \"<level>\"";

/// Walk up from `start` looking for lore.toml; None => caller reports E0402.
pub fn discover(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join("lore.toml"))
        .find(|candidate| candidate.is_file())
}

/// Parse + validate. The Finding carries the §18 code (E0401 unknown key, E0403 invalid value).
pub fn parse(path: &Path, text: &str) -> Result<Manifest, Finding> {
    let span = || Span {
        file: path.to_path_buf(),
        line: 1,
        col: 1,
        end_line: 1,
        end_col: 1,
    };
    let unknown_key = |key: &str| {
        Finding::new(
            "E0401",
            span(),
            format!("unknown key \"{key}\" in lore.toml; {VALID_KEYS}"),
        )
    };
    let invalid = |key: &str, detail: String| {
        Finding::new(
            "E0403",
            span(),
            format!("invalid value for \"{key}\" in lore.toml: {detail}"),
        )
    };

    let root: Value = text.parse().map_err(|e: toml::de::Error| {
        Finding::new(
            "E0403",
            span(),
            format!("lore.toml is not valid TOML: {}", e.message()),
        )
    })?;
    let Some(root) = root.as_table() else {
        return Err(invalid(
            "lore.toml",
            "expected a table at the top level".into(),
        ));
    };

    let mut m = Manifest {
        name: String::new(),
        languages: vec!["python".into(), "typescript".into()],
        roots: vec!["src".into()],
        modules: Vec::new(),
        policy: Policy {
            unknown: PolicyLevel::Warn,
            stale: PolicyLevel::Warn,
            undeclared_effects: UndeclaredEffects::Off,
        },
        lint_overrides: Vec::new(),
    };

    for (key, value) in root {
        match key.as_str() {
            "project" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| invalid("project", "expected a table".into()))?;
                for (k, v) in table {
                    match k.as_str() {
                        "name" => m.name = string_value("project.name", v, &invalid)?,
                        "languages" => {
                            m.languages = string_array("project.languages", v, &invalid)?;
                            for lang in &m.languages {
                                if !LANGUAGES.contains(&lang.as_str()) {
                                    return Err(invalid(
                                        "project.languages",
                                        format!(
                                            "unknown language \"{lang}\"; valid languages: {}",
                                            LANGUAGES.join(", ")
                                        ),
                                    ));
                                }
                            }
                        }
                        "roots" => m.roots = string_array("project.roots", v, &invalid)?,
                        other => return Err(unknown_key(other)),
                    }
                }
            }
            "modules" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| invalid("modules", "expected a table".into()))?;
                for (glob, v) in table {
                    let module = v
                        .as_str()
                        .ok_or_else(|| invalid(glob, "a module name must be a string".into()))?;
                    // Validate glob syntax here so a malformed pattern fails
                    // loudly (E0403) instead of being silently dropped at match
                    // time (resolves the Annotations.scan §8.6 open question).
                    if let Err(e) = globset::Glob::new(glob) {
                        return Err(invalid(glob, format!("not a valid glob pattern: {e}")));
                    }
                    m.modules.push(lore_annotations::ModuleGlob {
                        glob: glob.clone(),
                        module: module.to_string(),
                    });
                }
            }
            "policy" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| invalid("policy", "expected a table".into()))?;
                for (k, v) in table {
                    match k.as_str() {
                        "unknown" => {
                            m.policy.unknown = policy_level("policy.unknown", v, &invalid)?
                        }
                        "stale" => m.policy.stale = policy_level("policy.stale", v, &invalid)?,
                        "undeclared_effects" => {
                            m.policy.undeclared_effects = match string_value(
                                "policy.undeclared_effects",
                                v,
                                &invalid,
                            )?
                            .as_str()
                            {
                                "off" => UndeclaredEffects::Off,
                                "warn" => UndeclaredEffects::Warn,
                                other => {
                                    return Err(invalid(
                                        "policy.undeclared_effects",
                                        format!(
                                            "\"{other}\" is not a level; expected \"off\" or \"warn\""
                                        ),
                                    ));
                                }
                            }
                        }
                        other => return Err(unknown_key(other)),
                    }
                }
            }
            "lint" => {
                let table = value
                    .as_table()
                    .ok_or_else(|| invalid("lint", "expected a table".into()))?;
                for (code, v) in table {
                    // D-056a: only registry W-codes can be overridden — E
                    // findings can never be silenced, and a typo'd code must
                    // fail loudly rather than silently fail to suppress.
                    if !W_CODES.contains(&code.as_str()) {
                        return Err(Finding::new(
                            "E0401",
                            span(),
                            format!(
                                "unknown key \"{code}\" in lore.toml [lint]; only §18 W-codes can be overridden ({})",
                                W_CODES.join(", ")
                            ),
                        ));
                    }
                    let level = match string_value(code, v, &invalid)?.as_str() {
                        "warn" => LintLevel::Warn,
                        "off" => LintLevel::Off,
                        other => {
                            return Err(invalid(
                                code,
                                format!(
                                    "\"{other}\" is not a lint level; expected \"warn\" or \"off\""
                                ),
                            ));
                        }
                    };
                    m.lint_overrides.push((code.clone(), level));
                }
            }
            other => return Err(unknown_key(other)),
        }
    }
    Ok(m)
}

fn string_value(
    key: &str,
    v: &Value,
    invalid: &dyn Fn(&str, String) -> Finding,
) -> Result<String, Finding> {
    v.as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid(key, "expected a string".into()))
}

fn string_array(
    key: &str,
    v: &Value,
    invalid: &dyn Fn(&str, String) -> Finding,
) -> Result<Vec<String>, Finding> {
    v.as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|i| i.as_str().map(str::to_owned))
                .collect()
        })
        .ok_or_else(|| invalid(key, "expected an array of strings".into()))
}

fn policy_level(
    key: &str,
    v: &Value,
    invalid: &dyn Fn(&str, String) -> Finding,
) -> Result<PolicyLevel, Finding> {
    match string_value(key, v, invalid)?.as_str() {
        "warn" => Ok(PolicyLevel::Warn),
        "error" => Ok(PolicyLevel::Error),
        other => Err(invalid(
            key,
            format!("\"{other}\" is not a level; expected \"warn\" or \"error\""),
        )),
    }
}
