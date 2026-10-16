//! The screens around a game (spec `docs/specs/game/campaign-flow.md`, rules in `campaign`): startup videos, the
//! title, a main menu, the new-campaign choices, act screens, briefings, the victory and defeat dialogs, the score
//! screen, the endings and the credits. The flow is a queue of [`Step`]s; [`advance`] takes the next one and names
//! the [`AppState`] that shows it.
//!
//! Every screen is drawn in software into 640x480 pictures shown by one camera on its own render layer, above the
//! game's cameras. The dialog files (`rez\*.bin`) aren't specified yet, so the layouts of the menu, the dialogs, the
//! briefing and the score screen are our own, and the menu has only what the flow needs (New Campaign, Load Game,
//! Show Credits, Quit; N, L, C and Q are our keys for them). The main menu and the campaign choices are drawn on the
//! menu picture (`menuscreen`): a switch between them doesn't fade, and the main menu fades in over 200 ms from any
//! other screen. The main menu fades out when it leaves the menus, and the credits fade in and out
//! (`ui/palette-effects.md`, "Screen fades": 200 ms and 100 ms), like the title and act screens. Choosing a campaign,
//! the briefing's close, the score screen's close and the main menu leaving the menus fade all sound first, then the
//! screen (`campaign::SoundThenBlack`); quitting a game to the main menu or restarting it fades all sound over the
//! game's picture ([`run_quit`]), and the main menu starts track 22 once it has faded in (`ui/music.md`). The title and the act screens run on the 100 ms real-time menu timer (spec "Terms";
//! `campaign::MenuTimer`, one for every screen), and their fades block it; fades run on real time.
//!
//! Entry points other screens reuse: [`restart`] (the game menu's restart: back to "Starting a mission"),
//! [`end_with`] (a game's end, through the victory or defeat dialog's Continue), and [`to_main_menu`].

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};

use glam::Vec2;
use tidewater_formats::fnt::Font;
use tidewater_formats::install::Install;
use tidewater_formats::pcx::Pcx;
use tidewater_formats::smk::{Smacker, YScale};
use tidewater_formats::tbl::StringTable;
use tidewater_formats::tileset::Palette;
use tidewater_sim::setup::Setup;
use tidewater_sim::victory::Outcome;

use crate::audio::{Audio, Group, Mode, Sound, UNITY};
use crate::campaign::{self as cf, Act, CampaignTotal, Choice, GameKind, Memory, Next, Scroll};
use crate::clock::FrameClock;
use crate::core::{Commands, Core, add_layer, remove_layers};
use crate::hud::{self, Rect};
use crate::input::{Button, Buttons, Key, LogCursor};
use crate::jukebox::Jukebox;
use crate::layers::{Fade, Layer as ScreenPlace, PictureId, Pictures, Plane, Role, Stack};
use crate::menuscreen::{self, MenuScreen};
use crate::palette::fades;
use crate::savedlg::SaveUi;
use crate::saves;
use crate::scene::Picture;
use crate::states::Machine;
use crate::video;

const W: u32 = 640;
const H: u32 = 480;

pub use crate::states::AppState;

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
    /// The credits page after a finale (`crdt_lst.tbl` or `crelst2x.tbl`).
    Credits {
        bdp: bool,
    },
    /// The credits screen the main menu's Show Credits opens (`rez\creditsBNE.tbl`, spec "Credits screen").
    ShowCredits,
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

#[derive(Default)]
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

/// The installation, fonts and tables the screens use, and their sounds by name.
pub struct Front {
    pub install: Install,
    pub font: Font,
    pub big: Font,
    pub small: Font,
    pub huge: Font,
    pub network: StringTable,
    /// The human normal arrow in the title screen's palette, with its hot spot: the pointer of the flow screens the
    /// screen spec doesn't name ([`FlowPointer::Default`]).
    pub arrow: Option<(Picture, (i32, i32))>,
    /// The menu buttons' faces (`menuscreen::FACES_FILE`); empty if the install lacks the file.
    pub(crate) button_faces: menuscreen::ButtonFaces,
    /// Sounds decoded for the mixer, by name; `None` for one the install lacks, so it is looked up once.
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
        let button_faces = install
            .read(menuscreen::FACES_FILE)
            .ok()
            .and_then(|d| tidewater_formats::grp::Grp::parse_uncompressed(&d).ok())
            .map(|g| menuscreen::ButtonFaces::from_grp(&g))
            .unwrap_or_default();
        Ok(Front {
            install,
            font: f,
            big,
            small,
            huge,
            network,
            arrow,
            button_faces,
            sounds: HashMap::new(),
        })
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
            Step::Credits { .. } | Step::ShowCredits => AppState::Credits,
        };
        flow.current = Some(step);
        return Some(state);
    }
}

