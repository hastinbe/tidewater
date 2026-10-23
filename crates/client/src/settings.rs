//! Tidewater's own Settings screen: the optional improvements (`extras`), the window, the mouse lock, the intro video,
//! the update check and the game files, changed in the game instead of in `settings.txt` or on the command line.
//! Not part of the original, so it is opened by a key and a link on the main menu (F2 or "Settings (F2)"), which leave
//! the original's menus as they were (`docs/design/extras.md`, "The Settings screen"). SDL-free: `frontend` opens the
//! screen, feeds it the frame's input ([`Input`]) and draws what [`Screen::draw`] gives, as for the Custom Scenario
//! screen (`customsetup`).
//!
//! **Look.** The menu picture, the game's fonts, the menu buttons' faces and colours (`menuscreen`) on a black panel
//! like the small dialogs'. A value is chosen in the same drop-down as the Custom Scenario screen's boxes
//! (`customsetup::Look::drop`, the open list `mpwidgets::Popup`). The description of the focused or hovered row is in
//! the small font under the rows.
//!
//! **Generated from the registry.** The rows of the Interface, Sound and Gameplay pages, and the Scaling and Widescreen
//! rows of the General page, come from [`extras::REGISTRY`]: the row's name is [`extras::Spec::label`], its values come
//! from the kind, its description is the registry's. Only the page each option is on ([`extra_page`]) and its values'
//! words ([`CHOICE_WORDS`]) are written here; the page's `match` has no catch-all arm, so a new option does not build
//! until it is on a page, and a test checks that every option is on exactly one.
//!
//! **What OK does.** The screen edits a [`Values`] copy. OK ([`commit`]) writes only the rows that were changed (or
//! reset by Defaults) into the settings file's view and into the run's live values, so an option the command line set
//! for this run stays as it is unless the player chose it here; the frontend then writes `settings.txt` at once.
//! Cancel and Esc throw the copy away.
//!
//! Choices of ours where nothing else says (the screen is ours): the tabs are the four pages, the selected one drawn
//! pressed; Up and Down move the focus like Tab and Shift+Tab; Left and Right change the focused drop-down's value, move
//! between the tabs, or between OK, Cancel and Defaults; PageUp and PageDown change page; O, C and D press the
//! bottom buttons.

use tidewater_formats::fnt::Font;

use crate::audio::Audio;
use crate::customsetup::{DropState, Input, Look};
use crate::extras::{self, ClientExtras, Id, Kind, REGISTRY, Scope};
use crate::hud::Rect;
use crate::input::Key;
use crate::menuscreen::{self, ButtonState};
use crate::mpwidgets::{self, Popup, PopupKey, Ptr};
use crate::prefs::{Prefs, bit};
use crate::scene::Picture;

// ---------------------------------------------------------------------------------------------------------------
// Pages and rows

/// A page of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    General,
    Interface,
    Sound,
    Gameplay,
}

impl Page {
    pub(crate) const ALL: [Page; 4] = [Page::General, Page::Interface, Page::Sound, Page::Gameplay];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Interface => "Interface",
            Page::Sound => "Sound",
            Page::Gameplay => "Gameplay",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Page::General => {
                "The window, the picture, the mouse and start-up. The game files Tidewater runs on are here too."
            }
            Page::Interface => "Extra help on the game screen. Each one is off until you turn it on.",
            Page::Sound => {
                "How loud the unit voices, the sound effects and the music are, as a share of the game's own volume."
            }
            Page::Gameplay => "Rule changes for the games you start. Point at a row to read what it does.",
        }
    }
}

/// The page an option is on. No catch-all arm: a new option has to be put on a page to build.
pub(crate) fn extra_page(id: Id) -> Page {
    match id {
        Id::Scaling | Id::Widescreen => Page::General,
        Id::HealthBars
        | Id::IdleWorker
        | Id::GameClock
        | Id::ControlGroupBar
        | Id::SmartAttackMove
        | Id::MinimapPing
        | Id::AutoReplay => Page::Interface,
        Id::VolumeVoices | Id::VolumeEffects | Id::VolumeMusic => Page::Sound,
        Id::RallyPoints | Id::ProductionQueue | Id::SelectionLimit | Id::SpeedLevels | Id::ShiftQueue => Page::Gameplay,
    }
}

/// One setting with a drop-down: an option of the registry or one of the stored ones that are not options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Extra(Id),
    /// Window or full screen (`Prefs::fullscreen`).
    Window,
    /// The mouse lock during a game (`Prefs::mouse_lock`).
    MouseLock,
    /// The introduction video at start-up (`prefs::bit::OPENING_FILM`).
    IntroVideo,
    /// The check for a newer release at start-up (`Prefs::update_check`).
    UpdateCheck,
}

/// The rows that are not registry options, after the registry's.
const OTHER_ITEMS: [Item; 4] = [Item::Window, Item::MouseLock, Item::IntroVideo, Item::UpdateCheck];

/// How many settings the screen edits.
const ITEMS: usize = extras::COUNT + OTHER_ITEMS.len();

fn index(item: Item) -> usize {
    match item {
        Item::Extra(id) => id as usize,
        other => extras::COUNT + OTHER_ITEMS.iter().position(|&i| i == other).unwrap_or(0),
    }
}

fn item_at(i: usize) -> Item {
    if i < extras::COUNT {
        Item::Extra(REGISTRY[i].id)
    } else {
        OTHER_ITEMS[i - extras::COUNT]
    }
}

/// The rows of a page in the order they are shown, before the platform's filter ([`shown`]). The General page leads
/// with the window row, then the registry's options for it, then the stored switches.
pub(crate) fn items_on(page: Page) -> Vec<Item> {
    let mut v = Vec::new();
    if page == Page::General {
        v.push(Item::Window);
    }
    v.extend(
        REGISTRY
            .iter()
            .filter(|s| extra_page(s.id) == page)
            .map(|s| Item::Extra(s.id)),
    );
    if page == Page::General {
        v.extend([Item::MouseLock, Item::IntroVideo, Item::UpdateCheck]);
    }
    v
}

/// What the platform the screen runs on allows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Env {
    /// A phone or tablet (`Core::touch.touch_only`): no window, mouse lock or settings folder rows.
    pub touch: bool,
    /// The program checks for a newer release at all (`update`): a desktop release build.
    pub updates: bool,
    /// The line about the imported game files, `None` when there is nothing to manage (`--install` reads the files in
    /// place): without it there are no game-files row and no "Import game files again..." button.
    pub data: Option<String>,
}

/// Whether the row is shown here.
pub(crate) fn shown(item: Item, env: &Env) -> bool {
    match item {
        Item::Extra(_) | Item::IntroVideo => true,
        Item::Window | Item::MouseLock => !env.touch,
        Item::UpdateCheck => !env.touch && env.updates,
    }
}

/// A button that is not a drop-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Open the folder with `settings.txt` in the system's file manager.
    OpenFolder,
    /// Open the first-run import screen again.
    Reimport,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::OpenFolder => "Open settings folder",
            Action::Reimport => "Import game files again...",
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Values

