//! `manifest.json`: the record of an import, kept next to the copied files: the source kind, an edition guess, every
//! copied file's relative path, size and SHA-256 (FIPS 180-4), and when it was done.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ImportError;

/// The manifest's file name inside the game folder.
pub const FILE_NAME: &str = "manifest.json";

/// Known archive hashes and the release each one identifies: `(SHA-256 of War2Dat.mpq, release name)`. Empty on
/// purpose: no hashes have been collected yet, so every import reports no known version.
pub const KNOWN_WAR2DAT: &[(&str, &str)] = &[];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// Relative to the game folder, `/` between components.
    pub path: String,
    pub size: u64,
    /// Lower-case hex.
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub version: u32,
    pub source_kind: String,
    pub edition: String,
    /// A friendly release name when `War2Dat.mpq`'s hash is in [`KNOWN_WAR2DAT`].
    pub known_version: Option<String>,
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub imported: String,
    pub files: Vec<FileEntry>,
}

impl Manifest {
    pub fn new(source_kind: &str, edition: &str, files: Vec<FileEntry>) -> Self {
        let known_version = files
            .iter()
            .find(|f| f.path.eq_ignore_ascii_case("war2dat.mpq"))
            .and_then(|f| KNOWN_WAR2DAT.iter().find(|(h, _)| *h == f.sha256))
            .map(|(_, name)| (*name).to_string());
        Self {
            version: 1,
            source_kind: source_kind.into(),
            edition: edition.into(),
            known_version,
            imported: now_utc(),
            files,
        }
    }

    pub fn write(&self, dir: &Path) -> Result<(), ImportError> {
        let json = serde_json::to_string_pretty(self).expect("a manifest is plain data");
        std::fs::write(dir.join(FILE_NAME), json + "\n").map_err(|e| ImportError::io("writing the manifest", e))
    }

    /// A manifest from the bytes of its file, or `None` when they aren't one.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    pub fn read(dir: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(dir.join(FILE_NAME)).ok()?).ok()
    }
}

fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_utc(secs)
}

/// Seconds since 1970 as an ISO 8601 UTC time (civil-from-days, H. Hinnant's public-domain algorithm).
pub fn format_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_format() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_709_210_096), "2024-02-29T12:34:56Z");
    }

    #[test]
    fn a_manifest_round_trips() {
        let t = tempfile::tempdir().unwrap();
        let m = Manifest::new(
            "install-folder",
            "x",
            vec![FileEntry {
                path: "War2Dat.mpq".into(),
                size: 3,
                sha256: "ab".into(),
            }],
        );
        m.write(t.path()).unwrap();
        assert_eq!(Manifest::read(t.path()), Some(m));
    }
}
