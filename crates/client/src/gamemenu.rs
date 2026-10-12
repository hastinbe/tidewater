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
//! - Save and Load open a notice, "Saving isn't available yet." and "Loading isn't available yet." (`game/save-load.md`
//!   is a later task). Load stays in the menu: this client has no network session.
//! - Restart rebuilds the mission through the flow (`frontend::restart`). Surrender sets the local player's result to
//!   defeat and closes the menus: the defeat dialog and score screen follow from the outcome dialog as for any defeat.
//! - Quit to menu, Return to chat and Load custom scenario all go to the main menu (the client has no chat room or
//!   custom-game screen yet). Exit ends the program.
//! - The speed dialog sets the game speed with speed commands when OK is pressed (the game is stopped while it is open,
//!   so nothing can tell the difference); the sim's speed starts at Normal whatever speed was stored.
//! - Fog of war changed in the preferences dialog takes effect on OK (the spec: nothing happens while the dialog is
//!   open); Cancel leaves it as it was.
//! - Sound dialog changes are applied 5 frames after the first change (the spec's open question default).
//! - Ctrl+S sets the digital volume to 0 and back (`Prefs::toggle_digital`); Ctrl+M isn't handled here, `music.rs`
//!   owns it (`Prefs::toggle_music` is ready for it).
//!
//! Not yet here:
//! - Reading and writing the settings file (`Prefs::from_text` / `to_text`); the values live for one run.
//! - The tips dialog at the start of a game, and the "play the opening film" and "system pointer" bits.
//! - Consuming the music volume, the music bit and colour cycling: `music.rs` reads [`Stored`]`.0.music_volume`
//!   (TODO there) and `palette.rs` [`crate::prefs::Prefs::colour_cycling`] (TODO there).
//! - The pause greyscale (`palette.rs`), the Enhanced multiplayer pause counts, Battle.net rules and the restart
//!   permission check (no multiplayer).
//! - The dialogs' own art, and the "Network" button of the Battle.net options menu.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use tidewater_sim::victory::Outcome;

use crate::campaign as cf;
use crate::frontend::{self, AppState, FRONT_LAYER, Flow, Front, FrontCamera, Persist};
use crate::hud::{self, Rect};
use crate::menu::{self, Confirm, EndButton, Kind, Nav, Saving, Screen};
use crate::mouse;
use crate::prefs::{self, Prefs, bit};
use crate::{PanelUi, ScreenUi, Sim, Speaker};

/// The stored settings in force.
#[derive(Resource)]
pub struct Stored(pub Prefs);

/// The open menu. It exists exactly while a menu or dialog is open.
#[derive(Resource)]
pub struct GameMenu {
    nav: Nav,
    image: Handle<Image>,
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
    /// The slider being dragged: its index in the item list.
    drag: Option<usize>,
    dirty: bool,
}

#[derive(Component)]
struct MenuLayer;

