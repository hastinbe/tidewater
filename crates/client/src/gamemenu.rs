//! The in-game menu and the dialogs under it (spec `docs/specs/ui/game-menu.md`): the game menu (F10, Alt+M, the menu
//! button), options, help, the sound, speed and preferences dialogs, the objectives screen and the end-scenario menu
//! with its confirmations. The rules are in [`crate::menu`] and [`crate::prefs`]; this module draws the dialogs on the
//! front end's layer (as the victory and defeat dialogs are) and applies what they decide.
//!
//! While a menu is open the game doesn't advance (single player; [`crate::frontend::game_running`] is false).
//!
//! Choices where the spec leaves a gap:
//! - The dialog files (`gamemenu.bin` and the others) have no spec, so the layouts, the labels and the texts of the
//!   confirmations and the keyboard help are our own, drawn like the outcome dialogs, without the race backdrops.
//! - Save and Load open the save and load dialogs (`game/save-load.md`; the rules are in [`crate::saves`], the layout and
//!   input in [`crate::savedlg`], the game's side in [`crate::savegame`]). They are screens of this menu's stack, so
//!   the game stays stopped while they are open. Load stays in the menu: this client has no network session. The
//!   victory dialog's Save opens the same dialog over it ([`open_from_outcome`]); a save there ends the game as
//!   Continue does, and Cancel shows the victory dialog again.
//! - Restart rebuilds the mission through the flow (`frontend::restart`). Surrender sets the local player's result to
//!   defeat and closes the menus: the defeat dialog and score screen follow from the outcome dialog as for any defeat.
//! - Quit to menu, Return to chat and Load custom scenario all go to the main menu (the client has no chat room or
//!   custom-game screen yet). Exit ends the program.
//! - The speed dialog sets the game speed with speed commands when OK is pressed (the game is stopped while it is open,
//!   so nothing can tell the difference); the sim's speed starts at Normal whatever speed was stored.
//! - Fog of war changed in the preferences dialog takes effect on OK (the spec: nothing happens while the dialog is
//!   open); Cancel leaves it as it was.
//! - Sound dialog changes are applied 5 frames after the first change (the spec's open question default).
//! - Keyboard focus (`ui/screen-and-cursors.md`, "Keyboard focus": every dialog works as the main menu does): each
//!   screen opens focused on its default button ([`default_act`]); Tab and Shift+Tab move the yellow ring among the
//!   buttons only (not the sliders, check boxes or radios); Enter activates the focused button.
//! - Ctrl+S sets the digital volume to 0 and back (`Prefs::toggle_digital`); Ctrl+M isn't handled here, `music.rs`
//!   owns it (`Prefs::toggle_music` is ready for it).
//!
//! Not yet here:
//! - The tips dialog at the start of a game, and the "play the opening film" and "system pointer" bits.
//! - Consuming the music volume, the music bit and colour cycling: `music.rs` reads [`Stored`]`.0.music_volume`
//!   and `palette.rs` [`crate::prefs::Prefs::colour_cycling`].
//! - The pause greyscale (`palette.rs`), the Enhanced multiplayer pause counts, Battle.net rules and the restart
//!   permission check (no multiplayer).
//! - The dialogs' own art, and the "Network" button of the Battle.net options menu.

use tidewater_sim::victory::Outcome;

use crate::campaign as cf;
use crate::core::{Commands, Core, add_layer, remove_layers};
use crate::frontend::{self, AppState, Flow, Front, FrontScreen};
use crate::game::{self, Sim};
use crate::hud::{self, Rect};
use crate::input::{Button, Input, Key, LogCursor};
use crate::layers::{PictureId, Pictures, Role};
use crate::menu::{self, Confirm, EndButton, Kind, Nav, Saving, Screen};
use crate::mouse;
use crate::prefs::{self, Prefs, bit};
use crate::savedlg::{self, SaveUi};
use crate::savefile;
use crate::savegame;
use crate::saves::{self, SaveDir};

/// The stored settings in force.
pub struct Stored(pub Prefs);

/// The open menu. It exists exactly while a menu or dialog is open.
pub struct GameMenu {
    nav: Nav,
    image: PictureId,
    /// The settings as they were when the current settings dialog opened (Cancel puts them back).
    snapshot: Prefs,
    /// The settings the current dialog edits.
    draft: Prefs,
    /// The game speed slider, applied with OK.
    speed: u8,
    /// The fog of war radio, applied with OK.
    fog: bool,
    fog_at_open: bool,
    enhanced: bool,
    /// Text shown by the current screen (wrapped).
    lines: Vec<String>,
    delay: menu::Delay,
    hot: Option<usize>,
    /// The focused button (the yellow ring): its index in the item list; `None` until the screen is next run, which
    /// puts it on the default button ([`default_act`]).
    focus: Option<usize>,
    /// The slider being dragged: its index in the item list.
    drag: Option<usize>,
    dirty: bool,
    /// The open save or load dialog (the `Saving` screen).
    save: Option<SaveUi>,
    /// Something the dialog decided that needs the whole game: done by [`service_saving`].
    request: Option<saves::Step>,
    /// Opened from the victory dialog's Save.
    from_outcome: bool,
}

/// The game menu's input state: the save dialog's input for this frame (collected by [`gather_saving_input`]), the
/// places in the typed events and the wheel steps it has read, and whether the menu development aid has run.
///
/// The game menu's functions run last in the frame (`core::frame_systems`), after the game screen's, so a key that
/// opens or closes a menu is seen by one side only, the same way every frame (an Escape that closes a menu doesn't
/// also reach the game).
#[derive(Default)]
pub struct MenuInput {
    saving: savedlg::Input,
    cursors: (LogCursor, LogCursor),
    demo_done: bool,
}

// ---------------------------------------------------------------------------------------------------------------
// The dialogs' layout

