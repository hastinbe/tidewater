//! Tidewater client. Loads a PUD from the user's installation, runs the simulation at its fixed tick rate and draws it.
//!
//! Controls follow `docs/specs/ui/selecting-and-commanding.md` (`mouse`): left click or drag selects (Shift adds or
//! removes, Ctrl or a double-click takes all of the type on screen, Alt recalls the remembered group), right click
//! gives the right-click command (with `--second-mouse-style`, right click centres the view instead). The command
//! panel (lower left) shows the selection's buttons (`panel`): click one or press its hotkey; commands that need a
//! target take the next left click on the map or minimap, and a right click ends the mode. In placement mode the
//! building's picture follows the pointer, and a click on a site that fails the site check (`tidewater_sim::site`)
//! shows why and keeps the mode. Ctrl+digit sets a control group, the digit recalls it (twice quickly centres the
//! view), Alt+digit centres and recalls. +/- change the game speed, Ctrl+P pauses, Ctrl+C centres on the selection,
//! F2 to F4 jump to a view position remembered with Shift, Ctrl+S mutes, Ctrl+Q or Ctrl+X quits. The view scrolls at
//! the screen edge and with the arrow, Home, End, Page Up, Page Down and keypad keys (`screen`, spec
//! `ui/screen-and-cursors.md`), the mouse wheel zooms, Escape cancels a panel mode and otherwise quits. On the
//! minimap, left click or drag moves the view. Space steps through the latest events (Enhanced rules). Messages,
//! notices and the status bar follow `ui/messages.md` (`messages`).

mod campaign;
mod cheats;
mod fog;
mod frontend;
mod gamemenu;
mod hud;
mod info;
mod jukebox;
mod menu;
mod messages;
mod minimap;
mod mouse;
#[allow(dead_code)] // the disco toggle and the sound dialog's calls are not wired yet
mod music;
mod palette;
mod panel;
mod prefs;
mod savedlg;
mod savefile;
mod savegame;
mod saves;
mod scene;
mod screen;
mod sound;
mod video;
mod view;

use std::path::PathBuf;

use anyhow::Result;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use clap::Parser;
use panel::PanelState;
use std::collections::HashMap;
use tidewater_formats::install::Install;
use tidewater_sim::economy::Event;
use tidewater_sim::{Command, TILE_PX, UnitId, World};

#[derive(Parser)]
#[command(about = "Tidewater map viewer")]
struct Args {
    /// Warcraft II: Battle.net Edition installation directory.
    #[arg(long, env = "TIDEWATER_INSTALL")]
    install: PathBuf,
    /// A .pud file path, or a name inside the archives; it starts at once. Without one the program starts with its
    /// startup videos and title (`docs/specs/game/campaign-flow.md`).
    map: Option<String>,
    /// Development aid: open a screen of the campaign flow: `title`, `menu`, `campaign`, `mission:E`, `act:E`,
    /// `briefing:E`, `score-victory:E` or `score-defeat:E` (synthetic statistics), `ending:E`, `credits:tod`,
    /// `credits:bdp` or `video:N`, with E a mission entry (0 to 51).
    #[arg(long)]
    goto: Option<String>,
    /// Don't play the intro video at startup (the "play the intro" preference bit cleared).
    #[arg(long)]
    no_intro: bool,
    /// Development aid: the game ends at once with `victory` or `defeat` (to see the dialog and the score screen).
    #[arg(long, value_parser = ["victory", "defeat"])]
    demo_outcome: Option<String>,
    /// Game setup options: `campaign:ENTRY`, or `custom` with choices such as
    /// `custom:race=orc,opponents=2,resources=high,terrain=swamp,units=one-worker,fixed-starts,fog=off`.
    /// By default a campaign map starts as its campaign mission and any other map as a custom game.
    #[arg(long, value_parser = tidewater_sim::setup::Setup::parse)]
    setup: Option<tidewater_sim::setup::Setup>,
    /// The folder for saved games. Without it, a `Save` folder in Tidewater's own user data directory; the program
    /// never writes into the installation.
    #[arg(long)]
    save_dir: Option<PathBuf>,
    /// Initial zoom: world pixels per screen pixel (2 shows twice as much map).
    #[arg(long, default_value_t = 1.0)]
    zoom: f32,
    /// Interface scale: window pixels per interface pixel (2 shows the original 640x480 layout in the default window).
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=8))]
    ui_scale: u32,
    /// Development aid: order all of your units to tile `X,Y` at the start.
    #[arg(long, value_parser = parse_tile)]
    demo_move: Option<(i32, i32)>,
    /// Development aid: send every player's units against the nearest enemy within 16 tiles at the start.
    #[arg(long)]
    demo_fight: bool,
    /// Development aid: select your first N units at the start and centre the view on them.
    #[arg(long)]
    demo_select: Option<usize>,
    /// Development aid: with `--demo-select`, select only units of this type number (any owner).
    #[arg(long)]
    demo_type: Option<u8>,
    /// Development aid: with `--demo-select`, start in placement mode for this building type number.
    #[arg(long)]
    demo_place: Option<u8>,
    /// Development aid: with `--demo-place`, the pointer's tile `X,Y` while the pointer is off the map view, and one
    /// placement click there at the start.
    #[arg(long, value_parser = parse_tile)]
    demo_site: Option<(i32, i32)>,
    /// The Preferences dialog's second mouse style: right click centres the view instead of giving a command.
    #[arg(long)]
    second_mouse_style: bool,
    /// The speed dialog's mouse scroll speed, 0 to 6 (fastest).
    #[arg(long, default_value_t = screen::DEFAULT_SPEED, value_parser = clap::value_parser!(u8).range(0..=6))]
    mouse_scroll_speed: u8,
    /// The speed dialog's key scroll speed, 0 to 6 (fastest).
    #[arg(long, default_value_t = screen::DEFAULT_SPEED, value_parser = clap::value_parser!(u8).range(0..=6))]
    key_scroll_speed: u8,
}

fn parse_tile(s: &str) -> Result<(i32, i32), String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok((
        x.trim().parse().map_err(|_| "bad X")?,
        y.trim().parse().map_err(|_| "bad Y")?,
    ))
}

#[derive(Resource)]
struct Loaded(Option<scene::Scene>);

#[derive(Resource)]
struct MapSize(Vec2);

#[derive(Resource)]
struct Sim {
    world: World,
    /// Commands collected since the last tick.
    pending: Vec<Command>,
    human: u8,
    /// Notices for the notice line since the last frame (`messages`).
    notices: Vec<messages::Text>,
    /// The resource bar's shown gold, lumber and oil, counted once per cycle.
    counter: screen::Counter,
    /// Passengers to put ashore one at a time (transport, place 0 to 5), from clicks on the information panel.
    unload_one: Vec<(UnitId, usize)>,
    /// Interface sounds to play this frame (the placement click's).
    ui_sounds: Vec<sound::UiSound>,
}

/// Plays the sim's sound cues (`sound.rs`, spec `docs/specs/ui/sounds.md`): reads the files from the install on first
/// use and keeps the decoded sources by name.
#[derive(Resource)]
struct Speaker {
    director: sound::Director,
    install: Install,
    /// Archive name to source; `None` for a file the install doesn't have, so it is looked up once.
    sources: HashMap<String, Option<Handle<AudioSource>>>,
    /// Ctrl+S turned the sounds off.
    muted: bool,
}

/// The command panel's input state (see `panel`).
#[derive(Resource, Default)]
struct PanelUi {
    mode: panel::Mode,
    /// A panel click or key was handled this frame; the map must not also act on it.
    consumed: bool,
    /// Escape was used by the panel this frame and must not quit.
    escape_used: bool,
    /// The menu button was pressed (`gamemenu` opens the game menu).
    menu_requested: bool,
    /// The tooltip of the button under the cursor, with its cost kind, gold, lumber and oil costs and mana cost.
    tooltip: Option<(String, (u8, panel::Cost, u32))>,
    /// The selection a build menu belongs to; a different selection returns the panel from it.
    last_sel: Vec<UnitId>,
    /// The mouse style (`mouse::Style`).
    style: mouse::Style,
    /// The placement site's top-left tile under the pointer's hot spot, kept while the pointer is off the map view.
    site: Option<(i32, i32)>,
    /// Development aid (`--demo-site`): the pointer's tile while the pointer is off the map view.
    demo_pointer: Option<(i32, i32)>,
    /// Development aid: one placement click at `demo_pointer` is still to be made.
    demo_click: bool,
}

/// The screen's and the messages' state (`screen`, `messages`).
#[derive(Resource, Default)]
struct ScreenUi {
    scroller: screen::Scroller,
    /// The scrolling edge the pointer is at, this frame.
    edge: (i32, i32),
    /// The pointer to draw and the mouse position, when the client draws the pointer.
    pointer: Option<(screen::Pointer, (i32, i32))>,
    /// The views F2 to F4 go to, from game setup's saved views (`World::start`); Shift+F2 to F4 replace them.
    views: [(i32, i32); 3],
    /// The event history, starting at game setup's starting view.
    history: Option<messages::History>,
    messages: messages::Messages,
    status: messages::StatusBar,
    was_paused: bool,
}

/// Links the sprite that shows the building being placed over its site.
#[derive(Component)]
struct Ghost;

/// The information panel's state (`info`, spec `ui/info-panel.md`).
#[derive(Resource, Default)]
struct InfoPanel {
    /// The grid: which unit each slot shows.
    slots: info::Slots,
    /// The selection the grid was last built for.
    built_for: Vec<UnitId>,
    frame: u8,
    /// The text for one selected unit, rebuilt when the selection changes or a change check fires.
    content: Option<info::Content>,
    /// The values the change checks last saw.
    watched: Option<Vec<u32>>,
    /// The slot whose icon is held down.
    pressed: Option<usize>,
    /// The name of the unit under the cursor, for the status line.
    hover: Option<String>,
    /// Speed per unit type, worked out once from the animation scripts.
    speeds: Vec<u8>,
}

/// The local selection and what the selecting rules remember (`mouse`).
#[derive(Resource, Default)]
struct Selection {
    units: Vec<UnitId>,
    /// Where a left-button drag started, in map pixels.
    drag_from: Option<Vec2>,
    /// Shift, Ctrl and the double-click as they were at the press.
    press: mouse::Mods,
    /// The last left press on the map view (seconds, map pixel), for the double-click.
    last_press: Option<(f64, Vec2)>,
    /// A left press on the minimap is moving the view.
    minimap_held: bool,
    remembered: mouse::Remembered,
    groups: mouse::GroupKeys,
    /// The ground marker (effect slot 2), counted in game cycles.
    marker: Option<view::Marker>,
}

