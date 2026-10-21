//! Checks that a [`Source`] holds a usable Battle.net Edition: opens every archive, requires `War2Dat.mpq`, and reads a
//! few files the client loads at start-up (the unit table, a font, a string table, a tileset). `Install.mpq` (the
//! videos and music), `War2Patch.mpq` and the `Maps` folder are optional; a missing `Install.mpq` or `Maps` is a
//! warning. Messages are written for the player.

use std::path::{Path, PathBuf};

use tidewater_formats::fnt::Font;
use tidewater_formats::install::{Files, Install};
use tidewater_formats::mpq::{Archive, ReadSeek, flags};
use tidewater_formats::pud::Era;
use tidewater_formats::tbl::StringTable;
use tidewater_formats::udta::UnitTable;

use crate::ImportError;
use crate::source::Source;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    /// No `War2Dat.mpq`.
    NoWar2Dat,
    /// An archive can't be opened.
    Damaged,
    /// An archive opens but a file the client needs is missing or unreadable.
    MissingKeyFile,
    NoInstallMpq,
    NoMaps,
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub severity: Severity,
    pub kind: IssueKind,
    pub message: String,
}

/// One archive that opened.
#[derive(Clone, Debug)]
pub struct ArchiveInfo {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    /// Files in use in the archive.
    pub files: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub archives: Vec<ArchiveInfo>,
    /// `.pud` maps under the source's `Maps` folder.
    pub map_count: usize,
    /// A guess at which game this is.
    pub edition: String,
    pub issues: Vec<Issue>,
}