const BOX_X: i32 = 160;
const BOX_W: u32 = 320;
const PAD: i32 = 16;
const ROW_H: u32 = 26;
const ROW_GAP: i32 = 6;
const INNER_W: u32 = BOX_W - 2 * PAD as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Open(Screen),
    CloseAll,
    Back,
    Ok,
    Cancel,
    Do(Confirm),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slide {
    Music,
    Digital,
    Game,
    Mouse,
    Key,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tick {
    Speech,
    Acknowledgements,
    Building,
    Subtitles,
    Colour,
}

impl Tick {
    fn bit(self) -> u32 {
        match self {
            Tick::Speech => bit::UNIT_SPEECH,
            Tick::Acknowledgements => bit::ACKNOWLEDGEMENTS,
            Tick::Building => bit::BUILDING_SOUNDS,
            Tick::Subtitles => bit::SUBTITLES,
            Tick::Colour => bit::COLOUR_CYCLING,
        }
    }
}

/// A radio option: which group, and the value that choosing it sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Radio {
    FirstMouseStyle(bool),
    Fog(bool),
    Terrain(bool),
}

#[derive(Clone, Debug, PartialEq)]
enum Kinds {
    Label(String),
    Lines(Vec<String>),
    Button(Act, String),
    Slider(Slide, String, u8, u8, bool),
    Check(Tick, String, bool),
    Radio(Radio, String, bool),
}

struct Item {
    r: Rect,
    kind: Kinds,
}

struct Built {
    title: &'static str,
    frame: Rect,
    items: Vec<Item>,
}

struct Builder {
    y: i32,
    line_h: i32,
    items: Vec<Item>,
}

impl Builder {
    fn new(line_h: i32) -> Self {
        Self {
            y: 0,
            line_h,
            items: Vec::new(),
        }
    }

    fn row(&mut self, h: u32, kind: Kinds) {
        self.items.push(Item {
            r: Rect::new(BOX_X + PAD, self.y, INNER_W, h),
            kind,
        });
        self.y += h as i32 + ROW_GAP;
    }

    fn button(&mut self, act: Act, label: &str) {
        self.row(ROW_H, Kinds::Button(act, label.to_string()));
    }

    /// Two buttons side by side (OK and Cancel).
    fn pair(&mut self, a: (Act, &str), b: (Act, &str)) {
        let w = (INNER_W - 8) / 2;
        for (i, (act, label)) in [a, b].into_iter().enumerate() {
            self.items.push(Item {
                r: Rect::new(BOX_X + PAD + i as i32 * (w as i32 + 8), self.y, w, ROW_H),
                kind: Kinds::Button(act, label.to_string()),
            });
        }
        self.y += ROW_H as i32 + ROW_GAP;
    }

    fn lines(&mut self, lines: &[String]) {
        let h = (lines.len() as i32 * self.line_h).max(self.line_h) as u32;
        self.row(h, Kinds::Lines(lines.to_vec()));
    }

    fn slider(&mut self, id: Slide, label: &str, value: u8, max: u8, enabled: bool) {
        self.row(34, Kinds::Slider(id, label.to_string(), value, max, enabled));
    }

    fn check(&mut self, id: Tick, label: &str, on: bool) {
        self.row(ROW_H - 6, Kinds::Check(id, label.to_string(), on));
    }

    /// A labelled pair of radio options.
    fn radios(&mut self, label: &str, a: (Radio, &str, bool), b: (Radio, &str, bool)) {
        self.row(self.line_h as u32, Kinds::Label(label.to_string()));
        let w = (INNER_W - 8) / 2;
        for (i, (id, text, on)) in [a, b].into_iter().enumerate() {
            self.items.push(Item {
                r: Rect::new(BOX_X + PAD + i as i32 * (w as i32 + 8), self.y, w, ROW_H - 6),
                kind: Kinds::Radio(id, text.to_string(), on),
            });
        }
        self.y += ROW_H as i32 - 6 + ROW_GAP;
    }

    fn finish(mut self, title: &'static str, title_h: i32) -> Built {
        let top = PAD + title_h + ROW_GAP;
        let h = top + self.y + PAD - ROW_GAP;
        let y0 = ((480 - h) / 2).max(0);
        for it in &mut self.items {
            it.r.y += y0 + top;
        }
        Built {
            title,
            frame: Rect::new(BOX_X, y0, BOX_W, h as u32),
            items: self.items,
        }
    }
}

impl GameMenu {
    fn screen(&self) -> Screen {
        self.nav.current().unwrap_or(Screen::Game)
    }