impl Selection {
    /// A selecting action changed the local selection: sends it as the synced selection and asks for the selection
    /// sound (sounds spec, "Selecting units": under Classic rules only for one unit).
    fn set(&mut self, sim: &mut Sim, units: Vec<UnitId>, sound: bool) {
        if mouse::sound_wanted(sim.world.rules.enhanced, sound, units.len()) {
            sim.world.select_sound(units[0]);
        }
        sim.pending.push(Command::Select {
            player: sim.human,
            units: units.clone(),
        });
        self.groups.forget();
        self.units = units;
    }
}

/// The terrain picture and what redraws its tiles.
#[derive(Resource)]
struct TerrainArt {
    image: Handle<Image>,
    tileset: tidewater_formats::tileset::Tileset,
}

/// Colour cycling (`palette`): the cycled palette every indexed picture is converted with, and the tiles' used entries.
#[derive(Resource)]
struct Cycling {
    state: palette::Cycling,
    started: std::time::Instant,
    /// Per megatile, the palette entries it uses; filled on first use.
    used: Option<Vec<palette::Entries>>,
}

/// The map view's sprite files, palette and fog shapes (`view`, `fog`).
#[derive(Resource)]
struct Art {
    view: view::Art,
    palette: tidewater_formats::tileset::Palette,
    shapes: fog::Shapes,
}

/// The sprite showing the map view's picture (`view::draw`) over the terrain.
#[derive(Component)]
struct ViewSprite;

#[derive(Resource)]
struct StartZoom(f32);

/// The interface: its art, current layout and the picture it draws into.
#[derive(Resource)]
struct Hud {
    art: hud::HudArt,
    /// Window pixels per interface pixel.
    scale: u32,
    layout: Option<hud::Layout>,
    frame: scene::Picture,
    image: Handle<Image>,
    /// Map size in tiles.
    map: (u32, u32),
    /// The minimap: its redraw countdown, terrain picture and last picture (`minimap`).
    minimap: minimap::Minimap,
}

impl Hud {
    /// The cursor in interface pixels.
    fn cursor(&self, window: &Window) -> Option<Vec2> {
        window.cursor_position().map(|p| p / self.scale as f32)
    }

    /// Whether the cursor is over the map view (and not the interface).
    fn over_map(&self, window: &Window) -> bool {
        match (self.layout, self.cursor(window)) {
            (Some(l), Some(p)) => l.viewport.contains(p.x, p.y),
            _ => false,
        }
    }
}

/// The map view's picture over the terrain, and the placement image over that.
const VIEW_Z: f32 = 10.0;
const PLACE_Z: f32 = 11.0;

#[derive(Component)]
struct MapCamera;

#[derive(Component)]
struct HudSprite;

const HUD_LAYER: usize = 1;