/// The words of each choice option's values, in the order of its registry names (`extras::Kind::Choice`).
pub(crate) const CHOICE_WORDS: &[(Id, &[&str])] = &[
    (Id::HealthBars, &["Off", "Hovered", "Selected", "Always"]),
    (Id::GameClock, &["Off", "Clock", "Clock and APM"]),
    (Id::Scaling, &["Whole numbers", "Any size, sharp"]),
    (Id::SelectionLimit, &["9 (original)", "12", "18", "Unlimited"]),
    (Id::SpeedLevels, &["Off", "1 more", "2 more", "3 more"]),
    (Id::AutoReplay, &["Off", "Save from menu", "Always save"]),
    (Id::Widescreen, &["Off", "16:10", "16:9", "Auto"]),
];

/// The default of an item.
fn default_of(item: Item) -> u8 {
    match item {
        Item::Extra(id) => id.spec().default,
        Item::Window => 0,
        Item::MouseLock | Item::IntroVideo | Item::UpdateCheck => 1,
    }
}

/// The values a drop-down offers, as (stored number, words), for an item whose value now is `current`.
pub(crate) fn options(item: Item, current: u8) -> Vec<(u8, String)> {
    let on_off = || vec![(0, "Off".to_string()), (1, "On".to_string())];
    match item {
        Item::Extra(id) => match id.spec().kind {
            Kind::Flag => on_off(),
            Kind::Choice(names) => {
                let words = CHOICE_WORDS.iter().find(|(i, _)| *i == id).map(|(_, w)| *w);
                (0..names.len())
                    .map(|i| {
                        let w = words.and_then(|w| w.get(i)).copied().unwrap_or(names[i]);
                        (i as u8, w.to_string())
                    })
                    .collect()
            }
            Kind::Percent => {
                let mut v: Vec<u8> = (0..=10).map(|i| i * 10).collect();
                // A stored value that is not a multiple of 10 is shown and kept until it is changed.
                if !v.contains(&current) {
                    v.push(current);
                    v.sort_unstable();
                }
                v.into_iter().map(|n| (n, format!("{n}%"))).collect()
            }
        },
        Item::Window => vec![(0, "Window".to_string()), (1, "Full screen".to_string())],
        Item::MouseLock | Item::IntroVideo | Item::UpdateCheck => on_off(),
    }
}

/// The settings as the screen edits them: a number for each item, and which items were changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Values {
    v: [u8; ITEMS],
    touched: [bool; ITEMS],
}

impl Values {
    /// The values now in force: the run's live options (the settings file's, then the command line's), the stored
    /// switches, and the intro video and update check as the command line left them (`--no-intro`,
    /// `--no-update-check`).
    pub(crate) fn new(prefs: &Prefs, live: &ClientExtras, no_intro: bool, no_update_check: bool) -> Values {
        let mut v = [0; ITEMS];
        for s in &REGISTRY {
            v[s.id as usize] = live.get(s.id);
        }
        v[index(Item::Window)] = u8::from(prefs.fullscreen);
        v[index(Item::MouseLock)] = u8::from(prefs.mouse_lock);
        v[index(Item::IntroVideo)] = u8::from(prefs.has(bit::OPENING_FILM) && !no_intro);
        v[index(Item::UpdateCheck)] = u8::from(prefs.update_check && !no_update_check);
        Values {
            v,
            touched: [false; ITEMS],
        }
    }

    pub(crate) fn get(&self, item: Item) -> u8 {
        self.v[index(item)]
    }

    pub(crate) fn set(&mut self, item: Item, value: u8) {
        self.v[index(item)] = value;
        self.touched[index(item)] = true;
    }

    /// Every shown row back to its default.
    fn defaults(&mut self, env: &Env) {
        for i in 0..ITEMS {
            let item = item_at(i);
            if shown(item, env) {
                self.set(item, default_of(item));
            }
        }
    }

    /// Writes the changed rows into the settings file's view and the run's live options.
    pub(crate) fn apply(&self, stored: &mut Prefs, live: &mut ClientExtras) {
        for i in (0..ITEMS).filter(|&i| self.touched[i]) {
            let v = self.v[i];
            match item_at(i) {
                Item::Extra(id) => {
                    stored.extras.set(id, v);
                    live.set(id, v);
                }
                Item::Window => stored.fullscreen = v != 0,
                Item::MouseLock => stored.mouse_lock = v != 0,
                Item::IntroVideo => stored.set(bit::OPENING_FILM, v != 0),
                Item::UpdateCheck => stored.update_check = v != 0,
            }
        }
    }
}

/// What OK does with the values: the changed rows into the stored settings and the live options, and the mixer's
/// volumes at once. The caller writes the settings file.
pub(crate) fn commit(values: &Values, stored: &mut Prefs, live: &mut ClientExtras, audio: &mut Audio) {
    values.apply(stored, live);
    audio.set_trims(live.trims());
}

// ---------------------------------------------------------------------------------------------------------------
// Descriptions

/// The note on the Gameplay page.
const GAMEPLAY_NOTE: &str = "These options change the game rules. They apply to single-player games you start; in a \
                             multiplayer game the host chooses them in the lobby.";

/// The description of a row, shown under the rows.
pub(crate) fn describe(item: Item) -> String {
    match item {
        Item::Extra(id) => {
            let s = id.spec();
            let when = match (s.scope, id) {
                (_, Id::Widescreen) => " Applies from the next game you start.",
                (Scope::Sim, _) => " Applies from the next game you start.",
                _ => "",
            };
            format!("{}.{when}", s.description.trim_end_matches('.'))
        }
        Item::Window => "Run Tidewater in a window or full screen. Applies at once.".to_string(),
        Item::MouseLock => "Keep the pointer inside the window during a game so it cannot slip out at the edge. Hold \
                            Cmd (the Windows key elsewhere) to free it. The --no-mouse-lock flag turns it off for one run."
            .to_string(),
        Item::IntroVideo => {
            "Play the introduction video when Tidewater starts. Takes effect the next time it starts.".to_string()
        }
        Item::UpdateCheck => "Look for a newer Tidewater release when it starts and offer it on the main menu. Takes \
                              effect the next time it starts."
            .to_string(),
    }
}

fn describe_action(a: Action) -> &'static str {
    match a {
        Action::OpenFolder => {
            "Open the folder that holds settings.txt in your file manager. Keys are rebound in the [keys] section of \
             that file."
        }
        Action::Reimport => {
            "Choose the game files again, for example after reinstalling the game. Esc on that screen leaves \
             everything as it was. Changes made here are discarded."
        }
    }
}

/// The line about the imported game files, from the import's manifest.
pub(crate) fn data_summary(m: &tidewater_import::Manifest) -> String {
    let how = match m.source_kind.as_str() {
        "install-folder" => "game folder",
        "single-mpq" => "one archive",
        "mpq-folder" => "folder of archives",
        "gog-installer" => "GOG installer",
        "cd-image" => "CD image",
        _ => "an earlier import",
    };
    let maps = m
        .files
        .iter()
        .filter(|f| {
            let p = f.path.to_ascii_lowercase();
            p.starts_with("maps/") && p.ends_with(".pud")
        })
        .count();
    let mut s = format!(
        "Game files: {how}, {} files, {maps} maps, imported {}",
        m.files.len(),
        m.imported.split('T').next().unwrap_or("")
    );
    if let Some(v) = &m.known_version {
        s.push_str(&format!(" ({v})"));
    }
    s
}

