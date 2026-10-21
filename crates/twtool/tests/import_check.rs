//! `twtool import-check`: the dry run of the game-file import. The check against a real installation runs only when
//! `TIDEWATER_INSTALL` names one; it reads the files and copies nothing.

use std::process::Command;

fn run(path: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_twtool"))
        .arg("import-check")
        .arg(path)
        .output()
        .expect("twtool runs")
}

#[test]
fn the_real_installation_can_be_imported() {
    let Some(install) = std::env::var_os("TIDEWATER_INSTALL") else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let out = run(std::path::Path::new(&install));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("Result: can be imported"), "{text}");
    assert!(text.contains("War2Dat.mpq"), "{text}");
}

#[test]
fn a_missing_path_exits_with_its_own_code() {
    let out = run(std::path::Path::new("/nonexistent/tidewater-import-check"));
    assert_eq!(out.status.code(), Some(10));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Couldn't find"));
}
