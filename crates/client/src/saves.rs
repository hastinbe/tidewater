//! The save folder, the saved-game list and the rules of the save and load dialogs (spec
//! `docs/specs/game/save-load.md`). Bevy-free; `gamemenu.rs` draws the dialogs and `savegame.rs` collects and applies
//! the game's state.
//!
//! Choices where the spec leaves a gap:
//! - **Where.** The spec's `Save` folder "inside the game's folder" would put files into the install, which Tidewater
//!   never writes to. The folder is `Save` inside our own per-user data directory ([`default_dir`], from the
//!   `directories` crate), overridable with `--save-dir`. The spec's "open question: Folder in a network session" has the
//!   same answer.
//! - **Typed characters.** "Printable, from space up to code 127": we take codes 0x20 to 0x7e (127 is not printable).
//!   The forbidden characters are network entry 105's; [`FORBIDDEN`] is the fallback when the table is missing.
//! - **Overwrite check.** The typed name is compared with the list after the device-name change ([`file_stem`]), so
//!   `con` and `con_` count as one name. For every other name this is the spec's exact comparison.
//! - **Overwrite, other answer / Load button / more than 100 files / full slots / session timing / saver's own copy /
//!   classic clock / selection:** the defaults of the spec's open questions (return to the save dialog; Load works on the
//!   chosen entry, and the newest is chosen when the load dialog opens; the first 100 found are used; the oldest is
//!   replaced; saving when the message is handled; every player including the sender saves once; the clock starts at
//!   0; selection kept in the world as saved, see `tidewater_sim` `world/save.rs`).
//! - **Multiplayer slots.** A multiplayer save request overwrites the file whose stored name matches, else takes a new
//!   file named after the request, else replaces the oldest ([`multiplayer_target`]).
//! - **Order of equal times.** Files saved in the same second keep the folder listing's order ([`sort_newest_first`] is
//!   stable).
//!
//! Not yet here: the multiplayer parts are logic and tests only ([`may_save`], [`BnetWait`], [`multiplayer_target`]); the
//! client has no multiplayer.

use std::path::{Path, PathBuf};

use crate::savefile::{self, Header, Kind, Reason};

/// Saved games of one kind that take part.
pub const MAX_SAVES: usize = 100;
/// The longest name that can be typed, in characters.
pub const MAX_NAME: usize = 27;
#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
/// Real seconds nobody may open the save dialog after a Battle.net save.
pub const BNET_WAIT_SECS: f64 = 120.0;
/// Network entry 105's characters, used when the table can't be read.
pub const FORBIDDEN: &str = "*\\/:><|&+^?\"";

