//! Fetches the signed server-info and recipes. Every file must carry a valid signature from the key built
//! into the launcher. The last verified copy is kept for offline starts, and an older server-info never
//! replaces a newer one, so an old signed file can't be replayed.

use crate::{
    model::{ModelError, Recipe, ServerInfo},
    signature,
};
use std::{fs, io, path::Path, time::Duration};

#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("couldn't reach the update server, and there is no saved copy yet")]
    Unreachable,
    #[error("the update server's files failed their signature check")]
    BadSignature,
    #[error("the update server sent a file the launcher can't use: {0}")]
    Invalid(String),
}

#[derive(Debug)]
pub struct Fetched<T> {
    pub value: T,
    /// True when this is the saved copy because the server couldn't give a newer verified one.
    pub offline: bool,
}

pub struct Feed {
    client: reqwest::blocking::Client,
    bases: Vec<String>,
    pubkey: String,
}

enum Remote {
    Got(Vec<u8>, Vec<u8>),
    Unreachable,
    BadSignature,
}

impl Feed {
    /// `bases` end with `/`; they are tried in order.
    pub fn new(bases: &[&str], pubkey: &str) -> Feed {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("DragonSoulReborn/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("the HTTP client builds");
        Feed {
            client,
            bases: bases.iter().map(|b| b.to_string()).collect(),
            pubkey: pubkey.to_string(),
        }
    }

    pub fn server_info(&self, cache: &Path) -> Result<Fetched<ServerInfo>, FeedError> {
        self.load(
            "server-info.json",
            cache,
            ServerInfo::from_json,
            |new, old| new.issued >= old.issued,
        )
    }

    pub fn recipe(
        &self,
        game: &str,
        cache: &Path,
        allowed_hosts: &[&str],
    ) -> Result<Fetched<Recipe>, FeedError> {
        if game.is_empty()
            || !game
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            || game.starts_with('.')
        {
            return Err(FeedError::Invalid(format!(
                "bad game version name {game:?}"
            )));
        }
        self.load(
            &format!("recipes/{game}.json"),
            cache,
            |b| Recipe::from_json(b, allowed_hosts),
            |_, _| true,
        )
    }

    fn load<T>(
        &self,
        name: &str,
        cache: &Path,
        parse: impl Fn(&[u8]) -> Result<T, ModelError>,
        may_replace: impl Fn(&T, &T) -> bool,
    ) -> Result<Fetched<T>, FeedError> {
        let cached = self
            .read_cache(name, cache)
            .and_then(|data| parse(&data).ok());
        let (data, sig) = match self.fetch_signed(name) {
            Remote::Got(data, sig) => (data, sig),
            Remote::Unreachable => {
                return cached
                    .map(|value| Fetched {
                        value,
                        offline: true,
                    })
                    .ok_or(FeedError::Unreachable)
            }
            Remote::BadSignature => {
                return cached
                    .map(|value| Fetched {
                        value,
                        offline: true,
                    })
                    .ok_or(FeedError::BadSignature)
            }
        };
        let fresh = match parse(&data) {
            Ok(v) => v,
            Err(e) => {
                return cached
                    .map(|value| Fetched {
                        value,
                        offline: true,
                    })
                    .ok_or(FeedError::Invalid(e.to_string()))
            }
        };
        if let Some(old) = cached {
            if !may_replace(&fresh, &old) {
                return Ok(Fetched {
                    value: old,
                    offline: true,
                });
            }
        }
        let _ = write_cache(name, cache, &data, &sig);
        Ok(Fetched {
            value: fresh,
            offline: false,
        })
    }

    fn fetch_signed(&self, name: &str) -> Remote {
        let get = |url: String| -> Option<Vec<u8>> {
            let resp = self.client.get(url).send().ok()?;
            if !resp.status().is_success() {
                return None;
            }
            resp.bytes().ok().map(|b| b.to_vec())
        };
        let mut saw_bad = false;
        for base in &self.bases {
            let (Some(data), Some(sig)) = (
                get(format!("{base}{name}")),
                get(format!("{base}{name}.sig")),
            ) else {
                continue;
            };
            let verified = std::str::from_utf8(&sig)
                .ok()
                .is_some_and(|s| signature::verify(&self.pubkey, &data, s).is_ok());
            if verified {
                return Remote::Got(data, sig);
            }
            saw_bad = true;
        }
        if saw_bad {
            Remote::BadSignature
        } else {
            Remote::Unreachable
        }
    }

    /// The cached copy, only if its signature still checks out.
    fn read_cache(&self, name: &str, cache: &Path) -> Option<Vec<u8>> {
        let data = fs::read(cache.join(name)).ok()?;
        let sig = fs::read_to_string(cache.join(format!("{name}.sig"))).ok()?;
        signature::verify(&self.pubkey, &data, &sig).ok()?;
        Some(data)
    }
}

fn write_cache(name: &str, cache: &Path, data: &[u8], sig: &[u8]) -> io::Result<()> {
    let path = cache.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cache.join(format!("{name}.sig")), sig)?;
    fs::write(path, data)
}
