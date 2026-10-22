//! Downloads recipe sources: allow-listed hosts only (redirects included), resumable, retried with growing
//! pauses, and checked against the recipe's fingerprint before use.

use crate::{
    hash,
    model::{host_allowed, Source},
    progress::Progress,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("cancelled")]
    Cancelled,
    #[error("{0} is not an allowed download address")]
    NotAllowed(String),
    #[error("could not download {url}: {reason}")]
    Unreachable { url: String, reason: String },
    #[error("the downloaded file does not match its fingerprint")]
    Mismatch,
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Fetcher {
    client: reqwest::blocking::Client,
    allowed_hosts: Vec<String>,
    retries: u32,
    backoff: Duration,
}

impl Fetcher {
    pub fn allowed_hosts(&self) -> Vec<&str> {
        self.allowed_hosts.iter().map(String::as_str).collect()
    }
    pub fn new(allowed_hosts: &[&str]) -> Fetcher {
        let hosts: Vec<String> = allowed_hosts.iter().map(|h| h.to_string()).collect();
        let policy_hosts = hosts.clone();
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            let allowed: Vec<&str> = policy_hosts.iter().map(String::as_str).collect();
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if host_allowed(attempt.url().as_str(), &allowed) {
                attempt.follow()
            } else {
                attempt.error("redirect to a host that is not allowed")
            }
        });
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("DragonSoulReborn/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(20))
            .timeout(None)
            .redirect(policy)
            .build()
            .expect("the HTTP client builds");
        Fetcher {
            client,
            allowed_hosts: hosts,
            retries: 5,
            backoff: Duration::from_secs(2),
        }
    }

    /// Retries after a network failure, pausing `backoff`, then twice as long each time (capped at 16×).
    pub fn with_retries(mut self, retries: u32, backoff: Duration) -> Fetcher {
        self.retries = retries;
        self.backoff = backoff;
        self
    }

    /// Downloads `src` to `dest`, resuming `dest.part` if one exists, and checks the fingerprint.
    pub fn fetch(
        &self,
        id: &str,
        src: &Source,
        dest: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<(), FetchError> {
        let allowed: Vec<&str> = self.allowed_hosts.iter().map(String::as_str).collect();
        if !host_allowed(&src.url, &allowed) {
            return Err(FetchError::NotAllowed(src.url.clone()));
        }
        if is_valid_copy(dest, src)? {
            return Ok(());
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let part = part_path(dest);
        let mut attempt = 0;
        loop {
            match self.download_once(id, src, &part, cancel, progress) {
                Ok(()) => break,
                Err(FetchError::Unreachable { url, reason }) if attempt < self.retries => {
                    log_retry(&url, &reason);
                    std::thread::sleep(self.backoff * 2u32.pow(attempt.min(4)));
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
        if hash::sha256_file(&part)? != src.sha256 {
            fs::remove_file(&part)?;
            return Err(FetchError::Mismatch);
        }
        fs::rename(&part, dest)?;
        Ok(())
    }

    fn download_once(
        &self,
        id: &str,
        src: &Source,
        part: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<(), FetchError> {
        let unreachable = |reason: String| FetchError::Unreachable {
            url: src.url.clone(),
            reason,
        };
        let mut have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have > src.size {
            fs::remove_file(part)?;
            have = 0;
        }
        if have == src.size {
            return Ok(());
        }
        let mut req = self.client.get(&src.url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut resp = req.send().map_err(|e| unreachable(e.to_string()))?;
        let status = resp.status();
        let (mut file, mut done) = if status == reqwest::StatusCode::PARTIAL_CONTENT && have > 0 {
            (OpenOptions::new().append(true).open(part)?, have)
        } else if status.is_success() {
            (File::create(part)?, 0) // the server sent the whole file: start over
        } else {
            return Err(unreachable(format!("HTTP {status}")));
        };
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(FetchError::Cancelled);
            }
            let n = resp
                .read(&mut buf)
                .map_err(|e| unreachable(e.to_string()))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            done += n as u64;
            progress(Progress::Downloading {
                source: id.to_string(),
                done,
                total: src.size,
            });
        }
        file.flush()?;
        if done != src.size {
            return Err(unreachable(format!(
                "the connection ended at {done} of {} bytes",
                src.size
            )));
        }
        Ok(())
    }
}

fn log_retry(url: &str, reason: &str) {
    eprintln!("retrying {url}: {reason}");
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

fn is_valid_copy(path: &Path, src: &Source) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(m) if m.is_file() && m.len() == src.size => Ok(hash::sha256_file(path)? == src.sha256),
        Ok(_) => Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// The first candidate that is exactly this source (size and fingerprint), e.g. an installer the player has.
pub fn find_local_copy(src: &Source, candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|p| is_valid_copy(p, src).unwrap_or(false))
        .cloned()
}