impl Report {
    pub fn is_ok(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn has_archive(&self, name: &str) -> bool {
        self.archives.iter().any(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// `Ok(self)` when nothing is wrong enough to stop, else the first error.
    pub fn into_result(self) -> Result<Self, ImportError> {
        match self.issues.iter().find(|i| i.severity == Severity::Error) {
            Some(i) => Err(ImportError::Invalid(i.clone())),
            None => Ok(self),
        }
    }
}

/// One check of a file the client needs.
pub struct KeyCheck {
    pub label: &'static str,
    pub check: fn(&mut Install) -> Result<(), String>,
}

fn read_ok(i: &mut Install, name: &str) -> Result<Vec<u8>, String> {
    i.read(name).map_err(|e| e.to_string())
}

/// What the client loads at start-up and for a first game.
pub const KEY_CHECKS: &[KeyCheck] = &[
    KeyCheck {
        label: "the unit table (rez\\unitdata.dat)",
        check: |i| {
            UnitTable::from_table(&read_ok(i, "rez\\unitdata.dat")?)
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    },
    KeyCheck {
        label: "a font (Art\\font10x.fnt)",
        check: |i| {
            Font::parse(&read_ok(i, "Art\\font10x.fnt")?)
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    },
    KeyCheck {
        label: "a string table (rez\\network.tbl)",
        check: |i| {
            StringTable::parse(&read_ok(i, "rez\\network.tbl")?)
                .map(|_| ())
                .map_err(|e| e.to_string())
        },
    },
    KeyCheck {
        label: "the Forest tileset",
        check: |i| i.tileset(Era::Forest).map(|_| ()).map_err(|e| e.to_string()),
    },
];

/// A file the title screen shows only in the Battle.net Edition; its presence is the edition guess.
const EDITION_MARKER: &str = "Art\\Title_Rel_BNE.pcx";

pub fn validate(source: &Source) -> Report {
    validate_with(source, KEY_CHECKS)
}

/// [`validate`] with a given list of key-file checks (tests use their own, as synthetic archives can't hold real data).
pub fn validate_with(source: &Source, checks: &[KeyCheck]) -> Report {
    let mut report = Report {
        edition: "unknown edition".into(),
        ..Report::default()
    };
    let mut known: Vec<(String, PathBuf)> = Vec::new();
    for path in &source.archives {
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match open_info(path, &name) {
            Ok(info) => {
                known.push((name.to_ascii_lowercase(), path.clone()));
                report.archives.push(info);
            }
            Err(why) => report.issues.push(error(
                IssueKind::Damaged,
                format!("{name} is damaged (it can't be opened: {why}). Re-install the game or copy the file again."),
            )),
        }
    }
    report.map_count = source.maps.as_deref().map_or(0, count_maps);

    let damaged = report.issues.iter().any(|i| i.kind == IssueKind::Damaged);
    if !report.has_archive("war2dat.mpq") && !damaged {
        let msg = if report.has_archive("install.mpq") {
            "Found Install.mpq but not War2Dat.mpq. If this is the CD, choose the folder where the game was \
             installed, not the disc."
        } else {
            "Found archive files, but not War2Dat.mpq. Choose the folder where Warcraft II: Battle.net Edition is \
             installed."
        };
        report.issues.push(error(IssueKind::NoWar2Dat, msg.into()));
    }
    if report.has_archive("war2dat.mpq") && !damaged {
        let wanted = ["install.mpq", "war2dat.mpq", "war2patch.mpq"];
        let set = Set(known
            .into_iter()
            .filter(|(n, _)| wanted.contains(&n.as_str()))
            .collect());
        match Install::from_files(Box::new(set)) {
            Ok(mut install) => {
                for c in checks {
                    if let Err(why) = (c.check)(&mut install) {
                        report.issues.push(error(
                            IssueKind::MissingKeyFile,
                            format!(
                                "War2Dat.mpq opens, but {} can't be read ({why}). The game files are incomplete or \
                                 damaged; re-install the game and try again.",
                                c.label
                            ),
                        ));
                    }
                }
                if install.contains(EDITION_MARKER) {
                    report.edition = "Warcraft II: Battle.net Edition".into();
                }
            }
            Err(e) => report.issues.push(error(
                IssueKind::Damaged,
                format!("The game archives can't be opened together ({e})."),
            )),
        }
    }
    if report.is_ok() {
        if !report.has_archive("install.mpq") {
            report.issues.push(warning(
                IssueKind::NoInstallMpq,
                "No Install.mpq: the game will have no videos or music.".into(),
            ));
        }
        if report.map_count == 0 {
            report.issues.push(warning(
                IssueKind::NoMaps,
                "No Maps folder: there will be no multiplayer map list.".into(),
            ));
        }
    }
    report
}

fn error(kind: IssueKind, message: String) -> Issue {
    Issue {
        severity: Severity::Error,
        kind,
        message,
    }
}

fn warning(kind: IssueKind, message: String) -> Issue {
    Issue {
        severity: Severity::Warning,
        kind,
        message,
    }
}

fn open_info(path: &Path, name: &str) -> Result<ArchiveInfo, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let archive = Archive::open(path).map_err(|e| e.to_string())?;
    let files = archive.blocks().iter().filter(|b| b.flags & flags::EXISTS != 0).count();
    Ok(ArchiveInfo {
        name: name.to_string(),
        path: path.to_path_buf(),
        size,
        files,
    })
}

fn count_maps(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.filter_map(Result::ok)
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => count_maps(&e.path()),
            Ok(t) if t.is_file() && e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("pud")) => 1,
            _ => 0,
        })
        .sum()
}

/// The archives by lower-case name, as the installation's files (loose files don't count).
struct Set(Vec<(String, PathBuf)>);

impl Files for Set {
    fn open(&self, name: &str) -> std::io::Result<Option<Box<dyn ReadSeek>>> {
        match self.0.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some((_, p)) => Ok(Some(Box::new(std::fs::File::open(p)?))),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::classify;
    use crate::testutil::write_mpq;

    fn marker(i: &mut Install) -> Result<(), String> {
        read_ok(i, "rez\\marker.txt").map(|_| ())
    }
    const MARKER: &[KeyCheck] = &[KeyCheck {
        label: "the marker",
        check: marker,
    }];

    #[test]
    fn a_complete_set_passes_with_warnings_for_the_optional_parts() {
        let t = tempfile::tempdir().unwrap();
        write_mpq(&t.path().join("War2Dat.mpq"), &[("rez\\marker.txt", b"m")]);
        let r = validate_with(&classify(t.path()).unwrap(), MARKER);
        assert!(r.is_ok(), "{:?}", r.issues);
        let kinds: Vec<_> = r.issues.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, [IssueKind::NoInstallMpq, IssueKind::NoMaps]);
        assert!(r.issues[0].message.contains("no videos or music"));
        assert_eq!(r.archives[0].files, 1);
    }

    #[test]
    fn a_missing_war2dat_is_an_error_with_the_cd_hint() {
        let t = tempfile::tempdir().unwrap();
        write_mpq(&t.path().join("Install.mpq"), &[("x", b"x")]);
        let r = validate_with(&classify(t.path()).unwrap(), MARKER);
        assert!(!r.is_ok());
        let err = r.into_result().unwrap_err();
        assert!(err.to_string().contains("not the disc"));
        assert_eq!(err.exit_code(), 14);
    }

    #[test]
    fn a_damaged_archive_is_named() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("War2Dat.mpq"), b"not an archive at all, just text").unwrap();
        let r = validate_with(&classify(t.path()).unwrap(), MARKER);
        let err = r.into_result().unwrap_err();
        assert!(err.to_string().starts_with("War2Dat.mpq is damaged"));
        assert_eq!(err.exit_code(), 15);
    }

    #[test]
    fn a_missing_key_file_is_incomplete_and_real_checks_flag_synthetic_data() {
        let t = tempfile::tempdir().unwrap();
        write_mpq(&t.path().join("War2Dat.mpq"), &[("other", b"o")]);
        let s = classify(t.path()).unwrap();
        let err = validate_with(&s, MARKER).into_result().unwrap_err();
        assert_eq!(err.exit_code(), 16);
        assert!(!validate(&s).is_ok());
    }
}