/// Goes on to the next step.
pub(crate) fn go_on(flow: &mut Flow, front: &Front, keys: &Buttons<Key>, next: &mut Machine, exit: &mut bool) {
    let ctrl = keys.any_pressed([Key::ControlLeft, Key::ControlRight]);
    match advance(flow, &front.install, ctrl) {
        Some(s) => next.set(s),
        None => *exit = true,
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

/// Where a game being left goes once its fade of all sound has ended ([`run_quit`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    /// Quit to Menu (and Ctrl+Q's confirmation): the main menu.
    Menu,
    /// Restart Scenario (the end menu's, and Ctrl+R's confirmation): the mission again ([`restart`]).
    Restart,
}

/// The flow's next steps for a game left for `leave`.
pub(crate) fn leave_flow(flow: &mut Flow, leave: Leave) {
    match leave {
        Leave::Menu => to_main_menu(flow),
        Leave::Restart => restart(flow),
    }
}

/// A frame of leaving a game (music spec, "Leaving a game for the main menu", and the "Restart Scenario" row of "Where
/// music fades out"; the game menu's Quit to Menu, Restart and their keys' confirmations start it): the fade of all
/// sound blocks with the game's picture kept and the game stopped (`core::game_running`); once it has ended, the
/// game's track is closed and the main menu or the restarted mission follows. The game end's second fade of all sound
/// has nothing left to fade. The main menu fades in in silence, then starts track 22 from its start; a restart's
/// briefing starts its own track from its start, as none is open. The restart's 1 ms fade to black ends within the
/// frame the game ends on, so no frame shows it: the next screen opens from black.
pub(crate) fn run_quit(c: &mut Core) {
    if c.jukebox.music().fading() {
        return;
    }
    let Some(leave) = c.quitting.take() else { return };
    let Core {
        flow,
        front,
        states: next,
        exit,
        input,
        ..
    } = c;
    leave_flow(flow, leave);
    go_on(flow, front, &input.keys, next, exit);
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
        "show-credits" => vec![Step::ShowCredits, Step::MainMenu],
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
// The screens' state

/// The flow's screen (`docs/design/client-core.md`, step 2): whether its layers cover what is under them (outside a
/// game, and over a game only where they are drawn), and whether the pointer can be read on it yet. The pointer is
/// read from the end of the program's first frame on, as when a camera gave it. `pointer` is the pointer the screen
/// draws outside a game ([`choose_pointer`]).
pub struct FrontScreen {
    pub clears: bool,
    pub(crate) ready: bool,
    pub(crate) pointer: ScreenPointer,
}

impl Default for FrontScreen {
    fn default() -> FrontScreen {
        FrontScreen {
            clears: true,
            ready: false,
            pointer: ScreenPointer::Default,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The pointer outside a game

/// The pointer of a screen around a game (screen spec, "Pointers outside a game").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlowPointer {
    /// No pointer: the videos, the title picture, the act screens and the campaign finale.
    Hidden,
    /// The orc normal arrow (`o_arrow.cur`) for every race and campaign: the main menu and the campaign choices.
    OrcArrow,
    /// The 3D arrow (`arrow_3d.cur`): the briefing, the score screen and the credits.
    Arrow3d,
    /// A screen with no pointer of its own (the start-up state, and the game, whose pointer follows the in-game
    /// rules): the human normal arrow in the title picture's palette.
    Default,
}

impl FlowPointer {
    /// The pointer file in `Art\`, `None` for no pointer.
    pub(crate) fn file(self) -> Option<&'static str> {
        match self {
            FlowPointer::Hidden => None,
            FlowPointer::OrcArrow => Some(crate::screen::Pointer::Arrow.file(true)),
            FlowPointer::Arrow3d => Some("arrow_3d.cur"),
            FlowPointer::Default => Some(crate::screen::Pointer::Arrow.file(false)),
        }
    }
}

/// The pointer each screen state shows. The game screen's pointer follows the in-game rules (`hud`), so `Default`
/// there is never drawn. Videos show none: the introduction was seen with none, and the spec makes that the default
/// for every video (round 70). The campaign finale shows none (round 72: the ToD human finale's picture, with the
/// mouse moved; the spec's default for the other finales and for the finale once its text scrolls). The credits page
/// that follows the finale (our `Step::Credits` after `Step::Finale`) wasn't observed; it keeps the credits screen's
/// 3D arrow, as before.
pub(crate) fn flow_pointer(state: AppState) -> FlowPointer {
    match state {
        AppState::Video | AppState::Title | AppState::Act | AppState::Finale => FlowPointer::Hidden,
        AppState::MainMenu | AppState::NewCampaign => FlowPointer::OrcArrow,
        AppState::Briefing | AppState::Score | AppState::Credits => FlowPointer::Arrow3d,
        AppState::Boot | AppState::Game => FlowPointer::Default,
    }
}

/// The picture whose palette colours a screen's pointer: the pointer is drawn into the 8-bit screen, so its pixels are
/// the screen's palette entries ("Colours"). The menus are drawn over the menu picture (`campaign::MENU_PICTURE`) with
/// all 256 entries of its palette ("Menu screens"), so it gives theirs. The credits are drawn over their own picture
/// (`campaign::CREDITS_PICTURE`), which gives theirs.
pub(crate) fn pointer_palette(state: AppState, flow: &Flow) -> &'static str {
    match (state, &flow.current, &flow.score) {
        (AppState::Briefing, Some(Step::Briefing(e)), _) => cf::briefing_background(*e),
        (AppState::Score, _, Some(s)) => cf::score_background(s.race, s.victory),
        (AppState::Credits, _, _) => cf::CREDITS_PICTURE,
        (AppState::MainMenu | AppState::NewCampaign, _, _) => cf::MENU_PICTURE,
        _ => cf::TITLE_PICTURE,
    }
}

/// A pointer's picture and its hot spot.
pub(crate) type ShapePicture = (Picture, (i32, i32));

/// A flow screen's pointer, as loaded for it.
pub(crate) enum ScreenPointer {
    /// The human normal arrow in the title picture's palette, loaded once (`Front::arrow`).
    Default,
    Hidden,
    /// The screen's own pointer in its palette, with its hot spot (`None` if the install lacks it).
    Own(Option<ShapePicture>),
}

/// The flow's change of screen picks the entering screen's pointer and colours it from that screen's palette.
pub(crate) fn choose_pointer(c: &mut Core) {
    let state = c.states.get();
    let p = flow_pointer(state);
    c.front_screen.pointer = match p {
        FlowPointer::Hidden => ScreenPointer::Hidden,
        FlowPointer::Default => ScreenPointer::Default,
        _ => own_pointer(&mut c.front.install, p, pointer_palette(state, &c.flow)),
    };
}

/// A screen's own pointer `p` in the palette of the picture `palette_of`.
fn own_pointer(install: &mut Install, p: FlowPointer, palette_of: &str) -> ScreenPointer {
    let pal = install.read(palette_of).ok().and_then(|d| Pcx::parse(&d).ok()?.palette);
    ScreenPointer::Own(pal.and_then(|pal| hud::load_pointer(install, p.file()?, &pal)))
}

/// Whether a flow screen shows its pointer, given each flow layer's tint and fade ("Pointer and fades"): it is gone
/// from the frame a fade-out is given (a fade to below full colour) through the black and the next screen's fade-in,
/// and comes back at full colour on the frame that shows the screen at its own palette. A screen with no fade shows
/// it from its first frame.
pub(crate) fn pointer_shows(layers: impl IntoIterator<Item = (f32, Option<Fade>)>) -> bool {
    layers
        .into_iter()
        .all(|(tint, fade)| tint >= 1.0 && fade.is_none_or(|f| f.to >= 1.0))
}

/// The pointer the flow's screen draws now outside a game, with its hot spot; `Err(())` when it shows none, `Ok(None)`
/// when it wants one the install lacks (the system's pointer shows instead). A screen that closes without a fade-out
/// (a video, the act screen's next screen) loses it on the frame that closes it (`leaving`), as one that fades out
/// does; a switch between the menus on the menu picture keeps it.
pub(crate) fn screen_pointer<'a>(
    screen: &'a FrontScreen,
    front: &'a Front,
    layers: &Stack,
    leaving: bool,
) -> Result<Option<&'a ShapePicture>, ()> {
    let flow = layers
        .iter()
        .filter(|e| matches!(e.role, Role::Flow(_)))
        .map(|e| (e.layer.tint, e.fade));
    if leaving || !pointer_shows(flow) {
        return Err(());
    }
    match &screen.pointer {
        ScreenPointer::Hidden => Err(()),
        ScreenPointer::Default => Ok(front.arrow.as_ref()),
        ScreenPointer::Own(p) => Ok(p.as_ref()),
    }
}

/// The flow's screens' own state: what each one that has opened keeps (a screen's state stays until the screen opens
/// again), and the main menu load dialog's input cursors.
#[derive(Default)]
pub struct Screens {
    clock: Option<Clock>,
    menu_clock: MenuClock,
    video: Option<VideoPlay>,
    title: Option<TitleScreen>,
    menu: Option<Menu>,
    /// The menu picture as last loaded, with its palette, kept for the switches between the menus drawn on it.
    menu_picture: Option<(Picture, Palette)>,
    menu_load: Option<MenuLoad>,
    act: Option<ActScreen>,
    briefing: Option<Briefing>,
    score: Option<ScoreScreen>,
    scroller: Option<Scroller>,
    credits: Option<CreditsScreen>,
    load_cursors: (LogCursor, LogCursor),
}

/// The victory or defeat dialog over a finished game.
pub struct OutcomeDialog {
    pub victory: bool,
    /// Save was pressed: the game menu opens the save dialog over this one (`gamemenu::open_from_outcome`).
    pub(crate) save_wanted: bool,
    /// The game was saved from here: the dialog ends the game as Continue does.
    pub(crate) saved: bool,
    /// The dialog is to be drawn again: it opened, or the game menu changed it.
    pub(crate) redraw: bool,
    /// The button that ends the game was used: the fade of all sound runs, and the game ends when it has.
    closing: bool,
    image: PictureId,
}

/// The flow's screens, each in its own state: at most one of them runs in a frame. Each one's condition is checked
/// just before it runs.
pub(crate) fn screen_systems(c: &mut Core) {
    let state = |c: &Core| c.states.get();
    if state(c) == AppState::Finale {
        run_scroller(c);
    }
    if state(c) == AppState::Credits {
        if c.flow.current == Some(Step::ShowCredits) {
            run_credits(c);
        } else {
            run_scroller(c);
        }
    }
    if state(c) == AppState::Score {
        run_score(c);
    }
    if state(c) == AppState::Briefing {
        run_briefing(c);
    }
    if state(c) == AppState::Act {
        run_act(c);
    }
    if state(c) == AppState::NewCampaign {
        run_new_campaign(c);
    }
    if state(c) == AppState::MainMenu && c.screens.menu_load.is_some() {
        run_menu_load(c);
    }
    if state(c) == AppState::MainMenu && c.screens.menu_load.is_none() {
        run_menu(c);
    }
    if state(c) == AppState::Title {
        run_title(c);
    }
    if state(c) == AppState::Video {
        play_video(c);
    }
    if state(c) == AppState::Boot {
        boot(c);
    }
}

/// The flow's screen clears to black except over the game screen.
pub(crate) fn camera_clear(c: &mut Core) {
    c.front_screen.clears = c.states.get() != AppState::Game;
}

fn boot(c: &mut Core) {
    let Core {
        flow,
        front,
        states,
        exit,
        input,
        ..
    } = c;
    go_on(flow, front, &input.keys, states, exit);
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
    pictures: &mut Pictures,
    p: Picture,
    layer: u8,
    state: AppState,
    size: Option<Vec2>,
) -> PictureId {
    let id = pictures.add(p);
    let mut place = ScreenPlace::new(id, Plane::Flow, layer as f32);
    place.size = size.map(|s| (s.x, s.y));
    add_layer(commands, place, Role::Flow(layer), Some(state));
    id
}

