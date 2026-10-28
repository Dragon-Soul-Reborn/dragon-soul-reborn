//! Builds a recipe from the original sources with the client assembly rules worked out in the lab: start from
//! the installer's files, leave out the dead launcher, patchers and voice plugin, then lay each 0.27 zip over
//! it in order, packing files as .aup and keeping the installer's copy when its content is already the same.

use crate::{
    aup,
    extract::{ExtractError, ZipSource},
    hash,
    model::{norm_path, FileEntry, FileFrom, Mode, Pack, Recipe, Source, SourceKind},
};
use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    path::PathBuf,
};

/// Installer files that must never be on a player's PC: they call home to dead Neonga domains (launcher,
/// loader, patchers) or carry their own downloader (the YY voice plugin).
pub const EXCLUDE: &[&str] = &[
    "launcher.exe",
    "loader.exe",
    "pipfactory.dll",
    "patcher96.exe.aup",
    "patcher120.exe.aup",
    "patcher144.exe.aup",
];

pub struct SourceSpec {
    pub id: String,
    pub url: String,
    pub file: PathBuf,
    pub kind: SourceKind,
    /// For zips laid over the installer: the member prefix that maps onto the game folder.
    pub overlay_prefix: Option<String>,
}

pub struct BuildSpec {
    pub game: String,
    pub min_launcher: String,
    pub sources: Vec<SourceSpec>,
    /// Folder holding the installer's unpacked `app/` tree.
    pub extracted: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error(transparent)]
    Musician(#[from] crate::musician::MusicianError),
    #[error("the Musician recipe needs minLauncher 1.1.0 or later and all four supported assets")]
    MusicianRecipe,
    #[error("the spec needs exactly one inno source")]
    Installer,
    #[error(transparent)]
    Extract(#[from] ExtractError),
    #[error(transparent)]
    Walk(#[from] walkdir::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Files the game rewrites or the launcher edits at Play: created once, never checked.
pub fn is_seed(path: &str) -> bool {
    let p = norm_path(path);
    p == "config/engine_config.xml" || p == "config/localsetup.xml" || p.starts_with("log/")
}

pub fn build_recipe(spec: &BuildSpec) -> Result<Recipe, BuildError> {
    build_recipe_inner(spec, false)
}

/// Build the native Musician menu and English text into a new versioned recipe.
/// The default builder remains byte-identical to the original game recipe.
pub fn build_recipe_with_musician(spec: &BuildSpec) -> Result<Recipe, BuildError> {
    let minimum = semver::Version::parse(crate::musician::MIN_LAUNCHER).unwrap();
    if !semver::Version::parse(&spec.min_launcher).is_ok_and(|v| v >= minimum) {
        return Err(BuildError::MusicianRecipe);
    }
    build_recipe_inner(spec, true)
}

fn build_recipe_inner(spec: &BuildSpec, musician: bool) -> Result<Recipe, BuildError> {
    let mut installers = spec.sources.iter().filter(|s| s.kind == SourceKind::Inno);
    let installer = installers.next().ok_or(BuildError::Installer)?;
    if installers.next().is_some() {
        return Err(BuildError::Installer);
    }
    let app = spec.extracted.join("app");
    let mode_for = |p: &str| if is_seed(p) { Mode::Seed } else { Mode::Exact };

    let mut files: Vec<FileEntry> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for entry in walkdir::WalkDir::new(&app).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file()
            || EXCLUDE.contains(&entry.file_name().to_string_lossy().to_lowercase().as_str())
        {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(&app)
            .expect("the walk stays under app")
            .to_string_lossy()
            .replace('\\', "/");
        index.insert(norm_path(&rel), files.len());
        files.push(FileEntry {
            path: rel.clone(),
            mode: mode_for(&rel),
            size: None,
            sha256: None,
            from: FileFrom {
                source: installer.id.clone(),
                path: format!("app/{rel}"),
                pack: None,
            },
        });
    }

    // Unpacked content of files an overlay replaced, to compare against later overlays.
    let mut overlaid: HashMap<usize, Vec<u8>> = HashMap::new();
    for s in spec.sources.iter().filter(|s| s.kind == SourceKind::Zip) {
        let Some(prefix) = &s.overlay_prefix else {
            continue;
        };
        let mut zip = ZipSource::open(&s.file)?;
        let mut names = zip.file_names();
        names.sort();
        for member in names {
            let Some(rest) = member.strip_prefix(prefix.as_str()) else {
                continue;
            };
            let raw = zip.read(&member)?;
            let target = format!("{}.aup", norm_path(rest));
            let from = FileFrom {
                source: s.id.clone(),
                path: member.clone(),
                pack: Some(Pack::Aup),
            };
            match index.get(&target).copied() {
                Some(i) => {
                    let current = match overlaid.get(&i) {
                        Some(prev) => Some(prev.clone()),
                        None => aup::decode(&fs::read(app.join(&files[i].path))?).ok(),
                    };
                    if current.as_deref() == Some(raw.as_slice()) {
                        continue;
                    }
                    files[i].from = from;
                    overlaid.insert(i, raw);
                }
                None => {
                    index.insert(target.clone(), files.len());
                    overlaid.insert(files.len(), raw);
                    files.push(FileEntry {
                        path: target.clone(),
                        mode: mode_for(&target),
                        size: None,
                        sha256: None,
                        from,
                    });
                }
            }
        }
    }

    let mut musician_assets = 0;
    for (i, e) in files.iter_mut().enumerate() {
        if e.mode == Mode::Seed {
            continue;
        }
        let (size, sha) = if let Some(pack) = musician
            .then(|| crate::musician::pack_for(&e.path))
            .flatten()
        {
            let raw = match overlaid.get(&i) {
                Some(raw) => raw.clone(),
                None => fs::read(app.join(&e.path))?,
            };
            let bytes = crate::musician::transform(pack, &raw)?;
            e.from.pack = Some(pack);
            musician_assets += 1;
            (bytes.len() as u64, hash::sha256_bytes(&bytes))
        } else {
            match overlaid.get(&i) {
                Some(raw) => {
                    let bytes = aup::encode(raw);
                    (bytes.len() as u64, hash::sha256_bytes(&bytes))
                }
                None => {
                    let p = app.join(&e.path);
                    (fs::metadata(&p)?.len(), hash::sha256_file(&p)?)
                }
            }
        };
        e.size = Some(size);
        e.sha256 = Some(sha);
    }
    if musician && musician_assets != 4 {
        return Err(BuildError::MusicianRecipe);
    }
    files.sort_by_key(|e| norm_path(&e.path));

    let mut sources = BTreeMap::new();
    for s in &spec.sources {
        let source = Source {
            url: s.url.clone(),
            size: fs::metadata(&s.file)?.len(),
            sha256: hash::sha256_file(&s.file)?,
            kind: s.kind,
        };
        sources.insert(s.id.clone(), source);
    }
    Ok(Recipe {
        schema: 1,
        game: spec.game.clone(),
        min_launcher: spec.min_launcher.clone(),
        sources,
        files,
        absent: EXCLUDE.iter().map(|s| s.to_string()).collect(),
    })
}