    fn build(&self, title_h: i32, line_h: i32, kind: Kind) -> Built {
        let mut b = Builder::new(line_h);
        let d = &self.draft;
        let (title, wide) = match self.screen() {
            Screen::Game => {
                b.button(Act::Open(Screen::Saving(Saving::Save)), "Save Game (F11)");
                b.button(Act::Open(Screen::Saving(Saving::Load)), "Load Game (F12)");
                b.button(Act::Open(Screen::Options), "Options (F5)");
                b.button(Act::Open(Screen::Help), "Help (F1)");
                b.button(Act::Open(Screen::Objectives), "Objectives (O)");
                b.button(Act::Open(Screen::End), "End Scenario (E)");
                b.button(Act::CloseAll, "Return to Game (Esc)");
                ("Game Menu", false)
            }
            Screen::Options => {
                b.button(Act::Open(Screen::Sound), "Sound (F7)");
                b.button(Act::Open(Screen::Speed), "Speeds (F8)");
                b.button(Act::Open(Screen::Prefs), "Preferences (F9)");
                b.button(Act::Back, "Previous");
                ("Options", false)
            }
            Screen::Help => {
                b.button(Act::Open(Screen::KeyHelp), "Keystroke Help");
                b.button(Act::Open(Screen::Tips), "Tips");
                b.button(Act::Back, "Previous");
                ("Help", false)
            }
            Screen::Sound => {
                b.slider(
                    Slide::Music,
                    "Music volume",
                    d.music_volume,
                    prefs::MAX_VOLUME,
                    d.has(bit::MUSIC),
                );
                b.slider(
                    Slide::Digital,
                    "Sound effects volume",
                    d.digital_volume,
                    prefs::MAX_VOLUME,
                    true,
                );
                for (t, l) in [
                    (Tick::Speech, "Unit speech"),
                    (Tick::Acknowledgements, "Acknowledgements"),
                    (Tick::Building, "Building sounds"),
                    (Tick::Subtitles, "Subtitles"),
                ] {
                    b.check(t, l, d.has(t.bit()));
                }
                b.pair((Act::Ok, "OK"), (Act::Cancel, "Cancel"));
                ("Sound", false)
            }
            Screen::Speed => {
                b.slider(
                    Slide::Game,
                    "Game speed",
                    self.speed,
                    menu::game_speed_max(self.enhanced),
                    true,
                );
                b.slider(
                    Slide::Mouse,
                    "Mouse scroll speed",
                    d.mouse_scroll,
                    prefs::MAX_SCROLL_SPEED,
                    true,
                );
                b.slider(
                    Slide::Key,
                    "Key scroll speed",
                    d.key_scroll,
                    prefs::MAX_SCROLL_SPEED,
                    true,
                );
                b.pair((Act::Ok, "OK"), (Act::Cancel, "Cancel"));
                ("Speeds", false)
            }
            Screen::Prefs => {
                let first = d.has(bit::FIRST_MOUSE_STYLE);
                b.radios(
                    "Mouse style",
                    (Radio::FirstMouseStyle(true), "Right click commands", first),
                    (Radio::FirstMouseStyle(false), "Right click centres", !first),
                );
                b.radios(
                    "Fog of war",
                    (Radio::Fog(true), "On", self.fog),
                    (Radio::Fog(false), "Off", !self.fog),
                );
                b.radios(
                    "Minimap terrain",
                    (Radio::Terrain(true), "On", d.minimap_terrain),
                    (Radio::Terrain(false), "Off", !d.minimap_terrain),
                );
                b.check(Tick::Colour, "Colour cycling", d.has(bit::COLOUR_CYCLING));
                b.pair((Act::Ok, "OK"), (Act::Cancel, "Cancel"));
                ("Preferences", false)
            }
            Screen::Objectives => {
                b.lines(&self.lines);
                b.button(Act::Back, "Previous");
                ("Objectives", true)
            }
            Screen::End => {
                for e in menu::end_buttons(kind) {
                    let label = match e {
                        EndButton::Restart => "Restart Scenario (Ctrl+R)",
                        EndButton::ReturnToChat => "Return to Chat Room",
                        EndButton::LoadCustom => "Load Another Scenario",
                        EndButton::Surrender => "Surrender (Ctrl+Q)",
                        EndButton::QuitToMenu => "Quit to Main Menu",
                        EndButton::Exit => "Exit Program (Ctrl+X)",
                    };
                    b.button(Act::Open(Screen::Confirm(e.confirm())), label);
                }
                b.button(Act::Back, "Previous");
                ("End Scenario", false)
            }
            Screen::Confirm(c) => {
                b.lines(&self.lines);
                b.pair((Act::Do(c), "OK"), (Act::Back, "Cancel"));
                (
                    match c {
                        Confirm::Restart => "Restart",
                        Confirm::Surrender | Confirm::QuitDraw => "Surrender",
                        Confirm::LoadCustom => "Load Scenario",
                        Confirm::ReturnToChat => "Chat Room",
                        Confirm::QuitToMenu => "Quit",
                        Confirm::Exit => "Exit",
                    },
                    false,
                )
            }
            // The save and load dialogs draw themselves (`savedlg`); this frame is only their place.
            Screen::Saving(_) => ("", false),
            Screen::KeyHelp => {
                b.lines(&self.lines);
                b.button(Act::Back, "Previous");
                ("Keystroke Help", true)
            }
            Screen::Tips => {
                b.lines(&self.lines);
                b.button(Act::Back, "OK");
                ("Tip", false)
            }
        };
        let _ = wide;
        b.finish(title, title_h)
    }
}

/// The texts and numbers a screen shows, set when it opens.
fn enter(m: &mut GameMenu, front: &mut Front, stored: &mut Stored, sim: &Sim, flow: &Flow, folder: &SaveDir) {
    let screen = m.screen();
    let wrap = |front: &Front, text: &str| cf::wrap(text, INNER_W, &|s: &str| front.font.width(s.as_bytes()));
    m.lines.clear();
    match screen {
        Screen::Sound | Screen::Speed | Screen::Prefs => {
            m.snapshot = stored.0.clone();
            m.draft = stored.0.clone();
            m.speed = sim.world.speed.min(menu::game_speed_max(m.enhanced));
            m.fog = sim.world.rules.fog_of_war;
            m.fog_at_open = m.fog;
            m.delay.clear();
        }
        Screen::Objectives => {
            let entry = menu::objectives_entry(flow.game);
            let text = frontend::entry_text(&mut front.install, "rez\\objctivs.tbl", entry);
            m.lines = wrap(front, &text);
        }
        Screen::Tips => {
            let n = stored.0.take_tip();
            let text = frontend::entry_text(&mut front.install, "rez\\tips.tbl", n as u16);
            m.lines = wrap(front, &text);
        }
        Screen::Saving(s) => {
            let mode = if s == Saving::Save {
                saves::Mode::Save
            } else {
                saves::Mode::Load
            };
            let entries = folder.list(savefile::Kind::Single, sim.world.rules.enhanced);
            // Network entry 105 lists the characters a name can't hold.
            let forbidden = front.net(105);
            let forbidden = if forbidden.is_empty() {
                saves::FORBIDDEN.to_string()
            } else {
                forbidden
            };
            m.save = Some(SaveUi::new(saves::Dialog::open(mode, entries, &forbidden)));
        }
        Screen::Confirm(c) => {
            let text = match c {
                Confirm::Restart => "Restart this scenario from the beginning?",
                Confirm::Surrender | Confirm::QuitDraw => "Surrender? You will be defeated.",
                Confirm::LoadCustom => "Leave this game and load another scenario?",
                Confirm::ReturnToChat => "Leave this game and return to the chat room?",
                Confirm::QuitToMenu => "Quit this game and return to the main menu?",
                Confirm::Exit => "Exit the program?",
            };
            m.lines = wrap(front, text);
        }
        Screen::KeyHelp => m.lines = KEY_HELP.iter().map(|s| s.to_string()).collect(),
        Screen::Game | Screen::Options | Screen::Help | Screen::End => {}
    }
    m.dirty = true;
    m.hot = None;
    m.focus = None;
    m.drag = None;
}

