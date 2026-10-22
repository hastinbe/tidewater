//! Game files kept inside the installer archive on a disc.
//!
//! The Battle.net Edition CD has no loose `War2Dat.mpq`. Its `INSTALL.EXE` is a small Windows program followed
//! directly by an MPQ archive (the archive header sits on a 512-byte boundary after the program, which the MPQ
//! format allows and which [`tidewater_formats::mpq::Archive`] already handles). The archive holds the files the
//! installer copies: the game's data archive and the stock maps under their install names, and the videos and music
//! under theirs. The installed game keeps that same file next to its data archive as `Install.mpq`, so the importer
//! does the same: `Install.mpq` is the whole disc file (the archive is read in place, with the program in front of it
//! left alone), and the data archive and maps are read out of it.
//!
//! The archive has no list of names, only hashes of them, so the files the importer wants are looked up by name:
//! the names the installer gives them are the ones an installed game has (`War2Dat.mpq` in a `Files` folder, and the
//! maps in a `Maps` folder).
//!
//! Sources: the MPQ format is described by the StormLib documentation and the Zezula MPQ pages (see
//! `tidewater_formats::mpq`); the disc's own layout is read with the ISO 9660 reader in the parent module.

use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::sync::Mutex;

use tidewater_formats::mpq::Archive;

use super::{Disc, GAME_DEPTH};

/// The folder in the installer archive that holds the files copied next to the game program.
const FILES_DIR: &str = "Files\\";
/// The data archives copied out of the installer archive, in the order they are listed.
const ARCHIVES: [&str; 2] = ["War2Dat.mpq", "War2Patch.mpq"];
/// The folder in the installer archive that holds the stock maps.
const MAPS_DIR: &str = "Maps\\";
/// The stock maps the installer puts in the game's `Maps` folder. The installer archive holds many more maps (the
/// older Warcraft II ones), which an installed game doesn't have in `Maps`, and their names can't be recovered from
/// the archive, so they aren't imported.
const MAPS: [&str; 26] = [
    "All You Need BNE.pud",
    "Ant Trails BNE.pud",
    "Big Rock Candy Mountain BNE.pud",
    "Cramped BNE.pud",
    "Crosshair BNE.pud",
    "Dark Paths BNE.pud",
    "Dark Peninsula BNE.pud",
    "Forsaken Isles BNE.pud",
    "Frosty Fjords BNE.pud",
    "Gold Rush BNE.pud",
    "Great White North BNE.pud",
    "Isolation BNE.pud",
    "Kaboom BNE.pud",
    "More Precious than Gold BNE.pud",
    "Mud in Your Eye BNE.pud",
    "Murky River BNE.pud",
    "Rose Petal BNE.pud",
    "Schwartzwald BNE.pud",
    "Skirmish BNE.pud",
    "Stir Crazy BNE.pud",
    "Taiga BNE.pud",
    "The River Kwai BNE.pud",
    "Training Ground BNE.pud",
    "Widow's End BNE.pud",
    "Winding ways BNE.pud",
    "World Domination BNE.pud",
];

/// How far into a file the archive's header is looked for. The program in front of it is a few hundred KiB.
const MAX_STUB: usize = 4 * 1024 * 1024;
/// Smallest file worth looking at: the archive holds the game's data.
const MIN_SIZE: u64 = 1024 * 1024;

/// A file inside the installer archive that the importer lists.
#[derive(Clone, Debug)]
pub(super) struct Inner {
    /// The name in the installer archive (backslash separated).
    pub name: String,
}

/// The installer archive found on a disc.
pub(super) struct Installer {
    /// Index of the disc file that holds the archive.
    pub file: usize,
    archive: Mutex<Archive>,
}

impl Installer {
    /// Reads a file out of the archive.
    pub fn read(&self, inner: &Inner) -> io::Result<Box<dyn Read>> {
        let mut a = self
            .archive
            .lock()
            .map_err(|_| io::Error::other("the archive lock is poisoned"))?;
        let bytes = a
            .read(&inner.name)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", inner.name)))?;
        Ok(Box::new(Cursor::new(bytes)))
    }

    /// The files to list, as (name on the importer's side, archive entry, size): the data archives and stock maps
    /// the archive has.
    pub fn entries(&self) -> Vec<(String, Inner, u64)> {
        let Ok(a) = self.archive.lock() else {
            return Vec::new();
        };
        let size = |name: &str| a.find(name).and_then(|b| a.blocks().get(b)).map(|b| u64::from(b.size));
        let mut out = Vec::new();
        for want in ARCHIVES {
            let name = format!("{FILES_DIR}{want}");
            if let Some(size) = size(&name) {
                out.push((want.to_string(), Inner { name }, size));
            }
        }
        for want in MAPS {
            let name = format!("{MAPS_DIR}{want}");
            if let Some(size) = size(&name) {
                out.push((format!("Maps/{want}"), Inner { name }, size));
            }
        }
        out
    }
}

/// Finds the installer archive on `disc`: a large `.exe` or `.mpq` near the top whose bytes contain an MPQ header
/// after a program, and which holds the data archive. `None` when the disc has no such file.
pub(super) fn find(disc: &Disc) -> Option<Installer> {
    let mut candidates: Vec<usize> = disc
        .files()
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let lower = f.path.to_ascii_lowercase();
            f.size >= MIN_SIZE
                && f.path.matches('/').count() <= GAME_DEPTH
                && (lower.ends_with(".exe") || lower.ends_with(".mpq"))
        })
        .map(|(i, _)| i)
        .collect();
    // The biggest first: the installer archive is the disc's bulk.
    candidates.sort_by_key(|&i| std::cmp::Reverse(disc.files()[i].size));
    for file in candidates {
        let Ok(mut r) = disc.read_file(&disc.files()[file]) else {
            continue;
        };
        if !has_archive_header(&mut r) {
            continue;
        }
        if r.seek(SeekFrom::Start(0)).is_err() {
            continue;
        }
        let Ok(archive) = Archive::from_reader(Box::new(r)) else {
            continue;
        };
        if ARCHIVES.iter().any(|n| archive.contains(&format!("{FILES_DIR}{n}"))) {
            return Some(Installer {
                file,
                archive: Mutex::new(archive),
            });
        }
    }
    None
}

/// Whether `r` has `MPQ\x1A` at the start or at a later 512-byte boundary within the first [`MAX_STUB`] bytes.
fn has_archive_header(r: &mut (impl Read + Seek)) -> bool {
    let mut head = Vec::new();
    if r.take(MAX_STUB as u64).read_to_end(&mut head).is_err() {
        return false;
    }
    (0..head.len())
        .step_by(512)
        .any(|at| head[at..].starts_with(b"MPQ\x1A"))
}
