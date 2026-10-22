//! Builds the planned files from verified sources in a staging folder, checks each one, and only then swaps it
//! into place (same volume, so the rename is atomic: a crash leaves the old file intact).

use crate::{
    aup,
    extract::{ExtractError, Unpacker, ZipSource},
    hash,
    model::{norm_path, FileEntry, Mode, Pack, Recipe, SourceKind},
    plan::Plan,
    progress::Progress,
    state::{staging_dir, Stamp},
};
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error(transparent)]
    Extract(#[from] ExtractError),
    #[error("{0} came out different from the recipe")]
    BuiltMismatch(String),
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Returns stamps for the exact files written, keyed by normalized path.
pub fn build(
    recipe: &Recipe,
    plan: &Plan,
    folder: &Path,
    sources: &BTreeMap<String, PathBuf>,
    unpacker: &Unpacker,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<BTreeMap<String, Stamp>, ApplyError> {
    let staging = staging_dir(folder);
    crate::paths::check_tree(folder, recipe)?;
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let by_path: HashMap<String, &FileEntry> = recipe
        .files
        .iter()
        .map(|e| (norm_path(&e.path), e))
        .collect();
    let entries: Vec<&FileEntry> = plan
        .build
        .iter()
        .filter_map(|p| by_path.get(&norm_path(p)).copied())
        .collect();

    let mut unpacked: HashMap<&str, PathBuf> = HashMap::new();
    let mut zips: HashMap<&str, ZipSource> = HashMap::new();
    for id in &plan.sources {
        match recipe.sources[id].kind {
            SourceKind::Inno => {
                progress(Progress::Unpacking { source: id.clone() });
                let wanted: Vec<String> = entries
                    .iter()
                    .filter(|e| e.from.source == *id)
                    .map(|e| e.from.path.clone())
                    .collect();
                let dir = staging.join("unpack").join(id);
                unpacker.inno(&sources[id], &wanted, &dir)?;
                unpacked.insert(id.as_str(), dir);
            }
            SourceKind::Zip => {
                zips.insert(id.as_str(), ZipSource::open(&sources[id])?);
            }
        }
    }

    let out = staging.join("out");
    let mut stamps = BTreeMap::new();
    let total = entries.len() as u64;
    for (i, e) in entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ApplyError::Cancelled);
        }
        progress(Progress::Building {
            done: i as u64,
            total,
        });
        let staged = stage(e, &unpacked, &mut zips, &out)?;
        let digest = hash::sha256_file(&staged)?;
        let size = fs::metadata(&staged)?.len();
        if e.mode == Mode::Exact
            && (e.sha256.as_deref() != Some(digest.as_str()) || e.size != Some(size))
        {
            return Err(ApplyError::BuiltMismatch(e.path.clone()));
        }
        let dest = folder.join(&e.path);
        replace_into(&staged, &dest)?;
        if e.mode == Mode::Exact {
            stamps.insert(norm_path(&e.path), Stamp::of(&dest, &digest)?);
        }
    }
    progress(Progress::Building { done: total, total });
    Ok(stamps)
}

/// Puts the bytes for `e` in a staging file and returns its path.
fn stage(
    e: &FileEntry,
    unpacked: &HashMap<&str, PathBuf>,
    zips: &mut HashMap<&str, ZipSource>,
    out: &Path,
) -> Result<PathBuf, ApplyError> {
    let unpacked_path = unpacked
        .get(e.from.source.as_str())
        .map(|dir| dir.join(&e.from.path));
    if let (Some(p), None) = (unpacked_path.as_ref(), e.from.pack) {
        return Ok(p.clone()); // an installer file used as it is
    }
    let raw = match unpacked_path {
        Some(p) => fs::read(p)?,
        None => zips
            .get_mut(e.from.source.as_str())
            .expect("zip source opened")
            .read(&e.from.path)?,
    };
    let bytes = match e.from.pack {
        Some(Pack::Aup) => aup::encode(&raw),
        None => raw,
    };
    let staged = out.join(&e.path);
    if let Some(parent) = staged.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&staged, bytes)?;
    Ok(staged)
}

/// Moves `staged` over `dest`, clearing a read-only flag or removing a directory in the way first.
fn replace_into(staged: &Path, dest: &Path) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Ok(m) = fs::metadata(dest) {
        if m.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a directory occupies a game file path; move it before repairing",
            ));
        } else if m.permissions().readonly() {
            let mut p = m.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            p.set_readonly(false);
            fs::set_permissions(dest, p)?;
        }
    }
    fs::rename(staged, dest)
}

pub fn delete_absent(folder: &Path, names: &[String]) -> io::Result<()> {
    for name in names {
        crate::paths::check_relative(folder, name)?;
        let p = folder.join(name);
        match fs::metadata(&p) {
            Ok(m) if m.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "a directory occupies a forbidden file path; it was left unchanged",
                ))
            }
            Ok(_) => fs::remove_file(&p)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