/// The default save folder: `Save` in the user's data directory for Tidewater.
pub fn default_dir() -> PathBuf {
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
            let d = default_dir();
            d.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Names

/// Whether `c` may be typed after `name` (spec "Typing a name"): a printable character, not a forbidden one, not a
/// space first, and the name stays within [`MAX_NAME`] characters.
pub fn accepts(name: &str, c: char, forbidden: &str) -> bool {
    ('\u{20}'..='\u{7e}').contains(&c)
        && !forbidden.contains(c)
        && !(c == ' ' && name.is_empty())
        && name.chars().count() < MAX_NAME
}

/// The name as saved: spaces at the end removed.
pub fn trimmed(name: &str) -> &str {
    name.trim_end_matches(' ')
}

const DEVICES: [&str; 22] = [
    "nul", "con", "prn", "aux", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2",
    "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The file name without the extension: the trimmed name, except that a reserved device name (any letter case) becomes
/// the device name in lower case followed by `_`.
pub fn file_stem(name: &str) -> String {
    let n = trimmed(name);
    let lower = n.to_ascii_lowercase();
    if DEVICES.contains(&lower.as_str()) {
        format!("{lower}_")
    } else {
        n.to_string()
    }
}

/// A table text with its `%s` filled in (and the table's `\'` written as `'`).
pub fn fill(template: &str, name: &str) -> String {
    template.replace("%s", name).replace("\\'", "'")
}

// ---------------------------------------------------------------------------------------------------------------
// The folder

/// One listed saved game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The file name without folder and extension: what the list shows.
    pub stem: String,
    pub path: PathBuf,
    /// The name typed when it was saved.
    pub stored_name: String,
    pub saved_at: u64,
    pub race: u8,
}

/// Newest first; equal times keep their order.
pub fn sort_newest_first(entries: &mut [Entry]) {
    entries.sort_by_key(|e| std::cmp::Reverse(e.saved_at));
}

/// The save folder.
#[derive(Clone, Debug)]
pub struct SaveDir {
    pub root: PathBuf,
}

impl SaveDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn path_of(&self, stem: &str, kind: Kind) -> PathBuf {
        self.root.join(format!("{stem}.{}", kind.extension()))
    }

    /// The saved games of `kind` that can be loaded under the rules `enhanced`, newest first, at most [`MAX_SAVES`].
    /// A file that isn't a valid saved game of this mode is not listed.
    pub fn list(&self, kind: Kind, enhanced: bool) -> Vec<Entry> {
        let Ok(dir) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for item in dir.flatten() {
            let path = item.path();
            let ext_ok = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(kind.extension()));
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
                continue;
            };
            if !ext_ok || !path.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let Ok((h, _)) = savefile::read_header(&bytes, enhanced) else {
                continue;
            };
            if h.kind != kind {
                continue;
            }
            found.push(Entry {
                stem,
                path,
                stored_name: h.name,
                saved_at: h.saved_at,
                race: h.race,
            });
            if found.len() >= MAX_SAVES {
                break;
            }
        }
        sort_newest_first(&mut found);
        found
    }

    /// Writes a saved game; a partly written file is deleted.
    pub fn write(&self, stem: &str, kind: Kind, bytes: &[u8]) -> Result<(), Reason> {
        std::fs::create_dir_all(&self.root).map_err(|_| Reason::CantCreate)?;
        let path = self.path_of(stem, kind);
        let mut file = std::fs::File::create(&path).map_err(|_| Reason::CantCreate)?;
        let written = std::io::Write::write_all(&mut file, bytes).and_then(|()| file.sync_all());
        if written.is_err() {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err(Reason::WriteFailed);
        }
        Ok(())
    }

    pub fn delete(&self, e: &Entry) -> std::io::Result<()> {
        std::fs::remove_file(&e.path)
    }

    /// Reads and checks a saved game for loading.
    pub fn read(path: &Path, enhanced: bool) -> Result<(Header, savefile::Body), Reason> {
        let bytes = std::fs::read(path).map_err(|_| Reason::CantOpen)?;
        savefile::decode(&bytes, enhanced)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The dialogs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Save,
    Load,
}

/// A question the dialog asks, with the name filled into network entry 24 or 25.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    Overwrite(String),
    Delete(String),
}

impl Confirm {
    /// The `network.tbl` entry of the question.
    pub fn entry(&self) -> u16 {
        match self {
            Confirm::Overwrite(_) => 24,
            Confirm::Delete(_) => 25,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Confirm::Overwrite(n) | Confirm::Delete(n) => n,
        }
    }
}

/// A message with an OK button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Network entry 26.
    TooMany,
    Failed {
        mode: Mode,
        reason: Reason,
        file: String,
    },
}

