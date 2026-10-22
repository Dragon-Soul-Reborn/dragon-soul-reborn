//! The launcher's own folder inside a game folder (`.reborn/`) and the record of verified files.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub fn reborn_dir(folder: &Path) -> PathBuf {
    folder.join(".reborn")
}

pub fn downloads_dir(folder: &Path) -> PathBuf {
    reborn_dir(folder).join("downloads")
}

pub fn staging_dir(folder: &Path) -> PathBuf {
    reborn_dir(folder).join("staging")
}

fn state_path(folder: &Path) -> PathBuf {
    reborn_dir(folder).join("state.json")
}

/// A file as it was when its fingerprint was last verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub size: u64,
    pub mtime_ns: u64,
    pub sha256: String,
}

impl Stamp {
    /// Size and modification time of `path` now, with a fingerprint the caller just verified.
    pub fn of(path: &Path, sha256: &str) -> io::Result<Stamp> {
        let m = fs::metadata(path)?;
        Ok(Stamp {
            size: m.len(),
            mtime_ns: mtime_ns(&m),
            sha256: sha256.to_string(),
        })
    }

    /// True while the file still has the size and modification time recorded here.
    pub fn matches(&self, m: &fs::Metadata) -> bool {
        self.size == m.len() && self.mtime_ns == mtime_ns(m)
    }
}

pub fn mtime_ns(m: &fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Verified files, keyed by normalized path, for the game revision they were verified against.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub game: String,
    pub files: BTreeMap<String, Stamp>,
}

impl State {
    /// None when there is no state file or it can't be read; the caller then checks everything.
    pub fn load(folder: &Path) -> Option<State> {
        serde_json::from_slice(&fs::read(state_path(folder)).ok()?).ok()
    }

    pub fn save(&self, folder: &Path) -> io::Result<()> {
        fs::create_dir_all(reborn_dir(folder))?;
        let tmp = reborn_dir(folder).join("state.json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).expect("state serializes"),
        )?;
        fs::rename(tmp, state_path(folder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        assert!(State::load(dir.path()).is_none());
        let mut s = State {
            game: "g".into(),
            files: BTreeMap::new(),
        };
        s.files.insert(
            "a.txt".into(),
            Stamp {
                size: 1,
                mtime_ns: 2,
                sha256: "x".into(),
            },
        );
        s.save(dir.path()).unwrap();
        assert_eq!(State::load(dir.path()), Some(s));
    }

    #[test]
    fn an_unreadable_state_file_counts_as_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(reborn_dir(dir.path())).unwrap();
        fs::write(state_path(dir.path()), b"{not json").unwrap();
        assert!(State::load(dir.path()).is_none());
    }
}