fn main() -> Result<()> {
    let args = Args::parse();
    let install = Install::open(&args.install)?;
    let mut front = frontend::Front::load(install)?;
    let mut flow = frontend::Flow::default();
    // The map given on the command line starts at once; otherwise the startup sequence ("Startup") or a screen
    // named by `--goto`.
    let steps = match (&args.map, &args.goto) {
        (Some(map), _) => vec![frontend::Step::Play {
            map: map.clone(),
            setup: args.setup,
        }],
        (None, Some(g)) => {
            let steps = frontend::goto_steps(g).map_err(anyhow::Error::msg)?;
            if let Some((what, e)) = g.split_once(':')
                && what.starts_with("score-")
            {
                let e: u8 = e.parse()?;
                frontend::synthetic_score(&mut flow, &mut front, e, what == "score-victory");
            }
            steps
        }
        (None, None) => frontend::startup_steps(false, flow.skip_videos, !args.no_intro),
    };
    flow.steps = steps.into();
    let stored = gamemenu::Stored(prefs::Prefs::from_args(
        args.mouse_scroll_speed,
        args.key_scroll_speed,
        args.second_mouse_style,
    ));
    let title = args
        .map
        .as_deref()
        .map_or("Tidewater".to_string(), |m| format!("Tidewater — {m}"));
    let ui_scale = args.ui_scale;
    let save_dir = args.save_dir.clone();
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()).set(WindowPlugin {
            primary_window: Some(Window {
                title,
                resolution: (640 * ui_scale, 480 * ui_scale).into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<frontend::AppState>()
        .insert_resource(front)
        .insert_resource(savegame::SaveFolder(saves::SaveDir::new(
            save_dir.unwrap_or_else(saves::default_dir),
        )))
        .insert_resource(flow)
        .insert_resource(stored)
        .insert_resource(Opts(args))
        .add_plugins(frontend::plugin)
        .add_plugins(gamemenu::plugin)
        .add_systems(OnEnter(frontend::AppState::Game), (start_game, setup).chain())
        .add_systems(OnExit(frontend::AppState::Game), end_game)
        .add_plugins(cheats::plugin)
        .add_systems(FixedUpdate, tick.run_if(frontend::game_running))
        .add_systems(
            Update,
            // Input and game-time systems stop while a menu or outcome dialog is open; drawing goes on, so the map and
            // panels stay drawn under the dialog.
            (
                hud_layout,
                (
                    scroll_view,
                    panel_input,
                    info_input,
                    keys_input,
                    mouse_input,
                    pan_and_zoom,
                    play_sounds,
                    update_messages,
                )
                    .chain()
                    .run_if(frontend::game_running),
                (
                    cycle_palette,
                    redraw_terrain,
                    draw_view,
                    draw_placement,
                    update_info,
                    draw_hud,
                )
                    .chain(),
                quit_on_escape.run_if(frontend::game_running),
            )
                .chain()
                .run_if(in_state(frontend::AppState::Game)),
        )
        .run();
    Ok(())
}

/// The command line, kept for starting games.
#[derive(Resource)]
struct Opts(Args);

/// Loads the game the flow asks for and inserts the game screen's resources (`setup` then spawns its entities). A
/// map that fails to load goes back to the main menu.
fn start_game(world: &mut bevy::prelude::World) {
    let Some((map, setup)) = world.resource_mut::<frontend::Flow>().pending.take() else {
        return;
    };
    if let Err(e) = load_game(world, &map, setup) {
        eprintln!("can't start {map}: {e:#}");
        world.insert_resource(Loaded(None));
        frontend::to_main_menu(&mut world.resource_mut::<frontend::Flow>());
        world
            .resource_mut::<NextState<frontend::AppState>>()
            .set(frontend::AppState::Boot);
    }
}

/// The game options `load_game` reads from the command line.
struct GameArgs {
    install: PathBuf,
    map: String,
    setup: Option<tidewater_sim::setup::Setup>,
    zoom: f32,
    ui_scale: u32,
    demo_move: Option<(i32, i32)>,
    demo_fight: bool,
    demo_select: Option<usize>,
    demo_type: Option<u8>,
    demo_place: Option<u8>,
    demo_site: Option<(i32, i32)>,
    second_mouse_style: bool,
    mouse_scroll_speed: u8,
    key_scroll_speed: u8,
    demo_outcome: Option<bool>,
}

fn load_game(world: &mut bevy::prelude::World, map: &str, setup: Option<tidewater_sim::setup::Setup>) -> Result<()> {
    let a = &world.resource::<Opts>().0;
    let args = GameArgs {
        install: a.install.clone(),
        map: map.to_string(),
        setup,
        zoom: a.zoom,
        ui_scale: a.ui_scale,
        demo_move: a.demo_move,
        demo_fight: a.demo_fight,
        demo_select: a.demo_select,
        demo_type: a.demo_type,
        demo_place: a.demo_place,
        demo_site: a.demo_site,
        second_mouse_style: a.second_mouse_style,
        mouse_scroll_speed: a.mouse_scroll_speed,
        key_scroll_speed: a.key_scroll_speed,
        demo_outcome: a.demo_outcome.as_deref().map(|o| o == "victory"),
    };
    let mut install = Install::open(&args.install)?;
    let options = args
        .setup
        .unwrap_or_else(|| tidewater_sim::setup::Setup::for_map(&args.map));
    // A saved game builds the same game again (same start seed), then its state goes over it.
    let resume = world.resource_mut::<frontend::Flow>().resume.take();
    let seed = resume.as_ref().map(|r| r.header.seed);
    let mut scene = scene::load(&mut install, &args.map, &options, seed)?;
    if let Some(r) = &resume {
        scene
            .world
            .restore(&r.body.world, savegame::reseed())
            .map_err(|e| anyhow::anyhow!("the saved game doesn't fit its map: {e}"))?;
    } else {
        // Timing-and-rng spec, scheduler rule 1: a new game runs its two start-up cycles at once.
        scene.world.start_up();
    }
    world.insert_resource(savegame::GameOrigin {
        map: args.map.clone(),
        setup: options,
        seed: scene.seed,
    });
    println!(
        "{:?} map, {} units, you are player {}: {}",
        scene.era,
        scene.world.units.len(),
        scene.human,
        scene.description
    );
    // The interface follows the local player's race setting (setup spec).
    let race = scene.world.start.race_setting;
    let hud_art = hud::HudArt::load(&mut install, scene.era, race, &scene.palette)?;
    let hz = 1000.0 / tidewater_sim::timing::ENHANCED_MS[tidewater_sim::timing::NORMAL as usize] as f64;
    if let Some(to) = args.demo_move {
        let units = scene
            .world
            .units
            .iter()
            .filter(|u| u.owner == scene.human && !u.building)
            .map(|u| u.id)
            .collect();
        scene.world.step(&[Command::Move {
            player: scene.human,
            units,
            to,
        }]);
    }
    if let Some(v) = args.demo_outcome {
        let h = scene.human as usize;
        scene.world.players[h].result = Some(if v {
            tidewater_sim::victory::Outcome::Victory
        } else {
            tidewater_sim::victory::Outcome::Defeat
        });
    }
    if args.demo_fight {
        let w = &scene.world;
        let orders: Vec<Command> = w
            .units
            .iter()
            .filter_map(|me| {
                let foe = w
                    .units
                    .iter()
                    .filter(|t| w.can_attack(me, t.id))
                    .map(|t| ((t.tile.0 - me.tile.0).abs().max((t.tile.1 - me.tile.1).abs()), t.id))
                    .filter(|&(d, _)| d <= 16)
                    .min()?;
                Some(Command::Attack {
                    player: me.owner,
                    units: vec![me.id],
                    target: foe.1,
                })
            })
            .collect();
        scene.world.step(&orders);
    }
    let size = Vec2::new(scene.terrain.width as f32, scene.terrain.height as f32);
    let view_art = Art {
        view: view::Art::load(&mut install, std::mem::take(&mut scene.sheets)),
        palette: scene.palette.clone(),
        shapes: fog::Shapes::from_vr4(&scene.tileset.minitiles),
    };
    let cycling = Cycling {
        state: match &resume {
            Some(r) => palette::Cycling::loaded(scene.era, &scene.palette, r.body.client.cycles_on),
            None => palette::Cycling::new(scene.era, &scene.palette),
        },
        started: std::time::Instant::now(),
        used: None,
    };
    let mut speaker = Speaker {
        director: sound::Director::new(sound::SoundSet::of_world(&scene.world), scene.era, scene.seed),
        install,
        sources: HashMap::new(),
        muted: false,
    };
    let mut selection = Selection::default();
    if let Some(n) = args.demo_select {
        selection.units = scene
            .world
            .units
            .iter()
            .filter(|u| match args.demo_type {
                Some(t) => u.kind == t,
                None => u.owner == scene.human,
            })
            .take(n)
            .map(|u| u.id)
            .collect();
    }
    let mut panel_ui = PanelUi {
        style: if args.second_mouse_style {
            mouse::Style::Second
        } else {
            mouse::Style::First
        },
        ..default()
    };
    if let Some(kind) = args.demo_place {
        panel_ui.mode = panel::Mode::Place(kind);
        panel_ui.last_sel = selection.units.clone();
        panel_ui.demo_pointer = args.demo_site;
        panel_ui.demo_click = args.demo_site.is_some();
    }
    // The client draws the pointer itself when the install has every pointer of the race.
    let own_pointer = hud_art.has_pointers();
    let screen_ui = ScreenUi {
        scroller: screen::Scroller::new(args.mouse_scroll_speed, args.key_scroll_speed),
        views: scene.world.start.saved_views,
        history: Some(messages::History::new(scene.world.start.view)),
        ..default()
    };
    // Music spec: the game's track (drawn after the map has loaded) plays if no music has started.
    match &resume {
        Some(r) => world
            .resource_mut::<jukebox::Jukebox>()
            .resume_game(options.campaign(), r.body.client.music),
        None => world.resource_mut::<jukebox::Jukebox>().start_game(
            scene.world.start.orc_sounds,
            options.campaign(),
            &mut speaker.director.shuffle,
        ),
    }
    // The flow's record of the game: its kind, the campaign total and the starting controllers (score screen).
    {
        let mut flow = world.resource_mut::<frontend::Flow>();
        let kind = match options.game {
            tidewater_sim::setup::Game::Campaign { entry } => campaign::GameKind::Campaign(entry),
            _ => campaign::GameKind::Custom,
        };
        flow.game = Some(kind);
        flow.total.start_game(kind);
        for (i, p) in scene.world.players.iter().take(8).enumerate() {
            flow.start_controllers[i] = p.controller;
        }
        // A loaded game keeps the flow's record of the saved one, and the screen's position.
        if let Some(r) = &resume {
            savegame::apply_client(&mut flow, &r.body.client);
        }
    }
    let mut q =
        world.query_filtered::<(&mut Window, &mut bevy::window::CursorOptions), With<bevy::window::PrimaryWindow>>();
    for (mut w, mut c) in q.iter_mut(world) {
        w.title = format!("Tidewater — {}", args.map);
        c.visible = !own_pointer;
    }
    if let Some(r) = &resume {
        let (x, y) = r.body.client.screen;
        world.insert_resource(savegame::ResumeLook(Vec2::new(x as f32, y as f32)));
    }
    world.insert_resource(cycling);
    world.insert_resource(Loaded(Some(scene)));
    world.insert_resource(MapSize(size));
    world.insert_resource(StartZoom(args.zoom));
    world.insert_resource(Hud {
        art: hud_art,
        scale: args.ui_scale,
        layout: None,
        frame: scene::Picture {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        },
        image: Handle::default(),
        map: (0, 0),
        minimap: minimap::Minimap::new(0),
    });
    world.insert_resource(Time::<Fixed>::from_hz(hz));
    world.insert_resource(selection);
    world.insert_resource(view_art);
    world.insert_resource(speaker);
    world.insert_resource(panel_ui);
    world.insert_resource(screen_ui);
    world.insert_resource(InfoPanel::default());
    Ok(())
}

/// The game screen's entities: everything drawn or heard that isn't the front end's.
type GameEntities<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Or<(With<Sprite>, With<Camera>, With<AudioPlayer>)>,
        Without<frontend::Persist>,
    ),
>;

/// Leaving the game screen: its entities go (not the front end's camera, pictures or music) and so does the game.
fn end_game(mut commands: Commands, q: GameEntities) {
    for e in &q {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<Sim>();
    commands.remove_resource::<Loaded>();
}

fn image_with(p: scene::Picture, usage: RenderAssetUsages) -> Image {
    Image::new(
        Extent3d {
            width: p.width,
            height: p.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        p.rgba,
        TextureFormat::Rgba8UnormSrgb,
        usage,
    )
}

/// Map pixels (y down) to world space (y up).
fn world(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, -y, z)
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut loaded: ResMut<Loaded>,
    mut images: ResMut<Assets<Image>>,
    size: Res<MapSize>,
    zoom: Res<StartZoom>,
    mut hud: ResMut<Hud>,
    sel: Res<Selection>,
    resume_look: Option<Res<savegame::ResumeLook>>,
) {
    let Some(scene) = loaded.0.take() else { return };
    // Start the view on the selection if there is one, else on game setup's starting view: its top-left tile, with
    // the original 14-tile view's centre in the middle of the window (setup spec, "After the units").
    let (vx, vy) = scene.world.start.view;
    let half = tidewater_sim::setup::VIEW_TILES as f32 / 2.0;
    let look = sel.units.first().and_then(|&id| scene.world.unit(id)).map_or(
        Vec2::new((vx as f32 + half) * TILE_PX as f32, (vy as f32 + half) * TILE_PX as f32),
        |u| Vec2::new(u.centre_px().0 as f32, u.centre_px().1 as f32),
    );
    // A loaded game starts the view where the saved game's view was.
    let look = resume_look.map_or(look, |r| r.0);
    commands.remove_resource::<savegame::ResumeLook>();
    hud.map = (
        scene.terrain.width / TILE_PX as u32,
        scene.terrain.height / TILE_PX as u32,
    );
    hud.minimap = minimap::Minimap::new(hud.map.0);
    hud.image = images.add(image_with(
        scene::Picture {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        },
        RenderAssetUsages::default(),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(HUD_LAYER),
    ));
    commands.spawn((
        Sprite::from_image(hud.image.clone()),
        Transform::default(),
        RenderLayers::layer(HUD_LAYER),
        HudSprite,
    ));
    commands.spawn((
        Camera2d,
        MapCamera,
        Projection::Orthographic(OrthographicProjection {
            scale: zoom.0,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_translation(world(look.x, look.y, 0.0)),
    ));
    // Kept on the CPU side too, so tiles that change in play can be redrawn.
    let terrain = images.add(image_with(scene.terrain, RenderAssetUsages::default()));
    commands.spawn((
        Sprite::from_image(terrain.clone()),
        Transform::from_translation(world(size.0.x / 2.0, size.0.y / 2.0, 0.0)),
    ));
    commands.insert_resource(TerrainArt {
        image: terrain,
        tileset: scene.tileset,
    });
    // The map view's picture, redrawn every frame for the part of the map in view (`draw_view`).
    let blank = || {
        image_with(
            scene::Picture {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
            },
            RenderAssetUsages::default(),
        )
    };
    commands.spawn((
        Sprite::from_image(images.add(blank())),
        Transform::from_translation(world(0.0, 0.0, VIEW_Z)),
        ViewSprite,
    ));
    // A selection made at the start (`--demo-select`) becomes the synced selection on the first cycle.
    let pending = if sel.units.is_empty() {
        Vec::new()
    } else {
        vec![Command::Select {
            player: scene.human,
            units: sel.units.clone(),
        }]
    };
    let counter = {
        let p = &scene.world.players[scene.human as usize];
        screen::Counter::open([p.gold, p.lumber, p.oil])
    };
    commands.insert_resource(Sim {
        world: scene.world,
        pending,
        human: scene.human,
        notices: Vec::new(),
        counter,
        unload_one: Vec::new(),
        ui_sounds: Vec::new(),
    });
    commands.spawn((
        Sprite::from_image(images.add(blank())),
        Transform::default(),
        Visibility::Hidden,
        Ghost,
    ));
}

/// The fixed timestep for the game speed (timing spec): Enhanced rules use the speed's cycle time, Classic rules the
/// mean of its row's three-cycle pattern.
fn cycle_secs(enhanced: bool, speed: u8) -> f64 {
    use tidewater_sim::timing::{CLASSIC_MS, CLASSIC_ROW, ENHANCED_MS};
    let s = (speed as usize).min(ENHANCED_MS.len() - 1);
    let ms = if enhanced {
        ENHANCED_MS[s] as f64
    } else {
        CLASSIC_MS[CLASSIC_ROW[s]].iter().sum::<u32>() as f64 / 3.0
    };
    ms / 1000.0
}

#[allow(clippy::too_many_arguments)]
fn tick(
    mut sim: ResMut<Sim>,
    mut sel: ResMut<Selection>,
    mut time: ResMut<Time<Fixed>>,
    mut hud: ResMut<Hud>,
    mut cycling: ResMut<Cycling>,
    art: Option<Res<TerrainArt>>,
    cam: Query<(&Camera, &Transform, &Projection), With<MapCamera>>,
    mut cheat_queue: ResMut<cheats::Queue>,
    stored: Res<gamemenu::Stored>,
) {
    // Info-panel spec, "Clicking an icon": a click on a passenger puts that one passenger ashore by the transports
    // spec's one-passenger rule (open question there: assumed to be that request). The sim has no command for it, so
    // its one-passenger unload runs here, between ticks, for transports the local player owns.
    for (t, place) in std::mem::take(&mut sim.unload_one) {
        let human = sim.human;
        if let Some(i) = sim.world.units.iter().position(|u| u.id == t && u.owner == human) {
            sim.world.unload_one(i, place);
        }
    }
    let cmds = std::mem::take(&mut sim.pending);
    sim.world.step(&cmds);
    let step = std::time::Duration::from_secs_f64(cycle_secs(sim.world.rules.enhanced, sim.world.speed));
    if time.timestep() != step {
        time.set_timestep(step);
    }
    // Minimap spec, "When it is redrawn": the countdown runs in the game cycle, after the units are updated.
    if let (Some(art), Ok((camera, tf, proj))) = (art, cam.single()) {
        let (l, t, r, b) = view_tiles(camera, tf, proj);
        let at = (l.floor().max(0.0) as i32, t.floor().max(0.0) as i32);
        let size = ((r - l).round().max(1.0) as i32, (b - t).round().max(1.0) as i32);
        hud.minimap
            .cycle(&mut sim.world, &art.tileset, at, size, stored.0.minimap_terrain);
    }
    // A lost unit (dying, or gone inside) leaves the local selection and the remembered groups (corpses spec;
    // selecting spec, "Two selections").
    let w = &sim.world;
    let present = |id: UnitId| w.unit(id).is_some_and(|u| u.present());
    sel.units.retain(|&id| present(id));
    sel.remembered.retain(present);
    // The ground marker's update in this cycle (map-drawing spec, "Overlays").
    if let Some(m) = sel.marker.as_mut()
        && !m.update()
    {
        sel.marker = None;
    }
    let human = sim.human;
    let events = std::mem::take(&mut sim.world.events);
    // A unit that changed owner by rescue leaves the selection (rescue spec, "Owner change").
    for e in &events {
        if let Event::OwnerChanged { unit, .. } = *e {
            // The pulse colour starts for the rest of the game (palette-effects spec, "Cycle 0, the pulse").
            cycling.state.owner_changed();
            sel.units.retain(|&id| id != unit);
        }
    }
    cheat_queue.take_events(&events);
    // Each notice replaces the last on the notice line, so the latest wins.
    if let Some(n) = events.iter().rev().find_map(|e| messages::event_notice(e, human)) {
        sim.notices.push(n);
    }
    // Screen spec, "Counting up and down": once per cycle.
    let p = &sim.world.players[human as usize];
    let real = [p.gold, p.lumber, p.oil];
    sim.counter.update(real);
}

/// Steps the colour cycles (`palette`, spec `ui/palette-effects.md`) and applies the cycled palette: the map view,
/// the interface and the terrain tiles that use the changed entries all convert with it from here on.
#[allow(clippy::too_many_arguments)]
fn cycle_palette(
    sim: Option<Res<Sim>>,
    art: Option<ResMut<Art>>,
    terrain: Option<Res<TerrainArt>>,
    mut cycling: ResMut<Cycling>,
    mut hud: ResMut<Hud>,
    ui: Res<PanelUi>,
    stored: Res<gamemenu::Stored>,
    menu: Option<Res<gamemenu::GameMenu>>,
    windows: Query<&Window>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(sim), Some(mut art), Some(terrain)) = (sim, art, terrain) else {
        return;
    };
    let gate = palette::Gate {
        preference: stored.0.colour_cycling(),
        paused: sim.world.paused,
        menu_open: menu.is_some(),
        focused: windows.single().is_ok_and(|w| w.focused),
    };
    let now = cycling.started.elapsed().as_millis() as u64;
    let placing = matches!(ui.mode, panel::Mode::Place(_));
    let turned = cycling.state.frame(now, gate, placing);
    if turned == 0 {
        return;
    }
    art.palette = cycling.state.palette().clone();
    hud.art.set_palette(cycling.state.palette());
    // Redraw the terrain tiles that use an entry a turn changed (water and coast).
    let changed = cycling.state.cycler().entries_of(turned);
    let ts = &terrain.tileset;
    let used = cycling.used.take().unwrap_or_else(|| {
        (0..ts.megatiles.len())
            .map(|m| {
                let mut px = vec![0u8; TILE_PX as usize * TILE_PX as usize];
                ts.draw_megatile(m, &mut px, TILE_PX as usize);
                palette::Entries::of_pixels(&px)
            })
            .collect()
    });
    if let Some(mut img) = images.get_mut(&terrain.image) {
        let width = img.width() as usize;
        if let Some(data) = img.data.as_mut() {
            let t = TILE_PX as usize;
            let v = &sim.world.vision;
            for y in 0..v.height as i32 {
                for x in 0..v.width as i32 {
                    let Some(m) = sim.world.terrain.tile(x, y) else {
                        continue;
                    };
                    if !used.get(m as usize).is_some_and(|u| u.intersects(&changed)) {
                        continue;
                    }
                    let pic = scene::tile_picture(ts, m as usize, cycling.state.palette());
                    for row in 0..t {
                        let at = ((y as usize * t + row) * width + x as usize * t) * 4;
                        data[at..at + t * 4].copy_from_slice(&pic.rgba[row * t * 4..(row + 1) * t * 4]);
                    }
                }
            }
        }
    }
    cycling.used = Some(used);
}

/// Redraws tiles whose shape changed (felled trees, broken walls) on the terrain picture. The minimap catches up at
/// its next terrain rebuild (`minimap`).
fn redraw_terrain(
    mut sim: ResMut<Sim>,
    art: Res<TerrainArt>,
    cycling: Res<Cycling>,
    mut images: ResMut<Assets<Image>>,
) {
    let changed = std::mem::take(&mut sim.world.terrain.redraw);
    if changed.is_empty() {
        return;
    }
    let Some(mut img) = images.get_mut(&art.image) else {
        return;
    };
    let width = img.width() as usize;
    let Some(data) = img.data.as_mut() else { return };
    let t = TILE_PX as usize;
    for (x, y) in changed {
        let Some(m) = sim.world.terrain.tile(x, y) else {
            continue;
        };
        let pic = scene::tile_picture(&art.tileset, m as usize, cycling.state.palette());
        for row in 0..t {
            let at = ((y as usize * t + row) * width + x as usize * t) * 4;
            data[at..at + t * 4].copy_from_slice(&pic.rgba[row * t * 4..(row + 1) * t * 4]);
        }
    }
}

/// The map pixels in view, clipped to the map: left, top, right and bottom (exclusive).
fn view_px(camera: &Camera, tf: &Transform, proj: &Projection, map: (i32, i32)) -> (i32, i32, i32, i32) {
    let (l, t, r, b) = view_tiles(camera, tf, proj);
    let p = TILE_PX as f32;
    (
        ((l * p).floor() as i32).clamp(0, map.0),
        ((t * p).floor() as i32).clamp(0, map.1),
        ((r * p).ceil() as i32).clamp(0, map.0),
        ((b * p).ceil() as i32).clamp(0, map.1),
    )
}

/// Puts a picture into a sprite's image and sizes the sprite to it.
fn show_picture(images: &mut Assets<Image>, sprite: &mut Sprite, width: i32, height: i32, rgba: Vec<u8>) {
    let (w, h) = (width.max(1) as u32, height.max(1) as u32);
    let rgba = if width > 0 && height > 0 { rgba } else { vec![0; 4] };
    if let Some(mut img) = images.get_mut(&sprite.image) {
        *img = image_with(
            scene::Picture {
                width: w,
                height: h,
                rgba,
            },
            RenderAssetUsages::default(),
        );
    }
    sprite.custom_size = Some(Vec2::new(w as f32, h as f32));
}

/// Draws the map view over the terrain (`view`, spec `ui/map-drawing.md`) every frame, for the part of the map in
/// view: buildings and ghosts, the unit passes, effects, fog, the ground marker and the selection rectangle.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_view(
    sim: Option<ResMut<Sim>>,
    art: Option<Res<Art>>,
    sel: Res<Selection>,
    ui: Res<PanelUi>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &Transform, &Projection, &GlobalTransform), With<MapCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Query<(&mut Sprite, &mut Transform), (With<ViewSprite>, Without<MapCamera>)>,
) {
    let (Some(mut sim), Some(art)) = (sim, art) else { return };
    let (Ok((camera, tf, proj, gtf)), Ok((mut sprite, mut at))) = (cams.single(), shown.single_mut()) else {
        return;
    };
    let v = &sim.world.vision;
    let map = (v.width as i32 * TILE_PX, v.height as i32 * TILE_PX);
    let (l, t, r, b) = view_px(camera, tf, proj, map);
    let mut canvas = view::Canvas::new(l, t, r - l, b - t);
    let drag = match (sel.drag_from, windows.single()) {
        (Some(from), Ok(window)) => cursor_map(window, (camera, gtf)).map(|p| (px(from), px(p))),
        _ => None,
    };
    let local = view::Local {
        selected: &sel.units,
        placing: matches!(ui.mode, panel::Mode::Place(_)),
        marker: sel.marker,
        drag,
    };
    let (frame, stop) = view::frame(&sim.world, &art.view, &canvas, &local);
    // A flashing unit that is gone or not detected stops flashing at once ("Boxes").
    for id in stop {
        if let Some(u) = sim.world.units.iter_mut().find(|u| u.id == id) {
            u.flash = tidewater_sim::rescue::Flash(0);
        }
    }
    view::draw(&mut canvas, &frame, &art.view, &art.shapes);
    let rgba = canvas.rgba(&art.palette);
    show_picture(&mut images, &mut sprite, canvas.width, canvas.height, rgba);
    at.translation = world(
        l as f32 + canvas.width.max(1) as f32 / 2.0,
        t as f32 + canvas.height.max(1) as f32 / 2.0,
        VIEW_Z,
    );
}

/// The map tile holding a point in map pixels.
fn pointer_tile(at: Vec2) -> (i32, i32) {
    ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX))
}