/// The button a screen opens focused on, by what it does (screen spec, "Buttons", "Focus", round 74): Return to Game
/// on the game menu, Previous on End Scenario and OK on a confirmation (Quit to Menu's). Ours where the spec saw no
/// capture: OK on the settings dialogs (as Enter confirmed them before), Previous (or OK) on the others.
fn default_act(screen: Screen) -> Option<Act> {
    match screen {
        Screen::Game => Some(Act::CloseAll),
        Screen::Confirm(c) => Some(Act::Do(c)),
        Screen::Sound | Screen::Speed | Screen::Prefs => Some(Act::Ok),
        Screen::Options | Screen::Help | Screen::Objectives | Screen::End | Screen::KeyHelp | Screen::Tips => {
            Some(Act::Back)
        }
        Screen::Saving(_) => None,
    }
}

/// The index of the default button in a screen's items (the first button if none matches).
fn default_focus(built: &Built, screen: Screen) -> Option<usize> {
    let act = default_act(screen);
    let button = |it: &Item| matches!(it.kind, Kinds::Button(..));
    built
        .items
        .iter()
        .position(|it| matches!(it.kind, Kinds::Button(a, _) if Some(a) == act))
        .or_else(|| built.items.iter().position(button))
}

/// The focus ring's colour: menu palette entry 251 (screen spec, "Buttons"). The game dialogs draw in RGB.
const FOCUS_RGB: [u8; 3] = [255, 255, 0];

const KEY_HELP: [&str; 17] = [
    "+ and -   Game speed",
    "Ctrl+P   Pause or resume",
    "F10, Alt+M   Game menu",
    "F1   Help      F5   Options",
    "F7   Sound     F8   Speeds    F9   Preferences",
    "F11   Save     F12   Load",
    "F2-F4   Go to a remembered view",
    "Shift+F2-F4   Remember the view",
    "0-9   Select a group",
    "Ctrl+0-9   Make a group",
    "Alt+C   Centre on the selected unit",
    "Ctrl+S   Sound effects on or off",
    "Ctrl+M   Music on or off",
    "Ctrl+R   Restart the scenario",
    "Ctrl+Q   Quit to the main menu",
    "Ctrl+X   Exit the program",
    "Ctrl+T   Tips",
];

// ---------------------------------------------------------------------------------------------------------------
// Drawing

fn draw(front: &Front, m: &GameMenu, built: &Built) -> crate::scene::Picture {
    let mut p = frontend::blank();
    hud::fill(&mut p, built.frame, [24, 20, 16]);
    hud::outline(&mut p, built.frame, [180, 160, 100]);
    let tw = front.big.width(built.title.as_bytes()) as i32;
    hud::text(
        &mut p,
        &front.big,
        built.frame.x + (built.frame.w as i32 - tw) / 2,
        built.frame.y + PAD,
        built.title,
    );
    let f = &front.font;
    for (i, it) in built.items.iter().enumerate() {
        let hot = m.hot == Some(i);
        match &it.kind {
            Kinds::Label(s) => {
                hud::text(&mut p, f, it.r.x, it.r.y, s);
            }
            Kinds::Lines(ls) => {
                for (n, l) in ls.iter().enumerate() {
                    hud::text(&mut p, f, it.r.x, it.r.y + n as i32 * (f.height as i32 + 4), l);
                }
            }
            Kinds::Button(_, label) => {
                frontend::button(&mut p, f, it.r, label, hot);
                if m.focus == Some(i) {
                    hud::outline(&mut p, it.r, FOCUS_RGB);
                }
            }
            Kinds::Slider(_, label, v, max, enabled) => {
                let text = format!("{label}: {v}");
                hud::text(&mut p, f, it.r.x, it.r.y, &text);
                let track = slider_track(it.r);
                hud::fill(&mut p, track, [12, 10, 8]);
                hud::outline(&mut p, track, if *enabled { [180, 160, 100] } else { [90, 80, 50] });
                if *max > 0 {
                    let x = track.x + (track.w as i32 - 6) * *v as i32 / *max as i32;
                    let knob = Rect::new(x, track.y - 2, 6, track.h + 4);
                    hud::fill(&mut p, knob, if *enabled { [220, 200, 130] } else { [110, 100, 70] });
                }
            }
            Kinds::Check(_, label, on) => {
                let b = Rect::new(it.r.x, it.r.y, 16, 16);
                hud::fill(&mut p, b, if hot { [70, 60, 30] } else { [36, 32, 24] });
                hud::outline(&mut p, b, [180, 160, 100]);
                if *on {
                    hud::fill(&mut p, Rect::new(b.x + 4, b.y + 4, 8, 8), [220, 200, 130]);
                }
                hud::text(&mut p, f, it.r.x + 24, it.r.y + 1, label);
            }
            Kinds::Radio(_, label, on) => {
                let r = it.r;
                hud::fill(
                    &mut p,
                    r,
                    if *on {
                        [90, 76, 36]
                    } else if hot {
                        [70, 60, 30]
                    } else {
                        [36, 32, 24]
                    },
                );
                hud::outline(&mut p, r, if *on { [220, 200, 130] } else { [180, 160, 100] });
                let w = f.width(label.as_bytes()) as i32;
                hud::text(
                    &mut p,
                    f,
                    r.x + (r.w as i32 - w) / 2,
                    r.y + (r.h as i32 - f.height as i32) / 2,
                    label,
                );
            }
        }
    }
    p
}

fn slider_track(r: Rect) -> Rect {
    Rect::new(r.x, r.y + 20, r.w, 10)
}

// ---------------------------------------------------------------------------------------------------------------
// Opening

