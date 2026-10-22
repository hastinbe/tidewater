//! Self-update for the desktop program (`docs/release.md`). Release builds carry their version and the GitHub
//! repository they come from (`owner/name`) in the `TIDEWATER_VERSION` and `TIDEWATER_REPO` environment variables at
//! build time; a build without both is a development build, which reports "dev" and never checks.
//!
//! At start-up a background thread asks GitHub for the latest release of the public repository (no sign-in, one
//! request). If it is newer than this build, the main menu offers "Update now" and "Later". Every failure of the check,
//! including the 404 an unpublished repository gives, is logged at debug level (shown with `TIDEWATER_DEBUG=1`) and otherwise ignored.
//!
//! "Update now" per platform: Windows downloads the Inno Setup installer and starts it silently; macOS downloads the
//! disk image and swaps the running `.app` bundle; Linux replaces the running AppImage (`$APPIMAGE`). Where none of that
//! applies (a bundle in a read-only place, a plain Linux binary, a build run from a source tree) the release page opens
//! in the browser. A download's size is checked against the size the release lists; there is no other signing.
//!
//! The pure parts (versions, the release JSON, asset choice, when the check runs) are tested below; the network and
//! file steps are in [`net`] and only exist on the desktop.

use std::sync::{Arc, Mutex};

use serde::Deserialize;

/// The repository releases are published to (`owner/name`), set by the release build.
fn repo() -> Option<&'static str> {
    option_env!("TIDEWATER_REPO").filter(|r| !r.is_empty())
}

/// The only place a release's page and downloads may be on; anything else in the JSON is ignored.
fn page_prefix(repo: &str) -> String {
    format!("https://github.com/{repo}/")
}

/// This build's version: the tag's number for a release build, `None` for a development build.
pub fn build_version() -> Option<&'static str> {
    repo()?;
    option_env!("TIDEWATER_VERSION").filter(|v| !v.is_empty())
}

/// A version as three numbers. A pre-release suffix (`1.2.3-rc1`) is not accepted: such builds neither check nor are
/// offered.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let mut it = s.split('.');
    let mut next = || it.next()?.parse::<u64>().ok();
    let v = (next()?, next()?, next()?);
    it.next().is_none().then_some(v)
}

/// Whether `latest` is a newer version than `current`; either one not being a plain version gives `false`.
pub fn is_newer(current: &str, latest: &str) -> bool {
    matches!((parse_version(current), parse_version(latest)), (Some(c), Some(l)) if l > c)
}

/// Whether the start-up check runs: a release build, not turned off by the flag or the setting, and not a scripted or
/// headless run.
pub fn enabled(version: Option<&str>, no_flag: bool, setting: bool, scripted: bool) -> bool {
    version.is_some_and(|v| parse_version(v).is_some()) && !no_flag && setting && !scripted
}

/// The systems the program has release downloads for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    /// This build's system, if it has downloads (x64 Windows and Linux, any Mac).
    pub fn current() -> Option<Self> {
        if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::Windows)
        } else if cfg!(target_os = "macos") {
            Some(Self::MacOs)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::Linux)
        } else {
            None
        }
    }

    /// The end of the asset name this system installs from (`tidewater-<version>-<this>`). Linux updates itself only
    /// as an AppImage; the plain archive is for the user to unpack.
    fn suffix(self) -> &'static str {
        match self {
            Self::Windows => "-windows-x64-setup.exe",
            Self::MacOs => "-macos.dmg",
            Self::Linux => "-linux-x64.AppImage",
        }
    }
}

/// A release's download for this system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A release newer than this build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The version without the `v`.
    pub version: String,
    pub page: String,
    /// The download for this system, if the release has one.
    pub asset: Option<Asset>,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Reads `repo`'s `releases/latest` answer: the release, with the asset for `platform`, if its version is newer than
