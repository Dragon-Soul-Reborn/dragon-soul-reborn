//! The player's launcher settings, saved as JSON in the app's config folder.

use dsr_core::display::ResolutionSetting;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub game_folder: Option<PathBuf>,
    pub resolution: ResolutionSetting,
    pub fullscreen: bool,
    /// Advanced: `address:port` replacing the official server (testing).
    pub server_override: Option<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            game_folder: None,
            resolution: ResolutionSetting::Auto,
            fullscreen: true,
            server_override: None,
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let tmp = dir.join("settings.json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).expect("settings serialize"),
        )?;
        fs::rename(tmp, dir.join("settings.json"))
    }
}

pub fn default_game_folder() -> PathBuf {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    PathBuf::from(format!(r"{drive}\Games\Dragon Soul"))
}

/// The uninstaller reads this key to offer deleting the game folder.
#[cfg(windows)]
pub fn remember_folder_for_uninstaller(folder: Option<&Path>) {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    if let Ok((key, _)) =
        RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Dragon Soul Reborn")
    {
        let _ = match folder {
            Some(f) => key.set_value("GameFolder", &f.display().to_string()),
            None => key.delete_value("GameFolder"),
        };
    }
}

#[cfg(not(windows))]
pub fn remember_folder_for_uninstaller(_: Option<&Path>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
        let s = Settings {
            game_folder: Some(PathBuf::from(r"C:\Games\Dragon Soul")),
            resolution: ResolutionSetting::Fixed {
                width: 1280,
                height: 720,
            },
            fullscreen: false,
            server_override: Some("127.0.0.1:7000".into()),
        };
        s.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), s);
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), b"{oops").unwrap();
        assert_eq!(Settings::load(dir.path()), Settings::default());
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), br#"{"fullscreen":false}"#).unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(
            (s.fullscreen, s.resolution),
            (false, ResolutionSetting::Auto)
        );
    }
}
