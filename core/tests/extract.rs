mod support;

use dsr_core::extract::{include_args, ExtractError, ZipSource};
use std::{fs, path::MAIN_SEPARATOR};
use support::recipes::make_zip;

#[test]
fn lists_and_reads_zip_members() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.zip");
    fs::write(
        &path,
        make_zip(&[("p/", b""), ("p/a.txt", b"alpha"), ("p/d/b.txt", b"bravo")]),
    )
    .unwrap();
    let mut z = ZipSource::open(&path).unwrap();
    assert_eq!(z.file_names(), vec!["p/a.txt", "p/d/b.txt"]);
    assert_eq!(z.read("p/d/b.txt").unwrap(), b"bravo");
    assert!(matches!(
        z.read("p/nope"),
        Err(ExtractError::MissingMember { .. })
    ));
}

#[test]
fn a_damaged_zip_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.zip");
    fs::write(&path, b"not a zip").unwrap();
    assert!(matches!(
        ZipSource::open(&path),
        Err(ExtractError::Zip { .. })
    ));
}

#[test]
fn include_arguments_anchor_each_path_at_the_root() {
    let args = include_args(&["app/data/x.aup".to_string()]);
    let s = MAIN_SEPARATOR;
    assert_eq!(
        args,
        vec!["--include".to_string(), format!("{s}app{s}data{s}x.aup")]
    );
}

#[test]
fn a_long_path_list_extracts_everything() {
    let many: Vec<String> = (0..201).map(|i| format!("app/{i}")).collect();
    assert!(include_args(&many).is_empty());
    assert!(include_args(&[]).is_empty());
}