/// The placement image (map-drawing spec, "Placement image"): the building's frame 0 in the local player's colours,
/// with the site check's good and bad tiles as a checkerboard, its top-left on the site's top-left tile. Shown only
/// while the pointer is over the map view (or at the `--demo-site` tile).
#[allow(clippy::too_many_arguments)]
fn draw_placement(
    sim: Option<Res<Sim>>,
    art: Option<Res<Art>>,
    mut images: ResMut<Assets<Image>>,
    ui: Res<PanelUi>,
    sel: Res<Selection>,
    hud: Res<Hud>,
    windows: Query<&Window>,
    mut ghost: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<Ghost>>,
) {
    let Ok((mut sprite, mut tf, mut vis)) = ghost.single_mut() else {
        return;
    };
    let (Some(sim), Some(art)) = (sim, art) else { return };
    let over = windows.single().is_ok_and(|w| hud.over_map(w));
    let pointer = ui.site.filter(|_| over).or(ui.demo_pointer);
    let builder = sel.units.first().and_then(|&id| sim.world.unit(id));
    let (panel::Mode::Place(kind), Some(tile), Some(b)) = (ui.mode, pointer, builder) else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    let size = panel::WorldState {
        world: &sim.world,
        local: sim.human,
    }
    .footprint(kind);
    let site = sim.world.site_at(kind, tile);
    let bad = sim.world.bad_site_tiles(kind, site, b.id);
    let base = view::base_of(&sim.world, sim.human);
    let pic = view::placement_image(art.view.units.get(&kind), base, size, &bad);
    let rgba = pic.rgba(&art.palette);
    show_picture(&mut images, &mut sprite, pic.width, pic.height, rgba);
    tf.translation = world(
        (site.0 * TILE_PX) as f32 + pic.width as f32 / 2.0,
        (site.1 * TILE_PX) as f32 + pic.height as f32 / 2.0,
        PLACE_Z,
    );
    vis.set_if_neq(Visibility::Inherited);
}

