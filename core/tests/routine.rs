mod support;

use dsr_core::{
    aup,
    extract::Unpacker,
    fetch::{FetchError, Fetcher},
    inspect::CheckMode,
    model::{Pack, Recipe},
    routine::{Outcome, Routine, RoutineError},
    state::State,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
use support::{http::TestServer, recipes::*};

const A: &[u8] = b"alpha file\n";
const B: &[u8] = b"bravo bravo bravo bravo\n";

struct Setup {
    server: TestServer,
    zip: Vec<u8>,
    recipe: Recipe,
}

fn setup() -> Setup {
    let server = TestServer::start();
    let zip = make_zip(&[
        ("pack/a.txt", A),
        ("pack/data/b.txt", B),
        ("pack/cfg.ini", b"x=1\n"),
    ]);
    server.put("pack.zip", zip.clone());
    let recipe = recipe(
        "t-r1",
        vec![("pack", zip_source(&server.url("pack.zip"), &zip))],
        vec![
            exact("a.txt", "pack", "pack/a.txt", A, None),
            exact(
                "data/b.txt.aup",
                "pack",
                "pack/data/b.txt",
                B,
                Some(Pack::Aup),
            ),
            seed("cfg.ini", "pack", "pack/cfg.ini"),
        ],
        &["launcher.exe"],
    );
    Setup {
        server,
        zip,
        recipe,
    }
}

fn routine(folder: &Path, copies: Vec<PathBuf>) -> Routine {
    Routine {
        folder: folder.to_path_buf(),
        fetcher: Fetcher::new(&["127.0.0.1"]).with_retries(1, Duration::from_millis(1)),
        unpacker: Unpacker {
            innoextract: PathBuf::from("not-needed-for-zips"),
        },
        extra_copies: copies,
    }
}

fn run(folder: &Path, recipe: &Recipe, copies: Vec<PathBuf>) -> Result<Outcome, RoutineError> {
    routine(folder, copies).run(
        recipe,
        CheckMode::Quick,
        &AtomicBool::new(false),
        &mut |_| {},
    )
}

#[test]
fn a_fresh_install_builds_everything_and_cleans_up() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    let out = run(&game, &s.recipe, vec![]).unwrap();
    assert_eq!(out.built, vec!["a.txt", "data/b.txt.aup", "cfg.ini"]);
    assert_eq!(fs::read(game.join("a.txt")).unwrap(), A);
    assert_eq!(
        aup::decode(&fs::read(game.join("data/b.txt.aup")).unwrap()).unwrap(),
        B
    );
    assert!(game.join("cfg.ini").exists());
    assert!(
        !game.join(".reborn/downloads/pack").exists(),
        "our download is removed after the build"
    );
    assert!(!game.join(".reborn/staging").exists());
    assert_eq!(State::load(&game).unwrap().game, "t-r1");
}

#[test]
fn a_second_run_needs_no_network() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    run(&game, &s.recipe, vec![]).unwrap();
    s.server.remove("pack.zip");
    assert_eq!(run(&game, &s.recipe, vec![]).unwrap(), Outcome::default());
}

#[test]
fn a_deleted_file_is_rebuilt_and_reported_as_vanished() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    run(&game, &s.recipe, vec![]).unwrap();
    fs::remove_file(game.join("a.txt")).unwrap();
    let out = run(&game, &s.recipe, vec![]).unwrap();
    assert_eq!(
        (out.built.clone(), out.vanished),
        (vec!["a.txt".to_string()], vec!["a.txt".to_string()])
    );
}

#[test]
fn seed_files_are_never_overwritten() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("cfg.ini"), b"player's own settings").unwrap();
    run(&game, &s.recipe, vec![]).unwrap();
    assert_eq!(
        fs::read(game.join("cfg.ini")).unwrap(),
        b"player's own settings"
    );
}

#[test]
fn forbidden_files_are_deleted() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    run(&game, &s.recipe, vec![]).unwrap();
    fs::write(game.join("launcher.exe"), b"old").unwrap();
    assert_eq!(
        run(&game, &s.recipe, vec![]).unwrap().deleted,
        vec!["launcher.exe"]
    );
    assert!(!game.join("launcher.exe").exists());
}

#[test]
fn a_copy_in_the_game_folder_replaces_the_download_and_is_kept() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    s.server.remove("pack.zip");
    fs::create_dir_all(&game).unwrap();
    fs::write(game.join("my-pack.zip"), &s.zip).unwrap();
    run(&game, &s.recipe, vec![]).unwrap();
    assert!(game.join("a.txt").exists() && game.join("my-pack.zip").exists());
}

#[test]
fn a_copy_the_player_picked_replaces_the_download() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    s.server.remove("pack.zip");
    let copy = dir.path().join("elsewhere.zip");
    fs::write(&copy, &s.zip).unwrap();
    run(&dir.path().join("game"), &s.recipe, vec![copy]).unwrap();
}

#[test]
fn a_source_that_never_matches_stops_after_one_retry() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    s.server.put("pack.zip", vec![0u8; s.zip.len()]);
    let err = run(&dir.path().join("game"), &s.recipe, vec![]).unwrap_err();
    assert!(matches!(err, RoutineError::Fetch(FetchError::Mismatch)));
    assert_eq!(
        s.server
            .requests()
            .iter()
            .filter(|(p, _)| p == "pack.zip")
            .count(),
        2
    );
}

#[test]
fn a_new_revision_rebuilds_only_what_changed() {
    let (s, dir) = (setup(), tempfile::tempdir().unwrap());
    let game = dir.path().join("game");
    run(&game, &s.recipe, vec![]).unwrap();
    let zip2 = make_zip(&[("pack/a.txt", b"alpha v2\n")]);
    s.server.put("pack2.zip", zip2.clone());
    let mut r2 = s.recipe.clone();
    r2.game = "t-r2".into();
    r2.sources.insert(
        "pack2".into(),
        zip_source(&s.server.url("pack2.zip"), &zip2),
    );
    r2.files[0] = exact("a.txt", "pack2", "pack/a.txt", b"alpha v2\n", None);
    assert_eq!(run(&game, &r2, vec![]).unwrap().built, vec!["a.txt"]);
    assert_eq!(fs::read(game.join("a.txt")).unwrap(), b"alpha v2\n");
}