/// Opens the menu or dialog a key or the menu button asks for.
pub(crate) fn open_requests(c: &mut Core) {
    let Core {
        commands,
        sim,
        panel_ui: ui,
        stored,
        front,
        flow,
        store: folder,
        pictures,
        menu_input: MenuInput { demo_done, .. },
        input,
        ..
    } = c;
    let keys = &input.keys;
    let Some(sim) = sim.as_ref() else { return };
    let mut asked = ui
        .as_mut()
        .is_some_and(|u| std::mem::take(&mut u.menu_requested))
        .then_some(Screen::Game);
    // Development aid: `TIDEWATER_DEMO_MENU=game|options|sound|speed|prefs|objectives|end|restart|tips|save|load` opens that
    // screen once, as its key would, after 50 cycles so the map and panels have been drawn from a running game.
    if sim.world.tick >= 50
        && !std::mem::replace(demo_done, true)
        && let Ok(v) = std::env::var("TIDEWATER_DEMO_MENU")
    {
        asked = match v.as_str() {
            "game" => Some(Screen::Game),
            "options" => Some(Screen::Options),
            "sound" => Some(Screen::Sound),
            "speed" => Some(Screen::Speed),
            "prefs" => Some(Screen::Prefs),
            "objectives" => Some(Screen::Objectives),
            "end" => Some(Screen::End),
            "restart" => Some(Screen::Confirm(Confirm::Restart)),
            "tips" => Some(Screen::Tips),
            "save" => Some(Screen::Saving(Saving::Save)),
            "load" => Some(Screen::Saving(Saving::Load)),
            _ => asked,
        };
    }
    let m = mouse::KeyMods {
        shift: keys.any_pressed([Key::ShiftLeft, Key::ShiftRight]),
        ctrl: keys.any_pressed([Key::ControlLeft, Key::ControlRight]),
        alt: keys.any_pressed([Key::AltLeft, Key::AltRight]),
    };
    for &k in keys.get_just_pressed() {
        let Some(key) = game::game_key(k) else { continue };
        if key == mouse::Key::F(10) {
            asked = Some(Screen::Game);
            continue;
        }
        let Some(a) = mouse::key_action(key, m, false, sim.world.rules.enhanced, false) else {
            continue;
        };
        use mouse::KeyAction as K;
        let s = match a {
            K::GameMenu => Screen::Game,
            K::Options => Screen::Options,
            K::HelpMenu => Screen::Help,
            K::KeyboardHelp => Screen::KeyHelp,
            K::SoundDialog => Screen::Sound,
            K::SpeedDialog => Screen::Speed,
            K::Preferences => Screen::Prefs,
            K::Tips => Screen::Tips,
            K::SaveGame => Screen::Saving(Saving::Save),
            K::LoadGame => Screen::Saving(Saving::Load),
            K::Restart => Screen::Confirm(Confirm::Restart),
            K::Quit => Screen::Confirm(Confirm::QuitToMenu),
            K::QuitDialog => Screen::Confirm(Confirm::Exit),
            K::ToggleSounds => {
                stored.0.toggle_digital();
                continue;
            }
            _ => continue,
        };
        asked = Some(s);
    }
    let Some(screen) = asked else { return };
    open_menu(
        commands,
        screen,
        pictures,
        stored,
        front,
        sim,
        flow,
        &folder.folder(),
        false,
    );
}

/// Opens the menu on `screen`.
#[allow(clippy::too_many_arguments)]
fn open_menu(
    commands: &mut Commands,
    screen: Screen,
    pictures: &mut Pictures,
    stored: &mut Stored,
    front: &mut Front,
    sim: &Sim,
    flow: &Flow,
    folder: &SaveDir,
    from_outcome: bool,
) {
    let mut nav = Nav::default();
    nav.open(screen);
    let image = pictures.add(frontend::blank());
    add_layer(
        commands,
        crate::layers::Layer::new(image, crate::layers::Plane::Flow, 2.0),
        Role::Menu,
        Some(AppState::Game),
    );
    let mut m = GameMenu {
        nav,
        image,
        snapshot: stored.0.clone(),
        draft: stored.0.clone(),
        speed: sim.world.speed,
        fog: sim.world.rules.fog_of_war,
        fog_at_open: sim.world.rules.fog_of_war,
        enhanced: sim.world.rules.enhanced,
        lines: Vec::new(),
        delay: menu::Delay::default(),
        hot: None,
        focus: None,
        drag: None,
        dirty: true,
        save: None,
        request: None,
        from_outcome,
    };
    enter(&mut m, front, stored, sim, flow, folder);
    commands.queue(move |c| c.menu = Some(m));
}

/// The victory dialog's Save opens the save dialog over it.
pub(crate) fn open_from_outcome(c: &mut Core) {
    let Core {
        commands,
        dialog: Some(dialog),
        sim,
        stored,
        front,
        flow,
        store: folder,
        pictures,
        ..
    } = c
    else {
        return;
    };
    let Some(sim) = sim.as_ref() else { return };
    if !dialog.save_wanted {
        return;
    }
    dialog.save_wanted = false;
    // The dialog changed: it draws again when it shows again.
    dialog.redraw = true;
    open_menu(
        commands,
        Screen::Saving(Saving::Save),
        pictures,
        stored,
        front,
        sim,
        flow,
        &folder.folder(),
        true,
    );
}

/// Collects the typed text, keys, pointer and wheel for the save dialog.
pub(crate) fn gather_saving_input(c: &mut Core) {
    let m = &mut c.menu_input;
    m.saving = saving_input(&c.input, &mut m.cursors, &c.front_screen);
}

/// One frame's input for a save or load dialog (also the main menu's load dialog, `frontend`).
/// `cursors`: the places in the typed events and the wheel steps.
pub(crate) fn saving_input(
    input: &Input,
    (typed, wheel): &mut (LogCursor, LogCursor),
    cam: &FrontScreen,
) -> savedlg::Input {
    let (keys, mouse_in) = (&input.keys, &input.mouse);
    let mut i = savedlg::Input {
        pointer: frontend::pointer(input, cam).map(|p| (p.x, p.y)),
        click: mouse_in.just_released(Button::Left),
        enter: keys.any_just_pressed([Key::Enter, Key::NumpadEnter]),
        escape: keys.just_pressed(Key::Escape),
        up: keys.just_pressed(Key::Up),
        down: keys.just_pressed(Key::Down),
        backspace: keys.just_pressed(Key::Backspace),
        ..Default::default()
    };
    for e in input.typed.read(typed) {
        if e.pressed
            && let Some(t) = &e.text
        {
            i.text.extend(t.chars().filter(|c| !c.is_control()));
        }
    }
    for w in input.wheel.read(wheel) {
        i.wheel += w.y.signum() as i32;
    }
    i
}

pub(crate) fn close_on_exit(c: &mut Core) {
    c.commands.queue(|c| c.menu = None);
}

fn close(commands: &mut Commands) {
    remove_layers(commands, |e| e.role == Role::Menu);
    commands.queue(|c| c.menu = None);
}

// ---------------------------------------------------------------------------------------------------------------
// Running

