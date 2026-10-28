//! recipe-tool: builds, checks and applies Dragon Soul Reborn game recipes.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use dsr_core::{
    extract::Unpacker,
    fetch::Fetcher,
    inspect::{inspect, CheckMode, FileStatus},
    model::{Recipe, ServerInfo, SourceKind, ARCHIVE_HOSTS},
    progress::Progress,
    recipe_build::{build_recipe, build_recipe_with_musician, BuildSpec, SourceSpec},
    routine::Routine,
    signature,
};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Parser)]
#[command(about = "Builds, checks and applies Dragon Soul Reborn game recipes")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build a recipe from the original sources and the installer's unpacked files.
    Build {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        extracted: PathBuf,
        #[arg(long)]
        game: String,
        #[arg(long)]
        min_launcher: String,
        /// Enable the native Musician class and English descriptions (launcher 1.1.0+).
        #[arg(long)]
        musician: bool,
        #[arg(long)]
        out: PathBuf,
    },
    /// Full fingerprint check of a game folder against a recipe. Exits 1 on any difference.
    Verify {
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        folder: PathBuf,
    },
    /// Make a folder match a recipe, exactly as the launcher does.
    Apply {
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        folder: PathBuf,
        #[arg(long)]
        innoextract: PathBuf,
        #[arg(long = "copy")]
        copies: Vec<PathBuf>,
        #[arg(long = "allow-host")]
        hosts: Vec<String>,
    },
    /// Check a signature made by `tauri signer sign`.
    SigVerify {
        #[arg(long)]
        pubkey: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        sig: PathBuf,
    },
    /// Validate server-info and every recipe in a publish folder.
    CheckFeed {
        #[arg(long)]
        dir: PathBuf,
    },
}

#[derive(Deserialize)]
struct SpecFile {
    source: Vec<SpecSource>,
}

#[derive(Deserialize)]
struct SpecSource {
    id: String,
    url: String,
    file: PathBuf,
    kind: SourceKind,
    overlay_prefix: Option<String>,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Build {
            spec,
            extracted,
            game,
            min_launcher,
            musician,
            out,
        } => {
            let parsed: SpecFile =
                toml::from_str(&fs::read_to_string(&spec)?).context("reading the spec")?;
            let sources = parsed
                .source
                .into_iter()
                .map(|s| SourceSpec {
                    id: s.id,
                    url: s.url,
                    file: s.file,
                    kind: s.kind,
                    overlay_prefix: s.overlay_prefix,
                })
                .collect();
            let builder = if musician {
                build_recipe_with_musician
            } else {
                build_recipe
            };
            let recipe = builder(&BuildSpec {
                game,
                min_launcher,
                sources,
                extracted,
            })?;
            recipe.validate(ARCHIVE_HOSTS)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut json = serde_json::to_vec_pretty(&recipe)?;
            json.push(b'\n');
            fs::write(&out, json)?;
            println!(
                "{}: {} files, {} sources",
                out.display(),
                recipe.files.len(),
                recipe.sources.len()
            );
        }
        Cmd::Verify { recipe, folder } => {
            let recipe = load_recipe(&recipe, ARCHIVE_HOSTS)?;
            let found = inspect(&folder, &recipe, None, CheckMode::Full, &mut |_| {})?;
            let bad: Vec<_> = found
                .statuses
                .iter()
                .filter(|(_, s)| **s != FileStatus::Good)
                .collect();
            for (path, status) in bad.iter().take(50) {
                println!("{status:?}: {path}");
            }
            for path in &found.absent_present {
                println!("Forbidden: {path}");
            }
            if !bad.is_empty() || !found.absent_present.is_empty() {
                bail!(
                    "{} files differ, {} forbidden files present",
                    bad.len(),
                    found.absent_present.len()
                );
            }
            println!("all {} files match", found.statuses.len());
        }
        Cmd::Apply {
            recipe,
            folder,
            innoextract,
            copies,
            hosts,
        } => {
            let hosts: Vec<&str> = if hosts.is_empty() {
                ARCHIVE_HOSTS.to_vec()
            } else {
                hosts.iter().map(String::as_str).collect()
            };
            let recipe = load_recipe(&recipe, &hosts)?;
            let routine = Routine {
                folder,
                fetcher: Fetcher::new(&hosts),
                unpacker: Unpacker { innoextract },
                extra_copies: copies,
            };
            let mut last = String::new();
            let out = routine.run(
                &recipe,
                CheckMode::Full,
                &AtomicBool::new(false),
                &mut |p| {
                    let line = describe(&p);
                    if line != last {
                        println!("{line}");
                        last = line;
                    }
                },
            )?;
            println!("built {}, deleted {}", out.built.len(), out.deleted.len());
        }
        Cmd::SigVerify { pubkey, file, sig } => {
            signature::verify(
                pubkey.trim(),
                &fs::read(&file)?,
                fs::read_to_string(&sig)?.trim(),
            )
            .with_context(|| format!("checking {}", file.display()))?;
            println!("signature OK: {}", file.display());
        }
        Cmd::CheckFeed { dir } => {
            let info = ServerInfo::from_json(&fs::read(dir.join("server-info.json"))?)
                .context("server-info.json")?;
            let mut found = false;
            for entry in fs::read_dir(dir.join("recipes"))? {
                let path = entry?.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let recipe = load_recipe(&path, ARCHIVE_HOSTS)?;
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if recipe.game != stem {
                    bail!(
                        "{} says game {:?} but is named {stem}",
                        path.display(),
                        recipe.game
                    );
                }
                found |= recipe.game == info.game;
            }
            if !found {
                bail!(
                    "server-info names game {:?} but recipes/{}.json is missing",
                    info.game,
                    info.game
                );
            }
            println!("feed OK");
        }
    }
    Ok(())
}

fn load_recipe(path: &Path, hosts: &[&str]) -> Result<Recipe> {
    Recipe::from_json(&fs::read(path)?, hosts)
        .with_context(|| format!("reading {}", path.display()))
}

/// One line per stage, with download progress in 10 % steps.
fn describe(p: &Progress) -> String {
    match p {
        Progress::Checking { .. } => "checking files".into(),
        Progress::Downloading {
            source,
            done,
            total,
        } => format!(
            "downloading {source}: {}%",
            done * 100 / (*total).max(1) / 10 * 10
        ),
        Progress::Unpacking { source } => format!("unpacking {source}"),
        Progress::Building { .. } => "building files".into(),
        Progress::CleaningUp => "cleaning up".into(),
    }
}
