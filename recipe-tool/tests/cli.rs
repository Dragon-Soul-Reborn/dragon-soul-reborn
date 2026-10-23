use std::{fs, path::PathBuf, process::Command};

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_recipe-tool"))
}

fn core_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/sig")
        .join(name)
}

const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn write_feed(dir: &std::path::Path, info_game: &str) {
    fs::create_dir_all(dir.join("recipes")).unwrap();
    fs::write(dir.join("server-info.json"), format!(r#"{{"schema":1,"issued":"2026-10-21T00:00:00Z","server":{{"ip":"127.0.0.1","port":7000,"zone":1,"gametype":1,"nettype":0}},"message":"","game":"{info_game}"}}"#)).unwrap();
    fs::write(dir.join("recipes/0.27-r1.json"), format!(r#"{{"schema":1,"game":"0.27-r1","minLauncher":"1.0.0","sources":{{"installer":{{"url":"https://archive.org/x","size":3,"sha256":"{SHA}","kind":"inno"}}}},"files":[],"absent":[]}}"#)).unwrap();
}

#[test]
fn sig_verify_accepts_the_tauri_fixture_and_rejects_tampering() {
    let pubkey = fs::read_to_string(core_fixture("tauri.pub")).unwrap();
    let ok = tool()
        .args(["sig-verify", "--pubkey", pubkey.trim()])
        .arg("--file")
        .arg(core_fixture("data.txt"))
        .arg("--sig")
        .arg(core_fixture("data.txt.sig"))
        .status()
        .unwrap();
    assert!(ok.success());
    let dir = tempfile::tempdir().unwrap();
    let tampered = dir.path().join("data.txt");
    fs::write(&tampered, b"something else").unwrap();
    let bad = tool()
        .args(["sig-verify", "--pubkey", pubkey.trim()])
        .arg("--file")
        .arg(&tampered)
        .arg("--sig")
        .arg(core_fixture("data.txt.sig"))
        .status()
        .unwrap();
    assert!(!bad.success());
}

#[test]
fn check_feed_wants_the_recipe_server_info_names() {
    let dir = tempfile::tempdir().unwrap();
    write_feed(dir.path(), "0.27-r1");
    assert!(tool()
        .arg("check-feed")
        .arg("--dir")
        .arg(dir.path())
        .status()
        .unwrap()
        .success());
    write_feed(dir.path(), "0.27-r9");
    assert!(!tool()
        .arg("check-feed")
        .arg("--dir")
        .arg(dir.path())
        .status()
        .unwrap()
        .success());
}
