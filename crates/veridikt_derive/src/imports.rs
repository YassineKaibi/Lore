//! The import-resolution strategy library (§8.2 rule 2, D-071): a built-in set
//! of strategies selected and parameterized per pack, tried in order, first
//! resolution wins; when none resolves the reference drops and is counted
//! (§8.2 rule 3). Strategies are pure functions over (specifier, importing
//! file, project data) — they never touch the filesystem (D-058 precedent).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use veridikt_intent::ImportStrategy;

/// Project data the strategies resolve against: import roots, the candidate
/// file set (normalized path -> index), language-manifest texts keyed by the
/// manifest's directory (for `manifest_prefix`), and the Rust crate module
/// tree (for `rust_use_paths`, D-078): each file's module path and the inverse
/// path -> file index. The module maps are empty for non-Rust packs.
pub(crate) struct ProjectData<'a> {
    pub roots: &'a [String],
    pub files: &'a HashMap<&'a Path, usize>,
    pub manifests: &'a HashMap<PathBuf, String>,
    pub module_paths: &'a HashMap<&'a Path, Vec<String>>,
    pub modules_by_path: &'a HashMap<Vec<String>, usize>,
}

pub(crate) fn resolve(
    strategies: &[ImportStrategy],
    module: &str,
    importer: &Path,
    data: &ProjectData<'_>,
) -> Option<usize> {
    strategies
        .iter()
        .find_map(|s| try_strategy(s, module, importer, data))
}

fn try_strategy(
    strategy: &ImportStrategy,
    module: &str,
    importer: &Path,
    data: &ProjectData<'_>,
) -> Option<usize> {
    match strategy {
        ImportStrategy::Relative {
            extensions,
            index_files,
        } => {
            if !(module.starts_with("./") || module.starts_with("../")) {
                return None;
            }
            let base = normalize(&importer.parent().unwrap_or(Path::new("")).join(module));
            find(&base, extensions, index_files, data.files)
        }
        ImportStrategy::RootRelative {
            separator,
            extensions,
            init_files,
        } => {
            for root in data.roots {
                let mut base = PathBuf::from(root);
                for seg in module.split(separator.as_str()) {
                    base.push(seg);
                }
                if let Some(i) = find(&normalize(&base), extensions, init_files, data.files) {
                    return Some(i);
                }
            }
            None
        }
        ImportStrategy::PackageDir { extensions } => {
            let base = normalize(&importer.parent().unwrap_or(Path::new("")).join(module));
            find(&base, extensions, &[], data.files)
        }
        ImportStrategy::ManifestPrefix {
            manifest_file,
            prefix_key,
        } => {
            // Walk up from the importer for the manifest; strip its declared
            // module prefix, then resolve the remainder as a directory path
            // under the manifest's directory.
            let (dir, text) = find_manifest(importer, manifest_file, data.manifests)?;
            let prefix = manifest_value(&text, prefix_key)?;
            let rest = module.strip_prefix(&prefix)?.trim_start_matches('/');
            let base = normalize(&dir.join(rest));
            // Bare directory reference: any sibling source file resolves it.
            data.files
                .iter()
                .find(|(p, _)| p.parent() == Some(base.as_path()))
                .map(|(_, &i)| i)
        }
        // The named-impl escape hatch (D-071b): dispatch on the registered
        // name. The loader (E0414) guarantees the name is known.
        ImportStrategy::Custom { name } => match name.as_str() {
            "rust_use_paths" => rust_use_paths(module, importer, data),
            _ => None,
        },
    }
}

/// `rust_use_paths` (D-071c/D-078): resolve a `use` path's module portion
/// through the crate module tree (`data.module_paths`/`modules_by_path`),
/// which the directory tree alone cannot express. A leading
/// `crate`/`self`/`super` anchors the path; `self` is the importer's own
/// module, `super` its parent. A bare or external-crate path is out of v1
/// scope and drops (None -> counted, G-7).
fn rust_use_paths(spec: &str, importer: &Path, data: &ProjectData<'_>) -> Option<usize> {
    let mut segs = spec.split("::");
    let first = segs.next()?;
    let mut path: Vec<String> = match first {
        "crate" => vec!["crate".to_string()],
        "self" => data.module_paths.get(importer)?.clone(),
        "super" => {
            let mut p = data.module_paths.get(importer)?.clone();
            p.pop()?; // climb to the parent module
            p
        }
        _ => return None,
    };
    for seg in segs {
        path.push(seg.to_string());
    }
    data.modules_by_path.get(&path).copied()
}

/// Try `base`, `base<ext>` for each extension, and `base/<index>` for each
/// index file, in that order; first that names a known file wins.
fn find(
    base: &Path,
    extensions: &[String],
    index_files: &[String],
    files: &HashMap<&Path, usize>,
) -> Option<usize> {
    if let Some(&i) = files.get(base) {
        return Some(i);
    }
    let raw = base.as_os_str().to_string_lossy();
    for ext in extensions {
        if let Some(&i) = files.get(PathBuf::from(format!("{raw}{ext}")).as_path()) {
            return Some(i);
        }
    }
    for index in index_files {
        if let Some(&i) = files.get(base.join(index).as_path()) {
            return Some(i);
        }
    }
    None
}

fn find_manifest(
    importer: &Path,
    manifest_file: &str,
    manifests: &HashMap<PathBuf, String>,
) -> Option<(PathBuf, String)> {
    let mut dir = importer.parent();
    while let Some(d) = dir {
        if let Some(text) = manifests.get(&d.join(manifest_file)) {
            return Some((d.to_path_buf(), text.clone()));
        }
        dir = d.parent();
    }
    None
}

/// Minimal `key value` lookup for line-oriented manifests (e.g. go.mod's
/// `module <path>`). Not TOML — go.mod has its own grammar; this reads the
/// first `<prefix_key> <value>` line.
fn manifest_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix(key)
            .map(|rest| rest.trim().to_string())
            .filter(|_| line.starts_with(key))
    })
}

/// Lexical normalization: drop `.`, resolve `..` against earlier segments.
/// Project paths are root-relative, so this cannot escape into surprises.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}
