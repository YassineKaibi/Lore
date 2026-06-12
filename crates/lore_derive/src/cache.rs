//! The extraction-facts cache (D-064, G-9): one JSON file per source file
//! under `.lore-cache/derive/`, keyed by a content hash over everything the
//! extraction depends on. Best-effort by design — any read or write failure
//! falls back to re-extraction, and the directory is safe to delete (§10.7).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::facts::FileFacts;
use crate::lang::Language;
use crate::{SourceUnit, StateSymbol};

/// Bump on any change to FileFacts or extraction semantics: a stale shape
/// must miss, never deserialize into wrong facts.
const FORMAT_VERSION: u32 = 1;

/// The full key string is stored inside the entry and compared on load, so
/// a hash collision degrades to a cache miss, never to wrong facts (G-7).
#[derive(Serialize, Deserialize)]
struct Entry {
    key: String,
    facts: FileFacts,
}

pub(crate) struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub(crate) fn new(dir: &Path) -> Cache {
        Cache {
            dir: dir.join("derive"),
        }
    }

    pub(crate) fn load(&self, key: &str) -> Option<FileFacts> {
        let text = std::fs::read_to_string(self.path(key)).ok()?;
        let entry: Entry = serde_json::from_str(&text).ok()?;
        (entry.key == key).then_some(entry.facts)
    }

    pub(crate) fn store(&self, key: &str, facts: &FileFacts) {
        let Ok(()) = std::fs::create_dir_all(&self.dir) else {
            return;
        };
        if let Ok(json) = serde_json::to_string(&Entry {
            key: key.to_string(),
            facts: facts.clone(),
        }) {
            let _ = std::fs::write(self.path(key), json);
        }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir
            .join(format!("{:016x}.json", fnv1a64(key.as_bytes())))
    }
}

/// Everything extraction reads (D-064): format version, language variant,
/// path, content, the file's module, the import roots, and the state-symbol
/// descriptors (extraction pre-matches occurrences against them).
pub(crate) fn key(
    language: Language,
    file: &SourceUnit,
    roots: &[String],
    states: &[StateSymbol],
) -> String {
    use std::fmt::Write;
    let mut key = format!(
        "v{FORMAT_VERSION}\x1f{}\x1f{}\x1f{}\x1f{}\x1f",
        language.name(),
        file.path.display(),
        file.module,
        roots.join("\x1e"),
    );
    for s in states {
        write!(
            key,
            "{}\x1e{}\x1e{}\x1e{}\x1f",
            s.qname,
            s.identifier,
            s.file.display(),
            s.module
        )
        .expect("writing to a String cannot fail");
    }
    key.push('\x1f');
    key.push_str(&file.text);
    key
}

/// FNV-1a 64: stable across runs and releases (std hashers are not), no
/// dependency, and collisions only cost a re-extraction (the stored key is
/// compared on load).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
