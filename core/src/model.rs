//! The two signed files the launcher reads: server-info and the game recipe.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Hosts recipe sources may come from. A host matches itself and its subdomains.
pub const ARCHIVE_HOSTS: &[&str] = &["archive.org"];

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

fn invalid(msg: impl Into<String>) -> ModelError {
    ModelError::Invalid(msg.into())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerAddress {
    pub ip: String,
    pub port: u16,
    pub zone: u32,
    pub gametype: u32,
    pub nettype: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub schema: u32,
    pub issued: String,
    pub server: ServerAddress,
    pub message: String,
    pub game: String,
}

impl ServerInfo {
    pub fn from_json(bytes: &[u8]) -> Result<ServerInfo, ModelError> {
        let info: ServerInfo = serde_json::from_slice(bytes)?;
        info.validate()?;
        Ok(info)
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema != 1 {
            return Err(invalid(format!(
                "unsupported server-info schema {}",
                self.schema
            )));
        }
        if !is_utc_stamp(&self.issued) {
            return Err(invalid("issued must look like 2026-10-21T12:00:00Z"));
        }
        if self.message.chars().count() > 280 {
            return Err(invalid("the message is longer than 280 characters"));
        }
        if self.server.ip.trim().is_empty() || self.game.trim().is_empty() {
            return Err(invalid("server ip and game must not be empty"));
        }
        Ok(())
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ`. The fixed shape makes string order equal time order.
pub fn is_utc_stamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Inno,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Must match its fingerprint.
    #[default]
    Exact,
    /// Created when missing, then never checked or overwritten.
    Seed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pack {
    Aup,
    LoginError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileFrom {
    pub source: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<Pack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "is_exact")]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub from: FileFrom,
}

fn is_exact(mode: &Mode) -> bool {
    *mode == Mode::Exact
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub schema: u32,
    pub game: String,
    #[serde(rename = "minLauncher")]
    pub min_launcher: String,
    pub sources: BTreeMap<String, Source>,
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub absent: Vec<String>,
}

impl Recipe {
    pub fn from_json(bytes: &[u8], allowed_hosts: &[&str]) -> Result<Recipe, ModelError> {
        let recipe: Recipe = serde_json::from_slice(bytes)?;
        recipe.validate(allowed_hosts)?;
        Ok(recipe)
    }

    pub fn validate(&self, allowed_hosts: &[&str]) -> Result<(), ModelError> {
        if self.schema != 1 {
            return Err(invalid(format!(
                "unsupported recipe schema {}",
                self.schema
            )));
        }
        if self.game.trim().is_empty() {
            return Err(invalid("game is empty"));
        }
        let min_launcher = semver::Version::parse(&self.min_launcher).map_err(|_| {
            invalid(format!(
                "minLauncher {:?} is not a version",
                self.min_launcher
            ))
        })?;
        for (id, s) in &self.sources {
            if !safe_id(id) {
                return Err(invalid(format!("unsafe source id {id:?}")));
            }
            if !host_allowed(&s.url, allowed_hosts) {
                return Err(invalid(format!(
                    "source {id}: {} is not on an allowed host",
                    s.url
                )));
            }
            if !is_sha256(&s.sha256) {
                return Err(invalid(format!("source {id}: bad sha256")));
            }
        }
        let mut seen = BTreeSet::new();
        for e in &self.files {
            if e.from.pack == Some(Pack::LoginError)
                && (norm_path(&e.path) != "gameclient_release.exe"
                    || e.mode != Mode::Exact
                    || min_launcher < semver::Version::new(1, 0, 1))
            {
                return Err(invalid("loginerror is restricted to the verified game client and launcher 1.0.1 or newer"));
            }
            if !safe_rel(&e.path) || !safe_rel(&e.from.path) {
                return Err(invalid(format!("unsafe path in entry {:?}", e.path)));
            }
            if !self.sources.contains_key(&e.from.source) {
                return Err(invalid(format!(
                    "{}: unknown source {:?}",
                    e.path, e.from.source
                )));
            }
            if e.mode == Mode::Exact
                && (e.size.is_none() || !e.sha256.as_deref().is_some_and(is_sha256))
            {
                return Err(invalid(format!(
                    "{}: an exact file needs size and sha256",
                    e.path
                )));
            }
            if !seen.insert(norm_path(&e.path)) {
                return Err(invalid(format!("{} is listed twice", e.path)));
            }
        }
        for a in &self.absent {
            if !safe_rel(a) {
                return Err(invalid(format!("unsafe absent path {a:?}")));
            }
            if seen.contains(&norm_path(a)) {
                return Err(invalid(format!("{a} is both required and forbidden")));
            }
            seen.insert(norm_path(a));
        }
        for path in &seen {
            for (i, _) in path.match_indices('/') {
                if seen.contains(&path[..i]) {
                    return Err(invalid(format!(
                        "a file is also used as a directory: {path}"
                    )));
                }
            }
        }
        Ok(())
    }
}

pub fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

/// Lower-case, forward-slash form used to compare paths (Windows file names ignore case).
pub fn norm_path(p: &str) -> String {
    p.replace('\\', "/").to_lowercase()
}

/// A relative path that stays inside the folder: no root, no drive, no `..`, no empty parts.
pub fn safe_rel(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with(['/', '\\'])
        && !p.contains(':')
        && p.split(['/', '\\']).all(safe_component)
        && !p
            .split(['/', '\\'])
            .next()
            .is_some_and(|part| part.eq_ignore_ascii_case(".reborn"))
}

fn safe_component(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !part.is_empty()
        && !part.ends_with(['.', ' '])
        && !part
            .chars()
            .any(|c| c.is_control() || "<>:\"|?*".contains(c))
        && !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        )
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn safe_id(id: &str) -> bool {
    safe_component(id)
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// HTTPS on an allowed host (or a subdomain of one). Plain HTTP only for loopback hosts, which only tests allow.
pub fn host_allowed(url: &str, allowed_hosts: &[&str]) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = u.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    let scheme_ok = u.scheme() == "https"
        || (u.scheme() == "http" && (host == "127.0.0.1" || host == "localhost"));
    scheme_ok
        && allowed_hosts
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const URL: &str = "https://archive.org/download/item/file.exe";

    fn recipe_json(url: &str, path: &str) -> String {
        format!(
            r#"{{"schema":1,"game":"0.27-r1","minLauncher":"1.0.0",
            "sources":{{"installer":{{"url":"{url}","size":3,"sha256":"{SHA}","kind":"inno"}}}},
            "files":[{{"path":"{path}","size":3,"sha256":"{SHA}","from":{{"source":"installer","path":"app/x"}}}},
                     {{"path":"config/engine_config.xml","mode":"seed","from":{{"source":"installer","path":"app/config/engine_config.xml"}}}}],
            "absent":["launcher.exe"]}}"#
        )
    }

    fn valid() -> Recipe {
        Recipe::from_json(recipe_json(URL, "data/x.aup").as_bytes(), ARCHIVE_HOSTS).unwrap()
    }

    #[test]
    fn reads_a_valid_recipe() {
        let r = valid();
        assert_eq!(r.files[1].mode, Mode::Seed);
        assert_eq!(r.sources["installer"].kind, SourceKind::Inno);
    }

    #[test]
    fn login_message_transform_requires_the_client_path_and_capable_launcher() {
        let mut r = valid();
        r.files[0].from.pack = Some(Pack::LoginError);
        assert!(r.validate(ARCHIVE_HOSTS).is_err());
        r.min_launcher = "1.0.1".into();
        assert!(r.validate(ARCHIVE_HOSTS).is_err());
        r.files[0].path = "gameclient_release.exe".into();
        r.validate(ARCHIVE_HOSTS).unwrap();
        r.files[0].mode = Mode::Seed;
        assert!(r.validate(ARCHIVE_HOSTS).is_err());
    }

    #[test]
    fn refuses_downloads_outside_archive_org() {
        for url in [
            "http://archive.org/x",
            "https://evilarchive.org/x",
            "https://archive.org.evil.com/x",
            "https://example.com/x",
        ] {
            assert!(
                Recipe::from_json(recipe_json(url, "x").as_bytes(), ARCHIVE_HOSTS).is_err(),
                "{url}"
            );
        }
        assert!(Recipe::from_json(
            recipe_json("https://ia800407.us.archive.org/x", "x").as_bytes(),
            ARCHIVE_HOSTS
        )
        .is_ok());
    }

    #[test]
    fn refuses_paths_that_leave_the_folder() {
        for p in ["../x", "/x", "C:/x", "a//b", "a/./b", ""] {
            assert!(
                Recipe::from_json(recipe_json(URL, p).as_bytes(), ARCHIVE_HOSTS).is_err(),
                "{p:?}"
            );
        }
    }

    #[test]
    fn refuses_the_same_path_twice_ignoring_case() {
        let json = recipe_json(URL, "Config/Engine_Config.xml");
        assert!(Recipe::from_json(json.as_bytes(), ARCHIVE_HOSTS).is_err());
    }

    #[test]
    fn an_exact_file_needs_a_fingerprint() {
        let mut r = valid();
        r.files[0].sha256 = None;
        assert!(r.validate(ARCHIVE_HOSTS).is_err());
    }

    #[test]
    fn a_file_cannot_be_both_required_and_forbidden() {
        let mut r = valid();
        r.absent.push("Data/X.aup".into());
        assert!(r.validate(ARCHIVE_HOSTS).is_err());
    }

    #[test]
    fn loopback_http_is_only_for_tests() {
        assert!(host_allowed("http://127.0.0.1:8080/x", &["127.0.0.1"]));
        assert!(!host_allowed("http://127.0.0.1:8080/x", ARCHIVE_HOSTS));
        assert!(!host_allowed("http://example.com/x", &["example.com"]));
    }

    #[test]
    fn reads_and_checks_server_info() {
        let ok = r#"{"schema":1,"issued":"2026-10-21T12:00:00Z","server":{"ip":"127.0.0.1","port":7000,"zone":1,"gametype":1,"nettype":0},"message":"hi","game":"0.27-r1"}"#;
        assert_eq!(
            ServerInfo::from_json(ok.as_bytes()).unwrap().server.port,
            7000
        );
        assert!(
            ServerInfo::from_json(ok.replace("2026-10-21T12:00:00Z", "yesterday").as_bytes())
                .is_err()
        );
        let long = ok.replace("\"hi\"", &format!("\"{}\"", "x".repeat(281)));
        assert!(ServerInfo::from_json(long.as_bytes()).is_err());
    }

    #[test]
    fn utc_stamps_sort_like_time() {
        assert!(is_utc_stamp("2026-10-21T12:00:00Z"));
        assert!(!is_utc_stamp("2026-10-21 12:00:00Z"));
        assert!("2026-10-21T12:00:00Z" < "2026-10-21T12:00:01Z");
    }

    #[test]
    fn rejects_source_ids_that_escape_the_download_folder() {
        for id in ["../outside", "C:/outside", "a/b", "a\\b", "", "NUL", "x."] {
            let mut recipe = valid();
            let source = recipe.sources.remove("installer").unwrap();
            recipe.sources.insert(id.into(), source);
            for file in &mut recipe.files {
                file.from.source = id.into();
            }
            assert!(recipe.validate(ARCHIVE_HOSTS).is_err(), "{id:?}");
        }
    }

    #[test]
    fn rejects_windows_aliases_and_launcher_state_paths() {
        for path in [
            "data/x.",
            "data/x ",
            "NUL",
            "data/CON.txt",
            "data/a?b",
            "data/a\u{0}b",
            ".reborn/state.json",
        ] {
            assert!(!safe_rel(path), "{path:?}");
        }
    }

    #[test]
    fn rejects_file_directory_collisions() {
        let mut recipe = valid();
        let mut parent = recipe.files[0].clone();
        parent.path = "data".into();
        recipe.files.push(parent);
        assert!(recipe.validate(ARCHIVE_HOSTS).is_err());
    }
}
