//! The game screen (`docs/design/client-core.md`): loading a map or a saved game, the cycle loop, the map view, the
//! interface and the player's input. SDL-free; the core calls these each frame (`core::frame_systems`).
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
//! `ui/screen-and-cursors.md`), Escape cancels a panel mode and otherwise quits. On the
//! minimap, left click or drag moves the view. Space steps through the latest events (Enhanced rules). Messages,
//! notices and the status bar follow `ui/messages.md` (`messages`).

use std::collections::HashMap;

use glam::Vec2;
use tidewater_formats::install::Install;
use tidewater_sim::economy::Event;
use tidewater_sim::{Command, TILE_PX, UnitId, World};

use crate::carry;
use crate::clock::FrameClock;
use crate::core::{self, Core, add_layer, remove_layers};
use crate::input::{self, Button, Key};
use crate::layers::{self, Pictures, Role};
use crate::panel::PanelState;
use crate::{
    audio, campaign, display, fog, frontend, hud, info, mapview, messages, minimap, mouse, palette, panel, savefile,
    savegame, scene, screen, sound, view,
};

pub struct Loaded(pub Option<scene::Scene>);

pub struct MapSize(pub Vec2);

pub struct Sim {
    pub world: World,
    /// Commands collected since the last tick.
    pub pending: Vec<Command>,
    pub human: u8,
    /// Notices for the notice line since the last frame (`messages`).
    pub notices: Vec<messages::Text>,
    /// The resource bar's shown gold, lumber and oil, counted once per cycle.
    pub counter: screen::Counter,
    /// Passengers to put ashore one at a time (transport, place 0 to 5), from clicks on the information panel.
    pub unload_one: Vec<(UnitId, usize)>,
    /// Interface sounds to play this frame (the placement click's).
    pub ui_sounds: Vec<sound::UiSound>,
    /// The carrying files of the workers and tankers, chosen again after each cycle (`carry`).
    pub carried: carry::Carried,
}

/// Plays the sim's sound cues (`sound.rs`, spec `docs/specs/ui/sounds.md`): reads the files from the install on first
/// use and keeps the decoded sources by name.
pub struct Speaker {
    pub director: sound::Director,
    pub install: Install,
    /// Archive name to sound; `None` for a file the install doesn't have, so it is looked up once.
    pub sounds: HashMap<String, Option<audio::Sound>>,
    /// Ctrl+S turned the sounds off.
    pub muted: bool,
}

/// The command panel's input state (see `panel`).
#[derive(Default)]
pub struct PanelUi {
    pub mode: panel::Mode,
    /// A panel click or key was handled this frame; the map must not also act on it.
    pub consumed: bool,
    /// Escape was used by the panel this frame (a hotkey, or leaving a mode).
    pub escape_used: bool,
    /// The menu button was pressed (`gamemenu` opens the game menu).
    pub menu_requested: bool,
    /// The tooltip of the button under the cursor, with its cost kind, gold, lumber and oil costs and mana cost.
    pub tooltip: Option<(String, (u8, panel::Cost, u32))>,
    /// The selection a build menu belongs to; a different selection returns the panel from it.
    pub last_sel: Vec<UnitId>,
    /// The mouse style (`mouse::Style`).
    pub style: mouse::Style,
    /// The placement site's top-left tile under the pointer's hot spot, kept while the pointer is off the map view.
    pub site: Option<(i32, i32)>,
    /// Development aid (`--demo-site`): the pointer's tile while the pointer is off the map view.
    pub demo_pointer: Option<(i32, i32)>,
    /// Development aid: one placement click at `demo_pointer` is still to be made.
    pub demo_click: bool,
}

/// The screen's and the messages' state (`screen`, `messages`).
#[derive(Default)]
pub struct ScreenUi {
    pub scroller: screen::Scroller,
    /// The scrolling edge the pointer is at, this frame.
    pub edge: (i32, i32),
    /// The pointer to draw and the mouse position, when the client draws the pointer.
    pub pointer: Option<(screen::Pointer, (i32, i32))>,
    /// The views F2 to F4 go to, from game setup's saved views (`World::start`); Shift+F2 to F4 replace them.
    pub views: [(i32, i32); 3],
    /// The event history, starting at game setup's starting view.
    pub history: Option<messages::History>,
    pub messages: messages::Messages,
    pub status: messages::StatusBar,
    pub was_paused: bool,
}

/// The information panel's state (`info`, spec `ui/info-panel.md`).
#[derive(Default)]
pub struct InfoPanel {
    /// The grid: which unit each slot shows.
    pub slots: info::Slots,
    /// The selection the grid was last built for.
    pub built_for: Vec<UnitId>,
    pub frame: u8,
    /// The text for one selected unit, rebuilt when the selection changes or a change check fires.
    pub content: Option<info::Content>,
    /// The values the change checks last saw.
    pub watched: Option<Vec<u32>>,
    /// The slot whose icon is held down.
    pub pressed: Option<usize>,
    /// The name of the unit under the cursor, for the status line.
    pub hover: Option<String>,
    /// Speed per unit type, worked out once from the animation scripts.
    pub speeds: Vec<u8>,
}

