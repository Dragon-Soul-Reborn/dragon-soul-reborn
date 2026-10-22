mod support;

use dsr_core::{
    aup, hash,
    model::{Mode, Pack, SourceKind},
    recipe_build::{build_recipe, BuildSpec, SourceSpec, EXCLUDE},
};
use std::{fs, path::Path};
use support::recipes::make_zip;

fn write(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}

#[test]
fn follows_the_client_assembly_rules() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("extracted/app");
    write(&app, "gameclient_release.exe", b"exe");
    write(&app, "launcher.exe", b"old launcher");
    write(&app, "pipfactory.dll", b"voice plugin");
    write(&app, "config/engine_config.xml", b"<cfg/>");
    write(&app, "log/setup.log", b"log");
    write(&app, "data/Same.txt.aup", &aup::encode(b"same\n"));
    write(&app, "data/changed.txt.aup", &aup::encode(b"old\n"));
    let installer = dir.path().join("setup.exe");
    fs::write(&installer, b"installer bytes").unwrap();
    let tr = dir.path().join("tr.zip");
    fs::write(
        &tr,
        make_zip(&[
            ("tr/client/data/same.txt", b"same\n"),
            ("tr/client/data/changed.txt", b"new\n"),
            ("tr/client/data/added.txt", b"added\n"),
            ("tr/server/x.txt", b"server only\n"),
        ]),
    )
    .unwrap();
    let bp = dir.path().join("bp.zip");
    fs::write(&bp, make_zip(&[("bp/data/changed.txt", b"newer\n")])).unwrap();
    let source = |id: &str, file: &Path, kind, prefix: Option<&str>| SourceSpec {
        id: id.into(),
        url: format!("https://archive.org/download/x/{id}"),
        file: file.to_path_buf(),
        kind,
        overlay_prefix: prefix.map(String::from),
    };
    let spec = BuildSpec {
        game: "0.27-r1".into(),
        min_launcher: "1.0.0".into(),
        extracted: dir.path().join("extracted"),
        sources: vec![
            source("installer", &installer, SourceKind::Inno, None),
            source("tr", &tr, SourceKind::Zip, Some("tr/client/")),
            source("bp", &bp, SourceKind::Zip, Some("bp/")),
        ],
    };
    let r = build_recipe(&spec).unwrap();
    r.validate(&["archive.org"]).unwrap();
    let get = |p: &str| {
        r.files
            .iter()
            .find(|e| e.path == p)
            .unwrap_or_else(|| panic!("{p} missing"))
    };

    assert!(r
        .files
        .iter()
        .all(|e| e.path != "launcher.exe" && e.path != "pipfactory.dll"));
    assert_eq!(
        r.absent,
        EXCLUDE.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    let exe = get("gameclient_release.exe");
    assert_eq!(
        (exe.from.path.as_str(), exe.sha256.as_deref()),
        (
            "app/gameclient_release.exe",
            Some(hash::sha256_bytes(b"exe").as_str())
        )
    );
    assert_eq!(
        get("data/Same.txt.aup").from.source,
        "installer",
        "identical content keeps the installer copy"
    );
    let changed = get("data/changed.txt.aup");
    assert_eq!(
        (
            changed.from.source.as_str(),
            changed.from.path.as_str(),
            changed.from.pack
        ),
        ("bp", "bp/data/changed.txt", Some(Pack::Aup))
    );
    assert_eq!(
        changed.sha256.as_deref(),
        Some(hash::sha256_bytes(&aup::encode(b"newer\n")).as_str())
    );
    assert_eq!(get("data/added.txt.aup").from.source, "tr");
    assert!(r.files.iter().all(|e| !e.path.contains("server")));
    assert_eq!(get("config/engine_config.xml").mode, Mode::Seed);
    assert_eq!(get("log/setup.log").mode, Mode::Seed);
    assert_eq!(
        r.sources["installer"].sha256,
        hash::sha256_bytes(b"installer bytes")
    );
}
