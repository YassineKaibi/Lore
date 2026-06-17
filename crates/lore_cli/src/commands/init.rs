//! `lore init` (§12): write a starter lore.toml — detect languages, propose
//! `[modules]` globs from directory names. Never overwrites.

use std::collections::BTreeSet;
use std::path::Path;

const SKIP_DIRS: [&str; 4] = [".git", "target", "node_modules", ".lore-cache"];

pub fn run() -> i32 {
    let cwd = std::env::current_dir().expect("cwd must exist");
    let manifest_path = cwd.join("lore.toml");
    if manifest_path.exists() {
        eprintln!("lore.toml already exists here; lore init never overwrites it");
        return 2;
    }

    let mut languages = BTreeSet::new();
    detect_languages(&cwd, &mut languages);

    let root = if cwd.join("src").is_dir() { "src" } else { "." };
    let name = cwd.file_name().map_or_else(
        || "project".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let modules = propose_modules(&cwd, root, &name);
    let mut text = String::new();
    text.push_str("[project]\n");
    text.push_str(&format!("name = \"{name}\"\n"));
    let langs: Vec<String> = languages.iter().map(|l| format!("\"{l}\"")).collect();
    text.push_str(&format!("languages = [{}]\n", langs.join(", ")));
    text.push_str(&format!("roots = [\"{root}\"]\n"));
    text.push_str("\n[modules]\n");
    if modules.is_empty() {
        text.push_str("# \"src/payments/**\" = \"Payment\"\n");
    }
    for (glob, module) in &modules {
        text.push_str(&format!("\"{glob}\" = \"{module}\"\n"));
    }

    std::fs::write(&manifest_path, text).expect("lore init writes into a directory it just read");

    println!("init: wrote lore.toml");
    println!(
        "  languages: {}",
        languages.into_iter().collect::<Vec<_>>().join(", ")
    );
    println!("  roots: {root}");
    println!("  modules: {} proposed", modules.len());
    0
}

fn detect_languages(dir: &Path, out: &mut BTreeSet<&'static str>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            detect_languages(&path, out);
        } else if let Some(lang) = language_of(&path) {
            out.insert(lang);
        }
    }
}

fn language_of(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "py" => Some("python"),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        "rs" => Some("rust"),
        _ => None,
    }
}

/// One proposal per child dir of the root that contains source files, plus --
/// when the root directory itself holds source files directly -- a disjoint
/// `<root>/*` glob so root-level files (`src/main.rs`) are scoped, not orphaned
/// (W0208, §7.5(3); D-081). `*` does not cross `/`, so `<root>/*` matches only
/// direct file children and never overlaps a `<root>/<dir>/**` glob (no E0103).
fn propose_modules(cwd: &Path, root: &str, project: &str) -> Vec<(String, String)> {
    let root_dir = if root == "." {
        cwd.to_path_buf()
    } else {
        cwd.join(root)
    };
    let Ok(entries) = std::fs::read_dir(&root_dir) else {
        return Vec::new();
    };
    let entries: Vec<_> = entries.flatten().collect();
    let mut dirs: Vec<String> = entries
        .iter()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            e.path().is_dir() && !SKIP_DIRS.contains(&name.as_ref()) && !name.starts_with('.')
        })
        .filter(|e| {
            let mut langs = BTreeSet::new();
            detect_languages(&e.path(), &mut langs);
            !langs.is_empty()
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    let mut out: Vec<(String, String)> = dirs
        .into_iter()
        .map(|dir| {
            let glob = if root == "." {
                format!("{dir}/**")
            } else {
                format!("{root}/{dir}/**")
            };
            (glob, pascal_case(&dir))
        })
        .collect();

    // Cover root-level source files with a trailing, disjoint catch-all.
    let has_root_files = entries
        .iter()
        .any(|e| e.path().is_file() && language_of(&e.path()).is_some());
    if has_root_files {
        let glob = if root == "." {
            "*".to_string()
        } else {
            format!("{root}/*")
        };
        out.push((glob, pascal_case(project)));
    }
    out
}

/// "user_accounts" / "user-accounts" -> "UserAccounts".
fn pascal_case(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}
