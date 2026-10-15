//! The screens around a game (spec `docs/specs/game/campaign-flow.md`, rules in `campaign`): startup videos, the
//! title, a main menu, the new-campaign choices, act screens, briefings, the victory and defeat dialogs, the score
//! screen, the endings and the credits. The flow is a queue of [`Step`]s; [`advance`] takes the next one and names
//! the [`AppState`] that shows it.
//!
//! Every screen is drawn in software into 640x480 pictures shown by one camera on its own render layer, above the
//! game's cameras. The dialog files (`rez\*.bin`) aren't specified yet, so the layouts of the menu, the dialogs, the
//! briefing and the score screen are our own, and the menu has only what the flow needs (New Campaign, Load Game,
//! Quit; N, L and Q are our keys for them). The main menu and the credits fade in and out (`ui/palette-effects.md`,
//! "Screen fades": 200 ms and 100 ms) like the title and act screens; the new-campaign choices, which the fade table
//! doesn't name, don't fade. The title and the act screens run on the 100 ms real-time menu timer (spec "Terms";
//! `campaign::MenuTimer`, one for every screen), and their fades block it; fades run on real time.
//!
//! Entry points other screens reuse: [`restart`] (the game menu's restart: back to "Starting a mission"),
//! [`end_with`] (a game's end, through the victory or defeat dialog's Continue), and [`to_main_menu`].

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::ScheduleSystem;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use tidewater_formats::fnt::Font;
use tidewater_formats::install::Install;
use tidewater_formats::pcx::Pcx;
use tidewater_formats::smk::{Smacker, YScale};
use tidewater_formats::tbl::StringTable;
use tidewater_sim::setup::Setup;
use tidewater_sim::victory::Outcome;

use crate::audio::{Audio, Group, Mode, Sound, UNITY};
use crate::campaign::{self as cf, Act, CampaignTotal, Choice, GameKind, Memory, Next, Scroll};
use crate::clock::FrameClock;
use crate::hud::{self, Rect};
use crate::input::{Button, Buttons, Key, LogCursor};
use crate::jukebox::Jukebox;
use crate::layers::{Layer as ScreenPlace, PictureId, Plane};
use crate::palette::fades;
use crate::savedlg::SaveUi;
use crate::saves;
use crate::scene::Picture;
use crate::shell::{CoreAudio, CoreTime, PlayerInput, ScreenLayer, ScreenPictures};
use crate::states::Machine;
use crate::video;

const W: u32 = 640;
const H: u32 = 480;

pub use crate::states::AppState;

/// The flow's state machine (`states`): the screen that shows and the change asked for.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct AppStates(pub Machine);

/// A layer or sound that belongs to a screen and goes when the screen is left (`states`, a change's step 3).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedBy(pub AppState);

/// A run condition: `state` shows.
pub fn in_screen(state: AppState) -> impl FnMut(Res<AppStates>) -> bool + Clone {
    move |states: Res<AppStates>| states.get() == state
}

/// One step of the flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Video(u8),
    Title,
    MainMenu,
    NewCampaign,
    /// "Starting a mission": the act screen check, the briefing, the game.
    Mission(u8),
    Act(Act),
    Briefing(u8),
    Play {
        map: String,
        setup: Option<Setup>,
    },
    Score,
    Ending {
        bdp: bool,
        race: u8,
    },
    Finale {
        bdp: bool,
        race: u8,
    },
    Credits {
        bdp: bool,
    },
}

/// The score screen's content.
#[derive(Clone, Debug)]
pub struct ScoreData {
    pub victory: bool,
    pub race: u8,
    pub game: GameKind,
    /// Label, name and statistics of each shown player.
    pub rows: Vec<(String, String, cf::Stats)>,
    pub mission: u32,
    pub total: u32,
    pub rank: String,
}

#[derive(Resource, Default)]
pub struct Flow {
    pub steps: VecDeque<Step>,
    pub current: Option<Step>,
    pub memory: Memory,
    pub total: CampaignTotal,
    /// The game being played, or last played.
    pub game: Option<GameKind>,
    /// The slots' controllers when the game started (for the score screen's rows).
    pub start_controllers: [u8; 8],
    pub score: Option<ScoreData>,
    /// A game the flow asks to start.
    pub pending: Option<(String, Option<Setup>)>,
    /// The skip setting: the act screen, the briefing and the score screen are skipped. The `noglues` developer word
    /// toggles it (`game/cheats.md`, kept in step by `cheats`); it lasts across games.
    pub skip_screens: bool,
    /// The "skip the startup videos" setting (`startup_steps`' `skip_videos`): the `title` developer word toggles it.
    pub skip_videos: bool,
    /// A saved game to put over the game that starts next (`savegame`).
    pub resume: Option<Box<crate::savegame::Resume>>,
    /// The cheat flags that carry into the next game (`cheats`).
    pub cheat_flags: tidewater_sim::cheats::Flags,
}

/// The installation, fonts and tables the screens use, and audio sources by name.
#[derive(Resource)]
pub struct Front {
    pub install: Install,
    pub font: Font,
    pub big: Font,
    pub small: Font,
    pub huge: Font,
    pub network: StringTable,
    /// The normal arrow the core draws outside a game (screen spec, "Pointer shapes"), with its hot spot: the human
    /// one, in the title screen's palette.
    pub arrow: Option<(Picture, (i32, i32))>,
    sources: HashMap<String, Option<Handle<AudioSource>>>,
    /// The same sounds decoded for the core's mixer.
    sounds: HashMap<String, Option<Sound>>,
}

impl Front {
    pub fn load(mut install: Install) -> anyhow::Result<Self> {
        let font = |i: &mut Install, names: &[&str]| -> anyhow::Result<Font> {
            for n in names {
                if let Some(f) = i.read(n).ok().and_then(|d| Font::parse(&d).ok()) {
                    return Ok(f);
                }
            }
            anyhow::bail!("no font among {names:?}")
        };
        let f = font(&mut install, &["Art\\font12x.fnt", "Art\\font10x.fnt"])?;
        let big = font(
            &mut install,
            &["Art\\font32.fnt", "Art\\font12x.fnt", "Art\\font10x.fnt"],
        )?;
        let huge = font(
            &mut install,
            &["Art\\font50.fnt", "Art\\font32.fnt", "Art\\font10x.fnt"],
        )?;
        let small = font(&mut install, &["Art\\font10x.fnt", "Art\\font12x.fnt"])?;
        let network = table(&mut install, "rez\\network.tbl").unwrap_or(StringTable(Vec::new()));
        let arrow = install
            .read(cf::TITLE_PICTURE)
            .ok()
            .and_then(|d| Pcx::parse(&d).ok()?.palette)
            .and_then(|pal| hud::load_pointer(&mut install, crate::screen::Pointer::Arrow.file(false), &pal));
        Ok(Front {
            install,
            font: f,
            big,
            small,
            huge,
            network,
            arrow,
            sources: HashMap::new(),
            sounds: HashMap::new(),
        })
    }

    fn sound(&mut self, assets: &mut Assets<AudioSource>, name: &str) -> Option<Handle<AudioSource>> {
        let install = &mut self.install;
        self.sources
            .entry(name.to_string())
            .or_insert_with(|| {
                let bytes = install.read(name).ok()?;
                Some(assets.add(AudioSource { bytes: bytes.into() }))
            })
            .clone()
    }

    /// A sound decoded for the mixer (`None` if the install lacks it or it isn't PCM WAV).
    pub(crate) fn core_sound(&mut self, name: &str) -> Option<Sound> {
        let install = &mut self.install;
        self.sounds
            .entry(name.to_string())
            .or_insert_with(|| Sound::from_wav(&install.read(name).ok()?))
            .clone()
    }

    pub(crate) fn net(&self, entry: u16) -> String {
        self.network.text(cf::text_index(entry)).unwrap_or_default()
    }
}

fn table(install: &mut Install, name: &str) -> Option<StringTable> {
    StringTable::parse(&install.read(name).ok()?).ok()
}

/// A table entry (1-based), or an empty string.
pub(crate) fn entry_text(install: &mut Install, name: &str, entry: u16) -> String {
    table(install, name)
        .and_then(|t| t.text(cf::text_index(entry)))
        .unwrap_or_default()
}

