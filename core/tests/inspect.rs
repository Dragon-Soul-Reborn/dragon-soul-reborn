mod support;

use dsr_core::{
    hash,
    inspect::{inspect, CheckMode, FileStatus, Inspection},
    model::{norm_path, Recipe},
    progress::Progress,
    state::{mtime_ns, Stamp, State},
};
use std::{fs, path::Path};
use support::recipes::{exact, recipe, seed, zip_source};

fn sample() -> Recipe {
    recipe(
        "t",
        vec![("pack", zip_source("https://archive.org/p.zip", b"zip"))],
        vec![
            exact("a.txt", "pack", "a", b"aaaa", None),
            exact("dir/b.txt", "pack", "b", b"bbbb", None),
            seed("cfg.ini", "pack", "c"),
        ],
        &["launcher.exe"],
    )
}

fn write(folder: &Path, rel: &str, data: &[u8]) {
    let p = folder.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}

fn status(i: &Inspection, p: &str) -> FileStatus {
    i.statuses[&norm_path(p)]
}

fn run(folder: &Path, state: Option<&State>, mode: CheckMode) -> Inspection {
    inspect(folder, &sample(), state, mode, &mut |_| {}).unwrap()
}

#[test]
fn reports_good_wrong_and_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"aaaa");
    write(dir.path(), "dir/b.txt", b"BBBB");
    let i = run(dir.path(), None, CheckMode::Full);
    assert_eq!(status(&i, "a.txt"), FileStatus::Good);
    assert_eq!(status(&i, "dir/b.txt"), FileStatus::Wrong);
    assert_eq!(status(&i, "cfg.ini"), FileStatus::Missing);
    assert!(i.stamps.contains_key("a.txt") && !i.stamps.contains_key("dir/b.txt"));
}

#[test]
fn a_wrong_size_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"aaaaa");
    assert_eq!(
        status(&run(dir.path(), None, CheckMode::Quick), "a.txt"),
        FileStatus::Wrong
    );
}

#[test]
fn quick_mode_trusts_a_matching_stamp_and_full_mode_does_not() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"XXXX");
    let m = fs::metadata(dir.path().join("a.txt")).unwrap();
    let mut state = State {
        game: "t".into(),
        ..Default::default()
    };
    state.files.insert(
        "a.txt".into(),
        Stamp {
            size: 4,
            mtime_ns: mtime_ns(&m),
            sha256: hash::sha256_bytes(b"aaaa"),
        },
    );
    assert_eq!(
        status(&run(dir.path(), Some(&state), CheckMode::Quick), "a.txt"),
        FileStatus::Good
    );
    assert_eq!(
        status(&run(dir.path(), Some(&state), CheckMode::Full), "a.txt"),
        FileStatus::Wrong
    );
}

#[test]
fn quick_mode_rehashes_a_file_changed_since_its_stamp() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"XXXX");
    let m = fs::metadata(dir.path().join("a.txt")).unwrap();
    let mut state = State::default();
    state.files.insert(
        "a.txt".into(),
        Stamp {
            size: 4,
            mtime_ns: mtime_ns(&m) + 1,
            sha256: hash::sha256_bytes(b"aaaa"),
        },
    );
    assert_eq!(
        status(&run(dir.path(), Some(&state), CheckMode::Quick), "a.txt"),
        FileStatus::Wrong
    );
}

#[test]
fn seed_files_only_need_to_exist() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "cfg.ini", b"anything the player saved");
    assert_eq!(
        status(&run(dir.path(), None, CheckMode::Full), "cfg.ini"),
        FileStatus::Good
    );
}

#[test]
fn finds_forbidden_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "launcher.exe", b"old");
    assert_eq!(
        run(dir.path(), None, CheckMode::Quick).absent_present,
        vec!["launcher.exe"]
    );
}

#[test]
fn reports_progress_up_to_the_total() {
    let dir = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    inspect(dir.path(), &sample(), None, CheckMode::Quick, &mut |p| {
        seen.push(p)
    })
    .unwrap();
    assert_eq!(seen.last(), Some(&Progress::Checking { done: 3, total: 3 }));
}