/// The local selection and what the selecting rules remember (`mouse`).
#[derive(Default)]
pub struct Selection {
    pub units: Vec<UnitId>,
    /// Where a left-button drag started, in map pixels.
    pub drag_from: Option<Vec2>,
    /// Shift, Ctrl and the double-click as they were at the press.
    pub press: mouse::Mods,
    /// The last left press on the map view (seconds, map pixel), for the double-click.
    pub last_press: Option<(f64, Vec2)>,
    /// A left press on the minimap is moving the view.
    pub minimap_held: bool,
    pub remembered: mouse::Remembered,
    pub groups: mouse::GroupKeys,
    /// The ground marker (effect slot 2), counted in game cycles.
    pub marker: Option<view::Marker>,
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
pub struct TerrainArt {
    pub image: layers::PictureId,
    pub tileset: tidewater_formats::tileset::Tileset,
}

/// Colour cycling (`palette`): the cycled palette every indexed picture is converted with, and the tiles' used entries.
pub struct Cycling {
    pub state: palette::Cycling,
    /// When the game started, in ms on the app's real-time clock (`Time<Real>`): the cycles' clock starts here.
    pub started_ms: u64,
    /// Per megatile, the palette entries it uses; filled on first use.
    pub used: Option<Vec<palette::Entries>>,
    /// The palette the screen shows now (`palette::shown`): the pictures are converted with it.
    pub shown: tidewater_formats::tileset::Palette,
    /// When the game start's fade-in began (ms on `started`'s clock).
    pub fade_start: Option<u64>,
    /// Frames the screenshot flash still shows the negative (`screenshot`).
    pub flash: u8,
    /// The effects in force this frame.
    pub effects: palette::Effects,
}

/// The map view's sprite files, palette and fog shapes (`view`, `fog`).
pub struct Art {
    pub view: view::Art,
    pub palette: tidewater_formats::tileset::Palette,
    pub shapes: fog::Shapes,
}

/// The map view's zoom in map pixels per screen pixel (`display`): 1, as the original, unless `--free-zoom`.
pub struct MapZoom {
    pub zoom: f32,
    pub free: bool,
    /// Mouse-wheel movement short of a step.
    pub wheel: f32,
}

/// The interface: its art, current layout and the picture it draws into.
pub struct Hud {
    pub art: hud::HudArt,
    pub layout: Option<hud::Layout>,
    pub frame: scene::Picture,
    /// The interface's picture, while a game is on.
    pub image: Option<layers::PictureId>,
    /// Map size in tiles.
    pub map: (u32, u32),
    /// The minimap: its redraw countdown, terrain picture and last picture (`minimap`).
    pub minimap: minimap::Minimap,
}

impl Hud {
    /// The cursor in screen pixels, through the black border (`display::Letterbox::screen_point`).
    pub fn cursor(&self, input: &input::Input) -> Option<Vec2> {
        let (w, p) = (input.window?, input.pointer?);
        let (x, y) = display::Letterbox::fit(w.size).screen_point(p, w.factor);
        Some(Vec2::new(x, y))
    }

