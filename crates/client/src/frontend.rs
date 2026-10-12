//! The screens around a game (spec `docs/specs/game/campaign-flow.md`, rules in `campaign`): startup videos, the
//! title, a main menu, the new-campaign choices, act screens, briefings, the victory and defeat dialogs, the score
//! screen, the endings and the credits. The flow is a queue of [`Step`]s; [`advance`] takes the next one and names
//! the [`AppState`] that shows it.
//!
//! Every screen is drawn in software into 640x480 pictures shown by one camera on its own render layer, above the
//! game's cameras. The dialog files (`rez\*.bin`) aren't specified yet, so the layouts of the menu, the dialogs, the
//! briefing and the score screen are our own, and the menu has only what the flow needs (New Campaign, Quit).
//!
//! Entry points other screens reuse: [`restart`] (the game menu's restart: back to "Starting a mission"),
//! [`end_with`] (a game's end, through the victory or defeat dialog's Continue), and [`to_main_menu`].

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};
use tidewater_formats::fnt::Font;
use tidewater_formats::install::Install;
use tidewater_formats::pcx::Pcx;
use tidewater_formats::smk::{Smacker, YScale};
use tidewater_formats::tbl::StringTable;
use tidewater_sim::setup::Setup;
use tidewater_sim::victory::Outcome;

use crate::campaign::{self as cf, Act, ActEvent, ActTimer, CampaignTotal, Choice, GameKind, Memory, Next, Scroll};
use crate::hud::{self, Rect};
use crate::jukebox::{self, Jukebox};
use crate::scene::Picture;
use crate::video;

/// The front end's render layer.
pub const FRONT_LAYER: usize = 7;
const W: u32 = 640;
const H: u32 = 480;

#[derive(States, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Boot,
    Video,
    Title,
    MainMenu,
    NewCampaign,
    Act,
    Briefing,
    Game,
    Score,
    Finale,
    Credits,
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
    sources: HashMap<String, Option<Handle<AudioSource>>>,
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
        Ok(Front {
            install,
            font: f,
            big,
            small,
            huge,
            network,
            sources: HashMap::new(),
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

    fn net(&self, entry: u16) -> String {
        self.network.text(cf::text_index(entry)).unwrap_or_default()
    }
}

fn table(install: &mut Install, name: &str) -> Option<StringTable> {
    StringTable::parse(&install.read(name).ok()?).ok()
}

/// A table entry (1-based), or an empty string.
fn entry_text(install: &mut Install, name: &str, entry: u16) -> String {
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
fn go_on(
    flow: &mut Flow,
    front: &Front,
    keys: &ButtonInput<KeyCode>,
    next: &mut NextState<AppState>,
    exit: &mut MessageWriter<AppExit>,
) {
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    match advance(flow, &front.install, ctrl) {
        Some(s) => next.set(s),
        None => {
            exit.write(AppExit::Success);
        }
    }
}

/// The game menu's restart ("Restart"): the campaign total goes back as on a defeat and the flow goes to "Starting a
/// mission" again, with no score screen. Returns the state to set.
#[allow(dead_code, reason = "the game menu's restart calls it")]
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

#[derive(Component)]
struct FrontCamera;

/// A front-end picture layer: 0 the background, 1 the overlay drawn over it.
#[derive(Component)]
struct Layer(u8);

/// The victory or defeat dialog over a finished game.
#[derive(Resource)]
pub struct OutcomeDialog {
    pub victory: bool,
    /// Save was pressed: a notice shows until the next click.
    notice: bool,
    image: Handle<Image>,
}

/// Whether the game screen runs (not while the outcome dialog is up).
pub fn game_running(state: Res<State<AppState>>, dialog: Option<Res<OutcomeDialog>>) -> bool {
    *state.get() == AppState::Game && dialog.is_none()
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Jukebox>()
        .add_systems(Update, jukebox::play_music)
        .add_systems(Startup, spawn_camera)
        .add_systems(OnEnter(AppState::Video), enter_video)
        .add_systems(OnEnter(AppState::Title), enter_title)
        .add_systems(OnEnter(AppState::MainMenu), enter_menu)
        .add_systems(OnEnter(AppState::NewCampaign), enter_new_campaign)
        .add_systems(OnEnter(AppState::Act), enter_act)
        .add_systems(OnEnter(AppState::Briefing), enter_briefing)
        .add_systems(OnEnter(AppState::Score), enter_score)
        .add_systems(OnEnter(AppState::Finale), enter_finale)
        .add_systems(OnEnter(AppState::Credits), enter_credits)
        .add_systems(OnEnter(AppState::Game), (stop_music, camera_clear))
        .add_systems(OnExit(AppState::Game), (camera_clear, close_dialog))
        .add_systems(
            Update,
            (
                boot.run_if(in_state(AppState::Boot)),
                play_video.run_if(in_state(AppState::Video)),
                run_title.run_if(in_state(AppState::Title)),
                run_menu.run_if(in_state(AppState::MainMenu)),
                run_new_campaign.run_if(in_state(AppState::NewCampaign)),
                run_act.run_if(in_state(AppState::Act)),
                run_briefing.run_if(in_state(AppState::Briefing)),
                run_score.run_if(in_state(AppState::Score)),
                run_scroller.run_if(in_state(AppState::Finale)),
                run_scroller.run_if(in_state(AppState::Credits)),
                watch_outcome.run_if(in_state(AppState::Game)),
                run_dialog.run_if(resource_exists::<OutcomeDialog>),
                show_cursor.run_if(not(game_running)),
                apply_fades,
            ),
        );
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 10,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: W as f32,
                min_height: H as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
        RenderLayers::layer(FRONT_LAYER),
        FrontCamera,
        Persist,
    ));
}

