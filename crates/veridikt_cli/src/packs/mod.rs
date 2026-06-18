//! Language-pack loading and validation (spec §8.6, D-070/D-071, band
//! E041x). `veridikt_cli` is the only crate that touches grammar crates: it
//! parses and structurally validates each pack, resolves its grammar id to a
//! statically linked handle, and hands the `PackSpec` plus the handle to the
//! adapters as data (D-070d). A pack that fails any check here is not
//! activated -- the language is simply not loaded, never partially.
//!
//! Validation is a pure function of a `PackSource` (the pack's raw bytes,
//! independent of whether they were embedded at build time or read from a
//! directory), so it is filesystem-free and testable at the boundary (G-4).
//! Unhappy path first (G-11): every `E041x` class has a malformed-pack test.

pub mod builtin;
mod grammar;

use std::path::{Path, PathBuf};

use toml::Value;
use veridikt_intent::{Finding, ImportStrategy, PackSpec, Span, Tier, WholeAlias};

/// Turn a validated pack into a `veridikt_annotations` adapter, compiling
/// `bind.scm` against the grammar at activation (D-070d); a bad capture is
/// `E0411`. Scan-tier packs get no binder.
pub fn activate(pack: &LoadedPack) -> Result<veridikt_annotations::ActivePack, Finding> {
    let span = Span {
        file: PathBuf::from(format!("packs/{}/queries/bind.scm", pack.spec.name)),
        line: 1,
        col: 1,
        end_line: 1,
        end_col: 1,
    };
    let binder = match pack.spec.tier {
        Tier::Scan => None,
        Tier::Bind | Tier::Derive => {
            let grammar = pack
                .grammar
                .as_ref()
                .expect("loader guarantees a grammar at bind+");
            Some(veridikt_annotations::Binder::new(
                &pack.spec, grammar, span,
            )?)
        }
    };
    Ok(veridikt_annotations::ActivePack {
        extensions: pack.spec.extensions.clone(),
        comment_token: pack.spec.comment_token.clone(),
        binder,
    })
}

/// The raw materials of one pack. `fixture_classes` lists the names of
/// non-empty subdirectories of `fixtures/` (the conformance classes present,
/// §8.6.4); the loader checks the mandatory set per tier without touching the
/// filesystem.
pub struct PackSource {
    pub name: String,
    pub manifest_path: PathBuf,
    pub manifest: String,
    pub bind_scm: Option<String>,
    pub derive_scm: Option<String>,
    pub fixture_classes: Vec<String>,
}

/// A validated, activatable pack: the data plus its grammar handle (separate,
/// so `veridikt_intent` stays tree-sitter-free, D-070d). `grammar` is `None` at
/// the `scan` tier (no grammar).
#[derive(Debug)]
pub struct LoadedPack {
    pub spec: PackSpec,
    pub grammar: Option<tree_sitter::Language>,
}

