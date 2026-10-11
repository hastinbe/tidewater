//! A user's installation: the data archives plus loose files, read through one interface.
//!
//! Lookup order (highest priority first): loose files under the install directory, `War2Patch.mpq`, `War2Dat.mpq`,
//! `Install.mpq`. Names use backslashes and are case-insensitive, as in the archives.

use std::path::{Path, PathBuf};

use crate::mpq::{Archive, ArchiveSet, MpqError};
use crate::pud::Era;
use crate::tileset::{ChangeTable, ChangeTables, Tileset, TilesetError};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("{0}")]
    Mpq(#[from] MpqError),
    #[error("{0}")]
    Tileset(#[from] TilesetError),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("no data archives found in {0}")]
    NoArchives(PathBuf),
}

pub struct Install {
    root: PathBuf,
    archives: ArchiveSet,
}

/// Archive files in priority order, lowest first.
const ARCHIVES: [&str; 3] = ["Install.mpq", "War2Dat.mpq", "War2Patch.mpq"];

impl Install {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, InstallError> {
        let root = root.as_ref().to_path_buf();
        let mut list = Vec::new();
        for name in ARCHIVES {
            if let Some(p) = find_case_insensitive(&root, name) {
                list.push(Archive::open(p)?);
            }
        }
        if list.is_empty() {
            return Err(InstallError::NoArchives(root));
        }
        Ok(Self {
            root,
            archives: ArchiveSet::new(list),
        })
    }

    /// Reads a game file by its archive name, e.g. `rez\unitdata.dat`.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>, InstallError> {
        let rel: PathBuf = name.split(['\\', '/']).collect();
        if let Some(p) = find_case_insensitive(&self.root, &rel.to_string_lossy()) {
            return Ok(std::fs::read(p)?);
        }
        Ok(self.archives.read(name)?)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.archives.contains(name)
    }

    /// The tileset directory name used by an era.
    pub fn tileset_name(era: Era) -> &'static str {
        match era {
            Era::Forest => "Forest",
            Era::Winter => "Iceland",
            Era::Wasteland => "Swamp",
            Era::Swamp => "XSwamp",
        }
    }

    pub fn tileset(&mut self, era: Era) -> Result<Tileset, InstallError> {
        let n = Self::tileset_name(era);
        let lower = n.to_ascii_lowercase();
        let mut f = |ext: &str| self.read(&format!("Art\\bgs\\{n}\\{lower}.{ext}"));
        let (ppl, cv4, vx4, vr4) = (f("ppl")?, f("cv4")?, f("vx4")?, f("vr4")?);
        let mut ts = Tileset::parse(&ppl, &cv4, &vx4, &vr4)?;
        // The change tables are named after the tileset's first letter, except XSwamp's.
        let letter = if n == "XSwamp" {
            'x'
        } else {
            lower.as_bytes()[0] as char
        };
        let mut table = |kind: &str| -> Result<ChangeTable, InstallError> {
            Ok(ChangeTable::parse(
                &self.read(&format!("Art\\bgs\\{n}\\n{letter}_{kind}.bin"))?,
            )?)
        };
        ts.changes = Some(ChangeTables {
            wall: table("wall")?,
            forest: table("tree")?,
            rock: table("rock")?,
        });
        Ok(ts)
    }
}

/// Resolves a relative path (components separated by `/`) under `root`, matching each component case-insensitively.
fn find_case_insensitive(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        let direct = cur.join(part);
        if direct.exists() {
            cur = direct;
            continue;
        }
        let entry = std::fs::read_dir(&cur)
            .ok()?
            .filter_map(Result::ok)
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?;
        cur = entry.path();
    }
    cur.is_file().then_some(cur)
}
