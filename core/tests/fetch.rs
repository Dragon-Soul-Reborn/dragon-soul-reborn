mod support;

use dsr_core::{
    fetch::{find_local_copy, FetchError, Fetcher},
    hash,
    model::{Source, SourceKind},
};
use std::{fs, path::Path, sync::atomic::AtomicBool, time::Duration};
use support::http::TestServer;

fn payload() -> Vec<u8> {
    (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect()
}

fn source(url: String, data: &[u8]) -> Source {
    Source {
        url,
        size: data.len() as u64,
        sha256: hash::sha256_bytes(data),
        kind: SourceKind::Zip,
    }
}

fn fetcher() -> Fetcher {
    Fetcher::new(&["127.0.0.1"]).with_retries(3, Duration::from_millis(1))
}

fn get(server: &TestServer, dest: &Path, data: &[u8]) -> Result<(), FetchError> {
    fetcher().fetch(
        "f",
        &source(server.url("f.bin"), data),
        dest,
        &AtomicBool::new(false),
        &mut |_| {},
    )
}

#[test]
fn downloads_and_verifies() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    server.put("f.bin", data.clone());
    let dest = dir.path().join("f");
    get(&server, &dest, &data).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), data);
}

#[test]
fn resumes_after_the_connection_drops() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    server.put("f.bin", data.clone());
    server.cut_next("f.bin", 100_000);
    let dest = dir.path().join("f");
    get(&server, &dest, &data).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), data);
    assert!(server
        .requests()
        .iter()
        .any(|(p, r)| p == "f.bin" && r.as_deref().is_some_and(|r| r.starts_with("bytes="))));
}

#[test]
fn starts_over_when_the_server_ignores_the_range() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    server.put("f.bin", data.clone());
    server.ignore_range("f.bin");
    let dest = dir.path().join("f");
    fs::write(dir.path().join("f.part"), vec![0xEE; 1000]).unwrap();
    get(&server, &dest, &data).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), data);
}

#[test]
fn a_corrupt_download_is_deleted_and_reported() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    server.put("f.bin", vec![0u8; data.len()]);
    let dest = dir.path().join("f");
    assert!(matches!(
        get(&server, &dest, &data),
        Err(FetchError::Mismatch)
    ));
    assert!(!dest.exists() && !dir.path().join("f.part").exists());
}

#[test]
fn refuses_hosts_outside_the_allow_list() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    let f = Fetcher::new(&["archive.org"]);
    let r = f.fetch(
        "f",
        &source(server.url("f.bin"), &data),
        &dir.path().join("f"),
        &AtomicBool::new(false),
        &mut |_| {},
    );
    assert!(matches!(r, Err(FetchError::NotAllowed(_))));
    assert!(server.requests().is_empty());
}

#[test]
fn gives_up_after_the_retries() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    assert!(matches!(
        get(&server, &dir.path().join("f"), &data),
        Err(FetchError::Unreachable { .. })
    ));
    assert_eq!(server.requests().len(), 4);
}

#[test]
fn refuses_a_redirect_to_another_host() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    server.redirect("f.bin", "http://example.invalid/f.bin");
    assert!(matches!(
        get(&server, &dir.path().join("f"), &data),
        Err(FetchError::Unreachable { .. })
    ));
}

#[test]
fn stops_when_cancelled() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    server.put("f.bin", data.clone());
    let r = fetcher().fetch(
        "f",
        &source(server.url("f.bin"), &data),
        &dir.path().join("f"),
        &AtomicBool::new(true),
        &mut |_| {},
    );
    assert!(matches!(r, Err(FetchError::Cancelled)));
}

#[test]
fn a_valid_existing_file_is_not_downloaded_again() {
    let (server, dir, data) = (TestServer::start(), tempfile::tempdir().unwrap(), payload());
    let dest = dir.path().join("f");
    fs::write(&dest, &data).unwrap();
    get(&server, &dest, &data).unwrap();
    assert!(server.requests().is_empty());
}

#[test]
fn finds_a_matching_local_copy() {
    let (dir, data) = (tempfile::tempdir().unwrap(), payload());
    let wrong = dir.path().join("wrong");
    let right = dir.path().join("right");
    fs::write(&wrong, vec![1u8; data.len()]).unwrap();
    fs::write(&right, &data).unwrap();
    let src = source("https://archive.org/f".into(), &data);
    assert_eq!(
        find_local_copy(&src, &[dir.path().join("none"), wrong, right.clone()]),
        Some(right)
    );
}