pub fn plugin(app: &mut App) {
    app.add_systems(OnExit(AppState::Game), close_on_exit).add_systems(
        Update,
        (
            open_requests.run_if(frontend::game_running),
            run_menu.run_if(resource_exists::<GameMenu>),
            sync_settings.run_if(in_state(AppState::Game)),
        )
            .chain(),
    );
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
            Screen::Saving(s) => {
                b.lines(&self.lines);
                b.button(Act::Back, "OK");
                (if s == Saving::Save { "Save Game" } else { "Load Game" }, false)
            }
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
fn enter(m: &mut GameMenu, front: &mut Front, stored: &mut Stored, sim: &Sim, flow: &Flow) {
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
            let text = match s {
                Saving::Save => "Saving isn't available yet.",
                Saving::Load => "Loading isn't available yet.",
            };
            m.lines = wrap(front, text);
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
    m.drag = None;
}

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
            Kinds::Button(_, label) => frontend::button(&mut p, f, it.r, label, hot),
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
#[allow(clippy::too_many_arguments)]
fn open_requests(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    sim: Option<Res<Sim>>,
    mut ui: Option<ResMut<PanelUi>>,
    mut stored: ResMut<Stored>,
    mut front: ResMut<Front>,
    flow: Res<Flow>,
    mut images: ResMut<Assets<Image>>,
    mut demo_done: Local<bool>,
) {
    let Some(sim) = sim else { return };
    let mut asked = ui
        .as_mut()
        .is_some_and(|u| std::mem::take(&mut u.menu_requested))
        .then_some(Screen::Game);
    // Development aid: `TIDEWATER_DEMO_MENU=game|options|sound|speed|prefs|objectives|end|restart|tips` opens that
    // screen once, as its key would.
    if !std::mem::replace(&mut *demo_done, true)
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
            _ => asked,
        };
    }
    let m = mouse::KeyMods {
        shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        ctrl: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
    };
    for &k in keys.get_just_pressed() {
        let Some(key) = crate::game_key(k) else { continue };
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
    let mut nav = Nav::default();
    nav.open(screen);
    let image = images.add(frontend::rgba_image(frontend::blank()));
    commands.spawn((
        Sprite::from_image(image.clone()),
        Transform::from_xyz(0.0, 0.0, 2.0),
        RenderLayers::layer(FRONT_LAYER),
        Persist,
        DespawnOnExit(AppState::Game),
        MenuLayer,
    ));
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
        drag: None,
        dirty: true,
    };
    enter(&mut m, &mut front, &mut stored, &sim, &flow);
    commands.insert_resource(m);
}

fn close_on_exit(mut commands: Commands) {
    commands.remove_resource::<GameMenu>();
}