/// Read a pack directory into a `PackSource` (external packs and tests). The
/// embedded builtin packs build their `PackSource` from `include_str!` instead
/// (no filesystem at runtime).
pub fn from_dir(dir: &Path) -> std::io::Result<PackSource> {
    let read_opt = |rel: &str| std::fs::read_to_string(dir.join(rel)).ok();
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let manifest = std::fs::read_to_string(dir.join("veridikt-lang.toml"))?;
    let mut fixture_classes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir.join("fixtures")) {
        for entry in entries.flatten() {
            if entry.path().is_dir() && dir_has_case(&entry.path()) {
                fixture_classes.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    Ok(PackSource {
        name,
        manifest_path: dir.join("veridikt-lang.toml"),
        manifest,
        bind_scm: read_opt("queries/bind.scm"),
        derive_scm: read_opt("queries/derive.scm"),
        fixture_classes,
    })
}

/// A fixture class is "present" only if it holds at least one case directory
/// (an empty class is treated as missing, §8.6.4 / E0415).
fn dir_has_case(class_dir: &Path) -> bool {
    std::fs::read_dir(class_dir)
        .map(|entries| entries.flatten().any(|e| e.path().is_dir()))
        .unwrap_or(false)
}

/// Validate and activate a set of packs. Per-pack validation plus the
/// cross-pack rule that no extension is claimed by two loaded packs (E0410,
/// D-070f). A pack that fails is dropped from the returned set; its finding
/// is collected.
pub fn load_all(sources: &[PackSource]) -> (Vec<LoadedPack>, Vec<Finding>) {
    let mut loaded = Vec::new();
    let mut findings = Vec::new();
    for src in sources {
        match load(src) {
            Ok(pack) => loaded.push(pack),
            Err(f) => findings.push(f),
        }
    }
    // Extension collisions across the surviving packs (D-070f).
    let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut collision: Option<Finding> = None;
    for pack in &loaded {
        for ext in &pack.spec.extensions {
            if let Some(other) = seen.insert(ext.as_str(), pack.spec.name.as_str())
                && other != pack.spec.name
            {
                collision.get_or_insert_with(|| {
                    Finding::new(
                        "E0410",
                        Span {
                            file: PathBuf::from("veridikt-lang.toml"),
                            line: 1,
                            col: 1,
                            end_line: 1,
                            end_col: 1,
                        },
                        format!(
                            "extension \"{ext}\" is claimed by two packs (\"{other}\" and \"{}\"); each extension belongs to exactly one pack",
                            pack.spec.name
                        ),
                    )
                });
            }
        }
    }
    if let Some(f) = collision {
        findings.push(f);
    }
    (loaded, findings)
}

/// Validate one `PackSource` into a `LoadedPack`. Returns the first problem
/// as a Finding (mirroring `manifest::parse`). Order matters: an unknown
/// format version is refused before any other key is interpreted (§8.6.1).
pub fn load(src: &PackSource) -> Result<LoadedPack, Finding> {
    let span = || Span {
        file: src.manifest_path.clone(),
        line: 1,
        col: 1,
        end_line: 1,
        end_col: 1,
    };
    let e0410 = |msg: String| Finding::new("E0410", span(), msg);
    let e0411 = |msg: String| Finding::new("E0411", span(), msg);
    let e0414 = |msg: String| Finding::new("E0414", span(), msg);

    let root: Value = src.manifest.parse().map_err(|e: toml::de::Error| {
        e0410(format!(
            "veridikt-lang.toml is not valid TOML: {}",
            e.message()
        ))
    })?;
    let root = root
        .as_table()
        .ok_or_else(|| e0410("expected a table at the top level of veridikt-lang.toml".into()))?;

    // [pack] table and the format gate, before anything else (§8.6.1).
    let pack = root
        .get("pack")
        .and_then(Value::as_table)
        .ok_or_else(|| e0410("missing required [pack] table in veridikt-lang.toml".into()))?;
    let format = match pack.get("format") {
        Some(Value::Integer(n)) => *n,
        Some(_) => return Err(e0410("[pack] format must be an integer".into())),
        None => return Err(e0410("missing required key [pack] format".into())),
    };
    if format != 1 {
        return Err(Finding::new(
            "E0412",
            span(),
            format!("unsupported pack format version {format}; this build supports version 1"),
        ));
    }

    // [pack] name must equal the pack directory name (§8.6.1).
    let name = str_key(pack, "name", "[pack] name", &e0410)?;
    if name != src.name {
        return Err(e0410(format!(
            "[pack] name \"{name}\" must equal the pack directory name \"{}\"",
            src.name
        )));
    }
    let tier = match str_key(pack, "tier", "[pack] tier", &e0410)?.as_str() {
        "scan" => Tier::Scan,
        "bind" => Tier::Bind,
        "derive" => Tier::Derive,
        other => {
            return Err(e0410(format!(
                "[pack] tier \"{other}\" is invalid; expected \"scan\", \"bind\", or \"derive\""
            )));
        }
    };
    reject_unknown(pack, "[pack]", &["name", "format", "tier"], &e0410)?;

    // Top-level tables: reject any not in the §8.6.1 set.
    reject_unknown(
        root,
        "veridikt-lang.toml",
        &["pack", "grammar", "scanner", "binder", "derive"],
        &e0410,
    )?;

    // [scanner] -- required at every tier.
    let scanner = root
        .get("scanner")
        .and_then(Value::as_table)
        .ok_or_else(|| e0410("missing required [scanner] table".into()))?;
    let extensions = str_array_key(scanner, "extensions", "[scanner] extensions", &e0410)?;
    if extensions.is_empty() {
        return Err(e0410(
            "[scanner] extensions must list at least one extension".into(),
        ));
    }
    let comment_token = str_key(scanner, "comment_token", "[scanner] comment_token", &e0410)?;
    reject_unknown(
        scanner,
        "[scanner]",
        &["extensions", "comment_token"],
        &e0410,
    )?;

    // [grammar] -- required at bind+, forbidden at scan (artifact vs tier).
    let grammar_table = root.get("grammar").and_then(Value::as_table);
    let mut grammar_id = None;
    let mut grammar_handle = None;
    match (tier, grammar_table) {
        (Tier::Scan, Some(_)) => {
            return Err(e0410(
                "[grammar] is present but tier is \"scan\"; a scan-tier pack declares no grammar (§8.6.2)".into(),
            ));
        }
        (Tier::Scan, None) => {}
        (_, None) => {
            return Err(e0410(format!(
                "tier \"{}\" requires a [grammar] table",
                tier.name()
            )));
        }
        (_, Some(g)) => {
            let source = str_key(g, "source", "[grammar] source", &e0410)?;
            match source.as_str() {
                "builtin" => {
                    let id = str_key(g, "name", "[grammar] name", &e0410)?;
                    let handle = grammar::lookup(&id).ok_or_else(|| {
                        Finding::new(
                            "E0413",
                            span(),
                            format!(
                                "unknown builtin grammar \"{id}\"; known grammars: {}",
                                grammar::KNOWN.join(", ")
                            ),
                        )
                    })?;
                    grammar_id = Some(id);
                    grammar_handle = Some(handle);
                }
                "wasm" => {
                    return Err(Finding::new(
                        "E0413",
                        span(),
                        "[grammar] source \"wasm\" is reserved and not accepted in v1 (§8.6.1)"
                            .into(),
                    ));
                }
                other => {
                    return Err(e0410(format!(
                        "[grammar] source \"{other}\" is invalid; expected \"builtin\""
                    )));
                }
            }
            reject_unknown(g, "[grammar]", &["source", "name", "path"], &e0410)?;
        }
    }

    // [binder] -- optional, bind+ only.
    let mut wrappers = Vec::new();
    let mut sibling_skips = Vec::new();
    if let Some(b) = root.get("binder").and_then(Value::as_table) {
        if tier == Tier::Scan {
            return Err(e0410(
                "[binder] is present but tier is \"scan\"; binding needs at least tier \"bind\""
                    .into(),
            ));
        }
        wrappers = opt_str_array(b, "wrappers", "[binder] wrappers", &e0410)?;
        sibling_skips = opt_str_array(b, "sibling_skips", "[binder] sibling_skips", &e0410)?;
        reject_unknown(b, "[binder]", &["wrappers", "sibling_skips"], &e0410)?;
    }

    // [derive] -- value functions + mutators + import strategies, derive tier only.
    let mut value_functions = Vec::new();
    let mut mutator_methods = Vec::new();
    let mut mutator_free_functions = Vec::new();
    let mut whole_alias = WholeAlias::Full;
    let mut imports = Vec::new();
    if let Some(d) = root.get("derive").and_then(Value::as_table) {
        if tier != Tier::Derive {
            return Err(e0410(format!(
                "[derive] is present but tier is \"{}\"; derivation needs tier \"derive\"",
                tier.name()
            )));
        }
        value_functions = opt_str_array(d, "value_functions", "[derive] value_functions", &e0410)?;
        whole_alias = match d.get("whole_alias") {
            None => WholeAlias::Full,
            Some(Value::String(s)) if s == "full" => WholeAlias::Full,
            Some(Value::String(s)) if s == "last_segment" => WholeAlias::LastSegment,
            Some(_) => {
                return Err(e0410(
                    "[derive] whole_alias must be \"full\" or \"last_segment\"".into(),
                ));
            }
        };
        if let Some(m) = d.get("mutators") {
            let m = m
                .as_table()
                .ok_or_else(|| e0410("[derive.mutators] must be a table".into()))?;
            mutator_methods = opt_str_array(m, "methods", "[derive.mutators] methods", &e0410)?;
            mutator_free_functions = opt_str_array(
                m,
                "free_functions",
                "[derive.mutators] free_functions",
                &e0410,
            )?;
            reject_unknown(
                m,
                "[derive.mutators]",
                &["methods", "free_functions"],
                &e0410,
            )?;
        }
        imports = parse_strategies(d, &e0410, &e0414)?;
        reject_unknown(
            d,
            "[derive]",
            &["value_functions", "mutators", "whole_alias", "imports"],
            &e0410,
        )?;
    }
    if tier == Tier::Derive && imports.is_empty() {
        return Err(e0410(
            "tier \"derive\" requires at least one [[derive.imports.strategy]] (§8.6.1)".into(),
        ));
    }

    // Query files vs tier (§8.6.2): present exactly when the tier reaches them.
    let bind_scm = check_artifact(tier, Tier::Bind, "bind.scm", &src.bind_scm, &e0410, &e0411)?;
    let derive_scm = check_artifact(
        tier,
        Tier::Derive,
        "derive.scm",
        &src.derive_scm,
        &e0410,
        &e0411,
    )?;

    // Conformance classes (§8.6.4 / E0415): mandatory cumulative set per tier.
    let required: &[&str] = match tier {
        Tier::Scan => &["scan"],
        Tier::Bind => &["scan", "bind"],
        Tier::Derive => &["scan", "bind", "derive"],
    };
    for class in required {
        if !src.fixture_classes.iter().any(|c| c == class) {
            return Err(Finding::new(
                "E0415",
                span(),
                format!(
                    "pack is missing the mandatory \"{class}\" conformance fixture class for tier \"{}\" (§8.6.4); a pack with no fixtures for its tier cannot be activated",
                    tier.name()
                ),
            ));
        }
    }

    Ok(LoadedPack {
        spec: PackSpec {
            name,
            format: format as u32,
            tier,
            grammar_id,
            extensions,
            comment_token,
            wrappers,
            sibling_skips,
            value_functions,
            mutator_methods,
            mutator_free_functions,
            whole_alias,
            imports,
            bind_scm,
            derive_scm,
        },
        grammar: grammar_handle,
    })
}

/// A query file must be present exactly when the tier reaches `needed_at`:
/// missing at or above that tier is `E0411` (unusable artifact); present
/// below it is `E0410` (an artifact above the declared tier, D-070b).
fn check_artifact(
    tier: Tier,
    needed_at: Tier,
    file: &str,
    content: &Option<String>,
    e0410: &dyn Fn(String) -> Finding,
    e0411: &dyn Fn(String) -> Finding,
) -> Result<Option<String>, Finding> {
    match (tier.at_least(needed_at), content) {
        (true, Some(c)) => Ok(Some(c.clone())),
        (true, None) => Err(e0411(format!(
            "tier \"{}\" requires queries/{file}, which is missing or unreadable",
            tier.name()
        ))),
        (false, Some(_)) => Err(e0410(format!(
            "queries/{file} is present but tier \"{}\" does not use it (§8.6.2)",
            tier.name()
        ))),
        (false, None) => Ok(None),
    }
}

/// `[[derive.imports.strategy]]` is an ordered array of stanzas (D-071).
fn parse_strategies(
    derive: &toml::Table,
    e0410: &dyn Fn(String) -> Finding,
    e0414: &dyn Fn(String) -> Finding,
) -> Result<Vec<ImportStrategy>, Finding> {
    let Some(imports) = derive.get("imports") else {
        return Ok(Vec::new());
    };
    let imports = imports
        .as_table()
        .ok_or_else(|| e0410("[derive.imports] must be a table".into()))?;
    reject_unknown(imports, "[derive.imports]", &["strategy"], e0410)?;
    let Some(arr) = imports.get("strategy") else {
        return Ok(Vec::new());
    };
    let arr = arr.as_array().ok_or_else(|| {
        e0414("[[derive.imports.strategy]] must be an array of strategy tables".into())
    })?;
    let mut out = Vec::new();
    for item in arr {
        let s = item
            .as_table()
            .ok_or_else(|| e0414("each import strategy must be a table".into()))?;
        let kind = s
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| e0414("import strategy is missing required key \"kind\"".into()))?;
        let strategy = match kind {
            "relative" => ImportStrategy::Relative {
                extensions: opt_str_array(s, "extensions", "strategy.extensions", e0414)?,
                index_files: opt_str_array(s, "index_files", "strategy.index_files", e0414)?,
            },
            "root_relative" => ImportStrategy::RootRelative {
                separator: s
                    .get("separator")
                    .and_then(Value::as_str)
                    .unwrap_or(".")
                    .to_string(),
                extensions: opt_str_array(s, "extensions", "strategy.extensions", e0414)?,
                init_files: opt_str_array(s, "init_files", "strategy.init_files", e0414)?,
            },
            "package_dir" => ImportStrategy::PackageDir {
                extensions: opt_str_array(s, "extensions", "strategy.extensions", e0414)?,
            },
            "manifest_prefix" => ImportStrategy::ManifestPrefix {
                manifest_file: req_str(s, "manifest_file", e0414)?,
                prefix_key: req_str(s, "prefix_key", e0414)?,
            },
            "custom" => {
                let name = req_str(s, "name", e0414)?;
                if !is_custom_strategy(&name) {
                    return Err(e0414(format!(
                        "unknown custom import strategy \"{name}\"; no such strategy is registered in veridikt_derive (D-071b)"
                    )));
                }
                ImportStrategy::Custom { name }
            }
            other => {
                return Err(e0414(format!(
                    "unknown import strategy kind \"{other}\"; expected relative, root_relative, package_dir, manifest_prefix, or custom"
                )));
            }
        };
        // Reject unknown keys per kind so a misconfigured strategy fails loud.
        let allowed: &[&str] = match kind {
            "relative" => &["kind", "extensions", "index_files"],
            "root_relative" => &["kind", "separator", "extensions", "init_files"],
            "package_dir" => &["kind", "extensions"],
            "manifest_prefix" => &["kind", "manifest_file", "prefix_key"],
            "custom" => &["kind", "name"],
            _ => unreachable!(),
        };
        reject_unknown(s, "[[derive.imports.strategy]]", allowed, e0414)?;
        out.push(strategy);
    }
    Ok(out)
}

/// The set of custom import-strategy names registered in `veridikt_derive`
/// (D-071b). Mirrors `veridikt_derive::custom_strategies()`; validated here so a
/// typo fails at load with `E0414`.
fn is_custom_strategy(name: &str) -> bool {
    veridikt_derive::custom_strategy_names().contains(&name)
}

// ---- toml helpers (mirroring manifest.rs) ----

fn str_key(
    table: &toml::Table,
    key: &str,
    label: &str,
    err: &dyn Fn(String) -> Finding,
) -> Result<String, Finding> {
    match table.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(err(format!("{label} must be a string"))),
        None => Err(err(format!("missing required key {label}"))),
    }
}