/// The cursor position in map pixels (y down), if it is over the window.
fn cursor_map(window: &Window, cam: (&Camera, &GlobalTransform)) -> Option<Vec2> {
    let p = cam.0.viewport_to_world_2d(cam.1, window.cursor_position()?).ok()?;
    Some(Vec2::new(p.x, -p.y))
}

fn pan_and_zoom(
    scroll: Res<AccumulatedMouseScroll>,
    size: Res<MapSize>,
    mut cam: Query<(&Camera, &mut Transform, &mut Projection), With<MapCamera>>,
) {
    let Ok((camera, mut tf, mut proj)) = cam.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = proj.as_mut() else {
        return;
    };
    // + and - are the game speed keys; only the wheel zooms.
    let zoom = -scroll.delta.y * 0.1;
    ortho.scale = (ortho.scale * (1.0 + zoom)).clamp(0.25, 8.0);

    // Runs every frame, so a view moved any other way (minimap, start position) is kept on the map too. The view
    // position is a whole tile (screen spec, "Layout"), so the view's top-left corner goes onto the tile grid first.
    let half = camera.logical_viewport_size().unwrap_or(Vec2::ZERO) * ortho.scale / 2.0;
    let t = TILE_PX as f32;
    let left = ((tf.translation.x - half.x) / t).round() * t;
    let top = ((-tf.translation.y - half.y) / t).round() * t;
    tf.translation.x = keep_on_map(left + half.x, half.x, size.0.x);
    tf.translation.y = -keep_on_map(top + half.y, half.y, size.0.y);
}

/// A view centre on one axis that keeps a view of half-width `half` inside a map `len` long; a map narrower than
/// the view is centred.
fn keep_on_map(c: f32, half: f32, len: f32) -> f32 {
    if 2.0 * half >= len {
        len / 2.0
    } else {
        c.clamp(half, len - half)
    }
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, ui: Res<PanelUi>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) && !ui.escape_used {
        exit.write(AppExit::Success);
    }
}

/// Fits the interface to the window: rebuilds the frame picture when the size changes and keeps the map camera's
/// viewport on the map view.
fn hud_layout(
    windows: Query<&Window>,
    mut hud: ResMut<Hud>,
    mut map_cam: Query<&mut Camera, With<MapCamera>>,
    mut sprite: Query<&mut Transform, With<HudSprite>>,
) {
    let (Ok(window), Ok(mut cam), Ok(mut tf)) = (windows.single(), map_cam.single_mut(), sprite.single_mut()) else {
        return;
    };
    let s = hud.scale;
    let (ww, wh) = (window.width(), window.height());
    let canvas = (
        (ww / s as f32).ceil().max(1.0) as u32,
        (wh / s as f32).ceil().max(1.0) as u32,
    );
    if hud.layout.is_some_and(|l| l.canvas == canvas) {
        return;
    }
    let layout = hud.art.layout(canvas);
    hud.frame = hud.art.frame(&layout);
    hud.layout = Some(layout);
    // Canvas pixel (0, 0) at the window's top-left corner; the HUD camera looks at the window centre.
    let size = Vec2::new(canvas.0 as f32, canvas.1 as f32) * s as f32;
    tf.translation = Vec3::new((size.x - ww) / 2.0, (wh - size.y) / 2.0, 0.0);
    tf.scale = Vec3::new(s as f32, s as f32, 1.0);
    let k = window.scale_factor() * s as f32;
    let phys = window.physical_size();
    let v = layout.viewport;
    let pos = UVec2::new((v.x as f32 * k) as u32, (v.y as f32 * k) as u32).min(phys.saturating_sub(UVec2::ONE));
    let size = UVec2::new((v.w as f32 * k) as u32, (v.h as f32 * k) as u32)
        .min(phys - pos)
        .max(UVec2::ONE);
    cam.viewport = Some(Viewport {
        physical_position: pos,
        physical_size: size,
        ..default()
    });
}

/// The hotkey a key press stands for.
fn key_char(k: KeyCode) -> Option<char> {
    use KeyCode::*;
    Some(match k {
        KeyA => 'A',
        KeyB => 'B',
        KeyC => 'C',
        KeyD => 'D',
        KeyE => 'E',
        KeyF => 'F',
        KeyG => 'G',
        KeyH => 'H',
        KeyI => 'I',
        KeyJ => 'J',
        KeyK => 'K',
        KeyL => 'L',
        KeyM => 'M',
        KeyN => 'N',
        KeyO => 'O',
        KeyP => 'P',
        KeyQ => 'Q',
        KeyR => 'R',
        KeyS => 'S',
        KeyT => 'T',
        KeyU => 'U',
        KeyV => 'V',
        KeyW => 'W',
        KeyX => 'X',
        KeyY => 'Y',
        KeyZ => 'Z',
        Digit0 => '0',
        Digit1 => '1',
        Digit2 => '2',
        Digit3 => '3',
        Digit4 => '4',
        Digit5 => '5',
        Digit6 => '6',
        Digit7 => '7',
        Digit8 => '8',
        Digit9 => '9',
        _ => return None,
    })
}

/// The command panel: clicks and hotkeys on its buttons, and the map click or cancel that follows a target or
/// placement mode (`panel`, spec `ui/command-buttons.md`).
#[allow(clippy::too_many_arguments)]
fn panel_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    sim: Option<ResMut<Sim>>,
    sel: Res<Selection>,
    hud: Res<Hud>,
    scr: Res<ScreenUi>,
    mut ui: ResMut<PanelUi>,
) {
    ui.consumed = false;
    ui.escape_used = false;
    ui.tooltip = None;
    let (Some(mut sim), Ok(window)) = (sim, windows.single()) else {
        return;
    };
    // Screen spec: at a scrolling edge, clicks on the panel do nothing.
    let pressed = buttons.just_pressed(MouseButton::Left) && scr.edge == (0, 0);
    // The menu and pause buttons (screen spec, "Menu buttons").
    if pressed
        && let Some(c) = hud.cursor(window)
        && let Some(b) = screen::menu_button_at((c.x, c.y), pause_button(&sim))
    {
        if b == screen::MenuButton::Pause {
            let player = sim.human;
            sim.pending.push(Command::Pause { player });
        } else {
            ui.menu_requested = true;
        }
        ui.consumed = true;
        return;
    }
    let human = sim.human;
    let (infos, effect) = {
        let state = panel::WorldState {
            world: &sim.world,
            local: human,
        };
        let infos: Vec<panel::UnitInfo> = sel.units.iter().filter_map(|&id| state.unit(id)).collect();
        let ids: Vec<UnitId> = infos.iter().map(|u| u.id).collect();
        // A mode belongs to a selection: none is left without one, and a build menu returns to the set when the
        // selection changes. Target mode stays across a change (Alt+digit works in it).
        let menu_left = ids != ui.last_sel && matches!(ui.mode, panel::Mode::Menu(_));
        if ids.is_empty() || infos[0].owner != human || menu_left {
            ui.mode = panel::Mode::None;
        }
        ui.last_sel = ids;
        let slots = panel::fill(panel::choose_set(ui.mode, &infos, &state), &state, &infos);
        let label = |n: u16| panel::parse_label(&hud.art.string(n));
        let mut effect = panel::Effect::Nothing;
        if let (Some(l), Some(c)) = (hud.layout, hud.cursor(window))
            && let Some(n) = panel::slot_at((l.buttons.x, l.buttons.y), (c.x, c.y))
            && let Some(en) = slots[n]
        {
            let l = label(en.string);
            ui.tooltip = Some((panel::tooltip(en, &l), panel::costs(en, &l, &state)));
            if pressed {
                effect = panel::activate(en, &state, &infos);
                ui.consumed = true;
            }
        }
        if keys.just_pressed(KeyCode::Escape) && ui.mode != panel::Mode::None {
            ui.escape_used = true;
        }
        // Ctrl and Alt make the in-game keys (`keys_input`), not hotkeys.
        let held = keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ]);
        if !ui.consumed && !held {
            for &key in keys.get_just_pressed() {
                let hk = if key == KeyCode::Escape {
                    Some(panel::Hotkey::Escape)
                } else {
                    key_char(key).map(panel::Hotkey::Char)
                };
                let Some(n) = hk.and_then(|hk| panel::hotkey_slot(&slots, &label, hk)) else {
                    continue;
                };
                if let Some(en) = slots[n] {
                    effect = panel::activate(en, &state, &infos);
                    ui.consumed = true;
                    ui.escape_used |= key == KeyCode::Escape;
                    break;
                }
            }
        }
        if !ui.consumed && ui.escape_used {
            effect = panel::Effect::SetMode(panel::Mode::None);
            ui.consumed = true;
        }
        (infos, effect)
    };
    match effect {
        panel::Effect::Nothing => {}
        panel::Effect::Send(c) => {
            sim.pending.push(c);
            if let Some(u) = infos.first() {
                sim.world.acknowledge(u.id);
            }
        }
        panel::Effect::Message(n) => sim.notices.push(messages::Text::Stat(n)),
        panel::Effect::SetMode(m) => ui.mode = m,
    }
}

/// Runs `f` on the selecting rules' snapshot of the game for the local player (`mouse::Board`), with the map view's
/// area `view` (in tiles) and the sprites' pixels for the click rules' pixel test.
fn with_board<R>(
    world: &World,
    local: u8,
    art: Option<&Art>,
    view: (f32, f32, f32, f32),
    f: impl FnOnce(&mouse::Board) -> R,
) -> R {
    let t = TILE_PX as f32;
    let rect = mouse::PxRect {
        l: (view.0 * t) as i32,
        t: (view.1 * t) as i32,
        r: (view.2 * t) as i32 - 1,
        b: (view.3 * t) as i32 - 1,
    };
    let opaque = |c: &mouse::Cand, p: (i32, i32)| {
        let (Some(u), Some(art)) = (world.unit(c.id), art) else {
            return false;
        };
        art.view.units.get(&u.kind).is_some_and(|g| {
            let (f, flip) = scene::sprite_frame(u, g.frames.len());
            scene::opaque(g, f, flip, u.centre_px(), p)
        })
    };
    let board = mouse::Board::of_world(world, local, rect, (view.2 - view.0).round() as i32, &opaque);
    f(&board)
}

fn px(p: Vec2) -> (i32, i32) {
    (p.x.floor() as i32, p.y.floor() as i32)
}

/// Starts the feedback of a command: the target's box flashes (its countdown, drawn by the box rule of
/// `view::box_colour`), or the ground marker shows where the click was.
fn feedback(sim: &mut Sim, sel: &mut Selection, target: Option<(UnitId, u8)>, at: (i32, i32)) {
    match target {
        Some((t, _)) => {
            if let Some(u) = sim.world.units.iter_mut().find(|u| u.id == t) {
                u.flash = tidewater_sim::rescue::Flash(mouse::FLASH_CYCLES as u8);
            }
        }
        None => sel.marker = Some(view::Marker::new(at)),
    }
}

