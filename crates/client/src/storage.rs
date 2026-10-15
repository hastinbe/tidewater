//! Storage (`docs/design/client-core.md`, migration step 8): everything the core reads or writes outside itself goes
//! through [`Storage`], which the shell provides. The installation's files, the saved games, the settings file,
//! Print Screen pictures and the wall clock. Bevy-free.
//!
//! [`Desktop`] is the desktop's: the installation folder given on the command line, the save folder and the user
//! folder (`saves`: `--save-dir`, else Tidewater's per-user data directory) and the system clock.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tidewater_formats::install::{Files, Folder, Install, InstallError};
pub use tidewater_formats::mpq::ReadSeek;

/// Why a saved game couldn't be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError {
    /// The folder or the file couldn't be made.
    CantCreate,
    /// The bytes couldn't all be written; nothing of the file is left.
    WriteFailed,
}

/// What the core reads and writes outside itself. Every method takes `&self`: a platform that needs to change state
/// to read or write keeps it inside.
pub trait Storage: Send + Sync {
    /// The installation's file `name` (archive-style, `\` or `/` between components, any letter case), or `None`.
    fn install(&self, name: &str) -> io::Result<Option<Box<dyn ReadSeek>>>;
    /// A map the user named by its own path (`--map`), or `None` when `path` is no such file; the installation's maps
    /// are read through [`install`](Self::install).
    fn user_map(&self, path: &str) -> io::Result<Option<Vec<u8>>>;
    /// The names of the files in the save folder, in the folder's own order.
    fn saves(&self) -> Vec<String>;
    fn read_save(&self, file: &str) -> io::Result<Vec<u8>>;
    /// Writes the saved game `file` whole, or leaves no file.
    fn write_save(&self, file: &str, bytes: &[u8]) -> Result<(), WriteError>;
    fn delete_save(&self, file: &str) -> io::Result<()>;
    /// The settings file's bytes, or `None` when there is none.
    fn read_settings(&self) -> Option<Vec<u8>>;
    fn write_settings(&self, bytes: &[u8]) -> io::Result<()>;
    /// Whether a Print Screen picture named `name` exists.
    fn has_screenshot(&self, name: &str) -> bool;
    fn write_screenshot(&self, name: &str, bytes: &[u8]) -> io::Result<()>;
    /// Whole seconds since 1970-01-01 UTC: a new game's seed, a loaded game's reseed, a save's time
    /// (`core/timing-and-rng.md`, "Seeding").
    fn wall_clock_secs(&self) -> u64;
}

/// The installation through a [`Storage`].
struct InstallFiles(Arc<dyn Storage>);

impl Files for InstallFiles {
    fn open(&self, name: &str) -> io::Result<Option<Box<dyn ReadSeek>>> {
        self.0.install(name)
    }
}

/// Opens the installation `storage` gives.
pub fn open_install(storage: &Arc<dyn Storage>) -> Result<Install, InstallError> {
    Install::from_files(Box::new(InstallFiles(storage.clone())))
}

/// The default save folder: `Save` in the user's data directory for Tidewater.
pub fn default_save_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "Tidewater")
        .map_or_else(|| PathBuf::from("Save"), |d| d.data_dir().join("Save"))
}

/// The folder for the program's other files (the settings file, Print Screen pictures): the folder given with
/// `--save-dir`, so a run with its own save folder writes nothing else; otherwise the folder that holds the default
/// save folder (the original keeps them in the game's folder, which holds `Save`; we never write into the install).
pub fn user_dir(save_dir: Option<&Path>) -> PathBuf {
    match save_dir {
        Some(d) => d.to_path_buf(),
        None => {
            let d = default_save_dir();
            d.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        }
    }
}

/// The desktop's storage: folders on disk and the system clock.
pub struct Desktop {
    install: Folder,
    saves: PathBuf,
    settings: PathBuf,
    shots: PathBuf,
    /// A wall clock that stands still (a scripted run's, so its games and saves come out the same every time).
    fixed_clock: Option<u64>,
}

impl Desktop {
    /// The installation in `install`, saved games in `saves`, the settings file `settings` and Print Screen pictures in
    /// `shots`.
    pub fn new(install: impl Into<PathBuf>, saves: PathBuf, settings: PathBuf, shots: PathBuf) -> Self {
        Self {
            install: Folder::new(install),
            saves,
            settings,
            shots,
            fixed_clock: None,
        }
    }