fn req_str(
    table: &toml::Table,
    key: &str,
    err: &dyn Fn(String) -> Finding,
) -> Result<String, Finding> {
    str_key(table, key, &format!("strategy.{key}"), err)
}

fn str_array_key(
    table: &toml::Table,
    key: &str,
    label: &str,
    err: &dyn Fn(String) -> Finding,
) -> Result<Vec<String>, Finding> {
    match table.get(key) {
        Some(v) => str_array(v, label, err),
        None => Err(err(format!("missing required key {label}"))),
    }
}

fn opt_str_array(
    table: &toml::Table,
    key: &str,
    label: &str,
    err: &dyn Fn(String) -> Finding,
) -> Result<Vec<String>, Finding> {
    match table.get(key) {
        Some(v) => str_array(v, label, err),
        None => Ok(Vec::new()),
    }
}

fn str_array(
    v: &Value,
    label: &str,
    err: &dyn Fn(String) -> Finding,
) -> Result<Vec<String>, Finding> {
    v.as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|i| i.as_str().map(str::to_owned))
                .collect()
        })
        .ok_or_else(|| err(format!("{label} must be an array of strings")))
}

fn reject_unknown(
    table: &toml::Table,
    label: &str,
    known: &[&str],
    err: &dyn Fn(String) -> Finding,
) -> Result<(), Finding> {
    for key in table.keys() {
        if !known.contains(&key.as_str()) {
            return Err(err(format!(
                "unknown key \"{key}\" in {label}; valid keys: {}",
                known.join(", ")
            )));
        }
    }
    Ok(())
}