/// The unit a map click at `at` (map pixels) on `tile` targets, with its flash colour.
fn click_target(b: &mouse::Board, world: &World, at: Vec2, tile: (i32, i32)) -> Option<(UnitId, u8)> {
    let hovered = mouse::hover(b, px(at)).first().copied();
    let explored = !world.vision.unexplored(tile.0, tile.1);
    let t = mouse::target(b, hovered, tile, world.rules.reveal_map, explored)?;
    Some((t, mouse::flash_colour(b, b.get(t)?.owner)))
}

/// The right-click command ("Right-click on the map"): from the map view at `at`, or from the minimap (`at` is
/// `None`) to `tile`.
fn right_click(
    sim: &mut Sim,
    sel: &mut Selection,
    art: Option<&Art>,
    view: (f32, f32, f32, f32),
    at: Option<Vec2>,
    tile: (i32, i32),
) {
    let human = sim.human;
    let (target, sends) = with_board(&sim.world, human, art, view, |b| {
        let target = at.and_then(|at| click_target(b, &sim.world, at, tile));
        let sends = mouse::right_click_sends(b, &sel.units, target.map(|t| t.0), sim.world.paused);
        (target, sends)
    });
    if !sends {
        return;
    }
    sim.pending.push(Command::RightClick {
        player: human,
        target: target.map(|t| t.0),
        tile,
    });
    sim.world.acknowledge(sel.units[0]);
    let mark = at.map_or((tile.0 * TILE_PX + 8, tile.1 * TILE_PX + 8), px);
    feedback(sim, sel, target, mark);
}

/// A target-mode click: the mode's command at the clicked unit or tile (`at` is `None` on the minimap), then the
/// mode ends.
fn target_click(
    sim: &mut Sim,
    sel: &mut Selection,
    ui: &mut PanelUi,
    art: Option<&Art>,
    view: (f32, f32, f32, f32),
    at: Option<Vec2>,
    tile: (i32, i32),
) {
    let panel::Mode::Target(t) = ui.mode else { return };
    ui.mode = panel::Mode::None;
    let human = sim.human;
    let Some(&primary) = sel.units.first() else { return };
    let hit = at.and_then(|at| with_board(&sim.world, human, art, view, |b| click_target(b, &sim.world, at, tile)));
    if let Some(c) = panel::target_command(t, human, hit.map(|h| h.0), tile) {
        sim.pending.push(c);
        sim.world.acknowledge(primary);
        let mark = at.map_or((tile.0 * TILE_PX + 8, tile.1 * TILE_PX + 8), px);
        feedback(sim, sel, hit, mark);
    }
}

/// Placement mode's click (spec `ui/placing-buildings.md`, "The click"): a refused site keeps the mode on.
fn place_click(sim: &mut Sim, builder: UnitId, ui: &mut PanelUi, kind: u8, tile: (i32, i32)) {
    match panel::place_click(&sim.world, sim.human, builder, kind, tile) {
        panel::PlaceClick::Build(command) => {
            sim.ui_sounds.push(sound::UiSound::Thunk);
            sim.world.acknowledge(builder);
            sim.pending.push(command);
            ui.mode = panel::Mode::None;
        }
        panel::PlaceClick::Refused(n) => {
            sim.ui_sounds.push(sound::UiSound::Error);
            sim.notices.push(messages::Text::Stat(n));
        }
    }
}

/// Moves the map view's centre to map pixel `c`; `pan_and_zoom` keeps it on the map.
fn centre_view(tf: &mut Transform, c: Vec2) {
    tf.translation.x = c.x;
    tf.translation.y = -c.y;
}

/// The mouse on the map view, minimap and panel by the handler table (`mouse::action`, spec
/// `ui/selecting-and-commanding.md`, "Mouse regions"): selecting, the right-click command, target-mode and placement
/// clicks, and moving the view.
#[allow(clippy::too_many_arguments)]
fn mouse_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    windows: Query<&Window>,
    mut cams: Query<(&Camera, &GlobalTransform, &mut Transform, &Projection), With<MapCamera>>,
    sim: Option<ResMut<Sim>>,
    art: Option<Res<Art>>,
    mut sel: ResMut<Selection>,
    hud: Res<Hud>,
    mut ui: ResMut<PanelUi>,
    mut scr: ResMut<ScreenUi>,
) {
    scr.pointer = None;
    let (Some(mut sim), Ok(window), Ok((camera, gt, mut tf, proj)), Some(layout)) =
        (sim, windows.single(), cams.single_mut(), hud.layout)
    else {
        return;
    };
    let (sim, sel, ui, art) = (&mut *sim, &mut *sel, &mut *ui, art.as_deref());
    let enhanced = sim.world.rules.enhanced;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let at = cursor_map(window, (camera, gt));
    let view = view_tiles(camera, &tf, proj);
    // Development aid (`--demo-site`): one placement click, after the first fog refresh so the site is explored.
    if ui.demo_click
        && let panel::Mode::Place(kind) = ui.mode
        && let (Some(tile), Some(&builder)) = (ui.demo_pointer, sel.units.first())
        && sim.world.tick >= 2
    {
        ui.demo_click = false;
        place_click(sim, builder, ui, kind, tile);
    }
    if buttons.just_released(MouseButton::Left) {
        sel.minimap_held = false;
        if let (Some(from), Some(to)) = (sel.drag_from.take(), at) {
            let press = mouse::Mods { alt, ..sel.press };
            let pick = with_board(&sim.world, sim.human, art, view, |b| {
                mouse::release(b, px(from), px(to), press, &sel.units, &mut sel.remembered)
            });
            if let mouse::Pick::Set { units, sound } = pick {
                sel.set(sim, units, sound);
            }
        }
    }
    // Minimap spec, "Moving the view with the minimap": presses in screen pixels.
    let width = hud.map.0;
    let geom = view_geom(camera, &tf, proj, hud.map);
    let pointer = hud.cursor(window).map(|c| (c.x.floor() as i32, c.y.floor() as i32));
    let regions = mouse::Regions::of_layout(&layout);
    let on_minimap = hud
        .cursor(window)
        .is_some_and(|c| regions.at((c.x, c.y)) == mouse::Region::Minimap);
    let minimap_tile = pointer
        .filter(|_| on_minimap)
        .map(|p| minimap::tile_for_press(width, p));
    if sel.minimap_held
        && buttons.pressed(MouseButton::Left)
        && let Some(p) = pointer
    {
        move_view(
            camera,
            &mut tf,
            proj,
            minimap::view_for_press(width, p, geom.tiles, geom.max()),
        );
    }
    let mode = match ui.mode {
        panel::Mode::Target(_) => mouse::ModeKind::Target,
        panel::Mode::Place(_) => mouse::ModeKind::Place,
        _ => mouse::ModeKind::Normal,
    };
    let map_tile = at.map(pointer_tile);
    // Screen spec, "Placement": the site follows the hot spot's tile while the pointer is in the map view.
    if !matches!(ui.mode, panel::Mode::Place(_)) {
        ui.site = None;
    } else if hud.over_map(window) && map_tile.is_some() {
        ui.site = map_tile;
    }
    // Screen spec, "Pointer shapes".
    if let Some(p) = pointer {
        let region = regions.at((p.0 as f32, p.1 as f32));
        let over = match region {
            mouse::Region::MapView => screen::Over::MapView,
            mouse::Region::Minimap => screen::Over::Minimap,
            _ => screen::Over::Other,
        };
        let hovered: Vec<u8> = match (over, at) {
            (screen::Over::MapView, Some(at)) => with_board(&sim.world, sim.human, art, view, |b| {
                mouse::hover(b, px(at))
                    .iter()
                    .filter_map(|&id| b.get(id).map(|c| c.owner))
                    .collect()
            }),
            _ => Vec::new(),
        };
        let shape = screen::pointer(&screen::PointerInput {
            waiting: false,
            edge: scr.edge,
            paused: sim.world.paused,
            over,
            mode: match mode {
                mouse::ModeKind::Target => screen::Mode::Target,
                mouse::ModeKind::Place => screen::Mode::Place,
                mouse::ModeKind::Normal => screen::Mode::Normal,
            },
            hovered: &hovered,
            local: sim.human,
        });
        scr.pointer = Some((shape, p));
    }
    let Some(cursor) = hud.cursor(window).filter(|_| !ui.consumed) else {
        return;
    };
    let region = regions.at((cursor.x, cursor.y));
    for (button, b) in [
        (MouseButton::Left, mouse::Button::Left),
        (MouseButton::Right, mouse::Button::Right),
    ] {
        if !buttons.just_pressed(button) {
            continue;
        }
        match mouse::action(mode, ui.style, region, b, sim.world.paused, scr.edge != (0, 0)) {
            mouse::Action::Nothing => {}
            mouse::Action::Select => {
                let Some(at) = at else { continue };
                let now = time.elapsed_secs_f64();
                let double = enhanced
                    && sel.last_press.is_some_and(|(t, p)| {
                        now - t <= mouse::DOUBLE_MS as f64 / 1000.0
                            && (at - p).abs().max_element() <= mouse::SMALL as f32
                    });
                sel.last_press = Some((now, at));
                sel.press = mouse::Mods {
                    shift,
                    ctrl,
                    alt: false,
                    double,
                };
                sel.drag_from = Some(at);
            }
            mouse::Action::RightClick => {
                if let Some(tile) = map_tile {
                    right_click(sim, sel, art, view, at, tile);
                }
            }
            mouse::Action::RightClickTile => {
                if let Some(tile) = minimap_tile {
                    right_click(sim, sel, art, view, None, tile);
                }
            }
            mouse::Action::CentreView => {
                if let Some(t) = map_tile {
                    centre_view(&mut tf, Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5) * TILE_PX as f32);
                }
            }
            mouse::Action::MoveView => {
                if let Some(p) = pointer {
                    move_view(
                        camera,
                        &mut tf,
                        proj,
                        minimap::view_for_press(width, p, geom.tiles, geom.max()),
                    );
                    sel.minimap_held = b == mouse::Button::Left;
                }
            }
            mouse::Action::GiveTarget => {
                if let Some(tile) = map_tile {
                    target_click(sim, sel, ui, art, view, at, tile);
                }
            }
            mouse::Action::GiveTargetTile => {
                if let Some(tile) = minimap_tile {
                    target_click(sim, sel, ui, art, view, None, tile);
                }
            }
            mouse::Action::Place => {
                if let (panel::Mode::Place(kind), Some(tile), Some(&builder)) = (ui.mode, map_tile, sel.units.first()) {
                    place_click(sim, builder, ui, kind, tile);
                }
            }
            mouse::Action::EndMode | mouse::Action::EndModeOrBack => ui.mode = panel::Mode::None,
        }
    }
}