impl Notice {
    /// The text: the `network.tbl` entries used, with the file name for a failure (`net` gives an entry's text).
    pub fn lines(&self, net: &dyn Fn(u16) -> String) -> Vec<String> {
        match self {
            Notice::TooMany => vec![net(26)],
            Notice::Failed { mode, reason, file } => {
                let head = if *mode == Mode::Save { 22 } else { 23 };
                vec![format!("{} {file}", net(head)), net(reason.entry())]
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    List,
    Confirm(Confirm),
    Notice(Notice),
}

/// What pressing a button decided for the caller to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Write the game to `stem` (the file name without the extension) under the typed `name`.
    Write {
        stem: String,
        name: String,
    },
    /// Delete this entry's file, then read the list again.
    Delete(Entry),
    /// Load this file.
    Load(Entry),
    Close,
}

/// The state of an open save or load dialog.
#[derive(Clone, Debug)]
pub struct Dialog {
    pub mode: Mode,
    pub entries: Vec<Entry>,
    pub name: String,
    pub selected: Option<usize>,
    pub stage: Stage,
    pub forbidden: String,
}

impl Dialog {
    pub fn open(mode: Mode, entries: Vec<Entry>, forbidden: &str) -> Self {
        let mut d = Dialog {
            mode,
            entries,
            name: String::new(),
            selected: None,
            stage: Stage::List,
            forbidden: forbidden.to_string(),
        };
        if mode == Mode::Load && !d.entries.is_empty() {
            d.selected = Some(0);
        }
        d
    }

    /// Reads the list again (after a delete or a failed save), keeping the typed name.
    pub fn relist(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        self.selected = None;
    }

    pub fn save_enabled(&self) -> bool {
        !self.name.is_empty()
    }

    pub fn delete_enabled(&self) -> bool {
        self.selected.is_some()
    }

    pub fn load_enabled(&self) -> bool {
        self.selected.is_some()
    }

    /// A typed character; ones that break the rules are ignored. Typing clears the list choice.
    pub fn type_char(&mut self, c: char) {
        if self.mode == Mode::Save && self.stage == Stage::List && accepts(&self.name, c, &self.forbidden) {
            self.name.push(c);
            self.selected = None;
        }
    }

    pub fn backspace(&mut self) {
        if self.mode == Mode::Save && self.stage == Stage::List && self.name.pop().is_some() {
            self.selected = None;
        }
    }

    /// Chooses a list entry; in the save dialog its name goes into the field.
    pub fn choose(&mut self, i: usize) {
        if self.stage != Stage::List || i >= self.entries.len() {
            return;
        }
        self.selected = Some(i);
        if self.mode == Mode::Save {
            self.name = self.entries[i].stem.clone();
        }
    }

    /// The Save button.
    pub fn press_save(&mut self) -> Step {
        if self.mode != Mode::Save || self.stage != Stage::List || !self.save_enabled() {
            return Step::Nothing;
        }
        let name = trimmed(&self.name).to_string();
        let stem = file_stem(&name);
        if name.is_empty() {
            return Step::Nothing;
        }
        if self.entries.iter().any(|e| e.stem == stem) {
            self.stage = Stage::Confirm(Confirm::Overwrite(stem));
            return Step::Nothing;
        }
        if self.entries.len() >= MAX_SAVES {
            self.stage = Stage::Notice(Notice::TooMany);
            return Step::Nothing;
        }
        Step::Write { stem, name }
    }

    /// The Delete button.
    pub fn press_delete(&mut self) -> Step {
        if self.mode != Mode::Save || self.stage != Stage::List {
            return Step::Nothing;
        }
        if let Some(e) = self.selected.and_then(|i| self.entries.get(i)) {
            self.stage = Stage::Confirm(Confirm::Delete(e.stem.clone()));
        }
        Step::Nothing
    }

    /// The Load button.
    pub fn press_load(&mut self) -> Step {
        if self.mode != Mode::Load || self.stage != Stage::List {
            return Step::Nothing;
        }
        self.selected
            .and_then(|i| self.entries.get(i))
            .map_or(Step::Nothing, |e| Step::Load(e.clone()))
    }

    /// OK on a question.
    pub fn confirm_ok(&mut self) -> Step {
        let Stage::Confirm(c) = std::mem::replace(&mut self.stage, Stage::List) else {
            return Step::Nothing;
        };
        match c {
            Confirm::Overwrite(stem) => Step::Write {
                stem,
                name: trimmed(&self.name).to_string(),
            },
            Confirm::Delete(stem) => self
                .entries
                .iter()
                .find(|e| e.stem == stem)
                .map_or(Step::Nothing, |e| Step::Delete(e.clone())),
        }
    }

    /// Cancel on a question: back to the dialog (as Delete's either answer does).
    pub fn confirm_cancel(&mut self) {
        if matches!(self.stage, Stage::Confirm(_)) {
            self.stage = Stage::List;
        }
    }

    /// OK on a message: back to the dialog.
    pub fn notice_ok(&mut self) {
        if matches!(self.stage, Stage::Notice(_)) {
            self.stage = Stage::List;
        }
    }

    /// The Cancel button (or Escape): closes the dialog when no question or message is up.
    pub fn press_cancel(&mut self) -> Step {
        match self.stage {
            Stage::List => Step::Close,
            Stage::Confirm(_) => {
                self.confirm_cancel();
                Step::Nothing
            }
            Stage::Notice(_) => {
                self.notice_ok();
                Step::Nothing
            }
        }
    }

    /// A save or load failed: show why.
    pub fn failed(&mut self, reason: Reason, file: &str) {
        self.stage = Stage::Notice(Notice::Failed {
            mode: self.mode,
            reason,
            file: file.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Multiplayer

#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
/// Who may save, load or restart in a multiplayer game: only the lowest-numbered player still in the game. `Err` holds
/// that player (network entry 28 names them).
pub fn may_save(multiplayer: bool, local: u8, present: &[u8]) -> Result<(), u8> {
    if !multiplayer {
        return Ok(());
    }
    match present.iter().copied().min() {
        Some(lowest) if lowest != local => Err(lowest),
        _ => Ok(()),
    }
}

#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
/// The Battle.net wait between saves (network entry 29 when it isn't over). The caller gives the real time in seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BnetWait {
    sent_at: Option<f64>,
}

#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
impl BnetWait {
    /// A save was sent at `now`.
    pub fn sent(&mut self, now: f64) {
        self.sent_at = Some(now);
    }

    /// Whether the save dialog may open at `now`. There is no wait at the start of a game.
    pub fn allows(&self, now: f64) -> bool {
        self.sent_at.is_none_or(|t| now - t >= BNET_WAIT_SECS)
    }
}

#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
/// Where a multiplayer save request goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// The file whose stored name equals the request.
    Overwrite(String),
    /// A free slot: a new file named after the request.
    New(String),
    /// All 100 are taken: the oldest is replaced.
    ReplaceOldest(String),
}

#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
impl Target {
    /// The file name, without the extension.
    pub fn stem(&self) -> &str {
        match self {
            Target::Overwrite(s) | Target::New(s) | Target::ReplaceOldest(s) => s,
        }
    }
}

#[allow(dead_code)] // multiplayer: logic and tests only, no session yet
/// Chooses the file for a multiplayer save request named `requested`, from the list (newest first).
pub fn multiplayer_target(entries: &[Entry], requested: &str) -> Target {
    if let Some(e) = entries.iter().find(|e| e.stored_name == requested) {
        return Target::Overwrite(e.stem.clone());
    }
    if entries.len() < MAX_SAVES {
        return Target::New(file_stem(requested));
    }
    Target::ReplaceOldest(entries.last().map_or_else(|| file_stem(requested), |e| e.stem.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::{CampaignTotal, GameKind, Memory};
    use crate::savefile::{Body, ClientState};
    use tidewater_sim::setup::Setup;

    fn bytes(name: &str, at: u64, enhanced: bool, kind: Kind) -> Vec<u8> {
        savefile::encode(
            &Header {
                name: name.into(),
                saved_at: at,
                enhanced,
                kind,
                map: "m.pud".into(),
                setup: Setup::default(),
                seed: 1,
                race: 1,
            },
            &Body {
                world: vec![0],
                client: ClientState {
                    cycles_on: 0,
                    music: None,
                    screen: (0, 0),
                    game: GameKind::Custom,
                    total: CampaignTotal::default(),
                    memory: Memory::default(),
                    start_controllers: [0; 8],
                    clock_ms: None,
                    pause_counts: None,
                },
            },
        )
    }

    fn entry(stem: &str, at: u64) -> Entry {
        Entry {
            stem: stem.into(),
            path: PathBuf::from(format!("{stem}.sav")),
            stored_name: stem.into(),
            saved_at: at,
            race: 0,
        }
    }

    fn typed(d: &mut Dialog, s: &str) {
        s.chars().for_each(|c| d.type_char(c));
    }

    fn save_dialog(entries: Vec<Entry>) -> Dialog {
        Dialog::open(Mode::Save, entries, FORBIDDEN)
    }

    #[test]
    fn typing_rules() {
        let mut d = save_dialog(vec![]);
        typed(&mut d, "a?b");
        assert_eq!(d.name, "ab", "the ? is ignored");
        let mut d = save_dialog(vec![]);
        d.type_char(' ');
        assert_eq!(d.name, "", "a space can't be first");
        typed(&mut d, "a b");
        assert_eq!(d.name, "a b");
        for c in "*\\/:><|&+^?\"\u{7}é".chars() {
            d.type_char(c);
        }
        assert_eq!(d.name, "a b");
        let mut d = save_dialog(vec![]);
        typed(&mut d, &"x".repeat(40));
        assert_eq!(d.name.chars().count(), MAX_NAME);
    }

    #[test]
    fn device_names_get_an_underscore() {
        assert_eq!(file_stem("CON"), "con_");
        assert_eq!(file_stem("con"), "con_");
        assert_eq!(file_stem("Lpt9"), "lpt9_");
        assert_eq!(file_stem("com1 "), "com1_");
        assert_eq!(file_stem("nul"), "nul_");
        assert_eq!(file_stem("com10"), "com10");
        assert_eq!(file_stem("lpt0"), "lpt0");
        assert_eq!(file_stem("Siege"), "Siege");
        assert_eq!(file_stem("Base   "), "Base", "trailing spaces are removed");
        for d in DEVICES {
            assert_eq!(file_stem(&d.to_uppercase()), format!("{d}_"));
        }
    }

    #[test]
    fn save_as_a_new_name_writes() {
        let mut d = save_dialog(vec![entry("Old", 5)]);
        typed(&mut d, "Siege");
        assert_eq!(
            d.press_save(),
            Step::Write {
                stem: "Siege".into(),
                name: "Siege".into()
            }
        );
        let mut d = save_dialog(vec![]);
        typed(&mut d, "Base   ".trim_start());
        d.name.push_str("  ");
        assert_eq!(
            d.press_save(),
            Step::Write {
                stem: "Base".into(),
                name: "Base".into()
            }
        );
    }

    #[test]
    fn save_is_enabled_only_with_a_name() {
        let mut d = save_dialog(vec![]);
        assert!(!d.save_enabled());
        assert_eq!(d.press_save(), Step::Nothing);
        typed(&mut d, "a");
        assert!(d.save_enabled());
    }

    #[test]
    fn saving_over_a_name_asks_and_case_matters() {
        let mut d = save_dialog(vec![entry("Siege", 5)]);
        typed(&mut d, "Siege");
        assert_eq!(d.press_save(), Step::Nothing);
        assert_eq!(d.stage, Stage::Confirm(Confirm::Overwrite("Siege".into())));
        assert_eq!(Confirm::Overwrite("Siege".into()).entry(), 24);
        assert_eq!(
            d.confirm_ok(),
            Step::Write {
                stem: "Siege".into(),
                name: "Siege".into()
            }
        );
        // The other answer goes back to the dialog.
        let mut d = save_dialog(vec![entry("Siege", 5)]);
        typed(&mut d, "Siege");
        d.press_save();
        d.confirm_cancel();
        assert_eq!(d.stage, Stage::List);
        // siege is not Siege.
        let mut d = save_dialog(vec![entry("Siege", 5)]);
        typed(&mut d, "siege");
        assert!(matches!(d.press_save(), Step::Write { .. }));
    }

    #[test]
    fn a_hundred_games_refuse_a_new_name_but_not_an_old_one() {
        let many: Vec<Entry> = (0..MAX_SAVES).map(|i| entry(&format!("g{i}"), i as u64)).collect();
        let mut d = save_dialog(many.clone());
        typed(&mut d, "fresh");
        assert_eq!(d.press_save(), Step::Nothing);
        assert_eq!(d.stage, Stage::Notice(Notice::TooMany));
        d.notice_ok();
        assert_eq!(d.stage, Stage::List);
        let mut d = save_dialog(many);
        typed(&mut d, "g7");
        d.press_save();
        assert_eq!(d.stage, Stage::Confirm(Confirm::Overwrite("g7".into())));
    }

    #[test]
    fn choosing_copies_the_name_and_typing_clears_the_choice() {
        let mut d = save_dialog(vec![entry("Siege", 9), entry("Raid", 3)]);
        assert!(!d.delete_enabled());
        d.choose(1);
        assert_eq!(d.name, "Raid");
        assert!(d.delete_enabled());
        d.type_char('x');
        assert_eq!(d.selected, None);
        assert!(!d.delete_enabled());
        d.choose(0);
        d.backspace();
        assert_eq!(d.selected, None);
    }

    #[test]
    fn delete_asks_then_returns_to_the_dialog() {
        let mut d = save_dialog(vec![entry("Siege", 9)]);
        d.choose(0);
        assert_eq!(d.press_delete(), Step::Nothing);
        assert_eq!(d.stage, Stage::Confirm(Confirm::Delete("Siege".into())));
        assert_eq!(Confirm::Delete("Siege".into()).entry(), 25);
        assert_eq!(d.confirm_ok(), Step::Delete(entry("Siege", 9)));
        assert_eq!(d.stage, Stage::List);
        d.relist(vec![]);
        assert!(d.entries.is_empty());
    }

    #[test]
    fn load_dialog_chooses_the_newest_and_loads_the_choice() {
        let mut d = Dialog::open(Mode::Load, vec![entry("B", 9), entry("A", 3)], FORBIDDEN);
        assert!(d.load_enabled());
        assert_eq!(d.press_load(), Step::Load(entry("B", 9)));
        d.choose(1);
        assert_eq!(d.press_load(), Step::Load(entry("A", 3)));
        assert_eq!(d.press_save(), Step::Nothing);
        d.type_char('a');
        assert_eq!(d.name, "", "no field in the load dialog");
        let d = Dialog::open(Mode::Load, vec![], FORBIDDEN);
        assert!(!d.load_enabled());
        let mut d = d;
        assert_eq!(d.press_cancel(), Step::Close);
    }

    #[test]
    fn failures_show_the_entry_reason_and_file() {
        let net = |n: u16| format!("<{n}>");
        let mut d = save_dialog(vec![]);
        d.failed(Reason::WriteFailed, "Siege.sav");
        let Stage::Notice(n) = &d.stage else { panic!() };
        assert_eq!(n.lines(&net), ["<22> Siege.sav", "<14>"]);
        d.notice_ok();
        assert_eq!(d.stage, Stage::List, "OK returns to the save dialog");
        let mut l = Dialog::open(Mode::Load, vec![], FORBIDDEN);
        l.failed(Reason::NotSaved, "x.sav");
        let Stage::Notice(n) = &l.stage else { panic!() };
        assert_eq!(n.lines(&net), ["<23> x.sav", "<12>"]);
        assert_eq!(Notice::TooMany.lines(&net), ["<26>"]);
        assert_eq!(fill("Replace the file \\'%s\\'?", "Siege"), "Replace the file 'Siege'?");
    }

    #[test]
    fn newest_first_keeps_the_listing_order_for_equal_times() {
        let mut v = vec![entry("a", 5), entry("b", 9), entry("c", 5), entry("d", 9)];
        sort_newest_first(&mut v);
        let order: Vec<_> = v.iter().map(|e| e.stem.as_str()).collect();
        assert_eq!(order, ["b", "d", "a", "c"]);
    }

    #[test]
    fn the_folder_lists_only_this_kind_under_these_rules() {
        let t = tempfile::tempdir().unwrap();
        let dir = SaveDir::new(t.path().join("Save"));
        assert!(dir.list(Kind::Single, true).is_empty(), "no folder yet");
        dir.write("Siege", Kind::Single, &bytes("Siege", 10, true, Kind::Single))
            .unwrap();
        dir.write("Newer", Kind::Single, &bytes("Newer", 20, true, Kind::Single))
            .unwrap();
        dir.write("Swamp", Kind::Net, &bytes("Swamp", 30, false, Kind::Net))
            .unwrap();
        dir.write("Classic", Kind::Single, &bytes("Classic", 40, false, Kind::Single))
            .unwrap();
        std::fs::write(dir.root.join("junk.sav"), b"not a saved game").unwrap();
        std::fs::write(dir.root.join("notes.txt"), b"x").unwrap();
        let names = |l: Vec<Entry>| l.into_iter().map(|e| e.stem).collect::<Vec<_>>();
        assert_eq!(
            names(dir.list(Kind::Single, true)),
            ["Newer", "Siege"],
            "newest first; .net, junk and Classic left out"
        );
        assert_eq!(
            names(dir.list(Kind::Net, false)),
            ["Swamp"],
            "a swamp-era Classic game is listed and loadable"
        );
        assert!(dir.list(Kind::Session, false).is_empty());
        let e = dir.list(Kind::Single, true).remove(0);
        assert_eq!(e.stored_name, "Newer");
        assert_eq!(e.race, 1);
        let (h, _) = SaveDir::read(&e.path, true).unwrap();
        assert_eq!(h.name, "Newer");
        assert_eq!(SaveDir::read(&e.path, false).unwrap_err(), Reason::OtherRules);
        assert_eq!(
            SaveDir::read(&dir.root.join("none.sav"), true).unwrap_err(),
            Reason::CantOpen
        );
        dir.delete(&e).unwrap();
        assert_eq!(names(dir.list(Kind::Single, true)), ["Siege"]);
    }

    #[test]
    fn overwriting_replaces_the_file_and_a_hundred_are_the_most_listed() {
        let t = tempfile::tempdir().unwrap();
        let dir = SaveDir::new(t.path());
        for i in 0..105u64 {
            dir.write(&format!("g{i}"), Kind::Single, &bytes("g", i, true, Kind::Single))
                .unwrap();
        }
        assert_eq!(dir.list(Kind::Single, true).len(), MAX_SAVES);
        dir.write("g1", Kind::Single, &bytes("again", 500, true, Kind::Single))
            .unwrap();
        assert!(dir.list(Kind::Single, true).iter().any(|e| e.stored_name == "again"));
    }

    #[test]
    fn writing_into_a_blocked_folder_says_cant_create() {
        let t = tempfile::tempdir().unwrap();
        let blocker = t.path().join("Save");
        std::fs::write(&blocker, b"a file where the folder should be").unwrap();
        let dir = SaveDir::new(&blocker);
        assert_eq!(dir.write("x", Kind::Single, &[1]), Err(Reason::CantCreate));
    }

    #[test]
    fn only_the_lowest_player_may_save_in_multiplayer() {
        assert_eq!(may_save(false, 3, &[0, 3]), Ok(()), "single player: always");
        assert_eq!(may_save(true, 0, &[0, 3]), Ok(()));
        assert_eq!(may_save(true, 3, &[0, 3]), Err(0), "entry 28 names player 0");
        assert_eq!(may_save(true, 3, &[3, 5]), Ok(()), "the lowest still in the game");
    }

    #[test]
    fn battle_net_waits_two_minutes_after_a_save() {
        let mut w = BnetWait::default();
        assert!(w.allows(0.0), "no wait at the start of a game");
        w.sent(1000.0);
        assert!(!w.allows(1060.0), "60 s later: notice 29");
        assert!(!w.allows(1119.9));
        assert!(w.allows(1120.0));
    }

    #[test]
    fn multiplayer_requests_pick_their_file() {
        let mut v = vec![entry("Newest", 9), entry("Mid", 5), entry("Oldest", 1)];
        v[1].stored_name = "Raid".into();
        assert_eq!(multiplayer_target(&v, "Raid"), Target::Overwrite("Mid".into()));
        assert_eq!(multiplayer_target(&v, "con"), Target::New("con_".into()));
        let full: Vec<Entry> = (0..MAX_SAVES)
            .rev()
            .map(|i| entry(&format!("g{i}"), i as u64))
            .collect();
        assert_eq!(multiplayer_target(&full, "new"), Target::ReplaceOldest("g0".into()));
        assert_eq!(multiplayer_target(&full, "g5"), Target::Overwrite("g5".into()));
    }
}