    /// The same storage with a wall clock that always reads `secs`.
    pub fn with_fixed_clock(mut self, secs: u64) -> Self {
        self.fixed_clock = Some(secs);
        self
    }
}

impl Storage for Desktop {
    fn install(&self, name: &str) -> io::Result<Option<Box<dyn ReadSeek>>> {
        self.install.open(name)
    }

    fn user_map(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        let p = Path::new(path);
        if p.is_file() {
            std::fs::read(p).map(Some)
        } else {
            Ok(None)
        }
    }

    fn saves(&self) -> Vec<String> {
        let Ok(dir) = std::fs::read_dir(&self.saves) else {
            return Vec::new();
        };
        dir.flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .collect()
    }

    fn read_save(&self, file: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.saves.join(file))
    }

    fn write_save(&self, file: &str, bytes: &[u8]) -> Result<(), WriteError> {
        std::fs::create_dir_all(&self.saves).map_err(|_| WriteError::CantCreate)?;
        let path = self.saves.join(file);
        let mut f = std::fs::File::create(&path).map_err(|_| WriteError::CantCreate)?;
        let written = io::Write::write_all(&mut f, bytes).and_then(|()| f.sync_all());
        if written.is_err() {
            drop(f);
            let _ = std::fs::remove_file(&path);
            return Err(WriteError::WriteFailed);
        }
        Ok(())
    }

    fn delete_save(&self, file: &str) -> io::Result<()> {
        std::fs::remove_file(self.saves.join(file))
    }

    fn read_settings(&self) -> Option<Vec<u8>> {
        std::fs::read(&self.settings).ok()
    }

    fn write_settings(&self, bytes: &[u8]) -> io::Result<()> {
        if let Some(dir) = self.settings.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.settings, bytes)
    }

    fn has_screenshot(&self, name: &str) -> bool {
        self.shots.join(name).exists()
    }

    fn write_screenshot(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(&self.shots)?;
        std::fs::write(self.shots.join(name), bytes)
    }

    fn wall_clock_secs(&self) -> u64 {
        // Truncated to 32 bits, as the seed takes it.
        self.fixed_clock.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as u32 as u64)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    fn desktop(root: &Path) -> Desktop {
        Desktop::new(
            root.join("install"),
            root.join("Save"),
            root.join("user").join("settings.txt"),
            root.join("user"),
        )
    }

    #[test]
    fn saves_settings_and_screenshots_round_trip() {
        let t = tempfile::tempdir().unwrap();
        let s = desktop(t.path());
        assert!(s.saves().is_empty(), "no folder yet");
        assert_eq!(s.read_settings(), None);
        s.write_save("one.sav", b"abc").unwrap();
        assert_eq!(s.saves(), ["one.sav"]);
        assert_eq!(s.read_save("one.sav").unwrap(), b"abc");
        s.delete_save("one.sav").unwrap();
        assert!(s.saves().is_empty());
        s.write_settings(b"x=1").unwrap();
        assert_eq!(s.read_settings().as_deref(), Some(&b"x=1"[..]));
        assert!(!s.has_screenshot("screen000.pcx"));
        s.write_screenshot("screen000.pcx", b"p").unwrap();
        assert!(s.has_screenshot("screen000.pcx"));
        assert_eq!(
            std::fs::read(t.path().join("user").join("screen000.pcx")).unwrap(),
            b"p"
        );
    }

    #[test]
    fn a_save_folder_that_cant_be_made_is_cant_create() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("Save"), b"a file where the folder should be").unwrap();
        assert_eq!(desktop(t.path()).write_save("a.sav", b"x"), Err(WriteError::CantCreate));
    }

    #[test]
    fn the_installation_is_read_in_any_letter_case() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("install").join("Rez")).unwrap();
        std::fs::write(t.path().join("install").join("Rez").join("UnitData.dat"), b"u").unwrap();
        let s = desktop(t.path());
        let mut f = s.install("rez\\unitdata.dat").unwrap().expect("found");
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        assert_eq!(b, b"u");
        assert!(s.install("rez\\none.dat").unwrap().is_none());
    }

    #[test]
    fn a_fixed_clock_stands_still() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(desktop(t.path()).with_fixed_clock(42).wall_clock_secs(), 42);
        assert!(desktop(t.path()).wall_clock_secs() > 1_000_000_000);
    }
}