/// Takes steps until one is shown: the state that shows it, or `None` to quit.
pub fn advance(flow: &mut Flow, install: &Install, ctrl: bool) -> Option<AppState> {
    loop {
        let Some(step) = flow.steps.pop_front() else {
            flow.current = Some(Step::MainMenu);
            return Some(AppState::MainMenu);
        };
        let state = match &step {
            Step::Video(n) => {
                if !install.contains(&cf::video_path(*n)) {
                    continue;
                }
                AppState::Video
            }
            Step::Title => AppState::Title,
            Step::MainMenu => {
                flow.steps.clear();
                AppState::MainMenu
            }
            Step::NewCampaign => AppState::NewCampaign,
            &Step::Mission(e) => {
                let mut then = vec![
                    Step::Briefing(e),
                    Step::Play {
                        map: cf::map_name(e),
                        setup: None,
                    },
                ];
                if !cf::screens_skipped(ctrl, flow.skip_screens)
                    && let Some(a) = flow.memory.act_to_show(e)
                {
                    then.insert(0, Step::Act(a));
                }
                for s in then.into_iter().rev() {
                    flow.steps.push_front(s);
                }
                continue;
            }
            Step::Act(_) | Step::Briefing(_) if cf::screens_skipped(ctrl, flow.skip_screens) => continue,
            Step::Act(_) => AppState::Act,
            Step::Briefing(_) => AppState::Briefing,
            Step::Play { map, setup } => {
                flow.pending = Some((map.clone(), *setup));
                AppState::Game
            }
            Step::Score if flow.skip_screens => {
                if let Some(s) = flow.score.clone() {
                    let then = after_score(flow, &s);
                    flow.steps.push_front(then);
                }
                continue;
            }
            Step::Score => AppState::Score,
            &Step::Ending { bdp, race } => {
                let e = cf::ending(bdp, race);
                for s in [
                    Step::Video(e.video),
                    Step::Finale { bdp, race },
                    Step::Credits { bdp },
                    Step::MainMenu,
                ]
                .into_iter()
                .rev()
                {
                    flow.steps.push_front(s);
                }
                continue;
            }
            Step::Finale { .. } => AppState::Finale,
            Step::Credits { .. } => AppState::Credits,
        };
        flow.current = Some(step);
        return Some(state);
    }
}

/// Goes on to the next step.
pub(crate) fn go_on(
    flow: &mut Flow,
    front: &Front,
    keys: &Buttons<Key>,
    next: &mut Machine,
    exit: &mut MessageWriter<AppExit>,
) {
    let ctrl = keys.any_pressed([Key::ControlLeft, Key::ControlRight]);
    match advance(flow, &front.install, ctrl) {
        Some(s) => next.set(s),
        None => {
            exit.write(AppExit::Success);
        }
    }
}

/// The game menu's restart ("Restart"): the campaign total goes back as on a defeat and the flow goes to "Starting a
/// mission" again, with no score screen. Returns the state to set.
pub fn restart(flow: &mut Flow) {
    flow.total.restart();
    flow.steps.clear();
    match flow.game.map(cf::after_restart) {
        Some(Next::Replay(e)) => flow.steps.push_back(Step::Mission(e)),
        _ => flow.steps.push_back(Step::MainMenu),
    }
}

/// Back to the main menu, dropping the rest of the flow.
pub fn to_main_menu(flow: &mut Flow) {
    flow.steps.clear();
    flow.steps.push_back(Step::MainMenu);
}

/// A game ended with `victory` (or a defeat): closes the mission score, builds the score screen and queues it.
pub fn end_with(flow: &mut Flow, front: &mut Front, world: &tidewater_sim::World, human: u8, victory: bool) {
    let game = flow.game.unwrap_or(GameKind::Custom);
    let bdp = matches!(game, GameKind::Campaign(e) if cf::is_bdp(e));
    let race = world.players[human as usize].race;
    let rows = cf::rows(world, human, &flow.start_controllers, bdp)
        .into_iter()
        .map(|r| (front.net(r.label), front.net(r.name), r.stats))
        .collect();
    let mission = flow.total.close(world.players[human as usize].score, victory);
    let rank_entry = cf::rank_entry(flow.total.total, race, false);
    flow.score = Some(ScoreData {
        victory,
        race,
        game,
        rows,
        mission,
        total: flow.total.total,
        rank: entry_text(&mut front.install, cf::RANK_TABLE, rank_entry),
    });
    flow.steps.clear();
    flow.steps.push_back(Step::Score);
}

/// A synthetic score screen for development (`--goto score-victory:E`).
pub fn synthetic_score(flow: &mut Flow, front: &mut Front, entry: u8, victory: bool) {
    let race = cf::race_of(entry);
    let you = cf::Stats {
        units: 24,
        buildings: 9,
        gold: 12_400,
        lumber: 4_100,
        oil: 0,
        kills: 31,
        razings: 6,
    };
    let enemy = cf::Stats {
        units: 40,
        buildings: 12,
        gold: 9_000,
        lumber: 2_500,
        oil: 300,
        kills: 11,
        razings: 1,
    };
    let bdp = cf::is_bdp(entry);
    flow.game = Some(GameKind::Campaign(entry));
    flow.total = CampaignTotal {
        total: 4000,
        at_start: 4000,
    };
    let mission = flow.total.close(900, victory);
    let rank_entry = cf::rank_entry(flow.total.total, race, false);
    flow.score = Some(ScoreData {
        victory,
        race,
        game: GameKind::Campaign(entry),
        rows: vec![
            (front.net(cf::label::YOU), front.net(cf::name_entry(0, race, bdp)), you),
            (
                front.net(cf::label::ENEMY),
                front.net(cf::name_entry(1, 1 - race, bdp)),
                enemy,
            ),
        ],
        mission,
        total: flow.total.total,
        rank: entry_text(&mut front.install, cf::RANK_TABLE, rank_entry),
    });
}

/// The `--goto` development steps.
pub fn goto_steps(s: &str) -> Result<Vec<Step>, String> {
    let (what, arg) = s.split_once(':').unwrap_or((s, ""));
    let num = || arg.parse::<u8>().map_err(|_| format!("bad number in {s}"));
    let entry = || {
        num().and_then(|e| {
            if e < 52 {
                Ok(e)
            } else {
                Err("entries are 0 to 51".into())
            }
        })
    };
    Ok(match what {
        "title" => vec![Step::Title, Step::MainMenu],
        "menu" => vec![Step::MainMenu],
        "campaign" => vec![Step::NewCampaign],
        "mission" => vec![Step::Mission(entry()?)],
        "act" => {
            let a = cf::act_for(entry()?).ok_or("not the first mission of an act")?;
            vec![Step::Act(a), Step::MainMenu]
        }
        "briefing" => vec![Step::Briefing(entry()?), Step::MainMenu],
        "score-victory" | "score-defeat" => vec![Step::Score],
        "ending" => {
            let e = entry()?;
            vec![Step::Ending {
                bdp: cf::is_bdp(e),
                race: cf::race_of(e),
            }]
        }
        "credits" => vec![Step::Credits { bdp: arg == "bdp" }, Step::MainMenu],
        "video" => vec![Step::Video(num()?), Step::MainMenu],
        _ => return Err(format!("unknown screen {what}")),
    })
}

/// What follows the score screen ("After the score screen"); a defeat puts the campaign total back first.
fn after_score(flow: &mut Flow, s: &ScoreData) -> Step {
    match cf::after_score(s.game, s.victory) {
        Next::Mission(e) => Step::Mission(e),
        Next::Replay(e) => {
            flow.total.defeat();
            Step::Mission(e)
        }
        Next::Ending { bdp, race } => Step::Ending { bdp, race },
        Next::MainMenu | Next::Multiplayer => Step::MainMenu,
    }
}

/// The startup steps ("Startup"): the videos, the title and the main menu. `skip_videos` is the setting the `title`
/// developer word turns on (`game/cheats.md`).
pub fn startup_steps(ctrl: bool, skip_videos: bool, play_intro: bool) -> Vec<Step> {
    let (videos, _cleared) = cf::startup_videos(cf::StartupInput {
        ctrl,
        skip_videos,
        movies: true,
        play_intro,
    });
    let mut v: Vec<Step> = videos.into_iter().map(Step::Video).collect();
    if !ctrl {
        v.push(Step::Title);
    }
    v.push(Step::MainMenu);
    v
}

// ---------------------------------------------------------------------------------------------------------------
// Bevy wiring

/// Entities that outlive the game screen (the front camera, the music).
#[derive(Component)]
pub struct Persist;

/// The flow's screen (`docs/design/client-core.md`, step 2): whether its layers cover what is under them (outside a
/// game, and over a game only where they are drawn), and whether the pointer can be read on it yet. The pointer is
/// read from the end of the program's first frame on, as when a camera gave it.
#[derive(Resource)]
pub struct FrontScreen {
    pub clears: bool,
    ready: bool,
}

impl Default for FrontScreen {
    fn default() -> FrontScreen {
        FrontScreen {
            clears: true,
            ready: false,
        }
    }
}

fn front_screen_ready(mut screen: ResMut<FrontScreen>) {
    if !screen.ready {
        screen.ready = true;
    }
}

/// A front-end picture layer: 0 the background, 1 the overlay drawn over it.
#[derive(Component)]
struct Layer(u8);

