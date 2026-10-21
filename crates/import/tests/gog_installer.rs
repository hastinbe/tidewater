//! Real-installer check: runs only when `TIDEWATER_GOG_INSTALLER` names a GOG `setup_*.exe` (with its `.bin` part next
//! to it) and `innoextract` is available; otherwise it skips, so CI without game data passes. When `TIDEWATER_INSTALL`
//! names an installed copy, every imported file is also compared with it byte for byte (SHA-256).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tidewater_import::{SourceKind, classify, import, inno, validate};

fn sha(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_real_installer_imports_and_matches_an_installed_copy() {
    let Some(exe) = std::env::var_os("TIDEWATER_GOG_INSTALLER").map(PathBuf::from) else {
        eprintln!("skipped: TIDEWATER_GOG_INSTALLER is not set");
        return;
    };
    if inno::find_tool().is_none() {
        eprintln!("skipped: innoextract not found");
        return;
    }
    let source = classify(&exe).expect("the installer is read");
    assert_eq!(source.kind, SourceKind::GogInstaller);
    let report = validate(&source);
    assert!(report.is_ok(), "{:?}", report.issues);

    let data = tempfile::tempdir().unwrap();
    let manifest = import(&source, &report, data.path(), &mut |_| {}).expect("the import works");
    assert_eq!(manifest.source_kind, "gog-installer");
    assert!(manifest.files.iter().any(|f| f.path == "War2Dat.mpq"));

    if let Some(installed) = std::env::var_os("TIDEWATER_INSTALL").map(PathBuf::from) {
        for f in &manifest.files {
            assert_eq!(
                f.sha256,
                sha(&installed.join(&f.path)),
                "{} differs from the installed copy",
                f.path
            );
        }
    }
}