/// The key a key press stands for in the in-game key table.
fn game_key(k: KeyCode) -> Option<mouse::Key> {
    use KeyCode::*;
    Some(match k {
        Equal | NumpadAdd => mouse::Key::Plus,
        Minus | NumpadSubtract => mouse::Key::Minus,
        F1 => mouse::Key::F(1),
        F2 => mouse::Key::F(2),
        F3 => mouse::Key::F(3),
        F4 => mouse::Key::F(4),
        F5 => mouse::Key::F(5),
        F6 => mouse::Key::F(6),
        F7 => mouse::Key::F(7),
        F8 => mouse::Key::F(8),
        F9 => mouse::Key::F(9),
        F10 => mouse::Key::F(10),
        F11 => mouse::Key::F(11),
        F12 => mouse::Key::F(12),
        k => match key_char(k)? {
            c @ '0'..='9' => mouse::Key::Digit(c as u8 - b'0'),
            c => mouse::Key::Letter(c),
        },
    })
}

/// The in-game keys (`mouse::key_action`, spec `ui/selecting-and-commanding.md`, "Keys" and "Control groups"). The
/// ones that open menus and dialogs, and Ctrl+S, are handled by `gamemenu`.
#[allow(clippy::too_many_arguments)]
fn keys_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    sim: Option<ResMut<Sim>>,
    art: Option<Res<Art>>,
    mut sel: ResMut<Selection>,
    mut stored: ResMut<gamemenu::Stored>,
    ui: Res<PanelUi>,
    hud: Res<Hud>,
    mut scr: ResMut<ScreenUi>,
    mut cams: Query<(&Camera, &mut Transform, &Projection), With<MapCamera>>,
) {
    let (Some(mut sim), Ok((camera, mut tf, proj))) = (sim, cams.single_mut()) else {
        return;
    };
    if ui.consumed {
        return;
    }
    let (sim, sel) = (&mut *sim, &mut *sel);
    let human = sim.human;
    // Messages spec, "Event history": the space key (no chat line yet to take it).
    if keys.just_pressed(KeyCode::Space)
        && let Some(h) = &mut scr.history
    {
        let now = (time.elapsed_secs_f64() * 1000.0) as u64;
        if let Some(v) = h.space(now, sim.world.rules.enhanced) {
            let geom = view_geom(camera, &tf, proj, hud.map);
            move_view(camera, &mut tf, proj, geom.clamp(v));
        }
    }
    let m = mouse::KeyMods {
        shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        ctrl: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
    };
    for &k in keys.get_just_pressed() {
        // The client plays one human player and has no chat line yet.
        let Some(a) = game_key(k).and_then(|key| mouse::key_action(key, m, false, sim.world.rules.enhanced, false))
        else {
            continue;
        };
        match a {
            mouse::KeyAction::Speed(faster) => sim.pending.push(Command::Speed { player: human, faster }),
            mouse::KeyAction::Pause => sim.pending.push(Command::Pause { player: human }),
            mouse::KeyAction::Group(key, g) => {
                let view = view_tiles(camera, &tf, proj);
                let group = sim.world.selections[human as usize]
                    .groups
                    .get(g as usize)
                    .cloned()
                    .unwrap_or_default();
                let mode = matches!(ui.mode, panel::Mode::Target(_) | panel::Mode::Place(_));
                let now = (time.elapsed_secs_f64() * 1000.0) as u64;
                let (plan, half) = with_board(&sim.world, human, art.as_deref(), view, |b| {
                    (sel.groups.press(b, key, g, &group, mode, now), b.view.view_tiles / 2)
                });
                if let Some(t) = plan.view {
                    let c = Vec2::new((t.0 + half) as f32 + 0.5, (t.1 + half) as f32 + 0.5);
                    centre_view(&mut tf, c * TILE_PX as f32);
                }
                if let Some(units) = plan.select {
                    if let Some(&first) = units.first() {
                        sim.world.select_sound(first);
                    }
                    sel.units = units;
                }
                match plan.send {
                    Some(mouse::GroupSend::Set(group)) => sim.pending.push(Command::SetGroup { player: human, group }),
                    Some(mouse::GroupSend::Recall(group)) => {
                        sim.pending.push(Command::RecallGroup { player: human, group })
                    }
                    None => {}
                }
            }
            mouse::KeyAction::CentrePrimary => {
                // Info-panel spec: an own unit, or one that isn't invisible.
                if let Some(u) = sel.units.first().and_then(|&id| sim.world.unit(id))
                    && (u.owner == human || u.invisibility == 0)
                {
                    centre_view(&mut tf, Vec2::new(u.centre_px().0 as f32, u.centre_px().1 as f32));
                }
            }
            // Game-menu spec, Ctrl+M: the stored music volume swaps with 0; `sync_settings` passes it to the music.
            mouse::KeyAction::ToggleMusic => {
                stored.0.toggle_music();
            }
            mouse::KeyAction::GotoView(n) => {
                let geom = view_geom(camera, &tf, proj, hud.map);
                move_view(camera, &mut tf, proj, geom.clamp(scr.views[n as usize]));
            }
            mouse::KeyAction::RememberView(n) => {
                scr.views[n as usize] = view_geom(camera, &tf, proj, hud.map).pos;
            }
            _ => {}
        }
    }
}

/// The main view in tiles: left, top, right, bottom.
fn view_tiles(camera: &Camera, tf: &Transform, proj: &Projection) -> (f32, f32, f32, f32) {
    let zoom = match proj {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    let half = camera.logical_viewport_size().unwrap_or(Vec2::ONE) * zoom / 2.0;
    let centre = Vec2::new(tf.translation.x, -tf.translation.y);
    let t = TILE_PX as f32;
    (
        (centre.x - half.x) / t,
        (centre.y - half.y) / t,
        (centre.x + half.x) / t,
        (centre.y + half.y) / t,
    )
}

/// Plays the cues the sim queued since the last frame (sounds spec, "How a sound is played"), and shows their
/// subtitles on the notice line.
fn play_sounds(
    mut commands: Commands,
    sim: Option<ResMut<Sim>>,
    mut speaker: ResMut<Speaker>,
    mut sources: ResMut<Assets<AudioSource>>,
    cam: Query<(&Camera, &Transform, &Projection), With<MapCamera>>,
) {
    let (Some(mut sim), Ok((camera, tf, proj))) = (sim, cam.single()) else {
        return;
    };
    let ui_sounds = std::mem::take(&mut sim.ui_sounds);
    if sim.world.sounds.cues.is_empty() && ui_sounds.is_empty() {
        return;
    }
    let (l, t, r, b) = view_tiles(camera, tf, proj);
    let view = sound::View {
        x: l.floor() as i32,
        y: t.floor() as i32,
        w: (r - l).floor() as i32,
        h: (b - t).floor() as i32,
    };
    let cues = std::mem::take(&mut sim.world.sounds.cues);
    let sp = &mut *speaker;
    if sp.muted {
        return;
    }
    let mut heard: Vec<sound::Heard> = ui_sounds.into_iter().map(|s| sp.director.hear_ui(s)).collect();
    heard.extend(cues.iter().map(|cue| sp.director.hear(cue, view)));
    for heard in heard {
        // Messages spec: a subtitle (the under-attack ones included) goes to the notice line.
        if let Some(n) = heard.subtitle {
            sim.notices.push(messages::Text::Net(n));
        }
        let Some((name, level)) = heard.play else { continue };
        let path = sound::path(&name);
        let install = &mut sp.install;
        let handle = sp
            .sources
            .entry(path)
            .or_insert_with_key(|p| {
                let bytes = install.read(p).ok()?;
                Some(sources.add(AudioSource { bytes: bytes.into() }))
            })
            .clone();
        if let Some(h) = handle {
            // Our choice: a level maps linearly to loudness (the game's volume table isn't specified).
            commands.spawn((
                AudioPlayer(h),
                PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(level as f32 / 255.0)),
            ));
        }
    }
}

/// The information panel's icons: pointing names the unit on the status line, a released click selects, removes,
/// unloads or centres the view (`info::click`).
#[allow(clippy::too_many_arguments)]
fn info_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    sim: Option<ResMut<Sim>>,
    mut sel: ResMut<Selection>,
    hud: Res<Hud>,
    mut ui: ResMut<PanelUi>,
    mut panel: ResMut<InfoPanel>,
    scr: Res<ScreenUi>,
    mut cam: Query<&mut Transform, With<MapCamera>>,
) {
    panel.hover = None;
    let (Some(mut sim), Ok(window), Some(l)) = (sim, windows.single(), hud.layout) else {
        return;
    };
    let slot = hud
        .cursor(window)
        .and_then(|c| info::slot_at((c.x - l.info.x as f32, c.y - l.info.y as f32)))
        .filter(|&n| panel.slots[n].is_some());
    let slots = panel.slots;
    let unit = |n: usize| slots[n].and_then(|id| sim.world.unit(id));
    panel.hover = slot
        .and_then(&unit)
        .and_then(|u| info::name_entry(u.kind))
        .map(|e| hud.art.string(e));
    if buttons.just_pressed(MouseButton::Left) && slot.is_some() && !ui.consumed && scr.edge == (0, 0) {
        panel.pressed = slot;
        ui.consumed = true;
    }
    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    let Some(n) = panel.pressed.take().filter(|&n| slot == Some(n)) else {
        return;
    };
    ui.consumed = true;
    let one = (sel.units.len() == 1).then(|| unit(0)).flatten();
    let transport = one.is_some_and(|u| {
        sim.world
            .stats
            .0
            .get(u.kind as usize)
            .is_some_and(|s| s.flags & tidewater_formats::udta::flags::TRANSPORT != 0)
    });
    let centre_ok = one.is_some_and(|u| u.owner == sim.human || u.invisibility == 0);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    match info::click(&slots, n, sel.units.len(), shift, transport, centre_ok) {
        // Selection changes are sent as the synced selection; our reading: with the selection sound, as a click.
        info::Click::SelectOnly(id) => sel.set(&mut sim, vec![id], true),
        info::Click::Remove(id) => {
            let rest = sel.units.iter().copied().filter(|&u| u != id).collect();
            sel.set(&mut sim, rest, false);
        }
        info::Click::Unload { transport, place } => sim.unload_one.push((transport, place)),
        info::Click::Centre(id) => {
            if let (Some(u), Ok(mut tf)) = (sim.world.unit(id), cam.single_mut()) {
                let (x, y) = u.centre_px();
                tf.translation.x = x as f32;
                tf.translation.y = -(y as f32);
            }
        }
        info::Click::Nothing => {}
    }
}