/// The `file://` address of a folder, for the system to open in its file manager (SDL's `SDL_OpenURL`). Letters,
/// digits and `-._~/:` stay as they are, every other byte of the path is `%XX`; a Windows path's backslashes become
/// slashes and its drive letter follows three slashes.
pub(crate) fn folder_url(dir: &std::path::Path) -> String {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let text = dir.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !text.starts_with('/') {
        url.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            url.push(b as char);
        } else {
            url.push_str(&format!("%{b:02X}"));
        }
    }
    url
}

/// The line when the files are imported but there is no readable record of the import.
pub(crate) const DATA_UNKNOWN: &str = "Game files: imported (no record of where from)";

// ---------------------------------------------------------------------------------------------------------------
// Layout (ours: the screen is not in the original)

const PANEL: Rect = Rect::new(40, 24, 560, 424);
const HEADING: Rect = Rect::new(40, 34, 560, 20);
const TAB_Y: i32 = 62;
const TAB_W: u32 = 106;
const TAB_H: u32 = 28;
const TAB_GAP: i32 = 8;
const ROWS_TOP: i32 = 100;
const ROW_X: i32 = 56;
const ROW_W: u32 = 528;
/// A row's tap target; rows are 30 pixels apart.
const ROW_H: u32 = 28;
const ROW_PITCH: i32 = 30;
const LABEL_X: i32 = 64;
const DROP_X: i32 = 352;
const DROP_W: u32 = 224;
const DROP_H: u32 = 24;
const TEXT_LINE: i32 = 14;
const ACTION_W: u32 = 224;
const ACTION_GAP: i32 = 16;
const DESC: Rect = Rect::new(ROW_X, 344, ROW_W, 4 * TEXT_LINE as u32);
const BUTTON_Y: i32 = 410;
const BUTTON_W: u32 = 106;
const BUTTON_H: u32 = 28;
const OK: Rect = Rect::new(470, BUTTON_Y, BUTTON_W, BUTTON_H);
const CANCEL: Rect = Rect::new(356, BUTTON_Y, BUTTON_W, BUTTON_H);
const DEFAULTS: Rect = Rect::new(64, BUTTON_Y, BUTTON_W, BUTTON_H);

fn tab_rect(p: Page) -> Rect {
    let n = Page::ALL.len() as i32;
    let total = n * TAB_W as i32 + (n - 1) * TAB_GAP;
    let i = Page::ALL.iter().position(|&x| x == p).unwrap_or(0) as i32;
    Rect::new(
        PANEL.x + (PANEL.w as i32 - total) / 2 + i * (TAB_W as i32 + TAB_GAP),
        TAB_Y,
        TAB_W,
        TAB_H,
    )
}

/// The drop-down box inside a row's tap target.
fn drop_rect(row: Rect) -> Rect {
    Rect::new(DROP_X, row.y + (row.h as i32 - DROP_H as i32) / 2, DROP_W, DROP_H)
}

/// Something the pointer can be on or the focus can be on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Tab(Page),
    Row(Item),
    Action(Action),
    Ok,
    Cancel,
    Defaults,
    /// A line of the open drop-down's list.
    PopupRow(usize),
}

/// A page's targets and its fixed text lines, from the platform.
struct Layout {
    targets: Vec<(Target, Rect)>,
    /// Text in the small font: the lines to draw and the rectangle they wrap in.
    texts: Vec<(String, Rect)>,
}

fn layout(page: Page, env: &Env) -> Layout {
    let mut targets = Vec::new();
    let mut texts = Vec::new();
    let mut y = ROWS_TOP;
    for item in items_on(page).into_iter().filter(|&i| shown(i, env)) {
        targets.push((Target::Row(item), Rect::new(ROW_X, y, ROW_W, ROW_H)));
        y += ROW_PITCH;
    }
    if page == Page::General {
        if let Some(line) = &env.data {
            texts.push((line.clone(), Rect::new(LABEL_X, y + 2, ROW_W - 16, TEXT_LINE as u32)));
            y += TEXT_LINE + 6;
        }
        let actions: Vec<Action> = [(!env.touch, Action::OpenFolder), (env.data.is_some(), Action::Reimport)]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, a)| a)
            .collect();
        for (i, a) in actions.into_iter().enumerate() {
            targets.push((
                Target::Action(a),
                Rect::new(ROW_X + i as i32 * (ACTION_W as i32 + ACTION_GAP), y, ACTION_W, ROW_H),
            ));
        }
    }
    if page == Page::Gameplay {
        texts.push((
            GAMEPLAY_NOTE.to_string(),
            Rect::new(LABEL_X, y + 6, ROW_W - 16, 3 * TEXT_LINE as u32),
        ));
    }
    Layout { targets, texts }
}

