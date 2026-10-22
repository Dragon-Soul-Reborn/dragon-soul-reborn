mod support;

use dsr_core::{
    apply::delete_absent, extract::Unpacker, fetch::Fetcher, inspect::CheckMode, routine::Routine,
};
use std::{fs, sync::atomic::AtomicBool};
use support::recipes;

#[test]
fn forbidden_file_names_do_not_authorize_removing_directories() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("launcher.exe");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("user-file.txt"), b"keep me").unwrap();
    assert!(delete_absent(dir.path(), &["launcher.exe".into()]).is_err());
    assert_eq!(fs::read(nested.join("user-file.txt")).unwrap(), b"keep me");
}

#[test]
fn routine_rejects_an_unsafe_recipe_before_creating_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("game");
    let source = recipes::zip_source("https://archive.org/x", b"x");
    let recipe = recipes::recipe("test", vec![("../outside", source)], vec![], &[]);
    let routine = Routine {
        folder: folder.clone(),
        fetcher: Fetcher::new(&["archive.org"]),
        unpacker: Unpacker {
            innoextract: "unused".into(),
        },
        extra_copies: vec![],
    };
    assert!(routine
        .run(
            &recipe,
            CheckMode::Full,
            &AtomicBool::new(false),
            &mut |_| {}
        )
        .is_err());
    assert!(!folder.exists());
}