/// Rebuilds the information panel when the selection changes (two-pass slot assignment, the selection put in slot
/// order so its first unit is the primary unit) and its text when a change check fires (`info::watch`). The icons
/// and health bars are drawn from the sim every frame, which redraws each icon when its hit points change.
fn update_info(sim: Option<Res<Sim>>, mut sel: ResMut<Selection>, mut panel: ResMut<InfoPanel>) {
    let Some(sim) = sim else { return };
    let w = &sim.world;
    if panel.speeds.is_empty() {
        panel.speeds = info::speeds(&w.scripts, &w.stats.0);
    }
    let rebuild = sel.units != panel.built_for;
    if rebuild {
        let slots = info::assign(&panel.slots, &sel.units);
        sel.units = info::in_slot_order(&slots);
        panel.slots = slots;
        panel.built_for = sel.units.clone();
    }
    let one = match sel.units.as_slice() {
        [id] => info::facts(w, *id, sim.human),
        _ => None,
    };
    // An own transport shows its passengers in slots 3 to 8.
    if let (Some(s), [id]) = (one, sel.units.as_slice())
        && s.own
        && let Some(u) = w.unit(*id)
        && w.stats
            .0
            .get(u.kind as usize)
            .is_some_and(|t| t.flags & tidewater_formats::udta::flags::TRANSPORT != 0)
    {
        info::with_passengers(&mut panel.slots, &u.passengers);
    }
    // The checks run only while exactly one unit is selected.
    let watched = one.and_then(|_| info::watch(w, &panel.slots, sim.human));
    if !rebuild && (watched.is_none() || watched == panel.watched) {
        return;
    }
    panel.watched = watched;
    panel.frame = info::frame(one.as_ref());
    panel.content = one.map(|s| {
        let o = w.unit(sel.units[0]).map_or(sim.human, |u| u.owner);
        let tables = info::Tables {
            stats: &w.stats.0,
            upgrades: &w.upgrade_data,
            speeds: &panel.speeds,
        };
        info::content(&s, &info::owner_facts(w, o), &tables)
    });
}

/// The view position, its size in tiles and the map's (screen spec, "Layout"), from the map camera.
fn view_geom(camera: &Camera, tf: &Transform, proj: &Projection, map: (u32, u32)) -> screen::ViewGeom {
    let (l, t, r, b) = view_tiles(camera, tf, proj);
    let size = |n: f32| {
        let n = n.round() as i32;
        // Before the camera has a viewport: the original screen's view.
        if n < 1 { minimap::VIEW_TILES } else { n }
    };
    screen::ViewGeom {
        pos: (l.round() as i32, t.round() as i32),
        tiles: (size(r - l), size(b - t)),
        map: (map.0 as i32, map.1 as i32),
    }
}

/// Moves the map view so that its top-left tile is `pos`; `pan_and_zoom` keeps it on the map.
fn move_view(camera: &Camera, tf: &mut Transform, proj: &Projection, pos: (i32, i32)) {
    let (l, t, _, _) = view_tiles(camera, tf, proj);
    let px = TILE_PX as f32;
    tf.translation.x += (pos.0 as f32 - l) * px;
    tf.translation.y -= (pos.1 as f32 - t) * px;
}

/// Scrolling at the screen edge and with the keys (`screen`, spec `ui/screen-and-cursors.md`, "Scrolling"); also sets
/// the scrolling edge that blocks this frame's clicks.
#[allow(clippy::too_many_arguments)]
fn scroll_view(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    windows: Query<&Window>,
    sim: Option<Res<Sim>>,
    hud: Res<Hud>,
    mut scr: ResMut<ScreenUi>,
    mut cams: Query<(&Camera, &mut Transform, &Projection), With<MapCamera>>,
) {
    scr.edge = (0, 0);
    let (Some(sim), Ok(window), Ok((camera, mut tf, proj)), Some(l)) =
        (sim, windows.single(), cams.single_mut(), hud.layout)
    else {
        return;
    };
    let geom = view_geom(camera, &tf, proj, hud.map);
    // No menus or demo yet: only a pause blocks scrolling.
    let blocked = sim.world.paused;
    if !blocked && let Some(c) = hud.cursor(window) {
        let canvas = (l.canvas.0 as i32, l.canvas.1 as i32);
        scr.edge = screen::edge((c.x.floor() as i32, c.y.floor() as i32), canvas, &geom);
    }
    use screen::ScrollKey as K;
    let held: Vec<K> = [
        (KeyCode::ArrowUp, K::Up),
        (KeyCode::ArrowDown, K::Down),
        (KeyCode::ArrowLeft, K::Left),
        (KeyCode::ArrowRight, K::Right),
        (KeyCode::Home, K::Home),
        (KeyCode::PageUp, K::PageUp),
        (KeyCode::End, K::End),
        (KeyCode::PageDown, K::PageDown),
        (KeyCode::Numpad8, K::Pad(8)),
        (KeyCode::Numpad2, K::Pad(2)),
        (KeyCode::Numpad4, K::Pad(4)),
        (KeyCode::Numpad6, K::Pad(6)),
        (KeyCode::Numpad7, K::Pad(7)),
        (KeyCode::Numpad9, K::Pad(9)),
        (KeyCode::Numpad1, K::Pad(1)),
        (KeyCode::Numpad3, K::Pad(3)),
    ]
    .into_iter()
    .filter(|(k, _)| keys.pressed(*k))
    .map(|(_, s)| s)
    .collect();
    let input = screen::ScrollInput {
        now: (time.elapsed_secs_f64() * 1000.0) as u64,
        edge: scr.edge,
        // No chat line yet.
        keys: screen::keys_direction(&held, false),
        ctrl_or_alt: keys.any_pressed([
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ]),
        blocked,
    };
    if let Some(d) = scr.scroller.update(input) {
        move_view(camera, &mut tf, proj, geom.step(d));
    }
}

/// Whether the pause button is shown: the client has no multiplayer games yet, so it always is.
fn pause_button(sim: &Sim) -> bool {
    screen::pause_shown(sim.world.rules.enhanced, false, 0)
}

/// The text of a message source.
fn text_of(hud: &Hud, t: &messages::Text) -> String {
    match t {
        messages::Text::Stat(n) => hud.art.string(*n),
        messages::Text::Net(n) => hud.art.net_string(*n),
        messages::Text::Own(s) => s.clone(),
    }
}

/// The message area, the status bar and the event history (`messages`, spec `ui/messages.md`): new notices and the
/// pause notices, real-time expiry, the event locations, the fixed prompts and what the pointer is over.
fn update_messages(
    time: Res<Time<Real>>,
    sim: Option<ResMut<Sim>>,
    hud: Res<Hud>,
    ui: Res<PanelUi>,
    info_panel: Res<InfoPanel>,
    mut scr: ResMut<ScreenUi>,
) {
    let Some(mut sim) = sim else { return };
    let scr = &mut *scr;
    let now = (time.elapsed_secs_f64() * 1000.0) as u64;
    for n in std::mem::take(&mut sim.notices) {
        scr.messages.notice(&text_of(&hud, &n), 0, now);
    }
    // Our wording for the player's name; open question default: the name only.
    let paused = sim.world.paused;
    if paused != scr.was_paused {
        let entry = if paused {
            messages::net::PAUSED
        } else {
            messages::net::RESUMED
        };
        let name = format!("Player {}", sim.human + 1);
        scr.messages
            .notice(&messages::fill_name(&hud.art.net_string(entry), &name), 0, now);
        scr.was_paused = paused;
    }
    scr.messages.expire(now);
    for tile in std::mem::take(&mut sim.world.sounds.events) {
        if let Some(h) = &mut scr.history {
            h.push(tile, now);
        }
    }
    let demo = sim.world.sounds.demo;
    let prompt = match ui.mode {
        panel::Mode::Target(_) => Some(messages::net::SELECT_TARGET),
        panel::Mode::Place(_) => Some(messages::net::SELECT_LOCATION),
        _ => None,
    };
    scr.status.set_prompt(prompt.map(|n| hud.art.net_string(n)));
    match (&ui.tooltip, &info_panel.hover) {
        (Some((text, (kind, cost, mana))), _) => scr.status.button(text, *kind, *cost, *mana, demo),
        (None, Some(name)) => scr.status.name(name, demo),
        (None, None) => scr.status.clear(demo),
    }
}

fn draw_hud(
    sim: Option<Res<Sim>>,
    sel: Res<Selection>,
    hud: Res<Hud>,
    ui: Res<PanelUi>,
    info_panel: Res<InfoPanel>,
    scr: Res<ScreenUi>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(sim), Some(layout)) = (sim, hud.layout) else {
        return;
    };
    let icons = info_panel.slots.map(|s| {
        let u = sim.world.unit(s?)?;
        Some(hud::Icon {
            kind: u.kind,
            owner: u.owner,
            hp: u.hp,
            max_hp: sim
                .world
                .stats
                .0
                .get(u.kind as usize)
                .map_or(1, |s| s.hit_points.max(1)),
        })
    });
    let state = panel::WorldState {
        world: &sim.world,
        local: sim.human,
    };
    let infos: Vec<panel::UnitInfo> = sel.units.iter().filter_map(|&id| state.unit(id)).collect();
    let slots = panel::fill(panel::choose_set(ui.mode, &infos, &state), &state, &infos);
    let buttons: Vec<(u8, u16)> = slots
        .iter()
        .enumerate()
        .filter_map(|(n, en)| en.map(|en| (n as u8, en.icon)))
        .collect();
    let pic = hud.art.draw(
        &hud.frame,
        &layout,
        &hud::View {
            resources: sim.counter.shown,
            food: screen::food(
                sim.world.rules.enhanced,
                sim.world.food_used(sim.human),
                sim.world.food_supply(sim.human),
            ),
            info: hud::InfoView {
                frame: info_panel.frame,
                slots: icons,
                pressed: info_panel.pressed,
                content: info_panel.content.as_ref(),
                local: sim.human,
            },
            status: &scr.status,
            messages: &scr.messages,
            paused: sim.world.paused,
            pause_button: pause_button(&sim),
            pointer: scr.pointer.filter(|_| hud.art.has_pointers()),
            minimap: &hud.minimap.pixels,
            buttons: &buttons,
        },
    );
    if let Some(mut img) = images.get_mut(&hud.image) {
        *img = image_with(pic, RenderAssetUsages::default());
    }
}

#[cfg(test)]
mod tests {
    use super::keep_on_map;

    #[test]
    fn the_view_stays_on_the_map() {
        assert_eq!(keep_on_map(10.0, 100.0, 1000.0), 100.0);
        assert_eq!(keep_on_map(990.0, 100.0, 1000.0), 900.0);
        assert_eq!(keep_on_map(500.0, 100.0, 1000.0), 500.0);
        assert_eq!(keep_on_map(10.0, 600.0, 1000.0), 500.0, "a small map is centred");
    }
}