/// `current`. `None` for anything else, including JSON that isn't one.
pub fn parse_latest(json: &str, repo: &str, current: &str, platform: Option<Platform>) -> Option<Release> {
    let r: ApiRelease = serde_json::from_str(json).ok()?;
    let prefix = page_prefix(repo);
    if r.draft || r.prerelease || !is_newer(current, &r.tag_name) || !r.html_url.starts_with(&prefix) {
        return None;
    }
    let version = r.tag_name.trim_start_matches('v').to_string();
    let asset = platform.and_then(|p| {
        r.assets
            .into_iter()
            .find(|a| a.name.starts_with("tidewater-") && a.name.ends_with(p.suffix()))
            .filter(|a| a.browser_download_url.starts_with(&prefix) && a.size > 0)
            .map(|a| Asset {
                name: a.name,
                url: a.browser_download_url,
                size: a.size,
            })
    });
    Some(Release {
        version,
        page: r.html_url,
        asset,
    })
}

/// What the main menu shows about the update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    None,
    /// A newer version (its number) is out: "Update now" and "Later".
    Offer(String),
    /// "Update now" was chosen and the download or the swap is running.
    Busy,
}

enum State {
    Idle,
    Offer(Release),
    Busy,
    /// The update is installed (or its installer started): the program should end.
    Finished,
}

/// The update check and install, shared between the core and the thread that does the work.
#[derive(Clone)]
pub struct Updater(Arc<Mutex<State>>);

impl Default for Updater {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State::Idle)))
    }
}