pub(crate) fn set_image(pictures: &mut Pictures, id: &PictureId, p: Picture) {
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

/// The pointer in screen pixels (640x480).
pub(crate) fn pointer(input: &crate::input::Input, cam: &FrontScreen) -> Option<Vec2> {
    let w = input.window?;
    let p = input.pointer?;
    if !cam.ready {
        return None;
    }
    let (x, y) = crate::display::Letterbox::fit(w.size).screen_of(p, w.factor);
    Some(Vec2::new(x, y))
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
    button_pressed(dst, font, r, label, hot, false);
}

/// A button, drawn pressed while the mouse button is held on it (screen spec, "Pressing": the label moves 2 pixels
/// right and 2 down; our flat face darkens).
pub(crate) fn button_pressed(dst: &mut Picture, font: &Font, r: Rect, label: &str, hot: bool, pressed: bool) {
    let face = match (pressed, hot) {
        (true, _) => [20, 18, 12],
        (false, true) => [70, 60, 30],
        (false, false) => [36, 32, 24],
    };
    hud::fill(dst, r, face);
    hud::outline(dst, r, [180, 160, 100]);
    let w = font.width(label.as_bytes()) as i32;
    let shift = if pressed { 2 } else { 0 };
    hud::text(
        dst,
        font,
        r.x + (r.w as i32 - w) / 2 + shift,
        r.y + (r.h as i32 - font.height as i32) / 2 + shift,
        label,
    );
}

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

// ---------------------------------------------------------------------------------------------------------------
// Fades

/// Screen time in ms since the screen opened (real time, as the fades' lengths are).
#[derive(Default)]
struct Clock(f64);

/// The menu timer (`campaign::MenuTimer`, spec "Terms"), one for every screen, on the real-time clock of [`real_ms`].
#[derive(Default)]
struct MenuClock(cf::MenuTimer);

/// Real time in ms since the program started: the menu timer's clock.
fn real_ms(time: &FrameClock) -> f64 {
    time.elapsed_secs_f64() * 1000.0
}

/// The fades, run first in the frame.
pub(crate) fn apply_fades(c: &mut Core) {
    let Some(clock) = &mut c.screens.clock else { return };
    clock.0 += c.time.delta_secs_f64() * 1000.0;
    for e in c.layers.iter_mut() {
        if let Some(f) = e.fade {
            e.layer.tint = f.tint(clock.0);
        }
    }
}

/// A fade-in over `ms` of every flow layer there is when the commands take effect: the screen being opened (its
/// layers are added by the same commands).
fn fade_in_new_screen(commands: &mut Commands, ms: u32) {
    set_fades(commands, None, 0.0, 1.0, 0.0, ms);
}

/// The flow layers on the screen now, or only those of `layer`.
fn flow_layers(layers: &Stack, layer: Option<u8>) -> Vec<PictureId> {
    layers
        .iter()
        .filter(|e| matches!(e.role, Role::Flow(n) if layer.is_none_or(|l| l == n)))
        .map(|e| e.layer.picture)
        .collect()
}

/// Gives a fade to the layers `which` names (all the flow layers there are then, if `None`) when the commands take
/// effect.
fn set_fades(commands: &mut Commands, which: Option<Vec<PictureId>>, from: f32, to: f32, at: f64, ms: u32) {
    commands.queue(move |c| {
        let layers = &mut c.layers;
        let which = which.unwrap_or_else(|| flow_layers(layers, None));
        for e in layers.iter_mut().filter(|e| which.contains(&e.layer.picture)) {
            e.fade = Some(Fade {
                from,
                to,
                at,
                ms: ms as f64,
            });
        }
    });
}

/// A fade-out over `ms` of every flow layer, starting at `at`; returns when it ends.
fn fade_out_all(commands: &mut Commands, layers: &Stack, at: f64, ms: u32) -> f64 {
    set_fades(commands, Some(flow_layers(layers, None)), 1.0, 0.0, at, ms);
    at + ms as f64
}

fn fade_layer(commands: &mut Commands, layers: &Stack, layer: u8, from: f32, to: f32, at: f64, ms: u32) {
    set_fades(commands, Some(flow_layers(layers, Some(layer))), from, to, at, ms);
}

// ---------------------------------------------------------------------------------------------------------------
// Music and sounds

/// The game screen opens: the screen's own music closes at once so the game's track can start (`music.rs`, "Our choice
/// for the campaign flow").
pub(crate) fn stop_music(c: &mut Core) {
    c.jukebox.start(None);
}

/// Plays a sound of the screen shown in `state` once; it stops if the screen is left first.
fn play_once(front: &mut Front, audio: &mut Audio, name: &str, state: AppState) {
    if let Some(s) = front.core_sound(name) {
        audio.play(&s, 1.0, UNITY, Mode::Despawn, Group::Screen(state), 0);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Videos

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

pub(crate) fn enter_video(c: &mut Core) {
    let Core {
        commands,
        flow,
        front,
        pictures,
        jukebox: music,
        ..
    } = c;
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
        commands.queue(|c| c.screens.clock = Some(Clock(f64::INFINITY)));
        return;
    };
    smk.set_decode_audio(false);
    let (width, height, y) = (smk.width, smk.height, smk.y_scale());
    let shown_h = video::shown_height(height, y);
    let k = (W as f32 / width as f32).min(H as f32 / shown_h as f32);
    let image = spawn_layer(
        commands,
        pictures,
        Picture {
            width,
            height: shown_h,
            rgba: vec![0; (width * shown_h * 4) as usize],
        },
        0,
        AppState::Video,
        Some(Vec2::new(width as f32 * k, shown_h as f32 * k)),
    );
    let play = VideoPlay {
        smk,
        image,
        width,
        height,
        y,
        elapsed: 0.0,
        shown: None,
        audio: Some(Mutex::new(rx)),
    };
    commands.queue(move |c| c.screens.video = Some(play));
}

fn play_video(c: &mut Core) {
    let Core {
        commands,
        time,
        screens,
        pictures,
        audio,
        flow,
        front,
        states: next,
        exit,
        script,
        input,
        ..
    } = c;
    let keys = &input.keys;
    let mouse = &input.mouse;
    let mut done = pressed(keys, mouse);
    if let Some(p) = &mut screens.video {
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
            set_image(pictures, &p.image, pic);
        }
    } else {
        done = true;
    }
    if done {
        commands.queue(|c| c.screens.video = None);
        go_on(flow, front, keys, next, exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The title

struct TitleScreen(cf::TitleRun);

pub(crate) fn enter_title(c: &mut Core) {
    let Core {
        commands,
        time,
        front,
        pictures,
        ..
    } = c;
    let p = picture(&mut front.install, cf::TITLE_PICTURE, None);
    spawn_layer(commands, pictures, p, 0, AppState::Title, None);
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let title = TitleScreen(cf::TitleRun::new(real_ms(time)));
    commands.queue(move |c| c.screens.title = Some(title));
    fade_in_new_screen(commands, cf::TITLE_FADE_MS);
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

fn run_title(c: &mut Core) {
    let Core {
        commands,
        time,
        screens:
            Screens {
                clock: Some(clock),
                menu_clock: menu_timer,
                title: Some(title),
                ..
            },
        layers,
        jukebox: music,
        flow,
        front,
        states: next,
        exit,
        input,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    let input = title_input(keys, mouse);
    for cue in title.0.update(&mut menu_timer.0, real_ms(time), input) {
        match cue {
            cf::TitleCue::Music => music.start(Some(cf::TITLE_MUSIC)),
            cf::TitleCue::FadeOut(ms) => fade_layer(commands, layers, 0, 1.0, 0.0, clock.0, ms),
            cf::TitleCue::Done => go_on(flow, front, keys, next, exit),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The main menu and the new-campaign choices

struct Menu {
    image: PictureId,
    background: Picture,
    /// The menu picture's palette: the buttons and text are drawn in its entries (screen spec, "Menu screens").
    palette: Palette,
    items: Vec<MenuItem>,
    /// The focused button (the yellow ring): the first when the menu opens.
    focus: usize,
    hot: Option<usize>,
    /// The enabled button the left mouse button went down on, while it is held (screen spec, "Pressing").
    held: menuscreen::Hold<usize>,
    /// What the buttons were last drawn for: the hot, pressed and focused buttons.
    drawn: Option<(Option<usize>, Option<usize>, usize)>,
    /// The menu's close (a fade of all sound, then a fade to black), and what was chosen.
    closing: Option<(cf::SoundThenBlack, MenuPick)>,
    /// When the menu's fade-in ends (0 for none): its buttons are drawn and it takes input from then on.
    ready_at: f64,
    /// The main menu's music test has run (once the menu is drawn: `ui/music.md`, "Main menu opens").
    music_tested: bool,
}

/// What a menu does once its close ends.
enum MenuPick {
    Load(Box<crate::savegame::Resume>),
    /// Show Credits: the credits screen (spec "Credits screen"), then the main menu again.
    ShowCredits,
    Quit,
    /// A campaign chosen on the campaign menu: the flow's next steps.
    Campaign(VecDeque<Step>),
}

/// The main menu's load dialog (`loadgamt.bin`, spec `game/save-load.md`, "Load dialog"), open over the menu.
struct MenuLoad {
    ui: SaveUi,
    image: PictureId,
}

/// A menu button: its place, label, hotkey letter (drawn white in the label) and whether it can be used.
struct MenuItem {
    r: Rect,
    label: String,
    key: Option<char>,
    enabled: bool,
}

/// A menu's buttons in the main menu's column (`menuscreen::button_rects`), from their labels and hotkeys.
fn menu_items(labels: &[(&str, Option<char>)]) -> Vec<MenuItem> {
    menuscreen::button_rects(labels.len())
        .into_iter()
        .zip(labels)
        .map(|(r, &(l, key))| MenuItem {
            r,
            label: l.to_string(),
            key,
            enabled: true,
        })
        .collect()
}

/// The menu picture as RGBA, with its palette (black and an all-black palette if the install lacks it).
fn menu_art(install: &mut Install) -> (Picture, Palette) {
    let pal = install
        .read(cf::MENU_PICTURE)
        .ok()
        .and_then(|d| Pcx::parse(&d).ok()?.palette)
        .unwrap_or(Palette([[0; 3]; 256]));
    (picture(install, cf::MENU_PICTURE, None), pal)
}

/// Opens a menu screen: the menu picture (`campaign::MENU_PICTURE`, in its own palette: screen spec, "Menu screens")
/// with its `headings` drawn over it on their rectangles (`menuscreen::draw_heading`) and the version line
/// ([`version_line`]), then its buttons. The headings and the version line are part of the picture, so they
/// fade in with it (screen spec, "Pointer and fades"); the buttons are drawn by [`menu_input`] once the fade-in has
/// ended.
fn open_menu(c: &mut Core, headings: &[(Rect, &str)], items: Vec<MenuItem>, state: AppState) {
    let screen = menu_screen(state).expect("a menu screen");
    let opening = screen.opening(menu_screen(c.states.previous()));
    let Core {
        commands,
        front,
        pictures,
        screens,
        ..
    } = c;
    // Screen spec, "Menu screens": the picture is loaded when the menu screen opens, and a switch between the menus on
    // it keeps it.
    let (mut bg, palette) = match (&screens.menu_picture, opening.reload) {
        (Some((p, pal)), false) => (dup(p), pal.clone()),
        _ => {
            let (p, pal) = menu_art(&mut front.install);
            screens.menu_picture = Some((dup(&p), pal.clone()));
            (p, pal)
        }
    };
    for &(r, h) in headings {
        menuscreen::draw_heading(&mut bg, &front.font, &palette, r, h);
    }
    if opening.version_line {
        version_line(&mut bg, &front.small, &palette);
    }
    let image = spawn_layer(commands, pictures, dup(&bg), 0, state, None);
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let menu = Menu {
        image,
        background: bg,
        palette,
        items,
        focus: 0,
        hot: None,
        held: menuscreen::Hold::default(),
        drawn: None,
        closing: None,
        ready_at: opening.fade_in_ms as f64,
        music_tested: false,
    };
    commands.queue(move |c| c.screens.menu = Some(menu));
    if opening.fade_in_ms > 0 {
        fade_in_new_screen(commands, opening.fade_in_ms);
    }
}

/// The menu screen a flow state shows, if it is one on the menu picture.
pub(crate) fn menu_screen(state: AppState) -> Option<MenuScreen> {
    match state {
        AppState::MainMenu => Some(MenuScreen::Main),
        AppState::NewCampaign => Some(MenuScreen::Campaign),
        _ => None,
    }
}

/// Whether the screen that shows hides its pointer on the frame a change of screen is asked for: every change does but
/// a switch between the menus on the menu picture, which has no fade (`menuscreen::pointer_stays`).
pub(crate) fn pointer_leaving(states: &Machine) -> bool {
    states
        .asked()
        .is_some_and(|to| !menuscreen::pointer_stays(menu_screen(states.get()), menu_screen(to)))
}

/// Our version line's text. The original shows its own name and version there; this program must not claim to be
/// the original, so it shows our name and version instead.
pub(crate) fn version_text() -> String {
    format!("Tidewater v{}", env!("CARGO_PKG_VERSION"))
}

/// Draws the version line into the menu picture (screen spec, "Pointer and fades" and "Text colours", round 73): in
/// `Art\font10x.fnt` (`Front::small`) at its glyph cell (`menuscreen::draw_version_line`). It is part of the picture,
/// so it moves with the picture wherever the picture is shown, and fades in with it.
fn version_line(dst: &mut Picture, font: &Font, pal: &Palette) {
    menuscreen::draw_version_line(dst, font, pal, &version_text());
}

/// The focus move a frame's keys ask for: Tab, or Shift+Tab (screen spec, "Keyboard focus").
pub(crate) fn focus_key(keys: &Buttons<Key>) -> Option<menuscreen::FocusKey> {
    keys.just_pressed(Key::Tab).then(|| {
        if keys.any_pressed([Key::ShiftLeft, Key::ShiftRight]) {
            menuscreen::FocusKey::Back
        } else {
            menuscreen::FocusKey::Next
        }
    })
}

/// Hover, the held button, the focus, redraw, and the item clicked, chosen by Enter on the focus, or by its number
/// or hotkey letter.
///
/// Screen spec, "Pressing": the left button going down on an enabled button holds it, and it shows its pressed face
/// while the pointer stays on it; releasing on it chooses it, releasing anywhere else does nothing.
fn menu_input(
    menu: &mut Menu,
    front: &Front,
    pictures: &mut Pictures,
    at: Option<Vec2>,
    mouse: &Buttons<Button>,
    keys: &Buttons<Key>,
) -> Option<usize> {
    menu.hot = at.and_then(|p| menu.items.iter().position(|it| it.enabled && it.r.contains(p.x, p.y)));
    if mouse.just_pressed(Button::Left) {
        menu.held.down(menu.hot);
        // Screen spec, "Pressing": pressing a button also gives it the focus.
        if let Some(h) = menu.hot {
            menu.focus = h;
        }
    }
    if let Some(k) = focus_key(keys) {
        let enabled: Vec<bool> = menu.items.iter().map(|it| it.enabled).collect();
        menu.focus = menuscreen::move_focus(&enabled, menu.focus, k);
    }
    let released = mouse.just_released(Button::Left);
    let chosen = if released { menu.held.up(menu.hot) } else { None };
    let pressed = menu.held.pressed(menu.hot);
    let key = (menu.hot, pressed, menu.focus);
    if menu.drawn != Some(key) {
        let mut p = dup(&menu.background);
        for (i, it) in menu.items.iter().enumerate() {
            let state = menuscreen::ButtonState {
                enabled: it.enabled,
                hover: menu.hot == Some(i),
                focused: menu.focus == i,
                pressed: pressed == Some(i),
            };
            menuscreen::draw_button(
                &mut p,
                &front.font,
                &menu.palette,
                Some(&front.button_faces),
                it.r,
                &it.label,
                it.key,
                state,
            );
        }
        set_image(pictures, &menu.image, p);
        menu.drawn = Some(key);
    }
    if chosen.is_some() {
        return chosen;
    }
    // Screen spec, "Keyboard focus": Enter activates the focused button, as a click on it does.
    if keys.any_just_pressed([Key::Enter, Key::NumpadEnter]) && menu.items.get(menu.focus).is_some_and(|it| it.enabled)
    {
        return Some(menu.focus);
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
        && menu.items.get(i).is_some_and(|it| it.enabled)
    {
        return Some(i);
    }
    // A button's hotkey letter, either case.
    menu.items.iter().position(|it| {
        it.enabled
            && it
                .key
                .is_some_and(|k| keys.just_pressed(Key::Letter(k.to_ascii_uppercase())))
    })
}

/// Opens the main menu. Its music test runs once it is drawn ([`run_menu`]), not here.
pub(crate) fn enter_menu(c: &mut Core) {
    let items = menu_items(&[
        ("New Campaign", Some('N')),
        ("Load Game", Some('L')),
        ("Show Credits", Some('C')),
        ("Quit", Some('Q')),
    ]);
    open_menu(c, &[], items, AppState::MainMenu);
}

/// Starts a close that fades all sound, then the screen to black over `black_ms` (`campaign::SoundThenBlack`).
fn close_with_sound(music: &mut Jukebox, black_ms: u32) -> cf::SoundThenBlack {
    music.fade_stop(true, None);
    cf::SoundThenBlack::new(black_ms)
}

/// One frame of such a close at screen time `now`: the fade to black starts once the sound's fade has ended. Returns
/// whether both have ended.
fn close_done(
    close: &mut cf::SoundThenBlack,
    music: &Jukebox,
    commands: &mut Commands,
    layers: &Stack,
    now: f64,
) -> bool {
    match close.update(music.music().fading(), now) {
        cf::CloseCue::Wait => false,
        cf::CloseCue::FadeToBlack(ms) => {
            fade_out_all(commands, layers, now, ms);
            false
        }
        cf::CloseCue::Done => true,
    }
}

/// One frame of a menu's close, if one runs (palette-effects spec, "Screen fades": nothing else runs meanwhile); when
/// it ends, what was chosen happens. Returns whether a close ran.
fn run_menu_close(c: &mut Core) -> bool {
    let Core {
        commands,
        screens: Screens {
            menu: Some(menu),
            clock: Some(clock),
            ..
        },
        layers,
        jukebox: music,
        front,
        flow,
        states: next,
        exit,
        input,
        ..
    } = c
    else {
        return false;
    };
    let Some((close, _)) = &mut menu.closing else {
        return false;
    };
    if !close_done(close, music, commands, layers, clock.0) {
        return true;
    }
    let Some((_, pick)) = menu.closing.take() else {
        return true;
    };
    let keys = &input.keys;
    match pick {
        MenuPick::Load(r) => crate::savegame::load_game(flow, next, r.header, r.body),
        MenuPick::ShowCredits => {
            flow.steps = VecDeque::from([Step::ShowCredits, Step::MainMenu]);
            go_on(flow, front, keys, next, exit);
        }
        MenuPick::Quit => *exit = true,
        MenuPick::Campaign(steps) => {
            flow.steps = steps;
            go_on(flow, front, keys, next, exit);
        }
    }
    true
}

fn run_menu(c: &mut Core) {
    if run_menu_close(c) {
        return;
    }
    let Core {
        commands,
        screens: Screens {
            menu: Some(menu),
            clock: Some(clock),
            ..
        },
        front,
        store: folder,
        pictures,
        front_screen: cam,
        flow,
        states: next,
        exit,
        input,
        jukebox: music,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    // Screen spec, "Pointer and fades": the buttons aren't drawn during the fade-in, which shows the picture and the
    // version line fading in together, and the menu takes no input until the fade ends. The
    // buttons and the pointer appear on the frame that ends it.
    if clock.0 < menu.ready_at {
        return;
    }
    // Music spec, "Main menu opens": track 22 starts if nothing is playing, tested once the menu is drawn (after its
    // fade-in when it fades in). After a game left by Quit to Menu nothing is, so 22 starts from its start; coming
    // back from the campaign menu it is still playing and plays on.
    if !std::mem::replace(&mut menu.music_tested, true) {
        music.start_unless_playing(cf::TITLE_MUSIC);
    }
    let at = pointer(input, cam);
    let pick = menu_input(menu, front, pictures, at, mouse, keys);
    // Music spec, "Where music fades out": Show Credits and Exit Program fade all sound, then the menu fades out.
    let mut close = |menu: &mut Menu, pick: MenuPick| {
        menu.closing = Some((close_with_sound(music, fades::MENUS), pick));
    };
    match pick {
        // Screen spec, "Menu screens": the campaign menu is on the same picture, so the switch has no fade.
        Some(0) => {
            flow.steps = VecDeque::from([Step::NewCampaign]);
            go_on(flow, front, keys, next, exit);
        }
        Some(1) => {
            // Loading from the main menu is single player, and every single-player game is Enhanced.
            let entries = folder.folder().list(crate::savefile::Kind::Single, true);
            let ui = SaveUi::new(saves::Dialog::open(saves::Mode::Load, entries, saves::FORBIDDEN));
            let pic = ui.draw(&front.font, &front.big, &|n| front.net(n));
            let image = pictures.add(pic);
            add_layer(
                commands,
                ScreenPlace::new(image, Plane::Flow, 1.0),
                Role::Flow(1),
                Some(AppState::MainMenu),
            );
            commands.queue(move |c| c.screens.menu_load = Some(MenuLoad { ui, image }));
        }
        Some(2) => close(menu, MenuPick::ShowCredits),
        Some(_) => close(menu, MenuPick::Quit),
        None => {}
    }
}

/// One frame of the main menu's load dialog: Load reads the chosen file and, if it is valid, the menu fades out and the
/// saved game starts; a file that isn't valid shows the reason (`saves::Dialog::failed`); Cancel closes the dialog.
fn run_menu_load(c: &mut Core) {
    let Core {
        commands,
        screens:
            Screens {
                menu_load: Some(load),
                menu: Some(menu),
                load_cursors: cursors,
                ..
            },
        front,
        pictures,
        front_screen: cam,
        input,
        store,
        jukebox: music,
        ..
    } = c
    else {
        return;
    };
    let input = crate::gamemenu::saving_input(input, cursors, cam);
    let snap = |u: &SaveUi| (u.dialog.selected, u.dialog.stage.clone(), u.scroll, u.hot, u.held);
    let before = snap(&load.ui);
    let step = load.ui.step(&input);
    let mut redraw = before != snap(&load.ui);
    match step {
        saves::Step::Close => {
            let image = load.image;
            remove_layers(commands, move |e| e.layer.picture == image);
            commands.queue(|c| c.screens.menu_load = None);
            return;
        }
        saves::Step::Load(entry) => match store.folder().read(&entry.file, true) {
            Ok((header, body)) => {
                // Music spec, "Where music fades out": a saved game loaded from the menus fades all sound first.
                commands.queue(|c| c.screens.menu_load = None);
                let close = close_with_sound(music, fades::MENUS);
                menu.closing = Some((
                    close,
                    MenuPick::Load(Box::new(crate::savegame::Resume { header, body })),
                ));
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
        set_image(pictures, &load.image, p);
    }
}

pub(crate) fn enter_new_campaign(c: &mut Core) {
    // Screen spec, "Main menu and campaign menu": the campaign menu's dialog-file layout.
    let items = menuscreen::CAMPAIGN_BUTTONS
        .iter()
        .map(|&(r, label, key)| MenuItem {
            r,
            label: label.to_string(),
            key: Some(key),
            enabled: true,
        })
        .collect();
    open_menu(c, &menuscreen::CAMPAIGN_HEADINGS, items, AppState::NewCampaign);
}

fn run_new_campaign(c: &mut Core) {
    if run_menu_close(c) {
        return;
    }
    let Core {
        screens: Screens {
            menu: Some(menu),
            clock: Some(clock),
            ..
        },
        front,
        pictures,
        front_screen: cam,
        flow,
        states: next,
        exit,
        input,
        jukebox: music,
        ..
    } = c
    else {
        return;
    };
    if clock.0 < menu.ready_at {
        return;
    }
    let keys = &input.keys;
    let mouse = &input.mouse;
    let at = pointer(input, cam);
    let pick = menu_input(menu, front, pictures, at, mouse, keys);
    let ctrl = keys.any_pressed([Key::ControlLeft, Key::ControlRight]);
    // Campaign flow, "Choosing a new campaign": a choice fades all sound (track 22 out), then the screen to black over
    // 200 ms, then the flow goes on; Previous Menu goes back at once, with no fade, and track 22 plays on.
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
            menu.closing = Some((
                close_with_sound(music, fades::CAMPAIGN_CHOSEN_OUT),
                MenuPick::Campaign(v),
            ));
            None
        }
        Some(_) => Some(VecDeque::from([Step::MainMenu])),
        None if keys.just_pressed(Key::Escape) => Some(VecDeque::from([Step::MainMenu])),
        None => None,
    };
    if let Some(s) = steps {
        flow.steps = s;
        go_on(flow, front, keys, next, exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Act screens

struct ActScreen {
    act: Act,
    run: cf::ActRun,
}

pub(crate) fn enter_act(c: &mut Core) {
    let Core {
        commands,
        flow,
        front,
        pictures,
        ..
    } = c;
    let Some(Step::Act(a)) = flow.current else { return };
    let pal = cf::art(cf::act_palette_picture(&a));
    let p = picture(&mut front.install, &cf::art(a.picture), Some(&pal));
    spawn_layer(commands, pictures, p, 0, AppState::Act, None);
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
    spawn_layer(commands, pictures, text, 1, AppState::Act, None);
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let act = ActScreen {
        act: a,
        run: cf::ActRun::default(),
    };
    commands.queue(move |c| c.screens.act = Some(act));
    // Black until the first tick fades the text in.
    set_fades(commands, None, 0.0, 0.0, 0.0, 1);
}

/// The act screen on the menu timer (`campaign::ActRun`): its fades, the act sound when the picture's fade ends, and
/// the close: a fade to black, then every sound effect stopped and a fade of all sound; the video after
/// (if any) or the briefing follows once the music's fade has ended.
fn run_act(c: &mut Core) {
    let Core {
        commands,
        time,
        screens:
            Screens {
                clock: Some(clock),
                menu_clock: menu_timer,
                act: Some(act),
                ..
            },
        layers,
        jukebox: music,
        flow,
        front,
        audio,
        states: next,
        exit,
        input,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    let input = pressed(keys, mouse);
    for cue in act.run.update(&mut menu_timer.0, real_ms(time), input) {
        match cue {
            cf::ActCue::TextFadeIn => fade_layer(commands, layers, 1, 0.0, 1.0, clock.0, cf::ACT_TEXT_FADE_MS),
            cf::ActCue::PictureFadeIn => fade_layer(commands, layers, 0, 0.0, 1.0, clock.0, cf::ACT_PICTURE_FADE_MS),
            cf::ActCue::ActSound => {
                let name = cf::act_sound(cf::race_of(act.act.entry));
                if let Some(s) = front.core_sound(name) {
                    audio.play(&s, 1.0, UNITY, Mode::Despawn, Group::Act, 0);
                }
            }
            cf::ActCue::FadeToBlack => {
                // The fades block, so every layer that shows is fully in: the text, and the picture from 50 on.
                fade_layer(commands, layers, 1, 1.0, 0.0, clock.0, cf::ACT_CLOSE_FADE_MS);
                if act.run.timer.picture_shown() {
                    fade_layer(commands, layers, 0, 1.0, 0.0, clock.0, cf::ACT_CLOSE_FADE_MS);
                }
            }
            cf::ActCue::FadeAllSound => {
                // Music spec, "Effects and narration": the act screen stops every sound effect, its narration
                // included, just before its fade of all sound.
                audio.cut_effects();
                music.fade_stop(true, None);
            }
        }
    }
    if act.run.done() && !music.music().fading() {
        // The window close is the only quit request here (open question default).
        if let Some(v) = flow.memory.video_after(&act.act, false) {
            flow.steps.push_front(Step::Video(v));
        }
        go_on(flow, front, keys, next, exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Briefings

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
    /// The close: a fade of all sound, then 100 ms to black.
    closing: Option<cf::SoundThenBlack>,
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

pub(crate) fn enter_briefing(c: &mut Core) {
    let Core {
        commands,
        flow,
        front,
        pictures,
        jukebox: music,
        ..
    } = c;
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
    let image = spawn_layer(commands, pictures, dup(&bg), 0, AppState::Briefing, None);
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let briefing = Briefing {
        image,
        background: bg,
        lines,
        panel: BRIEF_PANEL,
        line_h,
        scroll: Scroll::new(total, text_h, BRIEF_PANEL.h, line_h),
        narration_ms: total,
        voice: starts.into_iter().zip(files).collect(),
        drawn: None,
        closing: None,
    };
    commands.queue(move |c| c.screens.briefing = Some(briefing));
    music.start_unless_playing(cf::briefing_music(race));
    // Palette-effects spec, "Screen fades": the briefing fades in over 100 ms as it opens, after its track starts.
    fade_in_new_screen(commands, fades::BRIEFING);
}

/// What closes the briefing at once (campaign flow, "Closing"): its Continue button, which our layout doesn't draw, so a
/// click anywhere stands for it; and Enter. Escape does nothing (round 74, observed), and neither does any other key.
fn briefing_continue(keys: &Buttons<Key>, mouse: &Buttons<Button>) -> bool {
    mouse.just_pressed(Button::Left) || keys.any_just_pressed([Key::Enter, Key::NumpadEnter])
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

fn run_briefing(c: &mut Core) {
    let Core {
        commands,
        screens: Screens {
            clock: Some(clock),
            briefing: Some(b),
            ..
        },
        layers,
        jukebox: music,
        pictures,
        flow,
        front,
        audio,
        states: next,
        exit,
        input,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    let now = clock.0;
    // Campaign flow, "Closing": a fade of all sound (the briefing's track fades out and closes), then 100 ms to black,
    // then the game starts; nothing else runs meanwhile.
    if let Some(close) = &mut b.closing {
        if close_done(close, music, commands, layers, now) {
            go_on(flow, front, keys, next, exit);
        }
        return;
    }
    while b.voice.front().is_some_and(|(at, _)| (*at as f64) <= now) {
        let (_, f) = b.voice.pop_front().unwrap();
        play_once(front, audio, &f, AppState::Briefing);
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
        set_image(pictures, &b.image, p);
        b.drawn = Some(b.scroll.moved);
    }
    // The fade-in takes no input.
    let asked = now >= fades::BRIEFING as f64 && briefing_continue(keys, mouse);
    if asked || cf::briefing_closes(&b.scroll, now as u64, b.narration_ms) {
        // Music spec, "Effects and narration": the briefing stops every sound effect, its narration included, at the
        // click, just before its fade of all sound.
        audio.cut_effects();
        b.closing = Some(close_with_sound(music, fades::BRIEFING));
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
pub(crate) fn watch_outcome(c: &mut Core) {
    let Core {
        commands,
        sim,
        dialog,
        pictures,
        front_screen: screen,
        ..
    } = c;
    let (Some(sim), None) = (sim.as_ref(), dialog.as_ref()) else {
        return;
    };
    let Some(o) = sim.world.players[sim.human as usize].result else {
        return;
    };
    // Music spec, "The outcome dialog": nothing about the music changes while it is up; the game's track plays on.
    screen.clears = false;
    let image = spawn_layer(commands, pictures, blank(), 1, AppState::Game, None);
    let dialog = OutcomeDialog {
        victory: o == Outcome::Victory,
        save_wanted: false,
        saved: false,
        redraw: true,
        closing: false,
        image,
    };
    commands.queue(move |c| c.dialog = Some(dialog));
}

/// Our layout: a box in the middle of the screen.
const DIALOG: Rect = Rect {
    x: 200,
    y: 160,
    w: 240,
    h: 140,
};
const END_BUTTON: Rect = Rect {
    x: 230,
    y: 250,
    w: 180,
    h: 26,
};
const SAVE_BUTTON: Rect = Rect {
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
    // Campaign flow, "The victory and defeat dialogs": the buttons by the dialog data's names.
    if d.victory {
        button(
            &mut p,
            &front.font,
            SAVE_BUTTON,
            cf::DialogButton::SaveGame.label(),
            false,
        );
    }
    let end = cf::DialogButton::ending(d.victory);
    button(&mut p, &front.font, END_BUTTON, end.label(), false);
    p
}

pub(crate) fn run_dialog(c: &mut Core) {
    let Core {
        dialog: Some(dialog),
        sim,
        front,
        pictures,
        front_screen: cam,
        flow,
        states: next,
        exit,
        input,
        jukebox: music,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    // Drawn when it opens, and again when the game menu changed it (what Bevy's change detection told).
    if dialog.redraw {
        let p = draw_dialog(front, dialog);
        set_image(pictures, &dialog.image, p);
        dialog.redraw = false;
    }
    // Music spec, "The outcome dialog": the fade of all sound blocks, then the game ends and the score screen starts
    // its track, so the two tracks never overlap.
    if dialog.closing {
        if !music.music().fading() {
            let Some(sim) = sim else { return };
            end_with(flow, front, &sim.world, sim.human, dialog.victory);
            go_on(flow, front, keys, next, exit);
        }
        return;
    }
    let at = pointer(input, cam).filter(|_| mouse.just_released(Button::Left));
    let hit = |r: Rect| at.is_some_and(|p| r.contains(p.x, p.y));
    // The hotkeys from the dialog data: V for "Victory", O for "OK", F11 for "Save Game"; Enter is ours for the button
    // that ends the game.
    let end = cf::DialogButton::ending(dialog.victory);
    let end_key = Key::Letter(if dialog.victory { 'V' } else { 'O' });
    let button = if dialog.saved || hit(END_BUTTON) || keys.any_just_pressed([Key::Enter, end_key]) {
        Some(end)
    } else if dialog.victory && (hit(SAVE_BUTTON) || keys.just_pressed(Key::F(11))) {
        Some(cf::DialogButton::SaveGame)
    } else {
        None
    };
    match button.map(cf::dialog_button) {
        // The button that ends the game: a fade of all sound from the current volume.
        Some(cf::DialogResult::End) => {
            music.fade_stop(true, None);
            dialog.closing = true;
        }
        // The save dialog opens over this one; a save ends the game (`saved`), Cancel shows this dialog again.
        Some(cf::DialogResult::OpenSave) => dialog.save_wanted = true,
        None => {}
    }
}

pub(crate) fn close_dialog(c: &mut Core) {
    c.commands.queue(|c| c.dialog = None);
}

pub(crate) fn close_menu_load(c: &mut Core) {
    c.commands.queue(|c| c.screens.menu_load = None);
}

// ---------------------------------------------------------------------------------------------------------------
// The score screen

struct ScoreScreen {
    image: PictureId,
    background: Picture,
    /// Ticks run so far.
    ticks: u32,
    next_tick: f64,
    bars_done_at: Option<u32>,
    finished: bool,
    drawn: Option<u32>,
    /// The close: a fade of all sound, then 150 ms to black, then what follows the score screen.
    closing: Option<(cf::SoundThenBlack, Step)>,
}

/// Bars then the score count, one step a tick ("The score screen").
const SCORE_TICK_MS: f64 = 40.0;

pub(crate) fn enter_score(c: &mut Core) {
    let Core {
        commands,
        flow,
        front,
        pictures,
        jukebox: music,
        ..
    } = c;
    let Some(s) = flow.score.clone() else { return };
    let bg = picture(&mut front.install, cf::score_background(s.race, s.victory), None);
    let image = spawn_layer(commands, pictures, dup(&bg), 0, AppState::Score, None);
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let score = ScoreScreen {
        image,
        background: bg,
        ticks: 0,
        next_tick: 0.0,
        bars_done_at: None,
        finished: false,
        drawn: None,
        closing: None,
    };
    commands.queue(move |c| c.screens.score = Some(score));
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

fn run_score(c: &mut Core) {
    let Core {
        commands,
        screens: Screens {
            clock: Some(clock),
            score: Some(sc),
            ..
        },
        layers,
        jukebox: music,
        pictures,
        flow,
        front,
        audio,
        states: next,
        exit,
        input,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    let Some(s) = flow.score.clone() else {
        go_on(flow, front, keys, next, exit);
        return;
    };
    // Music spec, "Where music fades out": the score screen's close fades all sound (the score track closes), then the
    // screen fades to black over 150 ms (palette-effects spec), then the flow goes on.
    if let Some((close, _)) = &mut sc.closing {
        if close_done(close, music, commands, layers, clock.0)
            && let Some((_, step)) = sc.closing.take()
        {
            flow.steps = VecDeque::from([step]);
            go_on(flow, front, keys, next, exit);
        }
        return;
    }
    let press = pressed(keys, mouse);
    if press && sc.finished {
        let next_step = after_score(flow, &s);
        sc.closing = Some((close_with_sound(music, fades::SCORE_CLOSE_OUT), next_step));
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
            play_once(front, audio, cf::NUMBERS_SOUND, AppState::Score);
        }
    }
    if sc.drawn != Some(sc.ticks) {
        let (p, finished) = draw_score(front, &s, &sc.background, sc.ticks, sc.bars_done_at);
        if finished && !sc.finished {
            sc.finished = true;
            play_once(front, audio, cf::RANK_SOUND, AppState::Score);
        }
        set_image(pictures, &sc.image, p);
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
    pictures: &mut Pictures,
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
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let scroller = Scroller {
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
    };
    commands.queue(move |c| c.screens.scroller = Some(scroller));
}

pub(crate) fn enter_finale(c: &mut Core) {
    let Core {
        commands,
        flow,
        front,
        pictures,
        jukebox: music,
        ..
    } = c;
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
        commands,
        pictures,
        front,
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

pub(crate) fn enter_credits(c: &mut Core) {
    if c.flow.current == Some(Step::ShowCredits) {
        enter_show_credits(c);
        return;
    }
    let Core {
        commands,
        flow,
        front,
        pictures,
        jukebox: music,
        ..
    } = c;
    let Some(Step::Credits { bdp }) = flow.current else {
        return;
    };
    let name = cf::ending(bdp, 0).credits;
    let lines = table(&mut front.install, name).map_or(Vec::new(), |t| {
        (0..t.0.len()).filter_map(|i| t.text(i)).collect::<Vec<_>>()
    });
    // Screen spec, "Pointers outside a game": the credits show their own picture, in its palette.
    let bg = picture(&mut front.install, cf::CREDITS_PICTURE, None);
    open_scroller(
        commands,
        pictures,
        front,
        bg,
        &lines,
        rect(80, 0, 480, H),
        Vec::new(),
        &[],
        Some(cf::CREDITS_STEP_MS),
        AppState::Credits,
    );
    fade_in_new_screen(commands, fades::CREDITS);
    music.start_unless_started(crate::music::CREDITS_TRACK);
}

fn run_scroller(c: &mut Core) {
    let Core {
        commands,
        screens: Screens {
            clock: Some(clock),
            scroller: Some(s),
            ..
        },
        pictures,
        flow,
        front,
        audio,
        jukebox: music,
        states: next,
        exit,
        layers,
        input,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    let mouse = &input.mouse;
    let now = clock.0;
    // Palette-effects spec, "Screen fades": the credits fade out over 100 ms, and nothing else runs meanwhile.
    if let Some(end) = s.closing {
        if now >= end {
            go_on(flow, front, keys, next, exit);
        }
        return;
    }
    while s.voice.front().is_some_and(|(at, _)| (*at as f64) <= now) {
        let (_, f) = s.voice.pop_front().unwrap();
        play_once(front, audio, &f, next.get());
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
        set_image(pictures, &s.image, p);
        s.drawn = Some(s.scroll.moved);
    }
    if pressed(keys, mouse) || cf::briefing_closes(&s.scroll, now as u64, s.narration_ms) {
        if next.get() == AppState::Credits {
            // Music spec: the credits' close fades all sound.
            music.fade_stop(true, None);
            s.closing = Some(fade_out_all(commands, layers, now, fades::CREDITS));
            return;
        }
        go_on(flow, front, keys, next, exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The credits screen (spec "Credits screen"; rules in `campaign`)

/// A page of the credits screen, ready to draw.
struct CreditsSheet {
    background: Picture,
    lines: Vec<cf::CreditsLine>,
    /// The steps after which its last line has scrolled off the top.
    steps: u32,
}

struct CreditsScreen {
    image: PictureId,
    pages: Vec<CreditsSheet>,
    page: usize,
    /// When the first page's text started moving, in screen time.
    start: f64,
    /// The page and steps last drawn.
    drawn: (usize, u32),
    /// The fade-out: when it ends.
    closing: Option<f64>,
}

/// The style of a credits line: its font's space, the greys, and the text window's rows.
fn credits_style(title: bool) -> hud::TextStyle<'static> {
    let gap = if title {
        cf::CREDITS_TITLE_SPACE
    } else {
        cf::CREDITS_BODY_SPACE
    };
    hud::TextStyle {
        colours: &cf::CREDITS_SHADES,
        space: Some(cf::credits_space_advance(gap)),
        rows: cf::CREDITS_ROWS,
    }
}

/// The credits fonts as the rest of the screens load them: the title's is `Front::big` (`Art\font32.fnt`), the body's
/// `Front::font` (`Art\font12x.fnt`).
fn credits_font(front: &Front, title: bool) -> &Font {
    if title { &front.big } else { &front.font }
}

/// A credits page's picture with its text moved up `moved` steps.
fn draw_credits(front: &Front, sheet: &CreditsSheet, moved: u32) -> Picture {
    let mut p = dup(&sheet.background);
    let (top, bottom) = cf::CREDITS_ROWS;
    for l in &sheet.lines {
        let font = credits_font(front, l.title);
        let y = cf::CREDITS_TITLE_TOP + l.y - moved as i32;
        // Lines wholly outside the window draw nothing (the style's rows cut the rest).
        if y > bottom || y + 2 * font.height as i32 <= top {
            continue;
        }
        hud::text_styled(&mut p, font, (l.x, y), &l.text, &credits_style(l.title));
    }
    p
}

fn credits_sheet(front: &mut Front, table: Option<&StringTable>, page: &cf::CreditsPage) -> CreditsSheet {
    let text = |i: usize| table.and_then(|t| t.text(i)).unwrap_or_default();
    let (title, body) = (text(page.title), text(page.body));
    let title_style = credits_style(true);
    let big = &front.big;
    let wrap = |s: &str| cf::wrap(s, cf::CREDITS_TITLE_WRAP, &|t: &str| title_style.width(big, t));
    let lines = cf::credits_lines(&title, &body, &wrap);
    let steps = cf::credits_page_steps(&lines, front.font.height as i32);
    CreditsSheet {
        background: picture(&mut front.install, page.picture, None),
        lines,
        steps,
    }
}

fn enter_show_credits(c: &mut Core) {
    let Core {
        commands,
        front,
        pictures,
        jukebox: music,
        ..
    } = c;
    let table = table(&mut front.install, cf::CREDITS_TABLE);
    let pages: Vec<CreditsSheet> = cf::CREDITS_PAGES
        .iter()
        .map(|p| credits_sheet(front, table.as_ref(), p))
        .collect();
    let image = spawn_layer(
        commands,
        pictures,
        draw_credits(front, &pages[0], 0),
        0,
        AppState::Credits,
        None,
    );
    commands.queue(|c| c.screens.clock = Some(Clock(0.0)));
    let screen = CreditsScreen {
        image,
        pages,
        page: 0,
        // Palette-effects spec: nothing else runs during a fade, so the text starts moving when the fade-in ends.
        start: fades::CREDITS as f64,
        drawn: (0, 0),
        closing: None,
    };
    commands.queue(move |c| c.screens.credits = Some(screen));
    fade_in_new_screen(commands, fades::CREDITS);
    music.start_unless_started(crate::music::CREDITS_TRACK);
}

/// One frame of the credits screen: the text moves up one step per 40 ms; a key or click (once the fade-in has ended)
/// or the last page's body scrolling off closes it with its fade-out and a fade of all sound, then the flow goes on.
fn run_credits(c: &mut Core) {
    let Core {
        commands,
        screens: Screens {
            clock: Some(clock),
            credits: Some(s),
            ..
        },
        pictures,
        flow,
        front,
        front_screen: cam,
        jukebox: music,
        states: next,
        exit,
        layers,
        input,
        ..
    } = c
    else {
        return;
    };
    let now = clock.0;
    if let Some(end) = s.closing {
        if now >= end {
            go_on(flow, front, &input.keys, next, exit);
        }
        return;
    }
    if now < s.start {
        return;
    }
    let steps: Vec<u32> = s.pages.iter().map(|p| p.steps).collect();
    let (page, shown, done) = cf::credits_at(&steps, now - s.start);
    if page != s.page {
        // Screen spec, "Colours": the pointer's pixels are the current palette's entries, so on a new page's picture
        // it takes that picture's palette.
        s.page = page;
        cam.pointer = own_pointer(
            &mut front.install,
            FlowPointer::Arrow3d,
            cf::CREDITS_PAGES[page].picture,
        );
    }
    if s.drawn != (page, shown) {
        set_image(pictures, &s.image, draw_credits(front, &s.pages[page], shown));
        s.drawn = (page, shown);
    }
    if done || pressed(&input.keys, &input.mouse) {
        // Music spec: the credits' close fades all sound.
        music.fade_stop(true, None);
        s.closing = Some(fade_out_all(commands, layers, now, fades::CREDITS));
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

    // The screen spec's "Pointers outside a game" and its test vectors.

    #[test]
    fn each_flow_screen_has_its_pointer() {
        use AppState as S;
        assert_eq!(
            flow_pointer(S::Title),
            FlowPointer::Hidden,
            "title picture, any frame: no pointer"
        );
        assert_eq!(flow_pointer(S::Act), FlowPointer::Hidden);
        assert_eq!(
            flow_pointer(S::Video),
            FlowPointer::Hidden,
            "introduction video, pointer moved over it: no pointer (and the default for every video)"
        );
        for s in [S::MainMenu, S::NewCampaign] {
            assert_eq!(
                flow_pointer(s),
                FlowPointer::OrcArrow,
                "{s:?}: the orc arrow for every race"
            );
        }
        for s in [S::Briefing, S::Score, S::Credits] {
            assert_eq!(flow_pointer(s), FlowPointer::Arrow3d, "{s:?}");
        }
        assert_eq!(FlowPointer::OrcArrow.file(), Some("o_arrow.cur"));
        assert_eq!(FlowPointer::Arrow3d.file(), Some("arrow_3d.cur"));
        assert_eq!(FlowPointer::Hidden.file(), None);
        // Round 72: no pointer on the campaign finale, even when the mouse moves (the spec's default for every
        // finale and for its scrolling text).
        assert_eq!(
            flow_pointer(S::Finale),
            FlowPointer::Hidden,
            "campaign finale: no pointer"
        );
        // Pinned, not in the spec: the credits page after the finale keeps the credits screen's 3D arrow.
        assert_eq!(flow_pointer(S::Credits), FlowPointer::Arrow3d);
        // In a game (and its victory or defeat dialog) the in-game rules give the race arrow: a human campaign's is
        // the human normal arrow.
        assert_eq!(crate::screen::Pointer::Arrow.file(false), "h_arrow.cur");
    }

    #[test]
    fn the_pointer_takes_the_screen_pictures_palette() {
        let mut f = flow(&[]);
        for s in [AppState::MainMenu, AppState::NewCampaign] {
            // Human campaign, main menu after the title: the menu picture's palette, not the title picture's.
            assert_eq!(pointer_palette(s, &f), "Art\\TitleMenu_BNE.pcx", "{s:?}");
        }
        assert_ne!(cf::MENU_PICTURE, cf::TITLE_PICTURE);
        f.current = Some(Step::Briefing(0));
        assert_eq!(pointer_palette(AppState::Briefing, &f), cf::briefing_background(0));
        f.score = Some(ScoreData {
            race: cf::HUMAN,
            victory: true,
            game: GameKind::Campaign(0),
            rows: Vec::new(),
            mission: 0,
            total: 0,
            rank: String::new(),
        });
        assert_eq!(
            pointer_palette(AppState::Score, &f),
            cf::score_background(cf::HUMAN, true)
        );
        // Main menu, Show Credits clicked: the credits picture's own palette.
        f.current = Some(Step::Credits { bdp: false });
        assert_eq!(pointer_palette(AppState::Credits, &f), "Art\\credits.pcx");
    }

    #[test]
    fn the_pointer_is_drawn_in_the_palette_it_is_given() {
        // A 2 × 1 pointer, hot spot (1, 0): index 0 clear, index 7 the palette's entry 7.
        let file = [1, 0, 0, 0, 2, 0, 1, 0, 0, 7];
        let mut a = tidewater_formats::tileset::Palette([[0; 3]; 256]);
        let mut b = a.clone();
        a.0[7] = [10, 20, 30];
        b.0[7] = [200, 100, 50];
        let (pa, hot) = hud::pointer_picture(&file, &a).unwrap();
        let (pb, _) = hud::pointer_picture(&file, &b).unwrap();
        assert_eq!(hot, (1, 0));
        assert_eq!(pa.rgba[3], 0, "index 0 is clear");
        assert_eq!(pa.rgba[4..8], [10, 20, 30, 255]);
        assert_eq!(pb.rgba[4..8], [200, 100, 50, 255], "no palette of its own");
    }

    fn fade(from: f32, to: f32, at: f64, ms: f64) -> Option<Fade> {
        Some(Fade { from, to, at, ms })
    }

    /// A screen of two flow layers with `f`, at clock `t`.
    fn shows(f: Option<Fade>, t: f64) -> bool {
        let tint = f.map_or(1.0, |f| f.tint(t));
        pointer_shows([(tint, f), (tint, f)])
    }

    #[test]
    fn a_menu_fade_out_hides_the_pointer_until_the_next_fade_in_ends() {
        // Main menu, a button clicked at 1000: gone from the fade-out's first frame (still at full colour).
        let out = fade(1.0, 0.0, 1000.0, fades::MENUS as f64);
        assert!(!shows(out, 1000.0), "first frame of the 200 ms fade-out");
        assert!(!shows(out, 1100.0));
        assert!(!shows(out, 1200.0), "black");
        // The next screen's 200 ms fade-in: still gone until it has ended.
        let fade_in = fade(0.0, 1.0, 0.0, fades::MENUS as f64);
        assert!(!shows(fade_in, 0.0));
        assert!(!shows(fade_in, 199.0));
        assert!(
            shows(fade_in, 200.0),
            "the frame that shows the screen at its own palette, at full colour"
        );
        assert!(shows(fade_in, 5000.0));
    }

    #[test]
    fn a_screen_with_no_fade_in_shows_the_pointer_from_its_first_frame() {
        // The briefing and the score screen: no fade.
        assert!(shows(None, 0.0));
        assert!(pointer_shows([]), "no flow layer at all");
        // Any layer still faded hides it (the act screen's black before its first tick).
        assert!(!pointer_shows([(1.0, None), (0.0, fade(0.0, 0.0, 0.0, 1.0))]));
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
        assert_eq!(goto_steps("show-credits").unwrap(), [Step::ShowCredits, Step::MainMenu]);
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

    #[test]
    fn a_game_left_for_a_restart_replays_the_mission_and_one_left_for_the_menu_goes_there() {
        // Music spec, "Where music fades out" (round 75): Restart Scenario fades all sound first, as Quit to Menu
        // does; once the fade has ended the flow goes back to starting the mission, or to the main menu.
        let mut f = flow(&[]);
        f.game = Some(GameKind::Campaign(6));
        leave_flow(&mut f, Leave::Restart);
        assert_eq!(f.steps, [Step::Mission(6)]);
        leave_flow(&mut f, Leave::Menu);
        assert_eq!(f.steps, [Step::MainMenu]);
    }

    #[test]
    fn our_version_line_names_this_program() {
        // The original's version string isn't shown: this program doesn't claim to be the original.
        let t = version_text();
        assert!(t.starts_with("Tidewater v"), "{t}");
        assert!(!t.contains("Warcraft") && !t.contains("BNE"), "{t}");
    }
}
