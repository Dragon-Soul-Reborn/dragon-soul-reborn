//! The one routine: make a folder match a recipe, whatever state it is in. Install, repair, resume and update
//! are all this.

use crate::{
    apply::{build, delete_absent, ApplyError},
    extract::Unpacker,
    fetch::{find_local_copy, FetchError, Fetcher},
    inspect::{inspect, CheckMode, FileStatus},
    model::{norm_path, Mode, Recipe},
    plan::{make_plan, Plan},
    progress::Progress,
    state::{downloads_dir, reborn_dir, staging_dir, Stamp, State},
};
use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// Kept free on top of what the routine needs, so the disk is never filled to the last byte.
const SPACE_MARGIN: u64 = 64 << 20;

pub struct Routine {
    pub folder: PathBuf,
    pub fetcher: Fetcher,
    pub unpacker: Unpacker,
    /// Installer or zip copies the player pointed at.
    pub extra_copies: Vec<PathBuf>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub built: Vec<String>,
    pub deleted: Vec<String>,
    /// Files verified on an earlier run that have since disappeared (usually antivirus).
    pub vanished: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RoutineError {
    #[error(transparent)]
    Model(#[from] crate::model::ModelError),
    #[error("close the game before changing its files")]
    GameRunning,
    #[error("not enough free space: {needed} bytes needed, {available} available")]
    NoSpace { needed: u64, available: u64 },
    #[error("can't write to {0}")]
    NotWritable(PathBuf),
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error(transparent)]
    Apply(#[from] ApplyError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl Routine {
    pub fn run(
        &self,
        recipe: &Recipe,
        mode: CheckMode,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<Outcome, RoutineError> {
        recipe.validate(&self.fetcher.allowed_hosts())?;
        if crate::launch::exe_in_use(&self.folder) {
            return Err(RoutineError::GameRunning);
        }
        crate::paths::check_tree(&self.folder, recipe)?;
        let folder = self.folder.as_path();
        ensure_writable(folder)?;
        let state = State::load(folder);
        let mode = if state.is_some() {
            mode
        } else {
            CheckMode::Full
        };
        let inspection = inspect(folder, recipe, state.as_ref(), mode, progress)?;
        let plan = make_plan(recipe, &inspection);
        let vanished = plan
            .build
            .iter()
            .filter(|p| {
                let key = norm_path(p);
                inspection.statuses.get(&key) == Some(&FileStatus::Missing)
                    && state.as_ref().is_some_and(|s| s.files.contains_key(&key))
            })
            .cloned()
            .collect();
        let mut stamps = inspection.stamps.clone();
        if !plan.is_empty() {
            stamps.extend(self.converge(recipe, &plan, cancel, progress)?);
        }
        State {
            game: recipe.game.clone(),
            files: stamps,
        }
        .save(folder)?;
        Ok(Outcome {
            built: plan.build,
            deleted: plan.delete,
            vanished,
        })
    }

    fn converge(
        &self,
        recipe: &Recipe,
        plan: &Plan,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<BTreeMap<String, Stamp>, RoutineError> {
        if crate::launch::exe_in_use(&self.folder) {
            return Err(RoutineError::GameRunning);
        }
        let folder = self.folder.as_path();
        let downloads = downloads_dir(folder);
        let mut paths = BTreeMap::new();
        let mut ours = Vec::new(); // our own downloads, removed once the build succeeds
        let mut missing = Vec::new();
        for id in &plan.sources {
            let src = &recipe.sources[id];
            let mut candidates = vec![downloads.join(id)];
            candidates.extend(root_files_of_size(folder, src.size));
            candidates.extend(self.extra_copies.iter().cloned());
            match find_local_copy(src, &candidates) {
                Some(p) => {
                    if p.starts_with(&downloads) {
                        ours.push(p.clone());
                    }
                    paths.insert(id.clone(), p);
                }
                None => missing.push(id.clone()),
            }
        }
        check_space(folder, recipe, plan, &missing)?;
        for id in missing {
            let src = &recipe.sources[&id];
            let dest = downloads.join(&id);
            match self.fetcher.fetch(&id, src, &dest, cancel, progress) {
                Err(FetchError::Mismatch) => {
                    self.fetcher.fetch(&id, src, &dest, cancel, progress)?
                }
                other => other?,
            }
            ours.push(dest.clone());
            paths.insert(id, dest);
        }
        let stamps = build(
            recipe,
            plan,
            folder,
            &paths,
            &self.unpacker,
            cancel,
            progress,
        )?;
        delete_absent(folder, &plan.delete)?;
        progress(Progress::CleaningUp);
        for f in ours {
            let _ = fs::remove_file(f);
        }
        let _ = fs::remove_dir_all(staging_dir(folder));
        Ok(stamps)
    }
}

fn ensure_writable(folder: &Path) -> Result<(), RoutineError> {
    let dir = reborn_dir(folder);
    let probe = dir.join("write-test");
    fs::create_dir_all(&dir)
        .and_then(|_| fs::write(&probe, b"ok"))
        .and_then(|_| fs::remove_file(&probe))
        .map_err(|_| RoutineError::NotWritable(folder.to_path_buf()))
}

/// Files directly in the game folder with this size: where a player drops an installer copy.
fn root_files_of_size(folder: &Path, size: u64) -> Vec<PathBuf> {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && m.len() == size))
        .map(|e| e.path())
        .collect()
}

fn check_space(
    folder: &Path,
    recipe: &Recipe,
    plan: &Plan,
    downloads: &[String],
) -> Result<(), RoutineError> {
    let needed = space_needed(recipe, plan, downloads);
    let available = fs2::available_space(folder)?;
    if available < needed {
        return Err(RoutineError::NoSpace { needed, available });
    }
    Ok(())
}

/// Downloads plus every file built in staging, plus a margin.
fn space_needed(recipe: &Recipe, plan: &Plan, downloads: &[String]) -> u64 {
    let download: u64 = downloads.iter().map(|id| recipe.sources[id].size).sum();
    let wanted: HashSet<String> = plan.build.iter().map(|p| norm_path(p)).collect();
    let built: u64 = recipe
        .files
        .iter()
        .filter(|e| e.mode == Mode::Exact && wanted.contains(&norm_path(&e.path)))
        .filter_map(|e| e.size)
        .sum();
    download + built + SPACE_MARGIN
}

/// Rough peak disk use of a fresh install: every source plus every file.
pub fn install_size(recipe: &Recipe) -> u64 {
    recipe.sources.values().map(|s| s.size).sum::<u64>()
        + recipe.files.iter().filter_map(|e| e.size).sum::<u64>()
}

/// Free space on the drive holding `path`, asking the nearest folder that exists.
pub fn free_space_near(path: &Path) -> Option<u64> {
    path.ancestors()
        .find(|p| p.is_dir())
        .and_then(|p| fs2::available_space(p).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileEntry, FileFrom, Source, SourceKind};

    #[test]
    fn space_counts_downloads_and_built_files() {
        let src = Source {
            url: "https://archive.org/x".into(),
            size: 1000,
            sha256: "0".repeat(64),
            kind: SourceKind::Zip,
        };
        let file = |path: &str, size| FileEntry {
            path: path.into(),
            mode: Mode::Exact,
            size: Some(size),
            sha256: Some("0".repeat(64)),
            from: FileFrom {
                source: "s".into(),
                path: "x".into(),
                pack: None,
            },
        };
        let recipe = Recipe {
            schema: 1,
            game: "g".into(),
            min_launcher: "1.0.0".into(),
            sources: BTreeMap::from([("s".to_string(), src)]),
            files: vec![file("a", 10), file("b", 20)],
            absent: vec![],
        };
        let plan = Plan {
            sources: vec!["s".into()],
            build: vec!["B".into()],
            delete: vec![],
        };
        assert_eq!(
            space_needed(&recipe, &plan, &["s".to_string()]),
            1000 + 20 + SPACE_MARGIN
        );
        assert_eq!(space_needed(&recipe, &plan, &[]), 20 + SPACE_MARGIN);
        assert_eq!(install_size(&recipe), 1030);
    }
}