/// The victory or defeat dialog over a finished game.
#[derive(Resource)]
pub struct OutcomeDialog {
    pub victory: bool,
    /// Save was pressed: the game menu opens the save dialog over this one (`gamemenu::open_from_outcome`).
    pub(crate) save_wanted: bool,
    /// The game was saved from here: the dialog ends the game as Continue does.
    pub(crate) saved: bool,
    image: PictureId,
}

/// Whether the game screen runs (not while the outcome dialog or, in single player, a game menu is up).
pub fn game_running(
    state: Res<AppStates>,
    dialog: Option<Res<OutcomeDialog>>,
    menu: Option<Res<crate::gamemenu::GameMenu>>,
) -> bool {
    state.get() == AppState::Game && dialog.is_none() && menu.is_none()
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Jukebox>()
        .init_resource::<MenuClock>()
        .init_resource::<LockedPointer>()
        .init_resource::<FrontScreen>()
        .init_resource::<AppStates>()
        .add_systems(PostUpdate, front_screen_ready);
}

/// The fades, run first in the frame (`frame_systems` in `main.rs` has the order).
pub(crate) fn fade_systems() -> ScheduleConfigs<ScheduleSystem> {
    apply_fades.into_configs()
}

/// The outcome dialog, then the check for a game's end.
pub(crate) fn outcome_systems() -> ScheduleConfigs<ScheduleSystem> {
    (
        run_dialog.run_if(resource_exists::<OutcomeDialog>.and_then(not(resource_exists::<crate::gamemenu::GameMenu>))),
        watch_outcome.run_if(in_screen(AppState::Game)),
    )
        .chain()
}

/// The flow's screens, each in its own state: at most one of them runs in a frame.
pub(crate) fn screen_systems() -> ScheduleConfigs<ScheduleSystem> {
    (
        run_scroller.run_if(in_screen(AppState::Finale)),
        run_scroller.run_if(in_screen(AppState::Credits)),
        run_score.run_if(in_screen(AppState::Score)),
        run_briefing.run_if(in_screen(AppState::Briefing)),
        run_act.run_if(in_screen(AppState::Act)),
        run_new_campaign.run_if(in_screen(AppState::NewCampaign)),
        run_menu_load.run_if(in_screen(AppState::MainMenu).and_then(resource_exists::<MenuLoad>)),
        run_menu.run_if(in_screen(AppState::MainMenu).and_then(not(resource_exists::<MenuLoad>))),
        run_title.run_if(in_screen(AppState::Title)),
        play_video.run_if(in_screen(AppState::Video)),
        boot.run_if(in_screen(AppState::Boot)),
    )
        .chain()
}

/// The flow's screen clears to black except over the game screen.
pub(crate) fn camera_clear(state: Res<AppStates>, mut screen: ResMut<FrontScreen>) {
    screen.clears = state.get() != AppState::Game;
}

fn boot(
    mut flow: ResMut<Flow>,
    front: Res<Front>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    go_on(&mut flow, &front, keys, &mut next, &mut exit);
}

fn dup(p: &Picture) -> Picture {
    Picture {
        width: p.width,
        height: p.height,
        rgba: p.rgba.clone(),
    }
}

pub(crate) fn blank() -> Picture {
    Picture {
        width: W,
        height: H,
        rgba: vec![0; (W * H * 4) as usize],
    }
}

/// Spawns a picture as a layer of the screen shown in `state`.
fn spawn_layer(
    commands: &mut Commands,
    pictures: &mut ScreenPictures,
    p: Picture,
    layer: u8,
    state: AppState,
    size: Option<Vec2>,
) -> PictureId {
    let id = pictures.add(p);
    let mut place = ScreenPlace::new(id, Plane::Flow, layer as f32);
    place.size = size.map(|s| (s.x, s.y));
    commands.spawn((ScreenLayer(place), Layer(layer), Persist, OwnedBy(state)));
    id
}

pub(crate) fn set_image(pictures: &mut ScreenPictures, id: &PictureId, p: Picture) {
    pictures.set(*id, p);
}

/// Reads an 8-bit picture with its own palette (or `pal_from`'s) as RGBA; black when missing.
fn picture(install: &mut Install, name: &str, pal_from: Option<&str>) -> Picture {
    let Some(p) = install.read(name).ok().and_then(|d| Pcx::parse(&d).ok()) else {
        return blank();
    };
    let pal = pal_from
        .and_then(|n| install.read(n).ok())
        .and_then(|d| Pcx::parse(&d).ok())
        .and_then(|q| q.palette)
        .or(p.palette.clone());
    let Some(pal) = pal else { return blank() };
    let rgba = p
        .pixels
        .iter()
        .flat_map(|&i| {
            let c = pal.0[i as usize];
            [c[0], c[1], c[2], 255]
        })
        .collect();
    Picture {
        width: p.width as u32,
        height: p.height as u32,
        rgba,
    }
}

/// The left button was pressed, or Esc, Enter or Space.
fn pressed(keys: &Buttons<Key>, mouse: &Buttons<Button>) -> bool {
    mouse.just_pressed(Button::Left) || keys.any_just_pressed([Key::Escape, Key::Enter, Key::NumpadEnter, Key::Space])
}

/// The pointer while the mouse lock (`main.rs`, `mouse_lock`) holds the system's pointer still: its place in logical
/// window points, moved by the mouse's motion. `None` while the lock is off, when the system's pointer is used.
#[derive(Resource, Clone, Copy, Default)]
pub struct LockedPointer(pub Option<Vec2>);

impl LockedPointer {
    /// The pointer in logical window points: the locked one, or the system's.
    pub fn at(&self, window: &Window) -> Option<Vec2> {
        self.0.or_else(|| window.cursor_position())
    }
}

/// The pointer in screen pixels (640x480).
pub(crate) fn pointer(
    window: &Query<&Window, With<PrimaryWindow>>,
    cam: &FrontScreen,
    locked: &LockedPointer,
) -> Option<Vec2> {
    let w = window.single().ok()?;
    let p = locked.at(w)?;
    if !cam.ready {
        return None;
    }
    // The flow's camera looked at the screen's place in the window, at least 640 × 480 across.
    let at = crate::camera_point(
        p,
        crate::letterbox(w).screen(),
        w.scale_factor(),
        1.0,
        Some(Vec2::new(W as f32, H as f32)),
        Vec2::ZERO,
    );
    Some(Vec2::new(at.x + W as f32 / 2.0, H as f32 / 2.0 - at.y))
}

pub(crate) fn centred(dst: &mut Picture, font: &Font, y: i32, s: &str) {
    let w = font.width(s.as_bytes()) as i32;
    hud::text(dst, font, (dst.width as i32 - w) / 2, y, s);
}

fn measure(font: &Font) -> impl Fn(&str) -> u32 + '_ {
    |s: &str| font.width(s.as_bytes())
}

/// A button: an outlined box with its label centred.
pub(crate) fn button(dst: &mut Picture, font: &Font, r: Rect, label: &str, hot: bool) {
    hud::fill(dst, r, if hot { [70, 60, 30] } else { [36, 32, 24] });
    hud::outline(dst, r, [180, 160, 100]);
    let w = font.width(label.as_bytes()) as i32;
    hud::text(
        dst,
        font,
        r.x + (r.w as i32 - w) / 2,
        r.y + (r.h as i32 - font.height as i32) / 2,
        label,
    );
}

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

// ---------------------------------------------------------------------------------------------------------------
// Fades

/// A brightness ramp on a layer: from `from` to `to` over `ms`, starting at `at` (screen time).
#[derive(Component, Clone, Copy)]
struct Fade {
    from: f32,
    to: f32,
    at: f64,
    ms: f64,
}

/// Screen time in ms since the screen opened (real time, as the fades' lengths are).
#[derive(Resource, Default)]
struct Clock(f64);

/// The menu timer (`campaign::MenuTimer`, spec "Terms"), one for every screen, on the real-time clock of [`real_ms`].
#[derive(Resource, Default)]
struct MenuClock(cf::MenuTimer);

/// Real time in ms since the program started: the menu timer's clock.
fn real_ms(time: &FrameClock) -> f64 {
    time.elapsed_secs_f64() * 1000.0
}

fn apply_fades(time: Res<CoreTime>, clock: Option<ResMut<Clock>>, mut q: Query<(&Fade, &mut ScreenLayer)>) {
    let Some(mut clock) = clock else { return };
    clock.0 += time.delta_secs_f64() * 1000.0;
    for (f, mut l) in &mut q {
        let k = ((clock.0 - f.at) / f.ms.max(1.0)).clamp(0.0, 1.0) as f32;
        l.tint = f.from + (f.to - f.from) * k;
    }
}