pub(crate) fn run_menu(c: &mut Core) {
    let Core {
        commands,
        menu: Some(m),
        stored,
        sim: Some(sim),
        front,
        flow,
        pictures,
        front_screen: cam,
        store: folder,
        states: next,
        exit,
        input,
        jukebox,
        quitting,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse_in = &input.mouse;
    // The save and load dialogs run in `run_saving`.
    if matches!(m.screen(), Screen::Saving(_)) {
        return;
    }
    let kind = Kind::of(flow.game);
    let title_h = front.big.height as i32;
    let line_h = front.font.height as i32 + 4;
    let built = m.build(title_h, line_h, kind);
    let at = frontend::pointer(input, cam);

    // The delayed application of the sound dialog's changes.
    if m.delay.tick() {
        stored.0 = m.draft.clone();
    }

    // Hover.
    let hot = at.and_then(|p| {
        built
            .items
            .iter()
            .position(|it| !matches!(it.kind, Kinds::Label(_) | Kinds::Lines(_)) && it.r.contains(p.x, p.y))
    });
    if hot != m.hot {
        m.hot = hot;
        m.dirty = true;
    }

    // Screen spec, "Keyboard focus" (every dialog works alike: its open-question default): the screen opens focused
    // on its default button; Tab and Shift+Tab move the focus among the buttons, wrapping, and pressing a button
    // gives it the focus.
    let screen = m.screen();
    let is_button: Vec<bool> = built
        .items
        .iter()
        .map(|it| matches!(it.kind, Kinds::Button(..)))
        .collect();
    let mut focus = m.focus.or_else(|| default_focus(&built, screen));
    if mouse_in.just_pressed(Button::Left)
        && let Some(i) = hot.filter(|&i| is_button[i])
    {
        focus = Some(i);
    }
    if let (Some(k), Some(f)) = (frontend::focus_key(keys), focus) {
        focus = Some(crate::menuscreen::move_focus(&is_button, f, k));
    }
    if focus != m.focus {
        m.focus = focus;
        m.dirty = true;
    }

    // Sliders follow the pointer while the button is held.
    if mouse_in.just_pressed(Button::Left)
        && let Some(i) = hot
        && matches!(built.items[i].kind, Kinds::Slider(..))
    {
        m.drag = Some(i);
    }
    if !mouse_in.pressed(Button::Left) {
        m.drag = None;
    }
    if let (Some(i), Some(p)) = (m.drag, at)
        && let Kinds::Slider(id, _, value, max, true) = built.items[i].kind
    {
        let t = slider_track(built.items[i].r);
        let v = menu::slider_value(p.x, t.x as f32, t.w as f32, max);
        if v != value {
            set_slider(m, id, v);
            changed(m, stored);
        }
    }

    // What was asked.
    let mut act = None;
    if mouse_in.just_released(Button::Left)
        && let Some(i) = hot
    {
        match &built.items[i].kind {
            Kinds::Button(a, _) => act = Some(*a),
            Kinds::Check(id, _, on) => {
                m.draft.set(id.bit(), !on);
                changed(m, stored);
            }
            Kinds::Radio(id, _, _) => {
                match *id {
                    Radio::FirstMouseStyle(v) => m.draft.set(bit::FIRST_MOUSE_STYLE, v),
                    Radio::Fog(v) => m.fog = v,
                    Radio::Terrain(v) => m.draft.minimap_terrain = v,
                }
                changed(m, stored);
            }
            _ => {}
        }
    }
    let settings = matches!(screen, Screen::Sound | Screen::Speed | Screen::Prefs);
    if keys.just_pressed(Key::Escape) {
        act = Some(match screen {
            Screen::Game => Act::CloseAll,
            _ if settings => Act::Cancel,
            _ => Act::Back,
        });
    } else if keys.any_just_pressed([Key::Enter, Key::NumpadEnter]) {
        // Enter activates the focused button, as a click on it does.
        act = match m.focus.map(|i| &built.items[i].kind) {
            Some(Kinds::Button(a, _)) => Some(*a),
            _ => None,
        };
    } else if act.is_none() {
        for &k in keys.get_just_pressed() {
            if let Some(key) = game::game_key(k)
                && let Some(s) = menu::menu_key(screen, key, true)
            {
                act = Some(Act::Open(s));
            }
        }
    }

    let Some(act) = act else {
        if m.dirty {
            m.dirty = false;
            let p = draw(front, m, &built);
            frontend::set_image(pictures, &m.image, p);
        }
        return;
    };
    m.dirty = true;
    match act {
        Act::Open(s) => {
            m.nav.open_child(s);
            enter(m, front, stored, sim, flow, &folder.folder());
        }
        Act::CloseAll => {
            m.nav.close();
            close(commands);
        }
        Act::Back | Act::Cancel => {
            if act == Act::Cancel {
                // Puts back what the dialog changed (the game speed and fog were never applied).
                stored.0 = m.snapshot.clone();
            }
            if m.nav.back() {
                close(commands);
            } else {
                enter(m, front, stored, sim, flow, &folder.folder());
            }
        }
        Act::Ok => {
            if screen == Screen::Speed {
                for faster in menu::speed_steps(sim.world.speed, m.speed) {
                    let player = sim.human;
                    sim.pending.push(tidewater_sim::Command::Speed { player, faster });
                }
                m.draft.game_speed = m.speed;
            }
            if screen == Screen::Prefs && m.fog != m.fog_at_open {
                sim.world.set_fog_of_war(m.fog);
            }
            stored.0 = m.draft.clone();
            m.nav.close();
            close(commands);
        }
        Act::Do(c) => {
            m.nav.close();
            close(commands);
            match c {
                Confirm::Restart => frontend::restart(flow),
                Confirm::Surrender | Confirm::QuitDraw => {
                    // The local player is defeated; the defeat dialog and score screen follow.
                    let h = sim.human as usize;
                    sim.world.players[h].result = Some(Outcome::Defeat);
                    sim.world.victory.over = true;
                    return;
                }
                // Music spec, "Leaving a game for the main menu": the game stops and a fade of all sound runs with the
                // game's picture kept; the main menu follows once it has ended (`frontend::run_quit`). Ours: Load
                // Another Scenario and Return to Chat Room go to the main menu too, the same way.
                Confirm::LoadCustom | Confirm::ReturnToChat | Confirm::QuitToMenu => {
                    jukebox.fade_stop(true, None);
                    *quitting = true;
                    return;
                }
                Confirm::Exit => {
                    *exit = true;
                    return;
                }
            }
            frontend::go_on(flow, front, keys, next, exit);
        }
    }
}

/// Development aid: `TIDEWATER_DEMO_SAVE=name` saves the running game as `name` at cycle 200; from cycle 300,
/// `TIDEWATER_DEMO_LOAD=name` loads that saved game (the dialogs' own paths, without typing).
pub(crate) fn demo_save_load(c: &mut Core) {
    let tick = c.sim.as_ref().map_or(0, |s| s.world.tick);
    if let Ok(name) = std::env::var("TIDEWATER_DEMO_SAVE")
        && tick == 200
    {
        let r = game::save_game(c, &saves::file_stem(&name), &name);
        eprintln!("demo save {name}: {r:?}");
    }
    if let Ok(name) = std::env::var("TIDEWATER_DEMO_LOAD")
        && tick >= 300
    {
        let folder = c.store.folder();
        let Some(sim) = &c.sim else { return };
        let enhanced = sim.world.rules.enhanced;
        match folder.read(
            &SaveDir::file_of(&saves::file_stem(&name), savefile::Kind::Single),
            enhanced,
        ) {
            Ok((header, body)) => {
                savegame::load_game(&mut c.flow, &mut c.states, header, body);
                eprintln!("demo load {name}");
            }
            Err(r) => eprintln!("demo load {name}: {r:?}"),
        }
    }
}

/// One frame of the save or load dialog: its input, drawing, and what it decided.
pub(crate) fn run_saving(c: &mut Core) {
    let Core {
        commands,
        menu: Some(m),
        menu_input: MenuInput { saving: input, .. },
        front,
        pictures,
        store: folder,
        sim: Some(sim),
        ..
    } = c
    else {
        return;
    };
    let Screen::Saving(_) = m.screen() else { return };
    let Some(ui) = m.save.as_mut() else { return };
    let snap = |u: &SaveUi| {
        (
            u.dialog.name.clone(),
            u.dialog.selected,
            u.dialog.stage.clone(),
            u.scroll,
            u.hot,
        )
    };
    let before = snap(ui);
    let step = ui.step(input);
    let mut redraw = before != snap(ui) || m.dirty;
    match step {
        saves::Step::Nothing => {}
        saves::Step::Close => {
            if m.nav.back() {
                close(commands);
                return;
            }
        }
        saves::Step::Delete(e) => {
            // A file that can't be removed simply stays in the list.
            let _ = folder.folder().delete(&e);
            let entries = folder.folder().list(savefile::Kind::Single, sim.world.rules.enhanced);
            if let Some(ui) = m.save.as_mut() {
                ui.dialog.relist(entries);
            }
            redraw = true;
        }
        step @ (saves::Step::Write { .. } | saves::Step::Load(_)) => {
            m.request = Some(step);
        }
    }
    if redraw {
        m.dirty = false;
        if let Some(ui) = &m.save {
            let p = ui.draw(&front.font, &front.big, &|n| front.net(n));
            frontend::set_image(pictures, &m.image, p);
        }
    }
}

/// Carries out a save or load the dialog asked for, at once: both need the whole game.
pub(crate) fn service_saving(c: &mut Core) {
    let Some(req) = c.menu.as_mut().and_then(|m| m.request.take()) else {
        return;
    };
    match req {
        saves::Step::Write { stem, name } => match game::save_game(c, &stem, &name) {
            Ok(()) => finish_save(c),
            Err(reason) => fail(c, reason, &stem),
        },
        saves::Step::Load(entry) => {
            let Some(sim) = &c.sim else { return };
            let enhanced = sim.world.rules.enhanced;
            match c.store.folder().read(&entry.file, enhanced) {
                Ok((header, body)) => {
                    savegame::load_game(&mut c.flow, &mut c.states, header, body);
                    remove_menu(c);
                }
                Err(reason) => fail(c, reason, &entry.stem),
            }
        }
        _ => {}
    }
}

fn remove_menu(c: &mut Core) {
    c.remove_layers_now(|e| e.role == Role::Menu);
    c.menu = None;
}

fn fail(c: &mut Core, reason: savefile::Reason, file: &str) {
    let Some(m) = &mut c.menu else { return };
    if let Some(ui) = m.save.as_mut() {
        ui.dialog.failed(reason, file);
    }
    m.dirty = true;
}

/// A save went through: the menus close; from the victory dialog the game ends as Continue does.
fn finish_save(c: &mut Core) {
    let from_outcome = c.menu.as_ref().is_some_and(|m| m.from_outcome);
    remove_menu(c);
    if from_outcome && let Some(d) = &mut c.dialog {
        d.saved = cf::after_save(true);
        d.redraw = true;
    }
}

fn set_slider(m: &mut GameMenu, id: Slide, v: u8) {
    match id {
        Slide::Music => m.draft.music_volume = v,
        Slide::Digital => m.draft.digital_volume = v,
        Slide::Game => m.speed = v,
        Slide::Mouse => m.draft.mouse_scroll = v,
        Slide::Key => m.draft.key_scroll = v,
    }
}

/// A value in the open dialog changed: the sound dialog waits out its delay, the others apply at once.
fn changed(m: &mut GameMenu, stored: &mut Stored) {
    m.dirty = true;
    if m.screen() == Screen::Sound {
        m.delay.changed();
    } else {
        stored.0 = m.draft.clone();
    }
}

/// Puts the stored settings into force in the game: the scroll speeds, the mouse style and the sound options. Bevy's
/// app put them in only when the settings had changed or the game's state was new; nothing else writes these fields,
/// so putting them in every frame comes to the same.
pub(crate) fn sync_settings(c: &mut Core) {
    let Core {
        stored,
        screen_ui: scr,
        panel_ui: ui,
        speaker,
        jukebox: music,
        ..
    } = c;
    let s = &stored.0;
    if let Some(scr) = scr {
        scr.scroller.mouse_speed = s.mouse_scroll;
        scr.scroller.key_speed = s.key_scroll;
    }
    if let Some(ui) = ui {
        ui.style = if s.has(bit::FIRST_MOUSE_STYLE) {
            mouse::Style::First
        } else {
            mouse::Style::Second
        };
    }
    if let Some(sp) = speaker {
        use crate::sound::option;
        let mask = option::UNIT_SPEECH | option::ACKNOWLEDGEMENTS | option::BUILDING_SOUNDS | option::SUBTITLES;
        sp.director.settings.volume = s.digital_level();
        sp.director.settings.options = s.bits & mask;
    }
    // Music spec, "Volume": the stored volume reaches the music when it changes (OK, the countdown's end, Ctrl+M).
    if music.volume() != s.music_volume {
        music.set_volume(s.music_volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_at(screen: Screen) -> GameMenu {
        let mut nav = Nav::default();
        nav.open(screen);
        GameMenu {
            nav,
            image: crate::layers::Pictures::default().add(frontend::blank()),
            snapshot: Prefs::default(),
            draft: Prefs::default(),
            speed: 4,
            fog: true,
            fog_at_open: true,
            enhanced: true,
            lines: vec!["text".into()],
            delay: menu::Delay::default(),
            hot: None,
            focus: None,
            drag: None,
            dirty: false,
            save: None,
            request: None,
            from_outcome: false,
        }
    }

    /// The label of the button a screen opens focused on.
    fn focused_label(s: Screen) -> Option<String> {
        let b = menu_at(s).build(20, 16, Kind::Campaign);
        let i = default_focus(&b, s)?;
        match &b.items[i].kind {
            Kinds::Button(_, l) => Some(l.clone()),
            _ => None,
        }
    }

    #[test]
    fn dialogs_open_focused_on_their_default_button() {
        // Screen spec, "Buttons", "Focus" (round 74 captures).
        assert_eq!(focused_label(Screen::Game).as_deref(), Some("Return to Game (Esc)"));
        assert_eq!(focused_label(Screen::End).as_deref(), Some("Previous"));
        assert_eq!(
            focused_label(Screen::Confirm(Confirm::QuitToMenu)).as_deref(),
            Some("OK")
        );
        // Ours, where the spec has no capture.
        for s in [Screen::Sound, Screen::Speed, Screen::Prefs] {
            assert_eq!(focused_label(s).as_deref(), Some("OK"), "{s:?}");
        }
        for s in [Screen::Options, Screen::Help, Screen::Objectives, Screen::KeyHelp] {
            assert_eq!(focused_label(s).as_deref(), Some("Previous"), "{s:?}");
        }
        assert_eq!(focused_label(Screen::Tips).as_deref(), Some("OK"));
        assert_eq!(default_act(Screen::Saving(Saving::Load)), None);
    }

    #[test]
    fn tab_moves_the_focus_among_a_dialogs_buttons_only() {
        // The preferences dialog: radios and a check box before OK and Cancel; Tab passes over them.
        let b = menu_at(Screen::Prefs).build(20, 16, Kind::Campaign);
        let is_button: Vec<bool> = b.items.iter().map(|it| matches!(it.kind, Kinds::Button(..))).collect();
        let ok = default_focus(&b, Screen::Prefs).unwrap();
        let next = crate::menuscreen::move_focus(&is_button, ok, crate::menuscreen::FocusKey::Next);
        assert!(matches!(&b.items[next].kind, Kinds::Button(Act::Cancel, _)));
        let wrapped = crate::menuscreen::move_focus(&is_button, next, crate::menuscreen::FocusKey::Next);
        assert_eq!(wrapped, ok, "from the last button back to the first");
    }

    #[test]
    fn dialogs_fit_the_screen_and_buttons_do_not_overlap() {
        for s in [
            Screen::Game,
            Screen::Options,
            Screen::Help,
            Screen::Sound,
            Screen::Speed,
            Screen::Prefs,
            Screen::Objectives,
            Screen::End,
            Screen::Confirm(Confirm::Exit),
            Screen::Saving(Saving::Save),
            Screen::Tips,
        ] {
            let b = menu_at(s).build(20, 16, Kind::Campaign);
            assert!(b.frame.y >= 0 && b.frame.y + b.frame.h as i32 <= 480, "{s:?}");
            for (i, a) in b.items.iter().enumerate() {
                assert!(
                    a.r.y >= b.frame.y && a.r.y + a.r.h as i32 <= b.frame.y + b.frame.h as i32,
                    "{s:?}"
                );
                for c in &b.items[i + 1..] {
                    let apart = a.r.x + a.r.w as i32 <= c.r.x
                        || c.r.x + c.r.w as i32 <= a.r.x
                        || a.r.y + a.r.h as i32 <= c.r.y
                        || c.r.y + c.r.h as i32 <= a.r.y;
                    assert!(apart, "{s:?}: {:?} overlaps {:?}", a.kind, c.kind);
                }
            }
        }
    }

    #[test]
    fn the_end_menu_follows_the_kind_of_game() {
        let n = |k| {
            menu_at(Screen::End)
                .build(20, 16, k)
                .items
                .iter()
                .filter(|i| matches!(i.kind, Kinds::Button(..)))
                .count()
        };
        // Four choices and Previous.
        assert_eq!(n(Kind::Campaign), 5);
        assert_eq!(n(Kind::Custom), 5);
    }

    #[test]
    fn the_game_speed_slider_range_follows_the_rules() {
        let mut m = menu_at(Screen::Speed);
        let max = |m: &GameMenu| {
            m.build(20, 16, Kind::Campaign).items.iter().find_map(|i| match i.kind {
                Kinds::Slider(Slide::Game, _, _, max, _) => Some(max),
                _ => None,
            })
        };
        assert_eq!(max(&m), Some(8));
        m.enhanced = false;
        assert_eq!(max(&m), Some(6));
    }

    #[test]
    fn music_slider_is_disabled_without_the_music_bit() {
        let mut m = menu_at(Screen::Sound);
        m.draft.set(bit::MUSIC, false);
        let enabled = m.build(20, 16, Kind::Campaign).items.iter().find_map(|i| match i.kind {
            Kinds::Slider(Slide::Music, _, _, _, e) => Some(e),
            _ => None,
        });
        assert_eq!(enabled, Some(false));
    }
}
