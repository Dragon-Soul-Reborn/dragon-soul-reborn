//! Gets files out of the sources: the bundled innoextract unpacks the Inno Setup installer without running
//! it, and zip members are read directly.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf, MAIN_SEPARATOR},
    process::Command,
};

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("could not run the unpacker: {0}")]
    Spawn(std::io::Error),
    #[error("the unpacker failed: {0}")]
    Failed(String),
    #[error("{member} is missing from {zip}")]
    MissingMember { zip: String, member: String },
    #[error("could not read {zip}: {reason}")]
    Zip { zip: String, reason: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Above this many paths, extract everything rather than list each one on the command line.
const MAX_INCLUDES: usize = 200;

pub struct Unpacker {
    pub innoextract: PathBuf,
}

impl Unpacker {
    /// Unpacks `paths` (installer paths such as `app/data/x.aup`; empty means everything) into `out_dir`,
    /// keeping their paths.
    pub fn inno(
        &self,
        installer: &Path,
        paths: &[String],
        out_dir: &Path,
    ) -> Result<(), ExtractError> {
        fs::create_dir_all(out_dir)?;
        let mut cmd = Command::new(&self.innoextract);
        cmd.arg("--silent")
            .arg("--extract")
            .arg("--output-dir")
            .arg(out_dir)
            .args(include_args(paths))
            .arg(installer);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let out = cmd.output().map_err(ExtractError::Spawn)?;
        if !out.status.success() {
            return Err(ExtractError::Failed(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(())
    }
}

/// innoextract `--include` arguments. A leading separator anchors the match at the start of the path.
pub fn include_args(paths: &[String]) -> Vec<String> {
    if paths.is_empty() || paths.len() > MAX_INCLUDES {
        return Vec::new();
    }
    let sep = MAIN_SEPARATOR.to_string();
    paths
        .iter()
        .flat_map(|p| {
            [
                "--include".to_string(),
                format!("{sep}{}", p.replace('/', &sep)),
            ]
        })
        .collect()
}

pub struct ZipSource {
    name: String,
    archive: zip::ZipArchive<fs::File>,
}

impl ZipSource {
    pub fn open(path: &Path) -> Result<ZipSource, ExtractError> {
        let name = path.display().to_string();
        let archive =
            zip::ZipArchive::new(fs::File::open(path)?).map_err(|e| ExtractError::Zip {
                zip: name.clone(),
                reason: e.to_string(),
            })?;
        Ok(ZipSource { name, archive })
    }

    /// Every file member (no directories), in archive order.
    pub fn file_names(&mut self) -> Vec<String> {
        (0..self.archive.len())
            .filter_map(|i| {
                self.archive
                    .by_index(i)
                    .ok()
                    .filter(|f| !f.is_dir())
                    .map(|f| f.name().to_string())
            })
            .collect()
    }

    pub fn read(&mut self, member: &str) -> Result<Vec<u8>, ExtractError> {
        let mut file = match self.archive.by_name(member) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => {
                return Err(ExtractError::MissingMember {
                    zip: self.name.clone(),
                    member: member.to_string(),
                })
            }
            Err(e) => {
                return Err(ExtractError::Zip {
                    zip: self.name.clone(),
                    reason: e.to_string(),
                })
            }
        };
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }
}
