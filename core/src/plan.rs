//! Turns an inspection into work: which files to (re)build, from which sources, and what to delete.
//! Pure: no I/O.

use crate::{
    inspect::{FileStatus, Inspection},
    model::{norm_path, Recipe},
};
use std::collections::BTreeSet;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Source ids needed for the files below, sorted.
    pub sources: Vec<String>,
    /// Recipe paths to build, in recipe order.
    pub build: Vec<String>,
    /// `absent` entries to delete.
    pub delete: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.build.is_empty() && self.delete.is_empty()
    }
}

pub fn make_plan(recipe: &Recipe, inspection: &Inspection) -> Plan {
    let mut sources = BTreeSet::new();
    let mut build = Vec::new();
    for e in &recipe.files {
        let status = inspection
            .statuses
            .get(&norm_path(&e.path))
            .copied()
            .unwrap_or(FileStatus::Missing);
        if status != FileStatus::Good {
            sources.insert(e.from.source.clone());
            build.push(e.path.clone());
        }
    }
    Plan {
        sources: sources.into_iter().collect(),
        build,
        delete: inspection.absent_present.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileEntry, FileFrom, Mode, Source, SourceKind};
    use std::collections::BTreeMap;
    use FileStatus::*;

    fn entry(path: &str, source: &str) -> FileEntry {
        FileEntry {
            path: path.into(),
            mode: Mode::Exact,
            size: Some(1),
            sha256: Some("0".repeat(64)),
            from: FileFrom {
                source: source.into(),
                path: format!("src/{path}"),
                pack: None,
            },
        }
    }

    fn recipe() -> Recipe {
        let src = |kind| Source {
            url: "https://archive.org/x".into(),
            size: 1,
            sha256: "0".repeat(64),
            kind,
        };
        Recipe {
            schema: 1,
            game: "g".into(),
            min_launcher: "1.0.0".into(),
            sources: BTreeMap::from([
                ("installer".to_string(), src(SourceKind::Inno)),
                ("tr".to_string(), src(SourceKind::Zip)),
            ]),
            files: vec![
                entry("A.exe", "installer"),
                entry("data/b.aup", "tr"),
                entry("data/c.aup", "installer"),
            ],
            absent: vec!["launcher.exe".into()],
        }
    }

    fn inspection(statuses: &[(&str, FileStatus)], absent: &[&str]) -> Inspection {
        Inspection {
            statuses: statuses.iter().map(|(p, s)| (norm_path(p), *s)).collect(),
            stamps: BTreeMap::new(),
            absent_present: absent.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn nothing_to_do_when_everything_is_good() {
        let i = inspection(
            &[("A.exe", Good), ("data/b.aup", Good), ("data/c.aup", Good)],
            &[],
        );
        assert!(make_plan(&recipe(), &i).is_empty());
    }

    #[test]
    fn a_missing_installer_file_needs_only_the_installer() {
        let i = inspection(
            &[
                ("A.exe", Missing),
                ("data/b.aup", Good),
                ("data/c.aup", Good),
            ],
            &[],
        );
        let p = make_plan(&recipe(), &i);
        assert_eq!(
            (p.sources, p.build),
            (vec!["installer".to_string()], vec!["A.exe".to_string()])
        );
    }

    #[test]
    fn a_wrong_zip_file_needs_only_that_zip() {
        let i = inspection(
            &[("A.exe", Good), ("data/b.aup", Wrong), ("data/c.aup", Good)],
            &[],
        );
        assert_eq!(make_plan(&recipe(), &i).sources, vec!["tr".to_string()]);
    }

    #[test]
    fn files_not_inspected_count_as_missing() {
        let p = make_plan(&recipe(), &inspection(&[], &[]));
        assert_eq!(p.build.len(), 3);
        assert_eq!(p.sources, vec!["installer".to_string(), "tr".to_string()]);
    }

    #[test]
    fn forbidden_files_are_deleted() {
        let i = inspection(
            &[("A.exe", Good), ("data/b.aup", Good), ("data/c.aup", Good)],
            &["launcher.exe"],
        );
        let p = make_plan(&recipe(), &i);
        assert_eq!(p.delete, vec!["launcher.exe".to_string()]);
        assert!(!p.is_empty());
    }
}