    /// Whether the cursor is over the map view (and not the interface).
    pub fn over_map(&self, input: &input::Input) -> bool {
        match (self.layout, self.cursor(input)) {
            (Some(l), Some(p)) => l.viewport.contains(p.x, p.y),
            _ => false,
        }
    }
}

/// The map view's picture over the terrain, and the placement image over that.
const VIEW_Z: f32 = 10.0;
const PLACE_Z: f32 = 11.0;

/// Writes the running game as `name` (the typed name; `stem` is the file name without the extension).
pub(crate) fn save_game(c: &mut Core, stem: &str, name: &str) -> Result<(), savefile::Reason> {
    let store = c.store.clone();
    let bytes = capture_game(c, name).ok_or(savefile::Reason::WriteFailed)?;
    store.folder().write(stem, savefile::Kind::Single, &bytes)
}

/// The bytes of the running game as a saved game (`savegame::capture`), or `None` outside a game.
pub(crate) fn capture_game(c: &mut Core, name: &str) -> Option<Vec<u8>> {
    let origin = c.origin.clone()?;
    let cycles_on = c.cycling.as_ref()?.state.cycler().on_set();
    let now = c.time.elapsed();
    let clock_ms = c.game_clock.as_ref().map(|g| g.ms(now));
    let pause_counts = c.pause_counts.as_ref().map(|p| p.0);
    let music = c.jukebox.music().current();
    let screen = c
        .view
        .as_ref()
        .map_or((0, 0), |v| (v.centre.0 as i32, v.centre.1 as i32));
    let client = savegame::client_state(&c.flow, cycles_on, music, screen, clock_ms, pause_counts);
    let saved_at = c.store.wall_clock_secs();
    let sim = c.sim.as_mut()?;
    Some(savegame::capture(&mut sim.world, &origin, client, name, saved_at))
}

/// Loads the game the flow asks for and puts in the game screen's state (`setup` then adds its layers). A map that
/// fails to load goes back to the main menu.
pub(crate) fn start_game(c: &mut Core) {
    let Some((map, setup)) = c.flow.pending.take() else {
        return;
    };
    if let Err(e) = load_game(c, &map, setup) {
        eprintln!("can't start {map}: {e:#}");
        c.loaded = Some(Loaded(None));
        frontend::to_main_menu(&mut c.flow);
        c.states.set(frontend::AppState::Boot);
    }
}

/// The game options `load_game` reads from the command line.
struct GameArgs {
    map: String,
    setup: Option<tidewater_sim::setup::Setup>,
    zoom: Option<f32>,
    free_zoom: bool,
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

/// The world of a game that is starting, from its template (`docs/specs/game/lifecycle.md`, "How a game starts").
///
/// - **A new game** is the template as the map and set-up built it: cheat flags, rescued counts, the victory
///   countdown and the other per-game counts start from their new-game values in `World::from_pud` and `World::new`.
///   It runs the two start-up cycles ([`World::start_up`]).
/// - **A loaded game** has the saved state put over the template ([`World::restore`]): the counts that are saved come
///   back and the others (random numbers, cheats) start afresh. It runs the two start-up cycles too, with the cycle
///   counter kept ([`World::start_up_loaded`], X3).
fn start_world(world: &mut World, resume: Option<&savegame::Resume>, wall: u64) -> anyhow::Result<()> {
    if let Some(r) = resume {
        world
            .restore(&r.body.world, savegame::reseed(wall))
            .map_err(|e| anyhow::anyhow!("the saved game doesn't fit its map: {e}"))?;
        // Timing-and-rng spec, scheduler rule 1 for a loaded game: the same two start-up cycles, counter kept.
        world.start_up_loaded();
    } else {
        // Timing-and-rng spec, scheduler rule 1: a new game runs its two start-up cycles at once.
        world.start_up();
    }
    Ok(())
}

/// The flow's record of the game that is starting: its kind, the campaign total and the starting controllers (score
/// screen). A new game takes the total from the new-game rule ([`campaign::CampaignTotal::start_game`]); a loaded
/// game then takes the saved record over it, the total included (`game/campaign-flow.md`, "Campaign total").
fn record_game(
    flow: &mut frontend::Flow,
    options: &tidewater_sim::setup::Setup,
    world: &World,
    resume: Option<&savegame::Resume>,
) {
    let kind = match options.game {
        tidewater_sim::setup::Game::Campaign { entry } => campaign::GameKind::Campaign(entry),
        _ => campaign::GameKind::Custom,
    };
    flow.game = Some(kind);
    flow.total.start_game(kind);
    for (i, p) in world.players.iter().take(8).enumerate() {
        flow.start_controllers[i] = p.controller;
    }
    // A loaded game keeps the flow's record of the saved one, and the screen's position.
    if let Some(r) = resume {
        savegame::apply_client(flow, &r.body.client);
    }
}

fn load_game(c: &mut Core, map: &str, setup: Option<tidewater_sim::setup::Setup>) -> anyhow::Result<()> {
    let a = &c.opts;
    let args = GameArgs {
        map: map.to_string(),
        setup,
        zoom: a.zoom,
        free_zoom: a.free_zoom,
        demo_move: a.demo_move,
        demo_fight: a.demo_fight,
        demo_select: a.demo_select,
        demo_type: a.demo_type,
        demo_place: a.demo_place,
        demo_site: a.demo_site,
        second_mouse_style: a.second_mouse_style,
        mouse_scroll_speed: a.mouse_scroll_speed.unwrap_or(screen::DEFAULT_SPEED),
        key_scroll_speed: a.key_scroll_speed.unwrap_or(screen::DEFAULT_SPEED),
        demo_outcome: a.demo_outcome.as_deref().map(|o| o == "victory"),
    };
    let store = c.store.clone();
    let mut install = crate::storage::open_install(&store.0)?;
    let options = args
        .setup
        .unwrap_or_else(|| tidewater_sim::setup::Setup::for_map(&args.map));
    // A saved game builds the same game again (same start seed), then its state goes over it.
    let resume = c.flow.resume.take();
    // Single player seeds with the time in whole seconds (`core/timing-and-rng.md`, "Seeding"), read through the
    // storage's wall clock.
    let wall = store.wall_clock_secs();
    let seed = resume.as_ref().map_or(wall as u32, |r| r.header.seed);
    let mut scene = scene::load(&mut install, &*store.0, &args.map, &options, seed)?;
    start_world(&mut scene.world, resume.as_deref(), wall)?;
    c.origin = Some(savegame::GameOrigin {
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
        started_ms: real_ms(&c.time),
        used: None,
        shown: scene.palette.clone(),
        fade_start: None,
        flash: 0,
        effects: palette::Effects::default(),
    };
    let mut speaker = Speaker {
        director: sound::Director::new(sound::SoundSet::of_world(&scene.world), scene.era, scene.seed),
        install,
        sounds: HashMap::new(),
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
        ..Default::default()
    };
    if let Some(kind) = args.demo_place {
        panel_ui.mode = panel::Mode::Place(kind);
        panel_ui.last_sel = selection.units.clone();
        panel_ui.demo_pointer = args.demo_site;
        panel_ui.demo_click = args.demo_site.is_some();
    }
    let screen_ui = ScreenUi {
        scroller: screen::Scroller::new(args.mouse_scroll_speed, args.key_scroll_speed),
        views: scene.world.start.saved_views,
        history: Some(messages::History::new(scene.world.start.view)),
        ..Default::default()
    };
    // Music spec: the game's track (drawn after the map has loaded) plays if no music has started.
    match &resume {
        Some(r) => c.jukebox.resume_game(options.campaign(), r.body.client.music),
        None => c.jukebox.start_game(
            scene.world.start.orc_sounds,
            options.campaign(),
            &mut speaker.director.shuffle,
        ),
    }
    record_game(&mut c.flow, &options, &scene.world, resume.as_deref());
    // The system's pointer follows from the frame's pointer (`core::compose_screen`).
    c.title = Some(format!("Tidewater — {}", args.map));
    if let Some(r) = &resume {
        let (x, y) = r.body.client.screen;
        c.resume_look = Some(savegame::ResumeLook((x as f32, y as f32)));
    }
    // The game clock starts now, from a loaded game's saved reading (Classic saves have none: 0).
    let (clock, pauses) = savegame::clock_and_pauses(
        resume.as_ref().and_then(|r| r.body.client.clock_ms),
        resume.as_ref().and_then(|r| r.body.client.pause_counts),
        c.time.elapsed(),
    );
    c.game_clock = Some(clock);
    c.pause_counts = Some(pauses);
    c.cycling = Some(cycling);
    c.loaded = Some(Loaded(Some(scene)));
    c.map_size = Some(MapSize(size));
    c.map_zoom = Some(MapZoom {
        zoom: display::zoom_step(args.zoom.unwrap_or(1.0), 0),
        free: args.free_zoom || args.zoom.is_some(),
        wheel: 0.0,
    });
    c.hud = Some(Hud {
        art: hud_art,
        layout: None,
        frame: scene::Picture {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        },
        image: None,
        map: (0, 0),
        minimap: minimap::Minimap::new(0),
    });
    c.time.restart_steps(hz);
    c.selection = Some(selection);
    c.art = Some(view_art);
    c.speaker = Some(speaker);
    c.panel_ui = Some(panel_ui);
    c.screen_ui = Some(screen_ui);
    c.info_panel = Some(InfoPanel::default());
    Ok(())
}

/// The game clock stops when the game ends (`savegame::GameClock`).
pub(crate) fn stop_clock(c: &mut Core) {
    if let (Some(sim), Some(clock)) = (&c.sim, &mut c.game_clock)
        && sim.world.victory.over
    {
        clock.stop(c.time.elapsed());
    }
}

/// Leaving the game screen: its layers go (not the flow's) and so does the game.
pub(crate) fn end_game(c: &mut Core) {
    remove_layers(&mut c.commands, |e| e.role.of_game());
    c.audio.stop_group(audio::Group::Effect);
    c.commands.queue(|c| {
        c.sim = None;
        c.view = None;
        c.loaded = None;
        c.game_clock = None;
        c.pause_counts = None;
    });
}

pub(crate) fn setup(c: &mut Core) {
    let Core {
        commands,
        loaded: Some(loaded),
        pictures,
        map_size: Some(size),
        hud: Some(hud),
        selection: Some(sel),
        resume_look,
        ..
    } = c
    else {
        return;
    };
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
    let look = resume_look.as_ref().map_or(look, |r| Vec2::new(r.0.0, r.0.1));
    commands.queue(|c| c.resume_look = None);
    hud.map = (
        scene.terrain.width / TILE_PX as u32,
        scene.terrain.height / TILE_PX as u32,
    );
    hud.minimap = minimap::Minimap::new(hud.map.0);
    let blank = || scene::Picture {
        width: 1,
        height: 1,
        rgba: vec![0; 4],
    };
    let image = pictures.add(blank());
    hud.image = Some(image);
    add_layer(
        commands,
        layers::Layer::new(image, layers::Plane::Interface, 0.0),
        Role::Hud,
        None,
    );
    // `hud_layout` puts the view on the screen and sets its zoom.
    let view = mapview::MapView::new((look.x, look.y));
    commands.queue(move |c| c.view = Some(view));
    // Kept on the CPU side too, so tiles that change in play can be redrawn.
    let terrain = pictures.add(scene.terrain);
    add_layer(
        commands,
        layers::Layer::new(terrain, layers::Plane::Map, 0.0).at(size.0.x / 2.0, size.0.y / 2.0),
        Role::Terrain,
        None,
    );
    let art = TerrainArt {
        image: terrain,
        tileset: scene.tileset,
    };
    commands.queue(move |c| c.terrain_art = Some(art));
    // The map view's picture, redrawn every frame for the part of the map in view (`draw_view`).
    add_layer(
        commands,
        layers::Layer::new(pictures.add(blank()), layers::Plane::Map, VIEW_Z),
        Role::View,
        None,
    );
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
    let sim = Sim {
        world: scene.world,
        pending,
        human: scene.human,
        notices: Vec::new(),
        counter,
        unload_one: Vec::new(),
        ui_sounds: Vec::new(),
        carried: carry::Carried::default(),
    };
    commands.queue(move |c| {
        c.sim = Some(sim);
        // A new game's world: `cheats::run` starts its flags (`Sim` was added).
        c.sim_new = true;
    });
    let mut ghost = layers::Layer::new(pictures.add(blank()), layers::Plane::Map, 0.0);
    ghost.visible = false;
    add_layer(commands, ghost, Role::Ghost, None);
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

/// One game cycle.
pub(crate) fn tick(c: &mut Core) {
    let Core {
        sim: Some(sim),
        selection: Some(sel),
        time,
        hud: Some(hud),
        cycling: Some(cycling),
        terrain_art: art,
        view,
        cheats: cheat_queue,
        stored,
        ..
    } = c
    else {
        return;
    };
    // Info-panel spec, "Clicking an icon": a click on a passenger puts that one passenger ashore by the transports
    // spec's one-passenger rule (open question there: assumed to be that request). It is a command like any other
    // (`Command::UnloadOne`, owner-checked in the sim), applied first in the cycle, ahead of the other pending commands.
    let human = sim.human;
    let mut cmds: Vec<tidewater_sim::Command> = std::mem::take(&mut sim.unload_one)
        .into_iter()
        .filter_map(|(unit, place)| {
            Some(tidewater_sim::Command::UnloadOne {
                player: human,
                unit,
                slot: u8::try_from(place).ok()?,
            })
        })
        .collect();
    cmds.append(&mut sim.pending);
    sim.world.step(&cmds);
    // Carried-goods spec, "When the file is chosen": the marks and take-overs of this cycle.
    sim.carried.update(&sim.world);
    let step = std::time::Duration::from_secs_f64(cycle_secs(sim.world.rules.enhanced, sim.world.speed));
    if time.step() != step {
        time.set_step(step);
    }
    // Minimap spec, "When it is redrawn": the countdown runs in the game cycle, after the units are updated.
    if let (Some(art), Some(view)) = (art, view) {
        let (l, t, r, b) = view.tiles();
        let at = (l.floor().max(0.0) as i32, t.floor().max(0.0) as i32);
        let size = ((r - l).round().max(1.0) as i32, (b - t).round().max(1.0) as i32);
        hud.minimap
            .cycle(&mut sim.world, &art.tileset, at, size, stored.0.minimap_terrain);
    }
    // A lost unit (dying, starting to enter a mine, oil platform or depot, or gone inside) leaves the local selection
    // and the remembered groups (corpses spec; selecting spec, "Losing a unit from the selection" and "Two
    // selections"), by the sim's own predicate, the one its synced selections use.
    let w = &sim.world;
    let present = |id: UnitId| w.unit(id).is_some_and(tidewater_sim::Unit::stays_selected);
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

/// Steps the colour cycles (`palette`, spec `ui/palette-effects.md`) and applies the palette the screen shows (the
/// cycled one, grey while paused, negative during the screenshot flash, faded at the game's start): the map view, the
/// interface and the terrain tiles that use a changed entry all convert with it from here on.
pub(crate) fn cycle_palette(c: &mut Core) {
    let Core {
        sim: Some(sim),
        art: Some(art),
        terrain_art: Some(terrain),
        cycling: Some(cycling),
        hud: Some(hud),
        panel_ui: Some(ui),
        stored,
        menu,
        input,
        pictures,
        time: real,
        ..
    } = c
    else {
        return;
    };
    let gate = palette::Gate {
        preference: stored.0.colour_cycling(),
        paused: sim.world.paused,
        menu_open: menu.is_some(),
        focused: input.focused,
    };
    let now = real_ms(real).saturating_sub(cycling.started_ms);
    let placing = matches!(ui.mode, panel::Mode::Place(_));
    let turned = cycling.state.frame(now, gate, placing);
    let fade_start = *cycling.fade_start.get_or_insert(now);
    let fade_t = now.saturating_sub(fade_start).min(palette::fades::GAME_START_IN as u64) as u32;
    let effects = palette::Effects {
        paused: sim.world.paused,
        negative: cycling.flash > 0,
        fade_in: (fade_t < palette::fades::GAME_START_IN).then_some((fade_t, palette::fades::GAME_START_IN)),
    };
    cycling.flash = cycling.flash.saturating_sub(1);
    cycling.effects = effects;
    let shown = palette::shown(cycling.state.palette(), effects);
    let changed = palette::Entries::changed(&cycling.shown, &shown);
    if turned != 0 {
        // The interface draws with the cycled palette; `draw_hud` puts the effects over its whole picture, as it has
        // colours of its own besides palette entries.
        hud.art.set_palette(cycling.state.palette());
    }
    if changed.is_empty() {
        return;
    }
    cycling.shown = shown;
    art.palette = cycling.shown.clone();
    // Redraw the terrain tiles that use a changed entry (water and coast for a cycle; all of them for the greyscale).
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
    if let Some(img) = pictures.get_mut(terrain.image) {
        let width = img.width as usize;
        {
            let data = &mut img.rgba;
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
                    let pic = scene::tile_picture(ts, m as usize, &cycling.shown);
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
pub(crate) fn redraw_terrain(c: &mut Core) {
    let Core {
        sim: Some(sim),
        terrain_art: Some(art),
        cycling: Some(cycling),
        pictures,
        ..
    } = c
    else {
        return;
    };
    let changed = std::mem::take(&mut sim.world.terrain.redraw);
    if changed.is_empty() {
        return;
    }
    let Some(img) = pictures.get_mut(art.image) else {
        return;
    };
    let width = img.width as usize;
    let data = &mut img.rgba;
    let t = TILE_PX as usize;
    for (x, y) in changed {
        let Some(m) = sim.world.terrain.tile(x, y) else {
            continue;
        };
        let pic = scene::tile_picture(&art.tileset, m as usize, &cycling.shown);
        for row in 0..t {
            let at = ((y as usize * t + row) * width + x as usize * t) * 4;
            data[at..at + t * 4].copy_from_slice(&pic.rgba[row * t * 4..(row + 1) * t * 4]);
        }
    }
}

/// Puts a picture into a layer and sizes the layer to it.
fn show_picture(pictures: &mut Pictures, layer: &mut layers::Layer, width: i32, height: i32, rgba: Vec<u8>) {
    let (w, h) = (width.max(1) as u32, height.max(1) as u32);
    let rgba = if width > 0 && height > 0 { rgba } else { vec![0; 4] };
    pictures.set(
        layer.picture,
        scene::Picture {
            width: w,
            height: h,
            rgba,
        },
    );
    layer.size = Some((w as f32, h as f32));
}

/// Draws the map view over the terrain (`view`, spec `ui/map-drawing.md`) every frame, for the part of the map in
/// view: buildings and ghosts, the unit passes, effects, fog, the ground marker and the selection rectangle.
pub(crate) fn draw_view(c: &mut Core) {
    let Core {
        sim: Some(sim),
        art: Some(art),
        selection: Some(sel),
        panel_ui: Some(ui),
        hud: Some(_),
        input,
        view: Some(view),
        pictures,
        layers,
        ..
    } = c
    else {
        return;
    };
    let Some(layer) = layers.get_mut(Role::View) else {
        return;
    };
    let v = &sim.world.vision;
    let map = (v.width as i32 * TILE_PX, v.height as i32 * TILE_PX);
    let (l, t, r, b) = view.pixels(map);
    let mut canvas = view::Canvas::new(l, t, r - l, b - t);
    let drag = match (sel.drag_from, input.window) {
        (Some(from), Some(_)) => cursor_map(input, view).map(|p| (px(from), px(p))),
        _ => None,
    };
    let local = view::Local {
        selected: &sel.units,
        placing: matches!(ui.mode, panel::Mode::Place(_)),
        marker: sel.marker,
        drag,
        carried: &sim.carried,
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
    show_picture(pictures, layer, canvas.width, canvas.height, rgba);
    layer.at = (
        l as f32 + canvas.width.max(1) as f32 / 2.0,
        t as f32 + canvas.height.max(1) as f32 / 2.0,
    );
}

/// The map tile holding a point in map pixels.
fn pointer_tile(at: Vec2) -> (i32, i32) {
    ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX))
}

/// The placement image (map-drawing spec, "Placement image"): the building's frame 0 in the local player's colours,
/// with the site check's good and bad tiles as a checkerboard, its top-left on the site's top-left tile. Shown only
/// while the pointer is over the map view (or at the `--demo-site` tile).
pub(crate) fn draw_placement(c: &mut Core) {
    let Core {
        sim,
        art,
        pictures,
        panel_ui: Some(ui),
        selection: Some(sel),
        hud: Some(hud),
        input,
        layers,
        ..
    } = c
    else {
        return;
    };
    let Some(layer) = layers.get_mut(Role::Ghost) else {
        return;
    };
    let (Some(sim), Some(art)) = (sim.as_ref(), art.as_ref()) else {
        return;
    };
    let over = hud.over_map(input);
    let pointer = ui.site.filter(|_| over).or(ui.demo_pointer);
    let builder = sel.units.first().and_then(|&id| sim.world.unit(id));
    let (panel::Mode::Place(kind), Some(tile), Some(b)) = (ui.mode, pointer, builder) else {
        layer.visible = false;
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
    show_picture(pictures, layer, pic.width, pic.height, rgba);
    layer.at = (
        (site.0 * TILE_PX) as f32 + pic.width as f32 / 2.0,
        (site.1 * TILE_PX) as f32 + pic.height as f32 / 2.0,
    );
    layer.z = PLACE_Z;
    layer.visible = true;
}

/// The cursor position in map pixels (y down), if it is over the window: by the view the last frame drew
/// (`mapview::MapView::pointer_map`).
fn cursor_map(input: &input::Input, view: &mapview::MapView) -> Option<Vec2> {
    let (w, p) = (input.window?, input.pointer?);
    let (x, y) = view.pointer_map(display::Letterbox::fit(w.size).screen_of(p, w.factor))?;
    Some(Vec2::new(x, y))
}

/// Keeps the map view on the map and on the tile grid; with `--free-zoom` the mouse wheel zooms first (`display`).
pub(crate) fn pan_and_zoom(c: &mut Core) {
    let Core {
        input,
        map_size: Some(size),
        map_zoom: Some(zoom),
        view,
        ..
    } = c
    else {
        return;
    };
    let Some(view) = view else { return };
    // + and - are the game speed keys; only the wheel zooms, and only as a development aid.
    if zoom.free && input.scroll != 0.0 {
        let steps = display::wheel_steps(&mut zoom.wheel, input.scroll, input.scroll_pixels);
        if steps != 0 {
            zoom.zoom = display::zoom_step(zoom.zoom, steps);
            if input.window.is_some() {
                view.zoom = zoom.zoom;
            }
        }
    }

    // Runs every frame, so a view moved any other way (minimap, start position) is kept on the map too. The view
    // position is a whole tile (screen spec, "Layout").
    view.keep_on_map((size.0.x, size.0.y));
}

/// The app's real-time clock in whole milliseconds since the program started.
fn real_ms(real: &FrameClock) -> u64 {
    real.elapsed().as_millis() as u64
}

/// Lays the interface out on the 640 × 480 screen once, and keeps the map view on its place in the layout at the
/// zoom (one map pixel per screen pixel unless `--free-zoom`).
pub(crate) fn hud_layout(c: &mut Core) {
    let Core {
        input,
        hud: Some(hud),
        map_zoom: Some(zoom),
        view,
        layers,
        ..
    } = c
    else {
        return;
    };
    let (Some(_), Some(view), true) = (input.window, view, layers.has(Role::Hud)) else {
        return;
    };
    let layout = match hud.layout {
        Some(l) => l,
        None => {
            let layout = hud.art.layout(display::SCREEN);
            hud.frame = hud.art.frame(&layout);
            hud.layout = Some(layout);
            layout
        }
    };
    let v = layout.viewport;
    let rect = mapview::ScreenRect {
        x: v.x.max(0),
        y: v.y.max(0),
        w: v.w.max(1),
        h: v.h.max(1),
    };
    if view.rect != Some(rect) {
        view.rect = Some(rect);
    }
    if view.zoom != zoom.zoom {
        view.zoom = zoom.zoom;
    }
}
/// The hotkey a key press stands for.
pub(crate) fn key_char(k: Key) -> Option<char> {
    match k {
        Key::Letter(c) => Some(c),
        Key::Digit(d) => Some((b'0' + d) as char),
        _ => None,
    }
}

/// The command panel: clicks and hotkeys on its buttons, and the map click or cancel that follows a target or
/// placement mode (`panel`, spec `ui/command-buttons.md`).
pub(crate) fn panel_input(c: &mut Core) {
    let Core {
        sim,
        selection: Some(sel),
        hud: Some(hud),
        screen_ui: Some(scr),
        panel_ui: Some(ui),
        input,
        ..
    } = c
    else {
        return;
    };
    let input = &*input;
    let keys = &input.keys;
    let buttons = &input.mouse;
    ui.consumed = false;
    ui.escape_used = false;
    ui.tooltip = None;
    let (Some(sim), Some(_)) = (sim.as_mut(), input.window) else {
        return;
    };
    // Screen spec: at a scrolling edge, clicks on the panel do nothing.
    let pressed = buttons.just_pressed(Button::Left) && scr.edge == (0, 0);
    // The menu and pause buttons (screen spec, "Menu buttons").
    if pressed
        && let Some(c) = hud.cursor(input)
        && let Some(b) = screen::menu_button_at((c.x, c.y), pause_button(sim))
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
        if let (Some(l), Some(c)) = (hud.layout, hud.cursor(input))
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
        if keys.just_pressed(Key::Escape) && ui.mode != panel::Mode::None {
            ui.escape_used = true;
        }
        // Ctrl and Alt make the in-game keys (`keys_input`), not hotkeys.
        let held = keys.any_pressed([Key::ControlLeft, Key::ControlRight, Key::AltLeft, Key::AltRight]);
        if !ui.consumed && !held {
            for &key in keys.get_just_pressed() {
                let hk = if key == Key::Escape {
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
                    ui.escape_used |= key == Key::Escape;
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
/// `view::box_colour`), or the ground marker shows where the click was. Only the local player's own clicks come here
/// (a right-click, or a target-mode click), never a command being carried out, so no other player's or computer
/// player's command makes a marker (selecting spec, "Feedback"); the new marker replaces the old one.
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

/// Where a command click's ground marker goes: the clicked map pixel on the map view; on the minimap, the tile's pixel
/// position plus `minimap_offset` on each axis: 8 for a right-click (selecting spec, "Right-click on the minimap"), 0
/// for a target-mode left click (tile × 32, "Target mode and placement mode clicks").
fn mark_spot(at: Option<Vec2>, tile: (i32, i32), minimap_offset: i32) -> (i32, i32) {
    at.map_or(
        (tile.0 * TILE_PX + minimap_offset, tile.1 * TILE_PX + minimap_offset),
        px,
    )
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
    let mark = mark_spot(at, tile, 8);
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
        let mark = mark_spot(at, tile, 0);
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
fn centre_view(view: &mut mapview::MapView, c: Vec2) {
    view.centre = (c.x, c.y);
}

/// The mouse on the map view, minimap and panel by the handler table (`mouse::action`, spec
/// `ui/selecting-and-commanding.md`, "Mouse regions"): selecting, the right-click command, target-mode and placement
/// clicks, and moving the view.
pub(crate) fn mouse_input(c: &mut Core) {
    let Core {
        time,
        view,
        sim,
        art,
        selection: Some(sel),
        hud: Some(hud),
        panel_ui: Some(ui),
        screen_ui: Some(scr),
        input,
        ..
    } = c
    else {
        return;
    };
    let input = &*input;
    let keys = &input.keys;
    let buttons = &input.mouse;
    scr.pointer = None;
    let (Some(sim), Some(_), Some(mv), Some(layout)) = (sim.as_mut(), input.window, view.as_mut(), hud.layout) else {
        return;
    };
    let art = art.as_ref();
    let enhanced = sim.world.rules.enhanced;
    let shift = keys.any_pressed([Key::ShiftLeft, Key::ShiftRight]);
    let ctrl = keys.any_pressed([Key::ControlLeft, Key::ControlRight]);
    let alt = keys.any_pressed([Key::AltLeft, Key::AltRight]);
    let at = cursor_map(input, mv);
    let view = mv.tiles();
    // Development aid (`--demo-site`): one placement click, after the first fog refresh so the site is explored.
    if ui.demo_click
        && let panel::Mode::Place(kind) = ui.mode
        && let (Some(tile), Some(&builder)) = (ui.demo_pointer, sel.units.first())
        && sim.world.tick >= 2
    {
        ui.demo_click = false;
        place_click(sim, builder, ui, kind, tile);
    }
    if buttons.just_released(Button::Left) {
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
    let geom = mv.geom(hud.map);
    let pointer = hud.cursor(input).map(|c| (c.x.floor() as i32, c.y.floor() as i32));
    let regions = mouse::Regions::of_layout(&layout);
    let on_minimap = hud
        .cursor(input)
        .is_some_and(|c| regions.at((c.x, c.y)) == mouse::Region::Minimap);
    let minimap_tile = pointer
        .filter(|_| on_minimap)
        .map(|p| minimap::tile_for_press(width, p));
    if sel.minimap_held
        && buttons.pressed(Button::Left)
        && let Some(p) = pointer
    {
        mv.move_to(minimap::view_for_press(width, p, geom.tiles, geom.max()));
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
    } else if hud.over_map(input) && map_tile.is_some() {
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
    let Some(cursor) = hud.cursor(input).filter(|_| !ui.consumed) else {
        return;
    };
    let region = regions.at((cursor.x, cursor.y));
    for (button, b) in [
        (Button::Left, mouse::Button::Left),
        (Button::Right, mouse::Button::Right),
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
                    centre_view(mv, Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5) * TILE_PX as f32);
                }
            }
            mouse::Action::MoveView => {
                if let Some(p) = pointer {
                    mv.move_to(minimap::view_for_press(width, p, geom.tiles, geom.max()));
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
pub(crate) fn game_key(k: Key) -> Option<mouse::Key> {
    Some(match k {
        Key::Equal | Key::NumpadAdd => mouse::Key::Plus,
        Key::Minus | Key::NumpadSubtract => mouse::Key::Minus,
        Key::F(n @ 1..=12) => mouse::Key::F(n),
        k => match key_char(k)? {
            c @ '0'..='9' => mouse::Key::Digit(c as u8 - b'0'),
            c => mouse::Key::Letter(c),
        },
    })
}

/// The in-game keys (`mouse::key_action`, spec `ui/selecting-and-commanding.md`, "Keys" and "Control groups"). The
/// ones that open menus and dialogs, and Ctrl+S, are handled by `gamemenu`.
pub(crate) fn keys_input(c: &mut Core) {
    let Core {
        time,
        sim: Some(sim),
        art,
        selection: Some(sel),
        stored,
        panel_ui: Some(ui),
        hud: Some(hud),
        screen_ui: Some(scr),
        view: Some(mv),
        input,
        ..
    } = c
    else {
        return;
    };
    let keys = &input.keys;
    if ui.consumed {
        return;
    }
    let human = sim.human;
    // Messages spec, "Event history": the space key (an open chat line takes it first, `chat`).
    if keys.just_pressed(Key::Space)
        && let Some(h) = &mut scr.history
    {
        let now = (time.elapsed_secs_f64() * 1000.0) as u64;
        if let Some(v) = h.space(now, sim.world.rules.enhanced) {
            let geom = mv.geom(hud.map);
            mv.move_to(geom.clamp(v));
        }
    }
    let m = mouse::KeyMods {
        shift: keys.any_pressed([Key::ShiftLeft, Key::ShiftRight]),
        ctrl: keys.any_pressed([Key::ControlLeft, Key::ControlRight]),
        alt: keys.any_pressed([Key::AltLeft, Key::AltRight]),
    };
    for &k in keys.get_just_pressed() {
        // The client plays one human player; while the chat line is open, `chat` has taken the typing keys already.
        let Some(a) = game_key(k).and_then(|key| mouse::key_action(key, m, false, sim.world.rules.enhanced, false))
        else {
            continue;
        };
        match a {
            mouse::KeyAction::Speed(faster) => sim.pending.push(Command::Speed { player: human, faster }),
            mouse::KeyAction::Pause => sim.pending.push(Command::Pause { player: human }),
            mouse::KeyAction::Group(key, g) => {
                let view = mv.tiles();
                let group = sim.world.selections[human as usize]
                    .groups
                    .get(g as usize)
                    .cloned()
                    .unwrap_or_default();
                let mode = matches!(ui.mode, panel::Mode::Target(_) | panel::Mode::Place(_));
                let now = (time.elapsed_secs_f64() * 1000.0) as u64;
                let (plan, half) = with_board(&sim.world, human, art.as_ref(), view, |b| {
                    (sel.groups.press(b, key, g, &group, mode, now), b.view.view_tiles / 2)
                });
                if let Some(t) = plan.view {
                    let c = Vec2::new((t.0 + half) as f32 + 0.5, (t.1 + half) as f32 + 0.5);
                    centre_view(mv, c * TILE_PX as f32);
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
                    centre_view(mv, Vec2::new(u.centre_px().0 as f32, u.centre_px().1 as f32));
                }
            }
            // Game-menu spec, Ctrl+M: the stored music volume swaps with 0; `sync_settings` passes it to the music.
            mouse::KeyAction::ToggleMusic => {
                stored.0.toggle_music();
            }
            mouse::KeyAction::GotoView(n) => {
                let geom = mv.geom(hud.map);
                mv.move_to(geom.clamp(scr.views[n as usize]));
            }
            mouse::KeyAction::RememberView(n) => {
                scr.views[n as usize] = mv.geom(hud.map).pos;
            }
            _ => {}
        }
    }
}

/// Plays the cues the sim queued since the last frame (sounds spec, "How a sound is played"), and shows their
/// subtitles on the notice line.
pub(crate) fn play_sounds(c: &mut Core) {
    let Core {
        sim: Some(sim),
        speaker: Some(speaker),
        view: Some(view),
        audio,
        ..
    } = c
    else {
        return;
    };
    let ui_sounds = std::mem::take(&mut sim.ui_sounds);
    if sim.world.sounds.cues.is_empty() && ui_sounds.is_empty() {
        return;
    }
    let (l, t, r, b) = view.tiles();
    let view = sound::View {
        x: l.floor() as i32,
        y: t.floor() as i32,
        w: (r - l).floor() as i32,
        h: (b - t).floor() as i32,
    };
    let cues = std::mem::take(&mut sim.world.sounds.cues);
    let sp = speaker;
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
        let decoded = sp
            .sounds
            .entry(path)
            .or_insert_with_key(|p| audio::Sound::from_wav(&install.read(p).ok()?))
            .clone();
        if let Some(s) = decoded {
            // Our choice: a level maps linearly to loudness (the game's volume table isn't specified).
            let (volume, gain) = (level as f32 / 255.0, audio::level_gain(level));
            audio.play(&s, volume, gain, audio::Mode::Despawn, audio::Group::Effect, 0);
        }
    }
}

/// The information panel's icons: pointing names the unit on the status line, a released click selects, removes,
/// unloads or centres the view (`info::click`).
pub(crate) fn info_input(c: &mut Core) {
    let Core {
        sim,
        selection: Some(sel),
        hud: Some(hud),
        panel_ui: Some(ui),
        info_panel: Some(panel),
        screen_ui: Some(scr),
        view,
        input,
        ..
    } = c
    else {
        return;
    };
    let input = &*input;
    let keys = &input.keys;
    let buttons = &input.mouse;
    panel.hover = None;
    let (Some(sim), Some(_), Some(l)) = (sim.as_mut(), input.window, hud.layout) else {
        return;
    };
    let slot = hud
        .cursor(input)
        .and_then(|c| info::slot_at((c.x - l.info.x as f32, c.y - l.info.y as f32)))
        .filter(|&n| panel.slots[n].is_some());
    let slots = panel.slots;
    let unit = |n: usize| slots[n].and_then(|id| sim.world.unit(id));
    panel.hover = slot
        .and_then(&unit)
        .and_then(|u| info::name_entry(u.kind))
        .map(|e| hud.art.string(e));
    if buttons.just_pressed(Button::Left) && slot.is_some() && !ui.consumed && scr.edge == (0, 0) {
        panel.pressed = slot;
        ui.consumed = true;
    }
    if !buttons.just_released(Button::Left) {
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
    let shift = keys.any_pressed([Key::ShiftLeft, Key::ShiftRight]);
    match info::click(&slots, n, sel.units.len(), shift, transport, centre_ok) {
        // Selection changes are sent as the synced selection; our reading: with the selection sound, as a click.
        info::Click::SelectOnly(id) => sel.set(sim, vec![id], true),
        info::Click::Remove(id) => {
            let rest = sel.units.iter().copied().filter(|&u| u != id).collect();
            sel.set(sim, rest, false);
        }
        info::Click::Unload { transport, place } => sim.unload_one.push((transport, place)),
        info::Click::Centre(id) => {
            if let (Some(u), Some(view)) = (sim.world.unit(id), view.as_mut()) {
                let (x, y) = u.centre_px();
                view.centre = (x as f32, y as f32);
            }
        }
        info::Click::Nothing => {}
    }
}

/// Rebuilds the information panel when the selection changes (two-pass slot assignment, the selection put in slot
/// order so its first unit is the primary unit) and its text when a change check fires (`info::watch`). The icons
/// and health bars are drawn from the sim every frame, which redraws each icon when its hit points change.
pub(crate) fn update_info(c: &mut Core) {
    let Core {
        sim: Some(sim),
        selection: Some(sel),
        info_panel: Some(panel),
        ..
    } = c
    else {
        return;
    };
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

/// Scrolling at the screen edge and with the keys (`screen`, spec `ui/screen-and-cursors.md`, "Scrolling"); also sets
/// the scrolling edge that blocks this frame's clicks.
pub(crate) fn scroll_view(c: &mut Core) {
    let Core {
        time,
        sim,
        hud: Some(hud),
        screen_ui: Some(scr),
        view,
        input,
        ..
    } = c
    else {
        return;
    };
    let input = &*input;
    let keys = &input.keys;
    scr.edge = (0, 0);
    let (Some(sim), Some(_), Some(view), Some(l)) = (sim.as_ref(), input.window, view.as_mut(), hud.layout) else {
        return;
    };
    let geom = view.geom(hud.map);
    // No menus or demo yet: only a pause blocks scrolling.
    let blocked = sim.world.paused;
    if !blocked && let Some(c) = hud.cursor(input) {
        let canvas = (l.canvas.0 as i32, l.canvas.1 as i32);
        scr.edge = screen::edge((c.x.floor() as i32, c.y.floor() as i32), canvas, &geom);
    }
    use screen::ScrollKey as K;
    let held: Vec<K> = [
        (Key::Up, K::Up),
        (Key::Down, K::Down),
        (Key::Left, K::Left),
        (Key::Right, K::Right),
        (Key::Home, K::Home),
        (Key::PageUp, K::PageUp),
        (Key::End, K::End),
        (Key::PageDown, K::PageDown),
        (Key::Numpad(8), K::Pad(8)),
        (Key::Numpad(2), K::Pad(2)),
        (Key::Numpad(4), K::Pad(4)),
        (Key::Numpad(6), K::Pad(6)),
        (Key::Numpad(7), K::Pad(7)),
        (Key::Numpad(9), K::Pad(9)),
        (Key::Numpad(1), K::Pad(1)),
        (Key::Numpad(3), K::Pad(3)),
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
        ctrl_or_alt: keys.any_pressed([Key::ControlLeft, Key::ControlRight, Key::AltLeft, Key::AltRight]),
        blocked,
    };
    if let Some(d) = scr.scroller.update(input) {
        view.move_to(geom.step(d));
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
pub(crate) fn update_messages(c: &mut Core) {
    let Core {
        time,
        sim: Some(sim),
        hud: Some(hud),
        panel_ui: Some(ui),
        info_panel: Some(info_panel),
        screen_ui: Some(scr),
        ..
    } = c
    else {
        return;
    };
    let now = (time.elapsed_secs_f64() * 1000.0) as u64;
    for n in std::mem::take(&mut sim.notices) {
        scr.messages.notice(&text_of(hud, &n), 0, now);
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_hud(c: &mut Core) {
    // While a menu or dialog is open the screen draws the normal arrow instead (`shell`).
    let running = core::game_running(c);
    let Core {
        sim,
        selection: Some(sel),
        hud: Some(hud),
        panel_ui: Some(ui),
        info_panel: Some(info_panel),
        screen_ui: Some(scr),
        cycling: Some(cycling),
        pictures,
        ..
    } = c
    else {
        return;
    };
    let (Some(sim), Some(layout)) = (sim.as_ref(), hud.layout) else {
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
            pause_button: pause_button(sim),
            pointer: scr.pointer.filter(|_| running && hud.art.has_pointers()),
            minimap: &hud.minimap.pixels,
            buttons: &buttons,
        },
    );
    let mut pic = pic;
    palette::apply(&mut pic.rgba, cycling.effects);
    if let Some(id) = hud.image {
        pictures.set(id, pic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_goes_at_the_click_or_the_minimap_tile() {
        assert_eq!(mark_spot(Some(Vec2::new(100.7, 41.2)), (3, 1), 8), (100, 41));
        assert_eq!(mark_spot(Some(Vec2::new(100.7, 41.2)), (3, 1), 0), (100, 41));
        assert_eq!(
            mark_spot(None, (3, 1), 8),
            (104, 40),
            "minimap right-click: the tile's pixel plus 8"
        );
        assert_eq!(
            mark_spot(None, (3, 1), 0),
            (96, 32),
            "minimap target-mode click: tile × 32, no + 8"
        );
    }

    #[test]
    fn the_pointer_is_the_exact_point() {
        // The map view at factor 2 and scale 2 (`display::Letterbox`): screen pixel (620, 460) is 220 pixels right of
        // and 220 below the view's centre, at one map pixel per screen pixel.
        let mut input = input::Input::default();
        input.window = Some(input::WindowGeom {
            size: (1280, 960),
            factor: 2.0,
        });
        input.pointer = Some((620.0, 460.0));
        let mut view = mapview::MapView::new((1000.0, 500.0));
        view.rect = Some(mapview::ScreenRect {
            x: 176,
            y: 16,
            w: 448,
            h: 448,
        });
        view.end_frame();
        let at = cursor_map(&input, &view).unwrap();
        assert!((at - Vec2::new(1220.0, 720.0)).abs().max_element() < 1e-3, "{at}");
        // The flow's screen: the window's middle is the screen's.
        input.pointer = Some((320.0, 240.0));
        let (x, y) = display::Letterbox::fit((1280, 960)).screen_of(input.pointer.unwrap(), 2.0);
        assert!((x - 320.0).abs() < 1e-3 && (y - 240.0).abs() < 1e-3, "{x} {y}");
    }
}