fn close(commands: &mut Commands, layer: &Query<Entity, With<MenuLayer>>) {
    for e in layer {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<GameMenu>();
}

// ---------------------------------------------------------------------------------------------------------------
// Running

#[allow(clippy::too_many_arguments)]
fn run_menu(
    mut commands: Commands,
    mut m: ResMut<GameMenu>,
    mut stored: ResMut<Stored>,
    mut sim: ResMut<Sim>,
    mut front: ResMut<Front>,
    mut flow: ResMut<Flow>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<FrontCamera>>,
    layer: Query<Entity, With<MenuLayer>>,
    mouse_in: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let kind = Kind::of(flow.game);
    let title_h = front.big.height as i32;
    let line_h = front.font.height as i32 + 4;
    let built = m.build(title_h, line_h, kind);
    let at = frontend::pointer(&window, &cam);

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

    // Sliders follow the pointer while the button is held.
    if mouse_in.just_pressed(MouseButton::Left)
        && let Some(i) = hot
        && matches!(built.items[i].kind, Kinds::Slider(..))
    {
        m.drag = Some(i);
    }
    if !mouse_in.pressed(MouseButton::Left) {
        m.drag = None;
    }
    if let (Some(i), Some(p)) = (m.drag, at)
        && let Kinds::Slider(id, _, value, max, true) = built.items[i].kind
    {
        let t = slider_track(built.items[i].r);
        let v = menu::slider_value(p.x, t.x as f32, t.w as f32, max);
        if v != value {
            set_slider(&mut m, id, v);
            changed(&mut m, &mut stored);
        }
    }

    // What was asked.
    let mut act = None;
    if mouse_in.just_released(MouseButton::Left)
        && let Some(i) = hot
    {
        match &built.items[i].kind {
            Kinds::Button(a, _) => act = Some(*a),
            Kinds::Check(id, _, on) => {
                m.draft.set(id.bit(), !on);
                changed(&mut m, &mut stored);
            }
            Kinds::Radio(id, _, _) => {
                match *id {
                    Radio::FirstMouseStyle(v) => m.draft.set(bit::FIRST_MOUSE_STYLE, v),
                    Radio::Fog(v) => m.fog = v,
                    Radio::Terrain(v) => m.draft.minimap_terrain = v,
                }
                changed(&mut m, &mut stored);
            }
            _ => {}
        }
    }
    let screen = m.screen();
    let settings = matches!(screen, Screen::Sound | Screen::Speed | Screen::Prefs);
    if keys.just_pressed(KeyCode::Escape) {
        act = Some(match screen {
            Screen::Game => Act::CloseAll,
            _ if settings => Act::Cancel,
            _ => Act::Back,
        });
    } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        act = match screen {
            _ if settings => Some(Act::Ok),
            Screen::Confirm(c) => Some(Act::Do(c)),
            Screen::Saving(_) | Screen::KeyHelp | Screen::Tips => Some(Act::Back),
            _ => None,
        };
    } else if act.is_none() {
        for &k in keys.get_just_pressed() {
            if let Some(key) = crate::game_key(k)
                && let Some(s) = menu::menu_key(screen, key, true)
            {
                act = Some(Act::Open(s));
            }
        }
    }

    let Some(act) = act else {
        if m.dirty {
            m.dirty = false;
            let p = draw(&front, &m, &built);
            frontend::set_image(&mut images, &m.image, p);
        }
        return;
    };
    m.dirty = true;
    match act {
        Act::Open(s) => {
            m.nav.open_child(s);
            enter(&mut m, &mut front, &mut stored, &sim, &flow);
        }
        Act::CloseAll => {
            m.nav.close();
            close(&mut commands, &layer);
        }
        Act::Back | Act::Cancel => {
            if act == Act::Cancel {
                // Puts back what the dialog changed (the game speed and fog were never applied).
                stored.0 = m.snapshot.clone();
            }
            if m.nav.back() {
                close(&mut commands, &layer);
            } else {
                enter(&mut m, &mut front, &mut stored, &sim, &flow);
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
            close(&mut commands, &layer);
        }
        Act::Do(c) => {
            m.nav.close();
            close(&mut commands, &layer);
            match c {
                Confirm::Restart => frontend::restart(&mut flow),
                Confirm::Surrender | Confirm::QuitDraw => {
                    // The local player is defeated; the defeat dialog and score screen follow.
                    let h = sim.human as usize;
                    sim.world.players[h].result = Some(Outcome::Defeat);
                    sim.world.victory.over = true;
                    return;
                }
                Confirm::LoadCustom | Confirm::ReturnToChat | Confirm::QuitToMenu => frontend::to_main_menu(&mut flow),
                Confirm::Exit => {
                    exit.write(AppExit::Success);
                    return;
                }
            }
            frontend::go_on(&mut flow, &front, &keys, &mut next, &mut exit);
        }
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

/// Puts the stored settings into force in the game: the scroll speeds, the mouse style and the sound options.
fn sync_settings(
    stored: Res<Stored>,
    scr: Option<ResMut<ScreenUi>>,
    ui: Option<ResMut<PanelUi>>,
    speaker: Option<ResMut<Speaker>>,
) {
    let s = &stored.0;
    if let Some(mut scr) = scr
        && (stored.is_changed() || scr.is_added())
    {
        scr.scroller.mouse_speed = s.mouse_scroll;
        scr.scroller.key_speed = s.key_scroll;
    }
    if let Some(mut ui) = ui
        && (stored.is_changed() || ui.is_added())
    {
        ui.style = if s.has(bit::FIRST_MOUSE_STYLE) {
            mouse::Style::First
        } else {
            mouse::Style::Second
        };
    }
    if let Some(mut sp) = speaker
        && (stored.is_changed() || sp.is_added())
    {
        use crate::sound::option;
        let mask = option::UNIT_SPEECH | option::ACKNOWLEDGEMENTS | option::BUILDING_SOUNDS | option::SUBTITLES;
        sp.director.settings.volume = s.digital_level();
        sp.director.settings.options = s.bits & mask;
    }
    // TODO: music.rs applies `s.music_volume` (and the music bit) to the track; palette.rs reads
    // `s.colour_cycling()`.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_at(screen: Screen) -> GameMenu {
        let mut nav = Nav::default();
        nav.open(screen);
        GameMenu {
            nav,
            image: Handle::default(),
            snapshot: Prefs::default(),
            draft: Prefs::default(),
            speed: 4,
            fog: true,
            fog_at_open: true,
            enhanced: true,
            lines: vec!["text".into()],
            delay: menu::Delay::default(),
            hot: None,
            drag: None,
            dirty: false,
        }
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