/// A fade-in over `ms` of every layer of the screen being opened (its layers are spawned by the same commands).
fn fade_in_new_screen(commands: &mut Commands, ms: u32) {
    commands.queue(move |w: &mut World| {
        let mut q = w.query::<(Entity, &Layer)>();
        let es: Vec<Entity> = q.iter(w).map(|(e, _)| e).collect();
        for e in es {
            w.entity_mut(e).insert(Fade {
                from: 0.0,
                to: 1.0,
                at: 0.0,
                ms: ms as f64,
            });
        }
    });
}

/// A fade-out over `ms` of every layer, starting at `at`; returns when it ends.
fn fade_out_all(commands: &mut Commands, q: &Query<(Entity, &Layer)>, at: f64, ms: u32) -> f64 {
    for (e, _) in q {
        commands.entity(e).insert(Fade {
            from: 1.0,
            to: 0.0,
            at,
            ms: ms as f64,
        });
    }
    at + ms as f64
}

fn fade_layer(commands: &mut Commands, q: &Query<(Entity, &Layer)>, layer: u8, from: f32, to: f32, at: f64, ms: u32) {
    for (e, l) in q {
        if l.0 == layer {
            commands.entity(e).insert(Fade {
                from,
                to,
                at,
                ms: ms as f64,
            });
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Music and sounds

/// The game screen opens: the screen's own music closes at once so the game's track can start (`music.rs`, "Our choice
/// for the campaign flow").
pub(crate) fn stop_music(mut music: ResMut<Jukebox>) {
    music.start(None);
}

fn play_once(
    commands: &mut Commands,
    front: &mut Front,
    assets: &mut Assets<AudioSource>,
    audio: &mut Audio,
    name: &str,
    state: AppState,
) {
    if let Some(h) = front.sound(assets, name) {
        commands.spawn((AudioPlayer(h), PlaybackSettings::DESPAWN, OwnedBy(state)));
    }
    if let Some(s) = front.core_sound(name) {
        audio.play(&s, 1.0, UNITY, Mode::Despawn, Group::Screen(state), 0);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Videos

#[derive(Resource)]
struct VideoPlay {
    smk: Smacker,
    image: PictureId,
    width: u32,
    height: u32,
    y: YScale,
    elapsed: f64,
    shown: Option<usize>,
    audio: Option<Mutex<Receiver<Option<Vec<u8>>>>>,
}

pub(crate) fn enter_video(
    mut commands: Commands,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut music: ResMut<Jukebox>,
) {
    let Some(Step::Video(n)) = flow.current else { return };
    music.start(None);
    flow.memory.played(n);
    let data = front.install.read(&cf::video_path(n)).unwrap_or_default();
    // The audio track is gathered from a second decoder on a thread and starts where the picture is.
    let (tx, rx) = channel();
    let copy = data.clone();
    std::thread::spawn(move || {
        let _ = tx.send(video::audio_wav(copy));
    });
    let Ok(mut smk) = Smacker::from_vec(data) else {
        commands.insert_resource(Clock(f64::INFINITY));
        return;
    };
    smk.set_decode_audio(false);
    let (width, height, y) = (smk.width, smk.height, smk.y_scale());
    let shown_h = video::shown_height(height, y);
    let k = (W as f32 / width as f32).min(H as f32 / shown_h as f32);
    let image = spawn_layer(
        &mut commands,
        &mut pictures,
        Picture {
            width,
            height: shown_h,
            rgba: vec![0; (width * shown_h * 4) as usize],
        },
        0,
        AppState::Video,
        Some(Vec2::new(width as f32 * k, shown_h as f32 * k)),
    );
    commands.insert_resource(VideoPlay {
        smk,
        image,
        width,
        height,
        y,
        elapsed: 0.0,
        shown: None,
        audio: Some(Mutex::new(rx)),
    });
}

#[allow(clippy::too_many_arguments)]
fn play_video(
    mut commands: Commands,
    time: Res<CoreTime>,
    play: Option<ResMut<VideoPlay>>,
    mut pictures: ResMut<ScreenPictures>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut audio: ResMut<CoreAudio>,
    mut flow: ResMut<Flow>,
    front: Res<Front>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    script: Option<Res<crate::script::Script>>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let mut done = pressed(keys, mouse);
    if let Some(mut p) = play {
        // The first frame shows at 0 even on a slow first update.
        if p.shown.is_some() {
            p.elapsed += time.game_delta().as_secs_f64() * 1000.0;
        }
        // A scripted run waits for the sound on the first frame, so that it starts on the same frame every time.
        let got = p.audio.as_ref().and_then(|m| {
            let rx = m.lock().ok()?;
            if script.is_some() {
                rx.recv().ok()
            } else {
                rx.try_recv().ok()
            }
        });
        if let Some(wav) = got {
            if let Some(bytes) = wav {
                // The sound starts where the picture is.
                if let Some(s) = Sound::from_wav(&bytes) {
                    let start = (p.elapsed * crate::audio::SAMPLE_RATE as f64 / 1000.0).round() as usize;
                    audio.play(&s, 1.0, UNITY, Mode::Despawn, Group::Screen(AppState::Video), start);
                }
                let h = sources.add(AudioSource { bytes: bytes.into() });
                let mut s = PlaybackSettings::DESPAWN;
                s.start_position = Some(Duration::from_secs_f64(p.elapsed / 1000.0));
                commands.spawn((AudioPlayer(h), s, OwnedBy(AppState::Video)));
            }
            p.audio = None;
        }
        let want = video::frame_at(p.elapsed, p.smk.fps());
        let mut last = None;
        while p.shown.is_none_or(|s| s < want) {
            match p.smk.next_frame() {
                Ok(Some(f)) => {
                    p.shown = Some(f.index);
                    last = Some(f);
                }
                _ => {
                    done = true;
                    break;
                }
            }
        }
        if let Some(f) = last {
            let rgba = video::frame_rgba(&f, p.width, p.height, p.y);
            let pic = Picture {
                width: p.width,
                height: video::shown_height(p.height, p.y),
                rgba,
            };
            set_image(&mut pictures, &p.image, pic);
        }
    } else {
        done = true;
    }
    if done {
        commands.remove_resource::<VideoPlay>();
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The title

#[derive(Resource)]
struct TitleScreen(cf::TitleRun);

pub(crate) fn enter_title(
    mut commands: Commands,
    time: Res<CoreTime>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
) {
    let p = picture(&mut front.install, cf::TITLE_PICTURE, None);
    spawn_layer(&mut commands, &mut pictures, p, 0, AppState::Title, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(TitleScreen(cf::TitleRun::new(real_ms(&time))));
    fade_in_new_screen(&mut commands, cf::TITLE_FADE_MS);
}

/// A key or click ends the title; Shift, Ctrl and Alt alone don't.
fn title_input(keys: &Buttons<Key>, mouse: &Buttons<Button>) -> bool {
    const MODIFIERS: [Key; 6] = [
        Key::ShiftLeft,
        Key::ShiftRight,
        Key::ControlLeft,
        Key::ControlRight,
        Key::AltLeft,
        Key::AltRight,
    ];
    mouse.get_just_pressed().next().is_some() || keys.get_just_pressed().any(|k| !MODIFIERS.contains(k))
}

#[allow(clippy::too_many_arguments)]
fn run_title(
    mut commands: Commands,
    time: Res<CoreTime>,
    clock: Res<Clock>,
    mut menu_timer: ResMut<MenuClock>,
    mut title: ResMut<TitleScreen>,
    layers: Query<(Entity, &Layer)>,
    mut music: ResMut<Jukebox>,
    mut flow: ResMut<Flow>,
    front: Res<Front>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let input = title_input(keys, mouse);
    for cue in title.0.update(&mut menu_timer.0, real_ms(&time), input) {
        match cue {
            cf::TitleCue::Music => music.start(Some(cf::TITLE_MUSIC)),
            cf::TitleCue::FadeOut(ms) => fade_layer(&mut commands, &layers, 0, 1.0, 0.0, clock.0, ms),
            cf::TitleCue::Done => go_on(&mut flow, &front, keys, &mut next, &mut exit),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The main menu and the new-campaign choices

#[derive(Resource)]
struct Menu {
    image: PictureId,
    background: Picture,
    items: Vec<(Rect, String)>,
    hot: Option<usize>,
    drawn: Option<Option<usize>>,
    /// The main menu's fade-out: when it ends, and what was chosen.
    closing: Option<(f64, MenuPick)>,
}

/// What the main menu does once its fade-out ends.
enum MenuPick {
    NewCampaign,
    Load(Box<crate::savegame::Resume>),
    Quit,
}

/// The main menu's load dialog (`loadgamt.bin`, spec `game/save-load.md`, "Load dialog"), open over the menu.
#[derive(Resource)]
struct MenuLoad {
    ui: SaveUi,
    image: PictureId,
    layer: Entity,
}

fn menu_items(labels: &[&str], top: i32) -> Vec<(Rect, String)> {
    labels
        .iter()
        .enumerate()
        .map(|(i, l)| (rect(170, top + i as i32 * 36, 300, 28), l.to_string()))
        .collect()
}

fn open_menu(
    commands: &mut Commands,
    front: &mut Front,
    pictures: &mut ScreenPictures,
    labels: &[&str],
    state: AppState,
) {
    let mut bg = picture(&mut front.install, cf::TITLE_PICTURE, None);
    // Darkened so the buttons read.
    for px in bg.rgba.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            *c /= 3;
        }
    }
    // Below the title picture's logo.
    let top = 300 - labels.len() as i32 * 10;
    let image = spawn_layer(commands, pictures, dup(&bg), 0, state, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(Menu {
        image,
        background: bg,
        items: menu_items(labels, top),
        hot: None,
        drawn: None,
        closing: None,
    });
}

/// Hover, redraw, and the item clicked (or chosen by its number key).
fn menu_input(
    menu: &mut Menu,
    front: &Front,
    pictures: &mut ScreenPictures,
    at: Option<Vec2>,
    mouse: &Buttons<Button>,
    keys: &Buttons<Key>,
    heading: &str,
) -> Option<usize> {
    menu.hot = at.and_then(|p| menu.items.iter().position(|(r, _)| r.contains(p.x, p.y)));
    if menu.drawn != Some(menu.hot) {
        let mut p = dup(&menu.background);
        let top = menu.items.first().map_or(200, |(r, _)| r.y) - 26;
        centred(&mut p, &front.font, top, heading);
        for (i, (r, l)) in menu.items.iter().enumerate() {
            button(&mut p, &front.font, *r, l, menu.hot == Some(i));
        }
        set_image(pictures, &menu.image, p);
        menu.drawn = Some(menu.hot);
    }
    const DIGITS: [Key; 6] = [
        Key::Digit(1),
        Key::Digit(2),
        Key::Digit(3),
        Key::Digit(4),
        Key::Digit(5),
        Key::Digit(6),
    ];
    if let Some(i) = DIGITS.iter().position(|&k| keys.just_pressed(k))
        && i < menu.items.len()
    {
        return Some(i);
    }
    if mouse.just_released(Button::Left) {
        return menu.hot;
    }
    None
}

pub(crate) fn enter_menu(
    mut commands: Commands,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut music: ResMut<Jukebox>,
) {
    music.start_unless_playing(cf::TITLE_MUSIC);
    open_menu(
        &mut commands,
        &mut front,
        &mut pictures,
        &["New Campaign", "Load Game", "Quit"],
        AppState::MainMenu,
    );
    fade_in_new_screen(&mut commands, fades::MENUS);
}

#[allow(clippy::too_many_arguments)]
fn run_menu(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    clock: Res<Clock>,
    layers: Query<(Entity, &Layer)>,
    front: Res<Front>,
    folder: Res<crate::savegame::SaveFolder>,
    mut pictures: ResMut<ScreenPictures>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Res<FrontScreen>,
    locked: Res<LockedPointer>,
    mut flow: ResMut<Flow>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    // Palette-effects spec, "Screen fades": nothing else runs during a fade.
    if let Some((end, _)) = &menu.closing {
        if clock.0 < *end {
            return;
        }
        let Some((_, pick)) = menu.closing.take() else { return };
        match pick {
            MenuPick::NewCampaign => {
                flow.steps = VecDeque::from([Step::NewCampaign]);
                go_on(&mut flow, &front, keys, &mut next, &mut exit);
            }
            MenuPick::Load(r) => crate::savegame::load_game(&mut flow, &mut next, r.header, r.body),
            MenuPick::Quit => {
                exit.write(AppExit::Success);
            }
        }
        return;
    }
    let at = pointer(&window, &cam, &locked);
    // The menu is drawn before its fade-in, but takes no input until the fade ends.
    let pick = menu_input(&mut menu, &front, &mut pictures, at, mouse, keys, "Tidewater");
    if clock.0 < fades::MENUS as f64 {
        return;
    }
    let pick = pick
        .or(keys.just_pressed(Key::Letter('N')).then_some(0))
        .or(keys.just_pressed(Key::Letter('L')).then_some(1))
        .or(keys.just_pressed(Key::Letter('Q')).then_some(2));
    let close = |commands: &mut Commands, menu: &mut Menu, pick: MenuPick| {
        let end = fade_out_all(commands, &layers, clock.0, fades::MENUS);
        menu.closing = Some((end, pick));
    };
    match pick {
        Some(0) => close(&mut commands, &mut menu, MenuPick::NewCampaign),
        Some(1) => {
            // Loading from the main menu is single player, and every single-player game is Enhanced.
            let entries = folder.0.list(crate::savefile::Kind::Single, true);
            let ui = SaveUi::new(saves::Dialog::open(saves::Mode::Load, entries, saves::FORBIDDEN));
            let pic = ui.draw(&front.font, &front.big, &|n| front.net(n));
            let image = pictures.add(pic);
            let layer = commands
                .spawn((
                    ScreenLayer(ScreenPlace::new(image, Plane::Flow, 1.0)),
                    Layer(1),
                    Persist,
                    OwnedBy(AppState::MainMenu),
                ))
                .id();
            commands.insert_resource(MenuLoad { ui, image, layer });
        }
        Some(_) => close(&mut commands, &mut menu, MenuPick::Quit),
        None => {}
    }
}

/// One frame of the main menu's load dialog: Load reads the chosen file and, if it is valid, the menu fades out and the
/// saved game starts; a file that isn't valid shows the reason (`saves::Dialog::failed`); Cancel closes the dialog.
#[allow(clippy::too_many_arguments)]
fn run_menu_load(
    mut commands: Commands,
    mut load: ResMut<MenuLoad>,
    mut menu: ResMut<Menu>,
    clock: Res<Clock>,
    layers: Query<(Entity, &Layer)>,
    front: Res<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut cursors: Local<(LogCursor, LogCursor)>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Res<FrontScreen>,
    locked: Res<LockedPointer>,
    input: Res<PlayerInput>,
) {
    let input = crate::gamemenu::saving_input(&input, &mut cursors, &window, &cam, &locked);
    let load = &mut *load;
    let snap = |u: &SaveUi| (u.dialog.selected, u.dialog.stage.clone(), u.scroll, u.hot);
    let before = snap(&load.ui);
    let step = load.ui.step(&input);
    let mut redraw = before != snap(&load.ui);
    match step {
        saves::Step::Close => {
            commands.entity(load.layer).despawn();
            commands.remove_resource::<MenuLoad>();
            return;
        }
        saves::Step::Load(entry) => match saves::SaveDir::read(&entry.path, true) {
            Ok((header, body)) => {
                commands.remove_resource::<MenuLoad>();
                let end = fade_out_all(&mut commands, &layers, clock.0, fades::MENUS);
                menu.closing = Some((end, MenuPick::Load(Box::new(crate::savegame::Resume { header, body }))));
            }
            Err(reason) => {
                load.ui.dialog.failed(reason, &entry.stem);
                redraw = true;
            }
        },
        _ => {}
    }
    if redraw {
        let p = load.ui.draw(&front.font, &front.big, &|n| front.net(n));
        set_image(&mut pictures, &load.image, p);
    }
}

pub(crate) fn enter_new_campaign(
    mut commands: Commands,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
) {
    let mut labels: Vec<&str> = Choice::ALL.iter().map(|c| c.label()).collect();
    labels.push("Cancel");
    open_menu(&mut commands, &mut front, &mut pictures, &labels, AppState::NewCampaign);
}

#[allow(clippy::too_many_arguments)]
fn run_new_campaign(
    mut menu: ResMut<Menu>,
    front: Res<Front>,
    mut pictures: ResMut<ScreenPictures>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Res<FrontScreen>,
    locked: Res<LockedPointer>,
    mut flow: ResMut<Flow>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let at = pointer(&window, &cam, &locked);
    let pick = menu_input(&mut menu, &front, &mut pictures, at, mouse, keys, "Choose a campaign");
    let ctrl = keys.any_pressed([Key::ControlLeft, Key::ControlRight]);
    let steps = match pick {
        Some(i) if i < 4 => {
            let c = Choice::ALL[i];
            flow.game = None;
            let mut v = VecDeque::new();
            if c.bdp() {
                // Open question default: the movie archive is always available (a missing video is skipped).
                let movies = true;
                match cf::bdp_intro(movies, ctrl) {
                    Err(()) => v.push_back(Step::MainMenu),
                    Ok(video) => v.extend(video.map(Step::Video)),
                }
            }
            v.push_back(Step::Mission(c.first_entry()));
            Some(v)
        }
        Some(_) => Some(VecDeque::from([Step::MainMenu])),
        None if keys.just_pressed(Key::Escape) => Some(VecDeque::from([Step::MainMenu])),
        None => None,
    };
    if let Some(s) = steps {
        flow.steps = s;
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Act screens

#[derive(Resource)]
struct ActScreen {
    act: Act,
    run: cf::ActRun,
}

/// The act sound, stopped by the close's fade of all sound if it is still playing.
#[derive(Component)]
struct ActSound;

pub(crate) fn enter_act(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
) {
    let Some(Step::Act(a)) = flow.current else { return };
    let pal = cf::art(cf::act_palette_picture(&a));
    let p = picture(&mut front.install, &cf::art(a.picture), Some(&pal));
    spawn_layer(&mut commands, &mut pictures, p, 0, AppState::Act, None);
    // The act's two lines, white; our layout: centred near the top.
    let acts = table(&mut front.install, "rez\\acts.tbl");
    let line = |n: u16| {
        acts.as_ref()
            .and_then(|t| t.text(cf::text_index(n)))
            .unwrap_or_default()
    };
    let mut text = Picture {
        width: W,
        height: H,
        rgba: vec![0; (W * H * 4) as usize],
    };
    centred(&mut text, &front.big, 40, &line(a.lines.0));
    let title = line(a.lines.1);
    let f = if front.huge.width(title.as_bytes()) < W - 40 {
        &front.huge
    } else {
        &front.big
    };
    centred(&mut text, f, 90, &title);
    spawn_layer(&mut commands, &mut pictures, text, 1, AppState::Act, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(ActScreen {
        act: a,
        run: cf::ActRun::default(),
    });
    // Black until the first tick fades the text in.
    commands.queue(|w: &mut World| {
        let mut q = w.query::<(Entity, &Layer)>();
        let es: Vec<Entity> = q.iter(w).map(|(e, _)| e).collect();
        for e in es {
            w.entity_mut(e).insert(Fade {
                from: 0.0,
                to: 0.0,
                at: 0.0,
                ms: 1.0,
            });
        }
    });
}

/// The act screen on the menu timer (`campaign::ActRun`): its fades, the act sound when the picture's fade ends, and
/// the close: a fade to black, then a fade of all sound that stops the act sound if it still plays; the video after
/// (if any) or the briefing follows once the music's fade has ended.
#[allow(clippy::too_many_arguments)]
fn run_act(
    mut commands: Commands,
    time: Res<CoreTime>,
    clock: Res<Clock>,
    mut menu_timer: ResMut<MenuClock>,
    mut act: ResMut<ActScreen>,
    layers: Query<(Entity, &Layer)>,
    sounds: Query<Entity, With<ActSound>>,
    mut music: ResMut<Jukebox>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut audio: ResMut<CoreAudio>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let act = &mut *act;
    let input = pressed(keys, mouse);
    for cue in act.run.update(&mut menu_timer.0, real_ms(&time), input) {
        match cue {
            cf::ActCue::TextFadeIn => fade_layer(&mut commands, &layers, 1, 0.0, 1.0, clock.0, cf::ACT_TEXT_FADE_MS),
            cf::ActCue::PictureFadeIn => {
                fade_layer(&mut commands, &layers, 0, 0.0, 1.0, clock.0, cf::ACT_PICTURE_FADE_MS)
            }
            cf::ActCue::ActSound => {
                let name = cf::act_sound(cf::race_of(act.act.entry));
                if let Some(s) = front.core_sound(name) {
                    audio.play(&s, 1.0, UNITY, Mode::Despawn, Group::Act, 0);
                }
                if let Some(h) = front.sound(&mut sources, name) {
                    commands.spawn((
                        AudioPlayer(h),
                        PlaybackSettings::DESPAWN,
                        ActSound,
                        OwnedBy(AppState::Act),
                    ));
                }
            }
            cf::ActCue::FadeToBlack => {
                // The fades block, so every layer that shows is fully in: the text, and the picture from 50 on.
                fade_layer(&mut commands, &layers, 1, 1.0, 0.0, clock.0, cf::ACT_CLOSE_FADE_MS);
                if act.run.timer.picture_shown() {
                    fade_layer(&mut commands, &layers, 0, 1.0, 0.0, clock.0, cf::ACT_CLOSE_FADE_MS);
                }
            }
            cf::ActCue::FadeAllSound => {
                for e in &sounds {
                    commands.entity(e).despawn();
                }
                audio.stop_group(Group::Act);
                music.fade_stop(true, None);
            }
        }
    }
    if act.run.done() && !music.music().fading() {
        // The window close is the only quit request here (open question default).
        if let Some(v) = flow.memory.video_after(&act.act, false) {
            flow.steps.push_front(Step::Video(v));
        }
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Briefings

#[derive(Resource)]
struct Briefing {
    image: PictureId,
    background: Picture,
    lines: Vec<String>,
    panel: Rect,
    line_h: u32,
    scroll: Scroll,
    narration_ms: u32,
    /// Speech files still to start, with their start times.
    voice: VecDeque<(u32, String)>,
    drawn: Option<u32>,
}

/// Our layout: the title across the top, the objectives on the left, the story scrolling on the right.
const BRIEF_PANEL: Rect = Rect {
    x: 330,
    y: 70,
    w: 290,
    h: 380,
};
const BRIEF_OBJECTIVES: Rect = Rect {
    x: 20,
    y: 70,
    w: 290,
    h: 380,
};

pub(crate) fn enter_briefing(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut music: ResMut<Jukebox>,
) {
    let Some(Step::Briefing(e)) = flow.current else { return };
    let race = cf::race_of(e);
    let mut bg = picture(&mut front.install, cf::briefing_background(e), None);
    let (title_n, objectives_n) = cf::objective_entries(e);
    let title = entry_text(&mut front.install, cf::OBJECTIVES, title_n);
    let objectives = entry_text(&mut front.install, cf::OBJECTIVES, objectives_n);
    let story = entry_text(&mut front.install, &cf::story_table(e), 1);
    // Shaded panels so the text reads over the picture.
    for r in [BRIEF_PANEL, BRIEF_OBJECTIVES] {
        shade(&mut bg, r);
    }
    centred(&mut bg, &front.font, 24, &title);
    let line_h = front.font.height as u32 + 2;
    let mut y = BRIEF_OBJECTIVES.y + 8;
    for l in cf::wrap(&objectives, BRIEF_OBJECTIVES.w - 16, &measure(&front.font)) {
        hud::text(&mut bg, &front.font, BRIEF_OBJECTIVES.x + 8, y, &l);
        y += line_h as i32;
    }
    let lines = cf::wrap(&story, BRIEF_PANEL.w - 16, &measure(&front.font));
    // The narration's parts and their lengths.
    let files = cf::narration_files(e);
    let lens: Vec<u32> = files
        .iter()
        .map(|f| {
            front
                .install
                .read(f)
                .ok()
                .and_then(|d| video::wav_length_ms(&d))
                .unwrap_or(0)
        })
        .collect();
    let total = cf::narration_total_ms(&lens);
    let starts = cf::narration_starts(&lens);
    let text_h = lines.len() as u32 * line_h;
    let image = spawn_layer(&mut commands, &mut pictures, dup(&bg), 0, AppState::Briefing, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(Briefing {
        image,
        background: bg,
        lines,
        panel: BRIEF_PANEL,
        line_h,
        scroll: Scroll::new(total, text_h, BRIEF_PANEL.h, line_h),
        narration_ms: total,
        voice: starts.into_iter().zip(files).collect(),
        drawn: None,
    });
    music.start_unless_playing(cf::briefing_music(race));
}

/// Darkens a rectangle.
fn shade(p: &mut Picture, r: Rect) {
    for y in r.y.max(0)..(r.y + r.h as i32).min(p.height as i32) {
        for x in r.x.max(0)..(r.x + r.w as i32).min(p.width as i32) {
            let d = ((y as u32 * p.width + x as u32) * 4) as usize;
            for c in &mut p.rgba[d..d + 3] {
                *c /= 3;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_briefing(
    mut commands: Commands,
    clock: Res<Clock>,
    mut b: ResMut<Briefing>,
    mut pictures: ResMut<ScreenPictures>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut audio: ResMut<CoreAudio>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let now = clock.0;
    while b.voice.front().is_some_and(|(at, _)| (*at as f64) <= now) {
        let (_, f) = b.voice.pop_front().unwrap();
        play_once(
            &mut commands,
            &mut front,
            &mut sources,
            &mut audio,
            &f,
            AppState::Briefing,
        );
    }
    b.scroll.at(now as u64);
    if b.drawn != Some(b.scroll.moved) {
        let mut p = dup(&b.background);
        let mut clip = blank_sized(b.panel.w, b.panel.h);
        let top = b.panel.h as i32 - b.scroll.moved as i32;
        for (i, l) in b.lines.iter().enumerate() {
            let y = top + (i as u32 * b.line_h) as i32;
            if y > -(b.line_h as i32) && y < b.panel.h as i32 {
                hud::text(&mut clip, &front.font, 8, y, l);
            }
        }
        overlay(&mut p, &clip, b.panel.x, b.panel.y);
        set_image(&mut pictures, &b.image, p);
        b.drawn = Some(b.scroll.moved);
    }
    if pressed(keys, mouse) || cf::briefing_closes(&b.scroll, now as u64, b.narration_ms) {
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
    }
}

fn blank_sized(w: u32, h: u32) -> Picture {
    Picture {
        width: w,
        height: h,
        rgba: vec![0; (w * h * 4) as usize],
    }
}

/// Draws the opaque pixels of `src` onto `dst`.
fn overlay(dst: &mut Picture, src: &Picture, x: i32, y: i32) {
    for sy in 0..src.height as i32 {
        for sx in 0..src.width as i32 {
            let s = ((sy as u32 * src.width + sx as u32) * 4) as usize;
            let (dx, dy) = (x + sx, y + sy);
            if src.rgba[s + 3] == 0 || dx < 0 || dy < 0 || dx >= dst.width as i32 || dy >= dst.height as i32 {
                continue;
            }
            let d = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
            dst.rgba[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The victory and defeat dialogs

/// Opens the dialog when the local player's result is in.
fn watch_outcome(
    mut commands: Commands,
    sim: Option<Res<crate::Sim>>,
    dialog: Option<Res<OutcomeDialog>>,
    mut pictures: ResMut<ScreenPictures>,
    mut screen: ResMut<FrontScreen>,
    mut music: ResMut<Jukebox>,
) {
    let (Some(sim), None) = (sim, dialog) else { return };
    let Some(o) = sim.world.players[sim.human as usize].result else {
        return;
    };
    // Music spec: a game that ends fades all sound before the score screen.
    music.fade_stop(true, None);
    screen.clears = false;
    let image = spawn_layer(&mut commands, &mut pictures, blank(), 1, AppState::Game, None);
    commands.insert_resource(OutcomeDialog {
        victory: o == Outcome::Victory,
        save_wanted: false,
        saved: false,
        image,
    });
}

/// Our layout: a box in the middle of the screen.
const DIALOG: Rect = Rect {
    x: 200,
    y: 160,
    w: 240,
    h: 140,
};
const CONTINUE: Rect = Rect {
    x: 230,
    y: 250,
    w: 180,
    h: 26,
};
const SAVE: Rect = Rect {
    x: 230,
    y: 216,
    w: 180,
    h: 26,
};

fn draw_dialog(front: &Front, d: &OutcomeDialog) -> Picture {
    let mut p = blank();
    hud::fill(&mut p, DIALOG, [24, 20, 16]);
    hud::outline(&mut p, DIALOG, [180, 160, 100]);
    let head = if d.victory { "Victory!" } else { "Defeat!" };
    let w = front.big.width(head.as_bytes()) as i32;
    hud::text(
        &mut p,
        &front.big,
        DIALOG.x + (DIALOG.w as i32 - w) / 2,
        DIALOG.y + 10,
        head,
    );
    if d.victory {
        button(&mut p, &front.font, SAVE, "Save", false);
    }
    button(&mut p, &front.font, CONTINUE, "Continue", false);
    p
}

#[allow(clippy::too_many_arguments)]
fn run_dialog(
    mut dialog: ResMut<OutcomeDialog>,
    sim: Option<Res<crate::Sim>>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Res<FrontScreen>,
    locked: Res<LockedPointer>,
    mut flow: ResMut<Flow>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    if dialog.is_added() || dialog.is_changed() {
        let p = draw_dialog(&front, &dialog);
        set_image(&mut pictures, &dialog.image, p);
    }
    let at = pointer(&window, &cam, &locked).filter(|_| mouse.just_released(Button::Left));
    let hit = |r: Rect| at.is_some_and(|p| r.contains(p.x, p.y));
    let button = if dialog.saved || hit(CONTINUE) || keys.any_just_pressed([Key::Enter, Key::Letter('C')]) {
        Some(cf::DialogButton::Continue)
    } else if dialog.victory && (hit(SAVE) || keys.just_pressed(Key::Letter('S'))) {
        Some(cf::DialogButton::Save)
    } else {
        None
    };
    match button.map(cf::dialog_button) {
        Some(cf::DialogResult::End) => {
            let Some(sim) = sim else { return };
            end_with(&mut flow, &mut front, &sim.world, sim.human, dialog.victory);
            go_on(&mut flow, &front, keys, &mut next, &mut exit);
        }
        // The save dialog opens over this one; a save ends the game (`saved`), Cancel shows this dialog again.
        Some(cf::DialogResult::OpenSave) => dialog.save_wanted = true,
        None => {}
    }
}

pub(crate) fn close_dialog(mut commands: Commands) {
    commands.remove_resource::<OutcomeDialog>();
}

pub(crate) fn close_menu_load(mut commands: Commands) {
    commands.remove_resource::<MenuLoad>();
}

// ---------------------------------------------------------------------------------------------------------------
// The score screen

#[derive(Resource)]
struct ScoreScreen {
    image: PictureId,
    background: Picture,
    /// Ticks run so far.
    ticks: u32,
    next_tick: f64,
    bars_done_at: Option<u32>,
    finished: bool,
    drawn: Option<u32>,
}

/// Bars then the score count, one step a tick ("The score screen").
const SCORE_TICK_MS: f64 = 40.0;

pub(crate) fn enter_score(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut music: ResMut<Jukebox>,
) {
    let Some(s) = flow.score.clone() else { return };
    let bg = picture(&mut front.install, cf::score_background(s.race, s.victory), None);
    let image = spawn_layer(&mut commands, &mut pictures, dup(&bg), 0, AppState::Score, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(ScoreScreen {
        image,
        background: bg,
        ticks: 0,
        next_tick: 0.0,
        bars_done_at: None,
        finished: false,
        drawn: None,
    });
    music.start(Some(cf::score_music(s.race, s.victory)));
}

/// Draws the score screen after `ticks` ticks; returns whether the bars and the score have finished.
fn draw_score(front: &Front, s: &ScoreData, bg: &Picture, ticks: u32, bars_done_at: Option<u32>) -> (Picture, bool) {
    let mut p = dup(bg);
    let font = &front.font;
    let head = if s.victory { "Victory!" } else { "Defeat!" };
    centred(&mut p, &front.big, 12, head);
    // The bars: each statistic grows by its step toward its value.
    let maxes: Vec<u32> = (0..7)
        .map(|k| s.rows.iter().map(|r| r.2.values()[k]).max().unwrap_or(0))
        .collect();
    let mut bars_done = true;
    let x0 = 196;
    let col_w = 60;
    let mut y = 70;
    for (k, name) in cf::Stats::NAMES.iter().enumerate() {
        hud::text(&mut p, &front.small, x0 + k as i32 * col_w, y, name);
    }
    y += 24;
    for (label, name, stats) in &s.rows {
        shade(&mut p, rect(16, y - 4, 608, 48));
        hud::text(&mut p, font, 24, y, label);
        hud::text(&mut p, &front.small, 24, y + 18, name);
        for (k, v) in stats.values().iter().enumerate() {
            let shown = cf::counted(*v, cf::bar_step(maxes[k]), ticks);
            bars_done &= shown == *v;
            let x = x0 + k as i32 * col_w;
            let len = if maxes[k] == 0 {
                0
            } else {
                (shown as u64 * (col_w as u64 - 8) / maxes[k] as u64) as u32
            };
            hud::fill(&mut p, rect(x, y + 20, len.max(1), 8), [200, 170, 60]);
            hud::text(&mut p, &front.small, x, y, &shown.to_string());
        }
        y += 56;
    }
    // Then the score counts up.
    let mut done = false;
    if let Some(at) = bars_done_at {
        let n = ticks - at;
        let shown_total = cf::counted(s.total, cf::score_step(s.total), n);
        hud::text(&mut p, font, 24, 400, &format!("Score: {shown_total}"));
        hud::text(&mut p, font, 24, 420, &format!("This mission: {}", s.mission));
        if shown_total == s.total {
            done = true;
            hud::text(&mut p, font, 24, 440, &format!("Rank: {}", s.rank));
        }
    }
    (p, bars_done && done)
}

#[allow(clippy::too_many_arguments)]
fn run_score(
    mut commands: Commands,
    clock: Res<Clock>,
    mut sc: ResMut<ScoreScreen>,
    mut pictures: ResMut<ScreenPictures>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut audio: ResMut<CoreAudio>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let Some(s) = flow.score.clone() else {
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
        return;
    };
    let press = pressed(keys, mouse);
    if press && sc.finished {
        let next_step = after_score(&mut flow, &s);
        flow.steps = VecDeque::from([next_step]);
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
        return;
    }
    if press {
        // A press before the end shows the end.
        sc.ticks = 1_000_000;
        sc.bars_done_at = Some(0);
    }
    while clock.0 >= sc.next_tick && !sc.finished {
        sc.next_tick += SCORE_TICK_MS;
        sc.ticks += 1;
        // Once the bars have grown, the score counts on from the next tick.
        if sc.bars_done_at.is_none() && bars_done(&s, sc.ticks) {
            sc.bars_done_at = Some(sc.ticks);
            play_once(
                &mut commands,
                &mut front,
                &mut sources,
                &mut audio,
                cf::NUMBERS_SOUND,
                AppState::Score,
            );
        }
    }
    if sc.drawn != Some(sc.ticks) {
        let (p, finished) = draw_score(&front, &s, &sc.background, sc.ticks, sc.bars_done_at);
        if finished && !sc.finished {
            sc.finished = true;
            play_once(
                &mut commands,
                &mut front,
                &mut sources,
                &mut audio,
                cf::RANK_SOUND,
                AppState::Score,
            );
        }
        set_image(&mut pictures, &sc.image, p);
        sc.drawn = Some(sc.ticks);
    }
}

/// Every bar has reached its value after `ticks` steps.
fn bars_done(s: &ScoreData, ticks: u32) -> bool {
    (0..7).all(|k| {
        let max = s.rows.iter().map(|r| r.2.values()[k]).max().unwrap_or(0);
        s.rows
            .iter()
            .all(|r| cf::counted(r.2.values()[k], cf::bar_step(max), ticks) == r.2.values()[k])
    })
}

// ---------------------------------------------------------------------------------------------------------------
// The finale and the credits

#[derive(Resource)]
struct Scroller {
    image: PictureId,
    background: Picture,
    lines: Vec<String>,
    panel: Rect,
    line_h: u32,
    scroll: Scroll,
    narration_ms: u32,
    voice: VecDeque<(u32, String)>,
    drawn: Option<u32>,
    /// The credits' fade-out: when it ends.
    closing: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
fn open_scroller(
    commands: &mut Commands,
    pictures: &mut ScreenPictures,
    front: &Front,
    background: Picture,
    text: &[String],
    panel: Rect,
    files: Vec<String>,
    lens: &[u32],
    step_ms: Option<u32>,
    state: AppState,
) {
    let line_h = front.font.height as u32 + 2;
    let lines: Vec<String> = text
        .iter()
        .flat_map(|t| cf::wrap(t, panel.w - 16, &measure(&front.font)))
        .collect();
    let total = cf::narration_total_ms(lens);
    let text_h = lines.len() as u32 * line_h;
    let mut scroll = Scroll::new(total, text_h, panel.h, line_h);
    if let Some(ms) = step_ms {
        scroll.interval_ms = ms;
    }
    let image = spawn_layer(commands, pictures, dup(&background), 0, state, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(Scroller {
        image,
        background,
        lines,
        panel,
        line_h,
        scroll,
        narration_ms: total,
        voice: cf::narration_starts(lens).into_iter().zip(files).collect(),
        drawn: None,
        closing: None,
    });
}

pub(crate) fn enter_finale(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut music: ResMut<Jukebox>,
) {
    let Some(Step::Finale { bdp, race }) = flow.current else {
        return;
    };
    let e = cf::ending(bdp, race);
    let bg = picture(&mut front.install, e.picture, None);
    let text = entry_text(&mut front.install, cf::FINALE_TABLE, e.finale_entry);
    let files: Vec<String> = e.narration.iter().map(|s| s.to_string()).collect();
    let lens: Vec<u32> = files
        .iter()
        .map(|f| {
            front
                .install
                .read(f)
                .ok()
                .and_then(|d| video::wav_length_ms(&d))
                .unwrap_or(0)
        })
        .collect();
    let panel = rect(120, 60, 400, 380);
    let mut bg = bg;
    shade(&mut bg, panel);
    open_scroller(
        &mut commands,
        &mut pictures,
        &front,
        bg,
        &[text],
        panel,
        files,
        &lens,
        None,
        AppState::Finale,
    );
    // Music spec: the finale's track starts after a fade of all sound.
    music.fade_stop(true, Some(e.music));
}

pub(crate) fn enter_credits(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut pictures: ResMut<ScreenPictures>,
    mut music: ResMut<Jukebox>,
) {
    let Some(Step::Credits { bdp }) = flow.current else {
        return;
    };
    let name = cf::ending(bdp, 0).credits;
    let lines = table(&mut front.install, name).map_or(Vec::new(), |t| {
        (0..t.0.len()).filter_map(|i| t.text(i)).collect::<Vec<_>>()
    });
    open_scroller(
        &mut commands,
        &mut pictures,
        &front,
        blank(),
        &lines,
        rect(80, 0, 480, H),
        Vec::new(),
        &[],
        Some(cf::CREDITS_STEP_MS),
        AppState::Credits,
    );
    fade_in_new_screen(&mut commands, fades::CREDITS);
    music.start_unless_started(crate::music::CREDITS_TRACK);
}

#[allow(clippy::too_many_arguments)]
fn run_scroller(
    mut commands: Commands,
    clock: Res<Clock>,
    mut s: ResMut<Scroller>,
    mut pictures: ResMut<ScreenPictures>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut audio: ResMut<CoreAudio>,
    mut music: ResMut<Jukebox>,
    mut next: ResMut<AppStates>,
    mut exit: MessageWriter<AppExit>,
    layers: Query<(Entity, &Layer)>,
    input: Res<PlayerInput>,
) {
    let keys = &input.keys;
    let mouse = &input.mouse;
    let now = clock.0;
    // Palette-effects spec, "Screen fades": the credits fade out over 100 ms, and nothing else runs meanwhile.
    if let Some(end) = s.closing {
        if now >= end {
            go_on(&mut flow, &front, keys, &mut next, &mut exit);
        }
        return;
    }
    while s.voice.front().is_some_and(|(at, _)| (*at as f64) <= now) {
        let (_, f) = s.voice.pop_front().unwrap();
        play_once(&mut commands, &mut front, &mut sources, &mut audio, &f, next.get());
    }
    s.scroll.at(now as u64);
    if s.drawn != Some(s.scroll.moved) {
        let mut p = dup(&s.background);
        let mut clip = blank_sized(s.panel.w, s.panel.h);
        let top = s.panel.h as i32 - s.scroll.moved as i32;
        for (i, l) in s.lines.iter().enumerate() {
            let y = top + (i as u32 * s.line_h) as i32;
            if y > -(s.line_h as i32) && y < s.panel.h as i32 {
                let w = front.font.width(l.as_bytes()) as i32;
                hud::text(&mut clip, &front.font, (s.panel.w as i32 - w) / 2, y, l);
            }
        }
        overlay(&mut p, &clip, s.panel.x, s.panel.y);
        set_image(&mut pictures, &s.image, p);
        s.drawn = Some(s.scroll.moved);
    }
    if pressed(keys, mouse) || cf::briefing_closes(&s.scroll, now as u64, s.narration_ms) {
        if next.get() == AppState::Credits {
            // Music spec: the credits' close fades all sound.
            music.fade_stop(true, None);
            s.closing = Some(fade_out_all(&mut commands, &layers, now, fades::CREDITS));
            return;
        }
        go_on(&mut flow, &front, keys, &mut next, &mut exit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(steps: &[Step]) -> Flow {
        Flow {
            steps: steps.iter().cloned().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn startup_steps_follow_ctrl() {
        assert_eq!(
            startup_steps(false, false, true),
            [Step::Video(0), Step::Video(1), Step::Title, Step::MainMenu]
        );
        assert_eq!(startup_steps(true, false, true), [Step::MainMenu]);
        assert_eq!(startup_steps(false, true, true), [Step::Title, Step::MainMenu]);
    }

    #[test]
    fn goto_parses() {
        assert_eq!(goto_steps("briefing:34").unwrap()[0], Step::Briefing(34));
        assert!(goto_steps("act:3").is_err());
        assert!(goto_steps("nope").is_err());
        assert_eq!(goto_steps("credits:bdp").unwrap()[0], Step::Credits { bdp: true });
    }

    #[test]
    fn restart_goes_back_to_starting_the_mission() {
        let mut f = flow(&[Step::Score]);
        f.game = Some(GameKind::Campaign(6));
        f.total = CampaignTotal {
            total: 900,
            at_start: 500,
        };
        restart(&mut f);
        assert_eq!(f.total.total, 500);
        assert_eq!(f.steps, [Step::Mission(6)]);
        to_main_menu(&mut f);
        assert_eq!(f.steps, [Step::MainMenu]);
    }
}
