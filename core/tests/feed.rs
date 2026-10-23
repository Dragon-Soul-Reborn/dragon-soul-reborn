mod support;

use dsr_core::feed::{Feed, FeedError};
use support::{http::TestServer, recipes, sign::TestKey};

fn info(issued: &str, game: &str) -> Vec<u8> {
    format!(r#"{{"schema":1,"issued":"{issued}","server":{{"ip":"203.0.113.5","port":7000,"zone":1,"gametype":1,"nettype":0}},"message":"hi","game":"{game}"}}"#).into_bytes()
}

fn publish(server: &TestServer, key: &TestKey, name: &str, data: &[u8]) {
    server.put(name, data.to_vec());
    server.put(&format!("{name}.sig"), key.sign_b64(data).into_bytes());
}

const T1: &str = "2026-10-21T10:00:00Z";
const T2: &str = "2026-10-21T11:00:00Z";

#[test]
fn fetches_verifies_and_caches_server_info() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    publish(&server, &key, "server-info.json", &info(T1, "g-r1"));
    let got = Feed::new(&[&server.base], &key.public_b64())
        .server_info(cache.path())
        .unwrap();
    assert!(!got.offline);
    assert_eq!(got.value.game, "g-r1");
    assert!(cache.path().join("server-info.json.sig").exists());
}

#[test]
fn uses_the_cached_copy_when_offline() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    publish(&server, &key, "server-info.json", &info(T1, "g-r1"));
    let feed = Feed::new(&[&server.base], &key.public_b64());
    feed.server_info(cache.path()).unwrap();
    server.remove("server-info.json");
    let got = feed.server_info(cache.path()).unwrap();
    assert!(got.offline);
    assert_eq!(got.value.issued, T1);
}

#[test]
fn offline_without_a_cached_copy_is_an_error() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    let err = Feed::new(&[&server.base], &key.public_b64())
        .server_info(cache.path())
        .unwrap_err();
    assert!(matches!(err, FeedError::Unreachable));
}

#[test]
fn a_file_signed_with_another_key_is_refused() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    let feed = Feed::new(&[&server.base], &key.public_b64());
    publish(
        &server,
        &TestKey::new(),
        "server-info.json",
        &info(T1, "evil"),
    );
    assert!(matches!(
        feed.server_info(cache.path()).unwrap_err(),
        FeedError::BadSignature
    ));
    publish(&server, &key, "server-info.json", &info(T1, "good"));
    feed.server_info(cache.path()).unwrap();
    publish(
        &server,
        &TestKey::new(),
        "server-info.json",
        &info(T2, "evil"),
    );
    let got = feed.server_info(cache.path()).unwrap();
    assert_eq!((got.value.game.as_str(), got.offline), ("good", true));
}

#[test]
fn an_older_server_info_never_replaces_a_newer_one() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    let feed = Feed::new(&[&server.base], &key.public_b64());
    publish(&server, &key, "server-info.json", &info(T2, "new"));
    feed.server_info(cache.path()).unwrap();
    publish(&server, &key, "server-info.json", &info(T1, "old"));
    let got = feed.server_info(cache.path()).unwrap();
    assert_eq!((got.value.game.as_str(), got.offline), ("new", true));
}

#[test]
fn a_newer_server_info_replaces_the_cached_one() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    let feed = Feed::new(&[&server.base], &key.public_b64());
    publish(&server, &key, "server-info.json", &info(T1, "old"));
    feed.server_info(cache.path()).unwrap();
    publish(&server, &key, "server-info.json", &info(T2, "new"));
    let got = feed.server_info(cache.path()).unwrap();
    assert_eq!((got.value.game.as_str(), got.offline), ("new", false));
}

#[test]
fn tries_the_next_address_when_the_first_is_down() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    publish(&server, &key, "server-info.json", &info(T1, "g-r1"));
    let feed = Feed::new(&["http://127.0.0.1:1/", &server.base], &key.public_b64());
    assert!(!feed.server_info(cache.path()).unwrap().offline);
}

#[test]
fn fetches_a_recipe_and_checks_its_download_hosts() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    let feed = Feed::new(&[&server.base], &key.public_b64());
    let good = recipes::recipe(
        "g-r1",
        vec![("p", recipes::zip_source("https://archive.org/p.zip", b"z"))],
        vec![],
        &[],
    );
    publish(
        &server,
        &key,
        "recipes/g-r1.json",
        &serde_json::to_vec(&good).unwrap(),
    );
    assert_eq!(
        feed.recipe("g-r1", cache.path(), &["archive.org"])
            .unwrap()
            .value,
        good
    );
    let bad = recipes::recipe(
        "bad-r1",
        vec![("p", recipes::zip_source("https://evil.example/p.zip", b"z"))],
        vec![],
        &[],
    );
    publish(
        &server,
        &key,
        "recipes/bad-r1.json",
        &serde_json::to_vec(&bad).unwrap(),
    );
    assert!(matches!(
        feed.recipe("bad-r1", cache.path(), &["archive.org"]),
        Err(FeedError::Invalid(_))
    ));
}

#[test]
fn refuses_odd_game_names() {
    let (server, key, cache) = (
        TestServer::start(),
        TestKey::new(),
        tempfile::tempdir().unwrap(),
    );
    let feed = Feed::new(&[&server.base], &key.public_b64());
    assert!(matches!(
        feed.recipe("../x", cache.path(), &["archive.org"]),
        Err(FeedError::Invalid(_))
    ));
}
