//! Compares a folder against a recipe. Quick mode trusts a file whose size and modification time still match
//! its last verified stamp *and* whose stamped fingerprint is the one the recipe wants, so a new revision only
//! re-hashes the files that changed. Full mode hashes every exact file.

use crate::{
    hash,
    model::{norm_path, Mode, Recipe},
    progress::Progress,
    state::{mtime_ns, Stamp, State},
};
use std::{collections::BTreeMap, fs, io, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    Quick,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Good,
    Missing,
    Wrong,
}

#[derive(Debug, Default)]
pub struct Inspection {
    /// Status of every recipe file, keyed by normalized path.
    pub statuses: BTreeMap<String, FileStatus>,
    /// Stamps of the exact files found good, keyed by normalized path.
    pub stamps: BTreeMap<String, Stamp>,
    /// `absent` entries that exist in the folder.
    pub absent_present: Vec<String>,
}

pub fn inspect(
    folder: &Path,
    recipe: &Recipe,
    state: Option<&State>,
    mode: CheckMode,
    progress: &mut dyn FnMut(Progress),
) -> io::Result<Inspection> {
    let mut out = Inspection::default();
    let total = recipe.files.len() as u64;
    for (i, entry) in recipe.files.iter().enumerate() {
        progress(Progress::Checking {
            done: i as u64,
            total,
        });
        let key = norm_path(&entry.path);
        let path = folder.join(&entry.path);
        let status = match fs::metadata(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => FileStatus::Missing,
            Err(e) => return Err(e),
            Ok(m) if !m.is_file() => FileStatus::Wrong,
            Ok(_) if entry.mode == Mode::Seed => FileStatus::Good,
            Ok(m) => {
                let want = entry.sha256.as_deref().unwrap_or_default();
                let trusted = state
                    .and_then(|s| s.files.get(&key))
                    .filter(|st| mode == CheckMode::Quick && st.sha256 == want && st.matches(&m));
                if Some(m.len()) != entry.size {
                    FileStatus::Wrong
                } else if let Some(st) = trusted {
                    out.stamps.insert(key.clone(), st.clone());
                    FileStatus::Good
                } else {
                    let got = hash::sha256_file(&path)?;
                    if got == want {
                        out.stamps.insert(
                            key.clone(),
                            Stamp {
                                size: m.len(),
                                mtime_ns: mtime_ns(&m),
                                sha256: got,
                            },
                        );
                        FileStatus::Good
                    } else {
                        FileStatus::Wrong
                    }
                }
            }
        };
        out.statuses.insert(key, status);
    }
    progress(Progress::Checking { done: total, total });
    out.absent_present = recipe
        .absent
        .iter()
        .filter(|a| folder.join(a).exists())
        .cloned()
        .collect();
    Ok(out)
}