/// The front camera clears to black except over the game screen.
fn camera_clear(state: Res<State<AppState>>, mut cams: Query<&mut Camera, With<FrontCamera>>) {
    for mut c in &mut cams {
        c.clear_color = if *state.get() == AppState::Game {
            ClearColorConfig::None
        } else {
            ClearColorConfig::Custom(Color::BLACK)
        };
    }
}

fn show_cursor(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    for mut c in &mut cursor {
        if !c.visible {
            c.visible = true;
        }
    }
}

fn boot(
    mut flow: ResMut<Flow>,
    front: Res<Front>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    go_on(&mut flow, &front, &keys, &mut next, &mut exit);
}

fn dup(p: &Picture) -> Picture {
    Picture {
        width: p.width,
        height: p.height,
        rgba: p.rgba.clone(),
    }
}

fn blank() -> Picture {
    Picture {
        width: W,
        height: H,
        rgba: vec![0; (W * H * 4) as usize],
    }
}

fn rgba_image(p: Picture) -> Image {
    Image::new(
        bevy::render::render_resource::Extent3d {
            width: p.width,
            height: p.height,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        p.rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Spawns a picture as a layer of the screen shown in `state`.
fn spawn_layer(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    p: Picture,
    layer: u8,
    state: AppState,
    size: Option<Vec2>,
) -> Handle<Image> {
    let h = images.add(rgba_image(p));
    commands.spawn((
        Sprite {
            custom_size: size,
            ..Sprite::from_image(h.clone())
        },
        Transform::from_xyz(0.0, 0.0, layer as f32),
        RenderLayers::layer(FRONT_LAYER),
        Layer(layer),
        Persist,
        DespawnOnExit(state),
    ));
    h
}

fn set_image(images: &mut Assets<Image>, h: &Handle<Image>, p: Picture) {
    if let Some(mut img) = images.get_mut(h) {
        *img = rgba_image(p);
    }
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
fn pressed(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    mouse.just_pressed(MouseButton::Left)
        || keys.any_just_pressed([KeyCode::Escape, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
}

/// The pointer in screen pixels (640x480).
fn pointer(
    window: &Query<&Window, With<PrimaryWindow>>,
    cam: &Query<(&Camera, &GlobalTransform), With<FrontCamera>>,
) -> Option<Vec2> {
    let w = window.single().ok()?;
    let (c, t) = cam.single().ok()?;
    let p = c.viewport_to_world_2d(t, w.cursor_position()?).ok()?;
    Some(Vec2::new(p.x + W as f32 / 2.0, H as f32 / 2.0 - p.y))
}

fn centred(dst: &mut Picture, font: &Font, y: i32, s: &str) {
    let w = font.width(s.as_bytes()) as i32;
    hud::text(dst, font, (dst.width as i32 - w) / 2, y, s);
}

fn measure(font: &Font) -> impl Fn(&str) -> u32 + '_ {
    |s: &str| font.width(s.as_bytes())
}

/// A button: an outlined box with its label centred.
fn button(dst: &mut Picture, font: &Font, r: Rect, label: &str, hot: bool) {
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

/// Screen time in ms since the screen opened.
#[derive(Resource, Default)]
struct Clock(f64);

fn apply_fades(time: Res<Time>, clock: Option<ResMut<Clock>>, mut q: Query<(&Fade, &mut Sprite)>) {
    let Some(mut clock) = clock else { return };
    clock.0 += time.delta_secs_f64() * 1000.0;
    for (f, mut s) in &mut q {
        let k = ((clock.0 - f.at) / f.ms.max(1.0)).clamp(0.0, 1.0) as f32;
        let b = f.from + (f.to - f.from) * k;
        s.color = Color::srgb(b, b, b);
    }
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
fn stop_music(mut music: ResMut<Jukebox>) {
    music.start(None);
}

fn play_once(
    commands: &mut Commands,
    front: &mut Front,
    assets: &mut Assets<AudioSource>,
    name: &str,
    state: AppState,
) {
    if let Some(h) = front.sound(assets, name) {
        commands.spawn((AudioPlayer(h), PlaybackSettings::DESPAWN, DespawnOnExit(state)));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Videos

#[derive(Resource)]
struct VideoPlay {
    smk: Smacker,
    image: Handle<Image>,
    width: u32,
    height: u32,
    y: YScale,
    elapsed: f64,
    shown: Option<usize>,
    audio: Option<Mutex<Receiver<Option<Vec<u8>>>>>,
}

fn enter_video(
    mut commands: Commands,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
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
        &mut images,
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
    time: Res<Time>,
    play: Option<ResMut<VideoPlay>>,
    mut images: ResMut<Assets<Image>>,
    mut sources: ResMut<Assets<AudioSource>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut flow: ResMut<Flow>,
    front: Res<Front>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut done = pressed(&keys, &mouse);
    if let Some(mut p) = play {
        // The first frame shows at 0 even on a slow first update.
        if p.shown.is_some() {
            p.elapsed += time.delta_secs_f64() * 1000.0;
        }
        let got = p.audio.as_ref().and_then(|m| m.lock().ok()?.try_recv().ok());
        if let Some(wav) = got {
            if let Some(bytes) = wav {
                let h = sources.add(AudioSource { bytes: bytes.into() });
                let mut s = PlaybackSettings::DESPAWN;
                s.start_position = Some(Duration::from_secs_f64(p.elapsed / 1000.0));
                commands.spawn((AudioPlayer(h), s, DespawnOnExit(AppState::Video)));
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
            set_image(&mut images, &p.image, pic);
        }
    } else {
        done = true;
    }
    if done {
        commands.remove_resource::<VideoPlay>();
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The title

#[derive(Resource)]
struct TitleScreen {
    /// When the fade-out ends.
    closing: Option<f64>,
}

fn enter_title(
    mut commands: Commands,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
    mut music: ResMut<Jukebox>,
    layers: Query<(Entity, &Layer)>,
) {
    let p = picture(&mut front.install, cf::TITLE_PICTURE, None);
    spawn_layer(&mut commands, &mut images, p, 0, AppState::Title, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(TitleScreen { closing: None });
    let _ = layers;
    commands.queue(|w: &mut World| {
        let mut q = w.query::<(Entity, &Layer)>();
        let es: Vec<Entity> = q.iter(w).map(|(e, _)| e).collect();
        for e in es {
            w.entity_mut(e).insert(Fade {
                from: 0.0,
                to: 1.0,
                at: 0.0,
                ms: cf::TITLE_FADE_MS as f64,
            });
        }
    });
    music.start(Some(cf::TITLE_MUSIC));
}

#[allow(clippy::too_many_arguments)]
fn run_title(
    mut commands: Commands,
    clock: Res<Clock>,
    mut title: ResMut<TitleScreen>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    layers: Query<(Entity, &Layer)>,
    mut flow: ResMut<Flow>,
    front: Res<Front>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = clock.0;
    match title.closing {
        None => {
            let skipped = pressed(&keys, &mouse) || keys.get_just_pressed().next().is_some();
            let ran = now >= (cf::TITLE_TICKS * cf::screen_tick_ms()) as f64;
            if skipped || ran {
                let ms = cf::title_fade_out_ms(skipped && !ran);
                fade_layer(&mut commands, &layers, 0, 1.0, 0.0, now, ms);
                title.closing = Some(now + ms as f64);
            }
        }
        Some(end) if now >= end => go_on(&mut flow, &front, &keys, &mut next, &mut exit),
        Some(_) => {}
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The main menu and the new-campaign choices

#[derive(Resource)]
struct Menu {
    image: Handle<Image>,
    background: Picture,
    items: Vec<(Rect, String)>,
    hot: Option<usize>,
    drawn: Option<Option<usize>>,
}

fn menu_items(labels: &[&str], top: i32) -> Vec<(Rect, String)> {
    labels
        .iter()
        .enumerate()
        .map(|(i, l)| (rect(170, top + i as i32 * 36, 300, 28), l.to_string()))
        .collect()
}

fn open_menu(commands: &mut Commands, front: &mut Front, images: &mut Assets<Image>, labels: &[&str], state: AppState) {
    let mut bg = picture(&mut front.install, cf::TITLE_PICTURE, None);
    // Darkened so the buttons read.
    for px in bg.rgba.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            *c /= 3;
        }
    }
    // Below the title picture's logo.
    let top = 300 - labels.len() as i32 * 10;
    let image = spawn_layer(commands, images, dup(&bg), 0, state, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(Menu {
        image,
        background: bg,
        items: menu_items(labels, top),
        hot: None,
        drawn: None,
    });
}

/// Hover, redraw, and the item clicked (or chosen by its number key).
fn menu_input(
    menu: &mut Menu,
    front: &Front,
    images: &mut Assets<Image>,
    at: Option<Vec2>,
    mouse: &ButtonInput<MouseButton>,
    keys: &ButtonInput<KeyCode>,
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
        set_image(images, &menu.image, p);
        menu.drawn = Some(menu.hot);
    }
    const DIGITS: [KeyCode; 6] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
    ];
    if let Some(i) = DIGITS.iter().position(|&k| keys.just_pressed(k))
        && i < menu.items.len()
    {
        return Some(i);
    }
    if mouse.just_released(MouseButton::Left) {
        return menu.hot;
    }
    None
}

fn enter_menu(
    mut commands: Commands,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
    mut music: ResMut<Jukebox>,
) {
    music.start_unless_playing(cf::TITLE_MUSIC);
    open_menu(
        &mut commands,
        &mut front,
        &mut images,
        &["New Campaign", "Quit"],
        AppState::MainMenu,
    );
}

#[allow(clippy::too_many_arguments)]
fn run_menu(
    mut menu: ResMut<Menu>,
    front: Res<Front>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<FrontCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut flow: ResMut<Flow>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let at = pointer(&window, &cam);
    let pick = menu_input(&mut menu, &front, &mut images, at, &mouse, &keys, "Tidewater");
    let pick = pick.or(keys.just_pressed(KeyCode::KeyN).then_some(0));
    match pick {
        Some(0) => {
            flow.steps = VecDeque::from([Step::NewCampaign]);
            go_on(&mut flow, &front, &keys, &mut next, &mut exit);
        }
        Some(_) => {
            exit.write(AppExit::Success);
        }
        None if keys.just_pressed(KeyCode::KeyQ) => {
            exit.write(AppExit::Success);
        }
        None => {}
    }
}

fn enter_new_campaign(mut commands: Commands, mut front: ResMut<Front>, mut images: ResMut<Assets<Image>>) {
    let mut labels: Vec<&str> = Choice::ALL.iter().map(|c| c.label()).collect();
    labels.push("Cancel");
    open_menu(&mut commands, &mut front, &mut images, &labels, AppState::NewCampaign);
}

#[allow(clippy::too_many_arguments)]
fn run_new_campaign(
    mut menu: ResMut<Menu>,
    front: Res<Front>,
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<FrontCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut flow: ResMut<Flow>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let at = pointer(&window, &cam);
    let pick = menu_input(&mut menu, &front, &mut images, at, &mouse, &keys, "Choose a campaign");
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
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
        None if keys.just_pressed(KeyCode::Escape) => Some(VecDeque::from([Step::MainMenu])),
        None => None,
    };
    if let Some(s) = steps {
        flow.steps = s;
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Act screens

#[derive(Resource)]
struct ActScreen {
    act: Act,
    timer: ActTimer,
    /// Screen time of the next tick.
    next_tick: f64,
}

fn enter_act(mut commands: Commands, flow: Res<Flow>, mut front: ResMut<Front>, mut images: ResMut<Assets<Image>>) {
    let Some(Step::Act(a)) = flow.current else { return };
    let pal = cf::art(cf::act_palette_picture(&a));
    let p = picture(&mut front.install, &cf::art(a.picture), Some(&pal));
    spawn_layer(&mut commands, &mut images, p, 0, AppState::Act, None);
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
    spawn_layer(&mut commands, &mut images, text, 1, AppState::Act, None);
    commands.insert_resource(Clock(0.0));
    commands.insert_resource(ActScreen {
        act: a,
        timer: ActTimer::default(),
        next_tick: cf::screen_tick_ms() as f64,
    });
    // At 60 only the text shows, fading in.
    commands.queue(|w: &mut World| {
        let mut q = w.query::<(Entity, &Layer)>();
        let es: Vec<(Entity, u8)> = q.iter(w).map(|(e, l)| (e, l.0)).collect();
        for (e, l) in es {
            let f = if l == 1 {
                Fade {
                    from: 0.0,
                    to: 1.0,
                    at: 0.0,
                    ms: cf::ACT_TEXT_FADE_MS as f64,
                }
            } else {
                Fade {
                    from: 0.0,
                    to: 0.0,
                    at: 0.0,
                    ms: 1.0,
                }
            };
            w.entity_mut(e).insert(f);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn run_act(
    mut commands: Commands,
    clock: Res<Clock>,
    mut act: ResMut<ActScreen>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    layers: Query<(Entity, &Layer)>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut close = pressed(&keys, &mouse) && act.timer.may_close_early();
    while clock.0 >= act.next_tick && !close {
        act.next_tick += cf::screen_tick_ms() as f64;
        match act.timer.tick() {
            Some(ActEvent::Picture) => {
                fade_layer(&mut commands, &layers, 0, 0.0, 1.0, clock.0, cf::ACT_PICTURE_FADE_MS);
                let race = cf::race_of(act.act.entry);
                play_once(
                    &mut commands,
                    &mut front,
                    &mut sources,
                    cf::act_sound(race),
                    AppState::Act,
                );
            }
            Some(ActEvent::Close) => close = true,
            None => {}
        }
    }
    if close {
        // The window close is the only quit request here (open question default).
        if let Some(v) = flow.memory.video_after(&act.act, false) {
            flow.steps.push_front(Step::Video(v));
        }
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Briefings

#[derive(Resource)]
struct Briefing {
    image: Handle<Image>,
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

fn enter_briefing(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
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
    let image = spawn_layer(&mut commands, &mut images, dup(&bg), 0, AppState::Briefing, None);
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
    mut images: ResMut<Assets<Image>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = clock.0;
    while b.voice.front().is_some_and(|(at, _)| (*at as f64) <= now) {
        let (_, f) = b.voice.pop_front().unwrap();
        play_once(&mut commands, &mut front, &mut sources, &f, AppState::Briefing);
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
        set_image(&mut images, &b.image, p);
        b.drawn = Some(b.scroll.moved);
    }
    if pressed(&keys, &mouse) || cf::briefing_closes(&b.scroll, now as u64, b.narration_ms) {
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
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
    mut images: ResMut<Assets<Image>>,
    mut cams: Query<&mut Camera, With<FrontCamera>>,
    mut music: ResMut<Jukebox>,
) {
    let (Some(sim), None) = (sim, dialog) else { return };
    let Some(o) = sim.world.players[sim.human as usize].result else {
        return;
    };
    // Music spec: a game that ends fades all sound before the score screen.
    music.fade_stop(true, None);
    for mut c in &mut cams {
        c.clear_color = ClearColorConfig::None;
    }
    let image = spawn_layer(&mut commands, &mut images, blank(), 1, AppState::Game, None);
    commands.insert_resource(OutcomeDialog {
        victory: o == Outcome::Victory,
        notice: false,
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
    if d.notice {
        let s = "Saving isn't available yet.";
        let w = front.font.width(s.as_bytes()) as i32;
        hud::text(&mut p, &front.font, DIALOG.x + (DIALOG.w as i32 - w) / 2, SAVE.y + 4, s);
    } else if d.victory {
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
    mut images: ResMut<Assets<Image>>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<FrontCamera>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut flow: ResMut<Flow>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if dialog.is_added() || dialog.is_changed() {
        let p = draw_dialog(&front, &dialog);
        set_image(&mut images, &dialog.image, p);
    }
    let at = pointer(&window, &cam).filter(|_| mouse.just_released(MouseButton::Left));
    let hit = |r: Rect| at.is_some_and(|p| r.contains(p.x, p.y));
    let button = if hit(CONTINUE) || keys.any_just_pressed([KeyCode::Enter, KeyCode::KeyC]) {
        Some(cf::DialogButton::Continue)
    } else if dialog.victory && !dialog.notice && (hit(SAVE) || keys.just_pressed(KeyCode::KeyS)) {
        Some(cf::DialogButton::Save)
    } else {
        None
    };
    match button.map(cf::dialog_button) {
        Some(cf::DialogResult::End) => {
            let Some(sim) = sim else { return };
            end_with(&mut flow, &mut front, &sim.world, sim.human, dialog.victory);
            go_on(&mut flow, &front, &keys, &mut next, &mut exit);
        }
        // Saving is a placeholder: as after a cancelled save, the victory dialog shows again (with a notice).
        Some(cf::DialogResult::OpenSave) => dialog.notice = !cf::after_save(false),
        None => {}
    }
}

fn close_dialog(mut commands: Commands) {
    commands.remove_resource::<OutcomeDialog>();
}

// ---------------------------------------------------------------------------------------------------------------
// The score screen

#[derive(Resource)]
struct ScoreScreen {
    image: Handle<Image>,
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

fn enter_score(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
    mut music: ResMut<Jukebox>,
) {
    let Some(s) = flow.score.clone() else { return };
    let bg = picture(&mut front.install, cf::score_background(s.race, s.victory), None);
    let image = spawn_layer(&mut commands, &mut images, dup(&bg), 0, AppState::Score, None);
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
    mut images: ResMut<Assets<Image>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(s) = flow.score.clone() else {
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
        return;
    };
    let press = pressed(&keys, &mouse);
    if press && sc.finished {
        let next_step = after_score(&mut flow, &s);
        flow.steps = VecDeque::from([next_step]);
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
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
                cf::NUMBERS_SOUND,
                AppState::Score,
            );
        }
    }
    if sc.drawn != Some(sc.ticks) {
        let (p, finished) = draw_score(&front, &s, &sc.background, sc.ticks, sc.bars_done_at);
        if finished && !sc.finished {
            sc.finished = true;
            play_once(&mut commands, &mut front, &mut sources, cf::RANK_SOUND, AppState::Score);
        }
        set_image(&mut images, &sc.image, p);
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
    image: Handle<Image>,
    background: Picture,
    lines: Vec<String>,
    panel: Rect,
    line_h: u32,
    scroll: Scroll,
    narration_ms: u32,
    voice: VecDeque<(u32, String)>,
    drawn: Option<u32>,
}

#[allow(clippy::too_many_arguments)]
fn open_scroller(
    commands: &mut Commands,
    images: &mut Assets<Image>,
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
    let image = spawn_layer(commands, images, dup(&background), 0, state, None);
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
    });
}

fn enter_finale(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
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
        &mut images,
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

fn enter_credits(
    mut commands: Commands,
    flow: Res<Flow>,
    mut front: ResMut<Front>,
    mut images: ResMut<Assets<Image>>,
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
        &mut images,
        &front,
        blank(),
        &lines,
        rect(80, 0, 480, H),
        Vec::new(),
        &[],
        Some(cf::CREDITS_STEP_MS),
        AppState::Credits,
    );
    music.start_unless_started(crate::music::CREDITS_TRACK);
}

#[allow(clippy::too_many_arguments)]
fn run_scroller(
    mut commands: Commands,
    clock: Res<Clock>,
    mut s: ResMut<Scroller>,
    mut images: ResMut<Assets<Image>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    state: Res<State<AppState>>,
    mut flow: ResMut<Flow>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut music: ResMut<Jukebox>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let now = clock.0;
    while s.voice.front().is_some_and(|(at, _)| (*at as f64) <= now) {
        let (_, f) = s.voice.pop_front().unwrap();
        play_once(&mut commands, &mut front, &mut sources, &f, *state.get());
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
        set_image(&mut images, &s.image, p);
        s.drawn = Some(s.scroll.moved);
    }
    if pressed(&keys, &mouse) || cf::briefing_closes(&s.scroll, now as u64, s.narration_ms) {
        if *state.get() == AppState::Credits {
            // Music spec: the credits' close fades all sound.
            music.fade_stop(true, None);
        }
        go_on(&mut flow, &front, &keys, &mut next, &mut exit);
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
