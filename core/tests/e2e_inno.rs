//! End-to-end against a real (tiny) Inno Setup installer. CI builds the installer and fetches innoextract;
//! locally the test skips unless DSR_E2E_INSTALLER and DSR_INNOEXTRACT are set.

mod support;

use dsr_core::{
    aup,
    extract::Unpacker,
    fetch::Fetcher,
    inspect::CheckMode,
    model::{Pack, SourceKind},
    recipe_build::{build_recipe, BuildSpec, SourceSpec},
    routine::{Outcome, Routine},
};
use std::{fs, path::PathBuf, sync::atomic::AtomicBool, time::Duration};
use support::{http::TestServer, recipes::make_zip};

#[test]
fn installs_resumes_repairs_and_uses_a_local_copy() {
    let (Ok(installer), Ok(innoextract)) = (
        std::env::var("DSR_E2E_INSTALLER"),
        std::env::var("DSR_INNOEXTRACT"),
    ) else {
        eprintln!("skipped: set DSR_E2E_INSTALLER and DSR_INNOEXTRACT (CI does)");
        return;
    };
    let installer = PathBuf::from(installer);
    let innoextract = PathBuf::from(innoextract);
    let work = tempfile::tempdir().unwrap();

    // Build the recipe exactly as the real one is built.
    let extracted = work.path().join("extracted");
    Unpacker {
        innoextract: innoextract.clone(),
    }
    .inno(&installer, &[], &extracted)
    .unwrap();
    let tr = work.path().join("tr.zip");
    fs::write(
        &tr,
        make_zip(&[
            ("tr/client/data/hello.txt", b"hello world v2\n"),
            ("tr/client/data/same.txt", b"unchanged\n"),
            ("tr/client/data/new.txt", b"brand new\n"),
        ]),
    )
    .unwrap();
    let server = TestServer::start();
    server.put("setup.exe", fs::read(&installer).unwrap());
    server.put("tr.zip", fs::read(&tr).unwrap());
    let spec = BuildSpec {
        game: "e2e-r1".into(),
        min_launcher: "1.0.0".into(),
        extracted,
        sources: vec![
            SourceSpec {
                id: "installer".into(),
                url: server.url("setup.exe"),
                file: installer.clone(),
                kind: SourceKind::Inno,
                overlay_prefix: None,
            },
            SourceSpec {
                id: "tr".into(),
                url: server.url("tr.zip"),
                file: tr,
                kind: SourceKind::Zip,
                overlay_prefix: Some("tr/client/".into()),
            },
        ],
    };
    let recipe = build_recipe(&spec).unwrap();
    recipe.validate(&["127.0.0.1"]).unwrap();
    let from = |p: &str| {
        recipe
            .files
            .iter()
            .find(|e| e.path == p)
            .map(|e| (e.from.source.clone(), e.from.pack))
            .unwrap()
    };
    assert_eq!(
        from("data/hello.txt.aup"),
        ("tr".to_string(), Some(Pack::Aup))
    );
    assert_eq!(from("data/same.txt.aup"), ("installer".to_string(), None));
    assert_eq!(
        from("data/new.txt.aup"),
        ("tr".to_string(), Some(Pack::Aup))
    );

    let game = work.path().join("game");
    let run = || -> Outcome {
        let routine = Routine {
            folder: game.clone(),
            fetcher: Fetcher::new(&["127.0.0.1"]).with_retries(3, Duration::from_millis(10)),
            unpacker: Unpacker {
                innoextract: innoextract.clone(),
            },
            extra_copies: vec![],
        };
        routine
            .run(
                &recipe,
                CheckMode::Quick,
                &AtomicBool::new(false),
                &mut |_| {},
            )
            .unwrap()
    };

    // 1. A fresh install through an installer download that breaks halfway.
    server.cut_next("setup.exe", fs::metadata(&installer).unwrap().len() / 2);
    run();
    assert!(
        server
            .requests()
            .iter()
            .any(|(p, r)| p == "setup.exe" && r.is_some()),
        "the download resumed"
    );
    assert!(game.join("gameclient_release.exe").exists());
    assert!(!game.join("launcher.exe").exists());
    assert_eq!(
        aup::decode(&fs::read(game.join("data/hello.txt.aup")).unwrap()).unwrap(),
        b"hello world v2\n"
    );
    assert_eq!(
        aup::decode(&fs::read(game.join("data/new.txt.aup")).unwrap()).unwrap(),
        b"brand new\n"
    );

    // 2. A deleted installer file comes back and is reported.
    fs::remove_file(game.join("data/same.txt.aup")).unwrap();
    assert_eq!(run().vanished, vec!["data/same.txt.aup"]);

    // 3. With archive.org gone, an installer copy the player dropped in the folder is used and kept.
    server.remove("setup.exe");
    fs::copy(&installer, game.join("dragonsoul_setup.exe")).unwrap();
    fs::remove_file(game.join("gameclient_release.exe")).unwrap();
    run();
    assert!(game.join("gameclient_release.exe").exists());
    assert!(game.join("dragonsoul_setup.exe").exists());

    // 4. A forbidden file is removed.
    fs::write(game.join("launcher.exe"), b"old").unwrap();
    assert_eq!(run().deleted, vec!["launcher.exe"]);
}
