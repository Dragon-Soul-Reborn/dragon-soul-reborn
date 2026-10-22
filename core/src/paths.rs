//! Refuse filesystem links under the chosen folder so repairs cannot follow them outside it.
use crate::model::{safe_rel, Recipe};
use std::{fs, io, path::Path};

fn ordinary(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            let linked = metadata.file_type().is_symlink();
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                linked || metadata.file_attributes() & 0x400 != 0
            };
            if linked {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the game folder contains a filesystem link; choose an ordinary folder",
                ));
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn check_relative(folder: &Path, relative: &str) -> io::Result<()> {
    if !safe_rel(relative) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsafe game file path",
        ));
    }
    ordinary(folder)?;
    let mut current = folder.to_path_buf();
    for component in relative.split(['/', '\\']) {
        current.push(component);
        ordinary(&current)?;
    }
    Ok(())
}

pub fn check_tree(folder: &Path, recipe: &Recipe) -> io::Result<()> {
    ordinary(folder)?;
    // This directory belongs to the launcher rather than to a recipe.
    let state = folder.join(".reborn");
    ordinary(&state)?;
    for child in [
        "downloads",
        "staging",
        "state.json",
        "state.json.tmp",
        "write-test",
    ] {
        ordinary(&state.join(child))?;
    }
    if state.is_dir() {
        for entry in walkdir::WalkDir::new(&state).follow_links(false) {
            ordinary(entry?.path())?;
        }
    }
    for relative in recipe
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(recipe.absent.iter().map(String::as_str))
    {
        check_relative(folder, relative)?;
    }
    Ok(())
}