impl Updater {
    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Starts the check in the background if [`enabled`] says so (and the system has the desktop's network code).
    pub fn start_check(&self, no_flag: bool, setting: bool, scripted: bool) {
        if !enabled(build_version(), no_flag, setting, scripted) {
            return;
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let this = self.clone();
            let current = build_version().unwrap_or_default().to_string();
            let spawned =
                std::thread::Builder::new()
                    .name("update-check".into())
                    .spawn(move || match net::latest(&current) {
                        Ok(Some(r)) => this.with(|s| *s = State::Offer(r)),
                        Ok(None) => debug("up to date"),
                        Err(e) => debug(&format!("check failed: {e:#}")),
                    });
            if let Err(e) = spawned {
                debug(&format!("no thread for the check: {e}"));
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let _ = self;
    }

    pub fn notice(&self) -> Notice {
        self.with(|s| match s {
            State::Offer(r) => Notice::Offer(r.version.clone()),
            State::Busy => Notice::Busy,
            State::Idle | State::Finished => Notice::None,
        })
    }

    /// "Later": hides the offer until the next start.
    pub fn dismiss(&self) {
        self.with(|s| {
            if matches!(s, State::Offer(_)) {
                *s = State::Idle;
            }
        });
    }

    /// The update is in place and the program should end now.
    pub fn finished(&self) -> bool {
        self.with(|s| matches!(s, State::Finished))
    }

    /// "Update now": installs in the background, or opens the release page where the system can't install by itself
    /// (or when the install fails).
    pub fn install(&self) {
        let Some(release) = self.with(|s| match std::mem::replace(s, State::Busy) {
            State::Offer(r) => Some(r),
            other => {
                *s = other;
                None
            }
        }) else {
            return;
        };
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            let this = self.clone();
            let page = release.page.clone();
            let spawned = std::thread::Builder::new()
                .name("update-install".into())
                .spawn(move || match net::install(&release) {
                    Ok(true) => this.with(|s| *s = State::Finished),
                    Ok(false) => {
                        net::open_page(&release.page);
                        this.with(|s| *s = State::Idle);
                    }
                    Err(e) => {
                        eprintln!("tidewater: the update failed ({e:#}); opening the release page");
                        net::open_page(&release.page);
                        this.with(|s| *s = State::Idle);
                    }
                });
            if spawned.is_err() {
                net::open_page(&page);
                self.with(|s| *s = State::Idle);
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        self.with(|s| *s = State::Idle);
    }
}

/// Debug-level logging: shown only with `TIDEWATER_DEBUG` set, as the check must stay quiet.
#[allow(dead_code, reason = "unused on the phones, which never check")]
fn debug(msg: &str) {
    if std::env::var_os("TIDEWATER_DEBUG").is_some() {
        eprintln!("tidewater: update: {msg}");
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod net {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use anyhow::{Context, Result, bail};

    use super::{Platform, Release, debug, page_prefix, parse_latest, repo};

    const USER_AGENT: &str = concat!("tidewater/", env!("CARGO_PKG_VERSION"));

    fn agent(timeout: Duration) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .user_agent(USER_AGENT)
            .build()
            .into()
    }

    /// Asks for the latest release; `Some` if it is newer than `current`.
    pub(super) fn latest(current: &str) -> Result<Option<Release>> {
        let Some(repo) = repo() else { return Ok(None) };
        let url = format!("https://api.github.com/repos/{repo}/releases/latest");
        let mut resp = agent(Duration::from_secs(10))
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .call()?;
        let body = resp.body_mut().with_config().limit(1 << 20).read_to_string()?;
        Ok(parse_latest(&body, repo, current, Platform::current()))
    }

    /// Installs the release. `Ok(true)`: done, the program should end (the installer runs or the new copy starts).
    /// `Ok(false)`: nothing to install from here, show the release page.
    pub(super) fn install(r: &Release) -> Result<bool> {
        let (Some(asset), Some(platform)) = (&r.asset, Platform::current()) else {
            return Ok(false);
        };
        match platform {
            Platform::Windows => {
                let dir = fresh_temp_dir()?;
                let setup = dir.join(&asset.name);
                download(&asset.url, asset.size, &setup)?;
                std::process::Command::new(&setup)
                    .arg("/SILENT")
                    .spawn()
                    .context("starting the installer")?;
                Ok(true)
            }
            Platform::MacOs => install_mac(&asset.url, asset.size, &asset.name),
            Platform::Linux => install_appimage(&asset.url, asset.size),
        }
    }

    fn fresh_temp_dir() -> Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("tidewater-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Downloads `url` to `dest`, failing unless it is exactly `size` bytes.
    fn download(url: &str, size: u64, dest: &Path) -> Result<()> {
        if !repo().is_some_and(|r| url.starts_with(&page_prefix(r))) {
            bail!("unexpected download address");
        }
        let mut resp = agent(Duration::from_secs(30 * 60)).get(url).call()?;
        let mut reader = resp.body_mut().with_config().limit(size + 1).reader();
        let mut file = std::fs::File::create(dest)?;
        let got = std::io::copy(&mut reader.by_ref().take(size + 1), &mut file)?;
        file.flush()?;
        if got != size {
            bail!("the download is {got} bytes, the release says {size}");
        }
        debug(&format!("downloaded {got} bytes"));
        Ok(())
    }

    fn run(cmd: &mut std::process::Command) -> Result<()> {
        let status = cmd
            .status()
            .with_context(|| format!("running {:?}", cmd.get_program()))?;
        if !status.success() {
            bail!("{:?} failed: {status}", cmd.get_program());
        }
        Ok(())
    }

    /// The `.app` bundle the running program is in, if it runs from one.
    fn running_bundle() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
        exe.ancestors()
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .map(Path::to_path_buf)
    }

    /// Downloads the disk image, copies its `.app` beside the running one, swaps them and starts the new copy. A
    /// bundle that can't be replaced (not running from one, a folder we can't write to) is `Ok(false)`.
    fn install_mac(url: &str, size: u64, name: &str) -> Result<bool> {
        let Some(bundle) = running_bundle() else {
            return Ok(false);
        };
        let parent = bundle.parent().context("the bundle has no folder")?;
        let new = parent.join(".Tidewater-update.app");
        let old = parent.join(".Tidewater-old.app");
        let _ = std::fs::remove_dir_all(&new);
        let _ = std::fs::remove_dir_all(&old);
        if std::fs::create_dir(&new).is_err() {
            return Ok(false);
        }
        std::fs::remove_dir(&new)?;

        let dir = fresh_temp_dir()?;
        let dmg = dir.join(name);
        download(url, size, &dmg)?;
        let mount = dir.join("mount");
        std::fs::create_dir(&mount)?;
        run(std::process::Command::new("hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-noverify", "-mountpoint"])
            .arg(&mount)
            .arg(&dmg))?;
        let copied = run(std::process::Command::new("ditto")
            .arg(mount.join("Tidewater.app"))
            .arg(&new));
        // Detach whatever happened above; a failure to detach doesn't matter to the update.
        let _ = std::process::Command::new("hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&mount)
            .status();
        copied?;
        // The old copy steps aside (a directory can't be renamed over a full one), the new one takes its place.
        std::fs::rename(&bundle, &old)?;
        if let Err(e) = std::fs::rename(&new, &bundle) {
            let _ = std::fs::rename(&old, &bundle);
            return Err(e.into());
        }
        let _ = std::fs::remove_dir_all(&old);
        let _ = std::fs::remove_dir_all(&dir);
        // Start the new copy a moment after this one has ended, so the settings written at exit are the ones it reads.
        std::process::Command::new("sh")
            .args(["-c", "sleep 1; exec open -n \"$0\""])
            .arg(&bundle)
            .spawn()?;
        Ok(true)
    }

    /// Replaces the running AppImage and starts the new one; `Ok(false)` when not running from one.
    fn install_appimage(url: &str, size: u64) -> Result<bool> {
        let Some(path) = std::env::var_os("APPIMAGE").map(PathBuf::from) else {
            return Ok(false);
        };
        let new = path.with_extension("update");
        download(url, size, &new)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&new, &path)?;
        std::process::Command::new("sh")
            .args(["-c", "sleep 1; exec \"$0\""])
            .arg(&path)
            .spawn()?;
        Ok(true)
    }

    /// Opens a web page in the browser (only ever the release page, whose address was checked).
    pub(super) fn open_page(url: &str) {
        if !repo().is_some_and(|r| url.starts_with(&page_prefix(r))) {
            return;
        }
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = std::process::Command::new("cmd");
            c.args(["/C", "start", ""]);
            c
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open")
        } else {
            std::process::Command::new("xdg-open")
        };
        if let Err(e) = cmd.arg(url).spawn() {
            debug(&format!("could not open the release page: {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "owner/tidewater";

    const SAMPLE: &str = r#"{
      "tag_name": "v1.4.0",
      "html_url": "https://github.com/owner/tidewater/releases/tag/v1.4.0",
      "draft": false,
      "prerelease": false,
      "assets": [
        {"name": "tidewater-1.4.0-linux-x64.tar.gz", "size": 11,
         "browser_download_url": "https://github.com/owner/tidewater/releases/download/v1.4.0/tidewater-1.4.0-linux-x64.tar.gz"},
        {"name": "tidewater-1.4.0-macos.dmg", "size": 22,
         "browser_download_url": "https://github.com/owner/tidewater/releases/download/v1.4.0/tidewater-1.4.0-macos.dmg"},
        {"name": "tidewater-1.4.0-windows-x64-setup.exe", "size": 33,
         "browser_download_url": "https://github.com/owner/tidewater/releases/download/v1.4.0/tidewater-1.4.0-windows-x64-setup.exe"}
      ]
    }"#;

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(is_newer("1.2.3", "v1.2.4"));
        assert!(is_newer("1.9.0", "1.10.0"));
        assert!(is_newer("0.9.9", "1.0.0"));
        assert!(!is_newer("1.2.3", "1.2.3"));
        assert!(!is_newer("1.3.0", "1.2.9"));
        // Not plain versions: never newer, never checked against.
        assert!(!is_newer("dev", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.1.0-rc1"));
        assert!(!is_newer("1.0.0", "1.1"));
        assert!(!is_newer("1.0.0", ""));
    }

    #[test]
    fn the_right_asset_is_chosen_per_system() {
        let w = parse_latest(SAMPLE, REPO, "1.3.0", Some(Platform::Windows)).unwrap();
        assert_eq!(w.version, "1.4.0");
        let a = w.asset.unwrap();
        assert_eq!((a.name.as_str(), a.size), ("tidewater-1.4.0-windows-x64-setup.exe", 33));
        let m = parse_latest(SAMPLE, REPO, "1.3.0", Some(Platform::MacOs)).unwrap();
        assert_eq!(m.asset.unwrap().size, 22);
        // Linux self-updates only as an AppImage, which this release doesn't have: the page is all there is.
        let l = parse_latest(SAMPLE, REPO, "1.3.0", Some(Platform::Linux)).unwrap();
        assert_eq!(l.asset, None);
        assert_eq!(l.page, "https://github.com/owner/tidewater/releases/tag/v1.4.0");
        let none = parse_latest(SAMPLE, REPO, "1.3.0", None).unwrap();
        assert_eq!(none.asset, None);
    }

    #[test]
    fn only_a_newer_published_release_counts() {
        assert_eq!(parse_latest(SAMPLE, REPO, "1.4.0", Some(Platform::MacOs)), None);
        assert_eq!(parse_latest(SAMPLE, REPO, "2.0.0", Some(Platform::MacOs)), None);
        let pre = SAMPLE.replace("\"prerelease\": false", "\"prerelease\": true");
        assert_eq!(parse_latest(&pre, REPO, "1.0.0", Some(Platform::MacOs)), None);
        let elsewhere = SAMPLE.replace("github.com/owner", "example.com/other");
        let r = parse_latest(&elsewhere, REPO, "1.0.0", Some(Platform::MacOs));
        assert_eq!(r, None);
        assert_eq!(parse_latest("not json", REPO, "1.0.0", None), None);
        assert_eq!(parse_latest(r#"{"message":"Not Found"}"#, REPO, "1.0.0", None), None);
    }

    #[test]
    fn a_download_from_another_place_is_not_offered() {
        let odd = SAMPLE.replace(
            "https://github.com/owner/tidewater/releases/download/v1.4.0/tidewater-1.4.0-macos.dmg",
            "https://example.com/tidewater-1.4.0-macos.dmg",
        );
        let r = parse_latest(&odd, REPO, "1.0.0", Some(Platform::MacOs)).unwrap();
        assert_eq!(r.asset, None);
    }

    #[test]
    fn the_check_runs_only_where_it_should() {
        assert!(enabled(Some("1.0.0"), false, true, false));
        // A development build, the flag, the setting and a scripted or headless run each turn it off.
        assert!(!enabled(None, false, true, false));
        assert!(!enabled(Some("dev"), false, true, false));
        assert!(!enabled(Some("1.0.0"), true, true, false));
        assert!(!enabled(Some("1.0.0"), false, false, false));
        assert!(!enabled(Some("1.0.0"), false, true, true));
    }

    #[test]
    fn a_test_build_never_checks() {
        // The tests are built without TIDEWATER_VERSION or TIDEWATER_REPO.
        assert_eq!(build_version(), None);
        let u = Updater::default();
        u.start_check(false, true, false);
        assert_eq!(u.notice(), Notice::None);
    }

    #[test]
    fn later_hides_the_offer() {
        let u = Updater::default();
        u.with(|s| {
            *s = State::Offer(Release {
                version: "2.0.0".into(),
                page: String::new(),
                asset: None,
            })
        });
        assert_eq!(u.notice(), Notice::Offer("2.0.0".into()));
        u.dismiss();
        assert_eq!(u.notice(), Notice::None);
    }
}
