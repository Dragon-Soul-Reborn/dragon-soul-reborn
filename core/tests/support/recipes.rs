//! Small recipes and zips for tests.

use dsr_core::{
    aup, hash,
    model::{FileEntry, FileFrom, Mode, Pack, Recipe, Source, SourceKind},
};
use std::io::Write;

/// A zip holding the given members, stored without compression.
pub fn make_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in members {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn zip_source(url: &str, bytes: &[u8]) -> Source {
    Source {
        url: url.into(),
        size: bytes.len() as u64,
        sha256: hash::sha256_bytes(bytes),
        kind: SourceKind::Zip,
    }
}

/// An exact file built from `member` of `source`, packed as .aup when `pack` is set.
pub fn exact(path: &str, source: &str, member: &str, raw: &[u8], pack: Option<Pack>) -> FileEntry {
    let bytes = match pack {
        Some(Pack::Aup) => aup::encode(raw),
        None => raw.to_vec(),
    };
    FileEntry {
        path: path.into(),
        mode: Mode::Exact,
        size: Some(bytes.len() as u64),
        sha256: Some(hash::sha256_bytes(&bytes)),
        from: FileFrom {
            source: source.into(),
            path: member.into(),
            pack,
        },
    }
}

pub fn seed(path: &str, source: &str, member: &str) -> FileEntry {
    FileEntry {
        path: path.into(),
        mode: Mode::Seed,
        size: None,
        sha256: None,
        from: FileFrom {
            source: source.into(),
            path: member.into(),
            pack: None,
        },
    }
}

pub fn recipe(
    game: &str,
    sources: Vec<(&str, Source)>,
    files: Vec<FileEntry>,
    absent: &[&str],
) -> Recipe {
    Recipe {
        schema: 1,
        game: game.into(),
        min_launcher: "1.0.0".into(),
        sources: sources
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        files,
        absent: absent.iter().map(|s| s.to_string()).collect(),
    }
}