/// The text of `s` in lines no wider than `max` pixels in `font` (spaces leaving [`menuscreen::SPACE_10X`] pixels).
pub(crate) fn wrap(font: &Font, s: &str, max: u32) -> Vec<String> {
    let width = |t: &str| menuscreen::text_width(font, t, menuscreen::SPACE_10X);
    let mut lines: Vec<String> = Vec::new();
    for word in s.split_whitespace() {
        match lines.last_mut() {
            Some(last) if width(&format!("{last} {word}")) <= max => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

// ---------------------------------------------------------------------------------------------------------------
// The screen

/// What a frame of the screen asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Stay,
    /// OK: apply and save ([`commit`]), then back to the main menu.
    Ok,
    /// Cancel or Esc: back to the main menu, the changes thrown away.
    Cancel,
    /// Open the settings folder.
    OpenFolder,
    /// Open the import screen again.
    Reimport,
}

/// An open drop-down list.
struct Open {
    item: Item,
    popup: Popup,
    /// The stored number of each line.
    values: Vec<u8>,
}

pub(crate) struct Screen {
    env: Env,
    values: Values,
    page: Page,
    focus: Target,
    open: Option<Open>,
    ptr: Ptr<Target>,
    changed: bool,
}

impl Screen {
    /// The screen as it opens, on the General page with the first tab focused.
    pub(crate) fn new(env: Env, values: Values) -> Screen {
        Screen {
            env,
            values,
            page: Page::General,
            focus: Target::Tab(Page::General),
            open: None,
            ptr: Ptr::default(),
            changed: true,
        }
    }

    pub(crate) fn values(&self) -> &Values {
        &self.values
    }

    #[cfg(test)]
    pub(crate) fn page(&self) -> Page {
        self.page
    }

    pub(crate) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Every target of the layer that takes the pointer, topmost first.
    fn targets(&self) -> Vec<(Target, Rect)> {
        if let Some(o) = &self.open {
            return (0..o.popup.labels.len())
                .map(|i| (Target::PopupRow(i), o.popup.row_rect(i)))
                .collect();
        }
        self.page_targets()
    }

    /// The targets of the page and the fixed ones, in Tab order.
    fn page_targets(&self) -> Vec<(Target, Rect)> {
        let mut v: Vec<(Target, Rect)> = Page::ALL.iter().map(|&p| (Target::Tab(p), tab_rect(p))).collect();
        v.extend(layout(self.page, &self.env).targets);
        v.extend([(Target::Ok, OK), (Target::Cancel, CANCEL), (Target::Defaults, DEFAULTS)]);
        v
    }

    fn stops(&self) -> Vec<Target> {
        self.page_targets().into_iter().map(|(t, _)| t).collect()
    }

    // --- Input -----------------------------------------------------------------------------------------------

    /// One frame: the pointer, the left button and the keys.
    pub(crate) fn update(&mut self, input: &Input) -> Outcome {
        let targets = self.targets();
        let ev = self.ptr.update(&targets, input);
        self.changed |= ev.changed;
        if input.down {
            if self.open.is_some() && ev.down.is_none() {
                // A press outside the open list closes it.
                self.open = None;
                self.ptr.reset();
                self.changed = true;
            } else if let Some(t) = ev.down
                && !matches!(t, Target::PopupRow(_))
            {
                self.focus = t;
                self.changed = true;
            }
        }
        let mut out = Outcome::Stay;
        if let Some(t) = ev.clicked {
            out = self.activate(t);
        }
        if out == Outcome::Stay {
            for &k in &input.keys {
                self.changed = true;
                out = if self.open.is_some() {
                    self.popup_key(k)
                } else {
                    self.main_key(k, input.shift)
                };
                if out != Outcome::Stay {
                    break;
                }
            }
        }
        out
    }

    fn activate(&mut self, t: Target) -> Outcome {
        self.changed = true;
        match t {
            Target::Tab(p) => {
                self.page = p;
                self.focus = Target::Tab(p);
            }
            Target::Row(item) => self.open_list(item),
            Target::Action(Action::OpenFolder) => return Outcome::OpenFolder,
            Target::Action(Action::Reimport) => return Outcome::Reimport,
            Target::Ok => return Outcome::Ok,
            Target::Cancel => return Outcome::Cancel,
            Target::Defaults => self.values.defaults(&self.env),
            Target::PopupRow(i) => self.choose(i),
        }
        Outcome::Stay
    }

    fn open_list(&mut self, item: Item) {
        let current = self.values.get(item);
        let opts = options(item, current);
        let at = opts.iter().position(|(n, _)| *n == current).unwrap_or(0);
        let anchor = self
            .page_targets()
            .into_iter()
            .find(|(t, _)| *t == Target::Row(item))
            .map_or_else(Rect::default, |(_, r)| drop_rect(r));
        let labels = opts.iter().map(|(_, w)| w.clone()).collect();
        self.open = Some(Open {
            item,
            popup: Popup::new(0, anchor, labels, at),
            values: opts.iter().map(|(n, _)| *n).collect(),
        });
    }

    /// Picks line `i` of the open list.
    fn choose(&mut self, i: usize) {
        let Some(o) = self.open.take() else { return };
        if let Some(&n) = o.values.get(i) {
            self.values.set(o.item, n);
        }
    }

    fn popup_key(&mut self, k: Key) -> Outcome {
        let Some(o) = &mut self.open else { return Outcome::Stay };
        match o.popup.key(k) {
            PopupKey::Open => {}
            PopupKey::Closed => self.open = None,
            PopupKey::Picked(i) => self.choose(i),
        }
        Outcome::Stay
    }

    fn step(&mut self, back: bool) {
        let key = if back {
            menuscreen::FocusKey::Back
        } else {
            menuscreen::FocusKey::Next
        };
        self.focus = menuscreen::step_focus(&self.stops(), self.focus, key);
    }

    /// The value of the focused drop-down moved by `by` places, wrapping.
    fn cycle(&mut self, item: Item, by: isize) {
        let current = self.values.get(item);
        let opts = options(item, current);
        let at = opts.iter().position(|(n, _)| *n == current).unwrap_or(0);
        let next = (at as isize + by).rem_euclid(opts.len() as isize) as usize;
        self.values.set(item, opts[next].0);
    }

    fn switch_page(&mut self, by: isize) {
        let at = Page::ALL.iter().position(|&p| p == self.page).unwrap_or(0) as isize;
        let p = Page::ALL[(at + by).rem_euclid(Page::ALL.len() as isize) as usize];
        self.page = p;
        if matches!(self.focus, Target::Tab(_)) {
            self.focus = Target::Tab(p);
        }
    }

    fn main_key(&mut self, k: Key, shift: bool) -> Outcome {
        match k {
            Key::Tab => self.step(shift),
            Key::Down => self.step(false),
            Key::Up => self.step(true),
            Key::Escape => return Outcome::Cancel,
            Key::Enter | Key::NumpadEnter | Key::Space => return self.activate(self.focus),
            Key::PageDown => self.switch_page(1),
            Key::PageUp => self.switch_page(-1),
            Key::Left | Key::Right => {
                let by = if k == Key::Left { -1 } else { 1 };
                match self.focus {
                    Target::Row(item) => self.cycle(item, by),
                    Target::Tab(_) => {
                        self.switch_page(by);
                        self.focus = Target::Tab(self.page);
                    }
                    Target::Ok | Target::Cancel | Target::Defaults => {
                        let row = [Target::Defaults, Target::Cancel, Target::Ok];
                        let at = row.iter().position(|&t| t == self.focus).unwrap_or(0) as isize;
                        self.focus = row[(at + by).clamp(0, 2) as usize];
                    }
                    _ => {}
                }
            }
            Key::Letter(c) => {
                let t = match c.to_ascii_uppercase() {
                    'O' => Target::Ok,
                    'C' => Target::Cancel,
                    'D' => Target::Defaults,
                    _ => return Outcome::Stay,
                };
                self.focus = t;
                return self.activate(t);
            }
            _ => {}
        }
        Outcome::Stay
    }

    // --- Drawing ---------------------------------------------------------------------------------------------

    /// The picture with the panel and the heading: what fades in with the screen.
    pub(crate) fn frame(bg: &Picture, look: &Look) -> Picture {
        let mut p = Picture {
            width: bg.width,
            height: bg.height,
            rgba: bg.rgba.clone(),
        };
        mpwidgets::draw_panel(look, &mut p, PANEL);
        menuscreen::draw_heading(&mut p, look.font, look.pal, HEADING, "Tidewater Settings");
        p
    }

    /// The description to show: the hovered row's, else the focused one's.
    fn described(&self) -> String {
        let t = match self.ptr.hot {
            Some(Target::PopupRow(_)) | None => self.open.as_ref().map(|o| Target::Row(o.item)).unwrap_or(self.focus),
            Some(t) => t,
        };
        match t {
            Target::Tab(p) => p.blurb().to_string(),
            Target::Row(item) => describe(item),
            Target::Action(a) => describe_action(a).to_string(),
            Target::Ok => "Apply the changes and save them.".to_string(),
            Target::Cancel => "Throw the changes away and go back to the main menu.".to_string(),
            Target::Defaults => {
                "Set every setting on every page to its default. Cancel still throws the change away.".to_string()
            }
            Target::PopupRow(_) => String::new(),
        }
    }

    /// The whole screen over `bg`, the menu picture.
    pub(crate) fn draw(&self, bg: &Picture, look: &Look, small: &Font) -> Picture {
        let mut p = Self::frame(bg, look);
        let modal = self.open.is_some();
        let state = |t: Target, enabled: bool| ButtonState {
            enabled,
            hover: !modal && self.ptr.is_hot(t),
            focused: !modal && self.focus == t,
            pressed: !modal && self.ptr.pressed(t),
            default: false,
        };
        for page in Page::ALL {
            let t = Target::Tab(page);
            let mut s = state(t, true);
            // The page that shows is drawn pressed, the way a tab is down.
            s.pressed |= page == self.page;
            menuscreen::draw_button(
                &mut p,
                look.font,
                look.pal,
                Some(look.faces),
                tab_rect(page),
                page.label(),
                None,
                s,
            );
        }
        let lay = layout(self.page, &self.env);
        for &(t, r) in &lay.targets {
            match t {
                Target::Row(item) => {
                    let label = match item {
                        Item::Extra(id) => id.spec().label,
                        Item::Window => "Window",
                        Item::MouseLock => "Mouse lock",
                        Item::IntroVideo => "Intro video",
                        Item::UpdateCheck => "Check for updates",
                    };
                    let b = drop_rect(r);
                    look.label(
                        &mut p,
                        (LABEL_X, b.y + (DROP_H as i32 - look.font.height as i32) / 2),
                        label,
                    );
                    let cur = self.values.get(item);
                    let word = options(item, cur)
                        .into_iter()
                        .find(|(n, _)| *n == cur)
                        .map(|(_, w)| w)
                        .unwrap_or_default();
                    look.drop(
                        &mut p,
                        b,
                        &word,
                        DropState::of(
                            !modal && self.ptr.is_hot(t),
                            !modal && self.focus == t,
                            !modal && self.ptr.pressed(t),
                        ),
                    );
                }
                Target::Action(a) => menuscreen::draw_button(
                    &mut p,
                    look.font,
                    look.pal,
                    Some(look.faces),
                    r,
                    a.label(),
                    None,
                    state(t, true),
                ),
                _ => {}
            }
        }
        for (text, r) in &lay.texts {
            self.draw_lines(&mut p, look, small, text, *r);
        }
        self.draw_lines(&mut p, look, small, &self.described(), DESC);
        for (t, r, label, key) in [
            (Target::Defaults, DEFAULTS, "Defaults", 'D'),
            (Target::Cancel, CANCEL, "Cancel", 'C'),
            (Target::Ok, OK, "OK", 'O'),
        ] {
            menuscreen::draw_button(
                &mut p,
                look.font,
                look.pal,
                Some(look.faces),
                r,
                label,
                Some(key),
                state(t, true),
            );
        }
        if let Some(o) = &self.open {
            let hot = match self.ptr.hot {
                Some(Target::PopupRow(i)) => Some(i),
                _ => None,
            };
            mpwidgets::draw_popup(look, &mut p, &o.popup, hot);
        }
        p
    }

    /// `text` wrapped in `r` in the small font, gold; what does not fit in the rectangle's height is cut with "...".
    fn draw_lines(&self, p: &mut Picture, look: &Look, small: &Font, text: &str, r: Rect) {
        let mut lines = wrap(small, text, r.w);
        let room = (r.h as i32 / TEXT_LINE).max(1) as usize;
        if lines.len() > room {
            lines.truncate(room);
            if let Some(last) = lines.last_mut() {
                *last = crate::customsetup::fit(small, &format!("{last}..."), r.w);
                if !last.ends_with("...") {
                    last.push_str("...");
                }
            }
        }
        for (i, l) in lines.iter().enumerate() {
            menuscreen::draw_text(
                p,
                small,
                look.pal,
                (r.x, r.y + i as i32 * TEXT_LINE),
                l,
                menuscreen::GOLD,
                menuscreen::SPACE_10X,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use tidewater_formats::tileset::Palette;

    use super::*;

    fn env(touch: bool) -> Env {
        Env {
            touch,
            updates: !touch,
            data: (!touch).then(|| "Game files: GOG installer, 156 files, 41 maps, imported 2026-10-23".to_string()),
        }
    }

    fn screen_with(env: Env) -> Screen {
        let v = Values::new(&Prefs::default(), &ClientExtras::default(), false, false);
        Screen::new(env, v)
    }

    fn screen() -> Screen {
        screen_with(env(false))
    }

    fn click_at(s: &mut Screen, x: i32, y: i32) -> Outcome {
        s.update(&Input {
            at: Some((x, y)),
            down: true,
            ..Default::default()
        });
        s.update(&Input {
            at: Some((x, y)),
            up: true,
            ..Default::default()
        })
    }

    fn click(s: &mut Screen, t: Target) -> Outcome {
        let r = s
            .targets()
            .into_iter()
            .find(|(x, _)| *x == t)
            .map(|(_, r)| r)
            .unwrap_or_else(|| panic!("{t:?} is not there"));
        let (x, y) = if let Target::Row(_) = t {
            let d = drop_rect(r);
            (d.x + 10, d.y + 10)
        } else {
            (r.x + r.w as i32 / 2, r.y + r.h as i32 / 2)
        };
        click_at(s, x, y)
    }

    fn key(s: &mut Screen, k: Key) -> Outcome {
        s.update(&Input {
            keys: vec![k],
            ..Default::default()
        })
    }

    fn pick(s: &mut Screen, item: Item, line: usize) {
        click(s, Target::Row(item));
        assert!(s.open.is_some(), "the list of {item:?} opened");
        let r = s.open.as_ref().unwrap().popup.row_rect(line);
        click_at(s, r.x + 5, r.y + 5);
        assert!(s.open.is_none());
    }

    #[test]
    fn every_option_is_on_exactly_one_page() {
        for s in &REGISTRY {
            let pages: Vec<Page> = Page::ALL
                .into_iter()
                .filter(|&p| items_on(p).contains(&Item::Extra(s.id)))
                .collect();
            assert_eq!(pages, [extra_page(s.id)], "{} is on exactly one page", s.name);
        }
        let rows: usize = Page::ALL
            .iter()
            .map(|&p| items_on(p).iter().filter(|i| matches!(i, Item::Extra(_))).count())
            .sum();
        assert_eq!(rows, REGISTRY.len());
    }

    #[test]
    fn the_pages_hold_what_the_brief_lists() {
        let names = |p: Page| -> Vec<&'static str> {
            items_on(p)
                .into_iter()
                .filter_map(|i| match i {
                    Item::Extra(id) => Some(id.spec().name),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(names(Page::General), ["scaling", "widescreen"]);
        assert_eq!(
            names(Page::Interface),
            [
                "health-bars",
                "idle-worker",
                "game-clock",
                "group-bar",
                "smart-attack-move",
                "minimap-ping",
                "auto-replay"
            ]
        );
        assert_eq!(names(Page::Sound), ["volume-voices", "volume-effects", "volume-music"]);
        assert_eq!(
            names(Page::Gameplay),
            [
                "rally-points",
                "production-queue",
                "selection-limit",
                "speed-levels",
                "shift-queue"
            ]
        );
        assert_eq!(items_on(Page::General).first(), Some(&Item::Window));
    }

    #[test]
    fn every_option_has_a_label_and_every_choice_has_words() {
        for s in &REGISTRY {
            assert!(!s.label.is_empty() && s.label.len() <= 24, "{}", s.name);
            let v = options(Item::Extra(s.id), s.default);
            match s.kind {
                Kind::Flag => assert_eq!(v.len(), 2),
                Kind::Choice(names) => {
                    assert_eq!(v.len(), names.len(), "{}", s.name);
                    let words = CHOICE_WORDS
                        .iter()
                        .find(|(i, _)| *i == s.id)
                        .expect("words for the choice")
                        .1;
                    assert_eq!(words.len(), names.len(), "{}", s.name);
                    // Human words, not the settings file's tokens.
                    for (w, n) in words.iter().zip(names) {
                        assert!(
                            w.chars().next().is_some_and(char::is_uppercase)
                                || w.chars().next().is_some_and(|c| c.is_ascii_digit()),
                            "{w} for {n}"
                        );
                    }
                }
                Kind::Percent => assert_eq!(v.len(), 11),
            }
        }
        let words = |id: Id| {
            options(Item::Extra(id), 0)
                .into_iter()
                .map(|(_, w)| w)
                .collect::<Vec<_>>()
        };
        assert_eq!(words(Id::HealthBars), ["Off", "Hovered", "Selected", "Always"]);
        assert_eq!(words(Id::Widescreen), ["Off", "16:10", "16:9", "Auto"]);
        assert_eq!(words(Id::SelectionLimit)[3], "Unlimited");
        assert_eq!(words(Id::IdleWorker), ["Off", "On"]);
    }

    #[test]
    fn a_volume_that_is_not_a_multiple_of_ten_is_shown_and_kept() {
        let o = options(Item::Extra(Id::VolumeMusic), 37);
        assert_eq!(o.len(), 12);
        assert!(o.iter().any(|(n, w)| *n == 37 && w == "37%"));
        assert!(o.windows(2).all(|w| w[0].0 < w[1].0), "in order");
        assert_eq!(options(Item::Extra(Id::VolumeMusic), 40).len(), 11);
        // Opening the list and closing it again keeps the odd value.
        let mut live = ClientExtras::default();
        live.set(Id::VolumeMusic, 37);
        let mut s = Screen::new(env(false), Values::new(&Prefs::default(), &live, false, false));
        s.page = Page::Sound;
        click(&mut s, Target::Row(Item::Extra(Id::VolumeMusic)));
        key(&mut s, Key::Escape);
        assert_eq!(s.values.get(Item::Extra(Id::VolumeMusic)), 37);
        assert!(!s.values.touched[index(Item::Extra(Id::VolumeMusic))]);
    }

    #[test]
    fn ok_applies_the_changed_rows_and_the_file_text_holds_them() {
        let mut s = screen();
        pick(&mut s, Item::Extra(Id::Scaling), 1);
        pick(&mut s, Item::Window, 1);
        pick(&mut s, Item::MouseLock, 0);
        pick(&mut s, Item::IntroVideo, 0);
        assert_eq!(click(&mut s, Target::Tab(Page::Sound)), Outcome::Stay);
        pick(&mut s, Item::Extra(Id::VolumeMusic), 3);
        assert_eq!(click(&mut s, Target::Ok), Outcome::Ok);
        let mut stored = Prefs::default();
        let mut live = ClientExtras::default();
        let mut audio = Audio::default();
        commit(s.values(), &mut stored, &mut live, &mut audio);
        assert_eq!(stored.extras.get(Id::Scaling), 1);
        assert_eq!(live.get(Id::Scaling), 1);
        assert!(stored.fullscreen && !stored.mouse_lock && !stored.has(bit::OPENING_FILM));
        assert_eq!(stored.extras.get(Id::VolumeMusic), 30);
        let back = Prefs::from_text(&stored.to_text());
        assert_eq!(back.extras.get(Id::VolumeMusic), 30);
        assert!(back.fullscreen && !back.mouse_lock && !back.has(bit::OPENING_FILM));
    }

    #[test]
    fn a_command_line_override_survives_ok_unless_the_row_was_chosen() {
        let stored_file = Prefs::default();
        let mut live = ClientExtras::default();
        live.set(Id::HealthBars, 3); // `--extra-health-bars always` for this run
        let mut s = Screen::new(env(false), Values::new(&stored_file, &live, false, false));
        assert_eq!(
            s.values.get(Item::Extra(Id::HealthBars)),
            3,
            "the screen shows what is in force"
        );
        pick(&mut s, Item::Window, 1);
        let (mut stored, mut l, mut audio) = (stored_file.clone(), live, Audio::default());
        commit(s.values(), &mut stored, &mut l, &mut audio);
        assert_eq!(l.get(Id::HealthBars), 3, "the override stays for the run");
        assert_eq!(stored.extras.get(Id::HealthBars), 0, "and is not written to the file");
        // Choosing the row replaces the override.
        s.page = Page::Interface;
        pick(&mut s, Item::Extra(Id::HealthBars), 1);
        commit(s.values(), &mut stored, &mut l, &mut audio);
        assert_eq!((l.get(Id::HealthBars), stored.extras.get(Id::HealthBars)), (1, 1));
    }

    #[test]
    fn cancel_and_esc_leave_without_a_change_to_apply() {
        let mut s = screen();
        pick(&mut s, Item::Window, 1);
        assert_eq!(click(&mut s, Target::Cancel), Outcome::Cancel);
        assert_eq!(key(&mut s, Key::Escape), Outcome::Cancel);
        // The settings were never touched: only `commit` writes them, and the frontend calls it for OK alone.
        let stored = Prefs::default();
        assert!(!stored.fullscreen);
    }

    #[test]
    fn defaults_resets_every_shown_row_on_every_page() {
        let mut live = ClientExtras::default();
        live.set(Id::HealthBars, 2);
        live.set(Id::RallyPoints, 1);
        live.set(Id::VolumeVoices, 20);
        let mut prefs = Prefs::default();
        prefs.fullscreen = true;
        prefs.mouse_lock = false;
        prefs.update_check = false;
        let mut s = Screen::new(env(false), Values::new(&prefs, &live, true, false));
        assert_eq!(click(&mut s, Target::Defaults), Outcome::Stay);
        for i in 0..ITEMS {
            let it = item_at(i);
            assert_eq!(s.values.get(it), default_of(it), "{it:?}");
        }
        let (mut stored, mut l, mut audio) = (prefs.clone(), live, Audio::default());
        commit(s.values(), &mut stored, &mut l, &mut audio);
        assert!(!stored.fullscreen && stored.mouse_lock && stored.update_check && stored.has(bit::OPENING_FILM));
        assert!(l.is_default());
        // On a phone the rows it cannot show are left as they are.
        let mut m = Screen::new(env(true), Values::new(&prefs, &ClientExtras::default(), false, false));
        key(&mut m, Key::Letter('D'));
        assert!(
            m.values.get(Item::Window) == 1,
            "the window row is not on a phone, so Defaults leaves it"
        );
    }

    #[test]
    fn focus_moves_with_tab_and_the_arrows_and_wraps() {
        let mut s = screen();
        assert_eq!(s.focus, Target::Tab(Page::General));
        key(&mut s, Key::Tab);
        assert_eq!(s.focus, Target::Tab(Page::Interface));
        s.update(&Input {
            keys: vec![Key::Tab],
            shift: true,
            ..Default::default()
        });
        assert_eq!(s.focus, Target::Tab(Page::General));
        // Shift+Tab from the first stop wraps to the last, Tab from the last to the first.
        s.update(&Input {
            keys: vec![Key::Tab],
            shift: true,
            ..Default::default()
        });
        assert_eq!(s.focus, Target::Defaults);
        key(&mut s, Key::Tab);
        assert_eq!(s.focus, Target::Tab(Page::General));
        // Down walks through the tabs, then the rows in order.
        for _ in 0..4 {
            key(&mut s, Key::Down);
        }
        assert_eq!(s.focus, Target::Row(Item::Window));
        key(&mut s, Key::Down);
        assert_eq!(s.focus, Target::Row(Item::Extra(Id::Scaling)));
        key(&mut s, Key::Up);
        assert_eq!(s.focus, Target::Row(Item::Window));
        // The buttons are reached after the rows.
        let stops = s.stops();
        assert_eq!(
            &stops[stops.len() - 3..],
            [Target::Ok, Target::Cancel, Target::Defaults]
        );
    }

    #[test]
    fn the_keyboard_opens_a_list_picks_and_cycles() {
        let mut s = screen();
        s.focus = Target::Row(Item::Window);
        key(&mut s, Key::Enter);
        assert!(s.open.is_some());
        key(&mut s, Key::Down);
        assert_eq!(key(&mut s, Key::Enter), Outcome::Stay);
        assert!(s.open.is_none());
        assert_eq!(s.values.get(Item::Window), 1);
        // Esc closes a list without leaving the screen.
        key(&mut s, Key::Enter);
        assert_eq!(key(&mut s, Key::Escape), Outcome::Stay);
        assert!(s.open.is_none());
        // Left and Right change the value and wrap.
        key(&mut s, Key::Right);
        assert_eq!(s.values.get(Item::Window), 0);
        key(&mut s, Key::Left);
        assert_eq!(s.values.get(Item::Window), 1);
        // Enter on a button presses it.
        s.focus = Target::Ok;
        assert_eq!(key(&mut s, Key::Enter), Outcome::Ok);
        s.focus = Target::Action(Action::OpenFolder);
        assert_eq!(key(&mut s, Key::Enter), Outcome::OpenFolder);
        s.focus = Target::Action(Action::Reimport);
        assert_eq!(key(&mut s, Key::Enter), Outcome::Reimport);
        assert_eq!(key(&mut s, Key::Letter('c')), Outcome::Cancel);
    }

    #[test]
    fn a_click_on_a_tab_shows_its_page_and_a_press_outside_a_list_closes_it() {
        let mut s = screen();
        click(&mut s, Target::Tab(Page::Gameplay));
        assert_eq!(s.page(), Page::Gameplay);
        assert_eq!(s.focus, Target::Tab(Page::Gameplay));
        click(&mut s, Target::Row(Item::Extra(Id::SelectionLimit)));
        assert!(s.open.is_some());
        s.update(&Input {
            at: Some((10, 10)),
            down: true,
            ..Default::default()
        });
        assert!(s.open.is_none());
        key(&mut s, Key::PageDown);
        assert_eq!(s.page(), Page::General);
        key(&mut s, Key::PageUp);
        assert_eq!(s.page(), Page::Gameplay);
    }

    #[test]
    fn a_release_off_the_button_chooses_nothing() {
        let mut s = screen();
        let r = OK;
        s.update(&Input {
            at: Some((r.x + 5, r.y + 5)),
            down: true,
            ..Default::default()
        });
        let out = s.update(&Input {
            at: Some((5, 5)),
            up: true,
            ..Default::default()
        });
        assert_eq!(out, Outcome::Stay);
    }

    #[test]
    fn the_platform_decides_which_rows_show() {
        let rows = |e: &Env, p: Page| -> Vec<Item> {
            layout(p, e)
                .targets
                .into_iter()
                .filter_map(|(t, _)| if let Target::Row(i) = t { Some(i) } else { None })
                .collect()
        };
        let desktop = env(false);
        let general = rows(&desktop, Page::General);
        assert_eq!(
            general,
            [
                Item::Window,
                Item::Extra(Id::Scaling),
                Item::Extra(Id::Widescreen),
                Item::MouseLock,
                Item::IntroVideo,
                Item::UpdateCheck
            ]
        );
        let phone = env(true);
        assert_eq!(
            rows(&phone, Page::General),
            [Item::Extra(Id::Scaling), Item::Extra(Id::Widescreen), Item::IntroVideo]
        );
        // A development build never checks for updates.
        let dev = Env {
            updates: false,
            ..env(false)
        };
        assert!(!rows(&dev, Page::General).contains(&Item::UpdateCheck));
        // The data management: the file line and the button need imported files; the folder button needs a desktop.
        let actions = |e: &Env| -> Vec<Action> {
            layout(Page::General, e)
                .targets
                .into_iter()
                .filter_map(|(t, _)| if let Target::Action(a) = t { Some(a) } else { None })
                .collect()
        };
        assert_eq!(actions(&desktop), [Action::OpenFolder, Action::Reimport]);
        let on_phone = Env {
            data: Some("x".into()),
            ..env(true)
        };
        assert_eq!(actions(&on_phone), [Action::Reimport]);
        let installed_in_place = Env {
            data: None,
            ..env(false)
        };
        assert_eq!(actions(&installed_in_place), [Action::OpenFolder]);
        assert!(layout(Page::General, &installed_in_place).texts.is_empty());
    }

    #[test]
    fn every_target_fits_the_screen_and_none_overlap_on_each_page() {
        let envs = [
            env(false),
            env(true),
            Env {
                updates: false,
                data: None,
                ..env(false)
            },
            Env {
                data: Some("x".into()),
                ..env(true)
            },
        ];
        let inside = |r: Rect, outer: Rect| {
            r.x >= outer.x
                && r.y >= outer.y
                && r.x + r.w as i32 <= outer.x + outer.w as i32
                && r.y + r.h as i32 <= outer.y + outer.h as i32
        };
        let apart = |a: Rect, b: Rect| {
            a.x + a.w as i32 <= b.x || b.x + b.w as i32 <= a.x || a.y + a.h as i32 <= b.y || b.y + b.h as i32 <= a.y
        };
        let screen = Rect::new(0, 0, 640, 480);
        assert!(inside(PANEL, screen));
        // The labels have room left of the drop-downs.
        const { assert!(DROP_X - LABEL_X >= 250) };
        for e in &envs {
            for page in Page::ALL {
                let s = screen_with(e.clone());
                let mut s = s;
                s.page = page;
                let mut all: Vec<(String, Rect)> = s
                    .page_targets()
                    .into_iter()
                    .map(|(t, r)| (format!("{t:?}"), r))
                    .collect();
                let lay = layout(page, e);
                for (i, (_, r)) in lay.texts.iter().enumerate() {
                    all.push((format!("text {i}"), *r));
                }
                all.push(("description".to_string(), DESC));
                all.push(("heading".to_string(), HEADING));
                for (name, r) in &all {
                    assert!(inside(*r, PANEL), "{name} on {page:?} is inside the panel: {r:?}");
                    assert!(inside(*r, screen), "{name} on {page:?} is on the screen");
                    if name != "description" && name != "heading" && !name.starts_with("text") {
                        assert!(r.h >= 24 && r.w >= 24, "{name} on {page:?} is big enough to tap: {r:?}");
                    }
                }
                for (i, (na, a)) in all.iter().enumerate() {
                    for (nb, b) in &all[i + 1..] {
                        assert!(apart(*a, *b), "{na} and {nb} overlap on {page:?}: {a:?} {b:?}");
                    }
                }
                // The drop-down boxes sit inside their rows, and the labels have room left of them.
                for (t, r) in &lay.targets {
                    if matches!(t, Target::Row(_)) {
                        assert!(inside(drop_rect(*r), *r));
                    }
                }
            }
        }
    }

    #[test]
    fn the_longest_list_fits_the_screen() {
        // The percent list has 11 lines (12 with an odd value) and opens under its box or above it.
        let mut s = screen();
        s.page = Page::Sound;
        click(&mut s, Target::Row(Item::Extra(Id::VolumeMusic)));
        let r = s.open.as_ref().unwrap().popup.rect();
        assert!(r.y >= 0 && r.y + r.h as i32 <= 480, "{r:?}");
        let mut s = screen();
        s.page = Page::Interface;
        click(&mut s, Target::Row(Item::Extra(Id::AutoReplay)));
        let r = s.open.as_ref().unwrap().popup.rect();
        assert!(r.y >= 0 && r.y + r.h as i32 <= 480, "{r:?}");
    }

    #[test]
    fn the_descriptions_say_when_a_change_takes_effect() {
        assert!(describe(Item::Extra(Id::RallyPoints)).contains("next game"));
        assert!(describe(Item::Extra(Id::Widescreen)).contains("next game"));
        assert!(describe(Item::Extra(Id::Widescreen)).contains("touch"));
        assert!(!describe(Item::Extra(Id::HealthBars)).contains("next game"));
        assert!(describe(Item::IntroVideo).contains("next time"));
        assert!(describe(Item::UpdateCheck).contains("next time"));
        assert!(describe_action(Action::OpenFolder).contains("[keys]"));
        assert!(GAMEPLAY_NOTE.contains("host") && GAMEPLAY_NOTE.contains("single-player"));
    }

    /// A synthetic font: every printable glyph 7 pixels wide, 10 high.
    fn font() -> Font {
        let g = tidewater_formats::fnt::Glyph {
            width: 6,
            height: 10,
            x: 0,
            y: 0,
            shades: vec![1; 60],
        };
        let mut glyphs = vec![None];
        glyphs.extend((33..=126).map(|_| Some(g.clone())));
        Font {
            first: 32,
            max_width: 6,
            height: 10,
            glyphs,
        }
    }

    #[test]
    fn every_description_wraps_into_the_room_it_has() {
        // At 7 pixels a letter (more than the game's small font uses) every description fits four lines.
        let f = font();
        let mut all: Vec<String> = Vec::new();
        for page in Page::ALL {
            for item in items_on(page) {
                all.push(describe(item));
            }
            all.push(page.blurb().to_string());
        }
        all.push(describe_action(Action::OpenFolder).to_string());
        all.push(describe_action(Action::Reimport).to_string());
        for d in all {
            let lines = wrap(&f, &d, DESC.w);
            assert!(lines.len() <= 4, "{} lines for: {d}", lines.len());
            assert!(lines.iter().all(|l| menuscreen::text_width(&f, l, 8) <= DESC.w));
        }
        assert!(wrap(&f, GAMEPLAY_NOTE, ROW_W - 16).len() <= 3);
    }

    #[test]
    fn the_screen_draws_every_page_without_leaving_the_panel() {
        let f = font();
        let mut pal = Palette([[0; 3]; 256]);
        for (i, c) in pal.0.iter_mut().enumerate() {
            *c = [i as u8, 255 - i as u8, 7];
        }
        let faces = menuscreen::ButtonFaces::default();
        let look = Look {
            font: &f,
            pal: &pal,
            faces: &faces,
        };
        let bg = Picture {
            width: 640,
            height: 480,
            rgba: vec![9; 640 * 480 * 4],
        };
        for e in [env(false), env(true)] {
            let mut s = screen_with(e);
            for page in Page::ALL {
                s.page = page;
                let p = s.draw(&bg, &look, &f);
                assert_eq!((p.width, p.height), (640, 480));
                assert_ne!(p.rgba, bg.rgba);
                // Nothing is drawn outside the panel.
                for y in 0..480usize {
                    for x in 0..640usize {
                        let outside = !(PANEL.x as usize..(PANEL.x as usize + PANEL.w as usize)).contains(&x)
                            || !(PANEL.y as usize..(PANEL.y as usize + PANEL.h as usize)).contains(&y);
                        if outside {
                            assert_eq!(p.rgba[(y * 640 + x) * 4], 9, "({x}, {y}) on {page:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_folder_becomes_a_file_url() {
        assert_eq!(
            folder_url(std::path::Path::new("/data/a b/Tide-water")),
            "file:///data/a%20b/Tide-water"
        );
        assert_eq!(folder_url(std::path::Path::new("/x/é")), "file:///x/%C3%A9");
        // A relative folder is made absolute first.
        assert!(folder_url(std::path::Path::new("some")).starts_with("file:///"));
    }

    #[test]
    fn the_data_line_names_the_source_the_files_and_the_maps() {
        use tidewater_import::manifest::FileEntry;
        let f = |p: &str| FileEntry {
            path: p.into(),
            size: 1,
            sha256: String::new(),
        };
        let mut m = tidewater_import::Manifest::new(
            "gog-installer",
            "x",
            vec![
                f("War2Dat.mpq"),
                f("Maps/A.pud"),
                f("Maps/Classic/B.PUD"),
                f("Maps/readme.txt"),
            ],
        );
        m.imported = "2026-10-23T21:18:44Z".into();
        let s = data_summary(&m);
        assert_eq!(s, "Game files: GOG installer, 4 files, 2 maps, imported 2026-10-23");
        m.source_kind = "something-new".into();
        assert!(data_summary(&m).contains("an earlier import"));
    }
}
