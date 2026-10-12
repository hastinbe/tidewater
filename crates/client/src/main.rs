//! Tidewater client. Loads a PUD from the user's installation, runs the simulation at its fixed tick rate and draws it.
//!
//! Controls: left click or drag selects your units, right click orders them to move. The command panel (lower left)
//! shows the selection's buttons (`panel`): click one or press its hotkey; commands that need a target take the next
//! left click on the map (right click or Escape cancels). Arrow keys pan, mouse wheel or +/- zoom, Escape cancels a
//! panel mode and otherwise quits. On the minimap, left click or drag moves the view and right click orders a move
//! there.

mod fog;
mod hud;
mod info;
mod panel;
mod scene;
mod sound;

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
use tidewater_sim::economy::{Event, Shortage};
use tidewater_sim::victory::Outcome;
use tidewater_sim::{Command, TILE_PX, UnitId, World};

#[derive(Parser)]
#[command(about = "Tidewater map viewer")]
struct Args {
    /// Warcraft II: Battle.net Edition installation directory.
    #[arg(long, env = "TIDEWATER_INSTALL")]
    install: PathBuf,
    /// A .pud file path, or a name inside the archives.
    map: String,
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
    /// The message in the bottom bar and the ticks it has left.
    message: Option<(String, u32)>,
    /// Passengers to put ashore one at a time (transport, place 0 to 5), from clicks on the information panel.
    unload_one: Vec<(UnitId, usize)>,
}

/// Plays the sim's sound cues (`sound.rs`, spec `docs/specs/ui/sounds.md`): reads the files from the install on first
/// use and keeps the decoded sources by name.
#[derive(Resource)]
struct Speaker {
    director: sound::Director,
    install: Install,
    /// Archive name to source; `None` for a file the install doesn't have, so it is looked up once.
    sources: HashMap<String, Option<Handle<AudioSource>>>,
    /// `Rez\network.tbl`, for the subtitles.
    subtitles: Option<tidewater_formats::tbl::StringTable>,
}

/// How long a message stays up: five seconds of game time.
const MESSAGE_TICKS: u32 = 5 * 25;

/// Our wording for a message to the player, if the event is theirs.
fn message(e: &Event, human: u8) -> Option<String> {
    match *e {
        Event::CantAfford { player, shortage } if player == human => Some(
            match shortage {
                Shortage::Food => "Not enough food: build more farms",
                Shortage::Gold => "Not enough gold",
                Shortage::Lumber => "Not enough lumber",
                Shortage::Oil => "Not enough oil",
            }
            .to_string(),
        ),
        Event::NoRoom { player, .. } if player == human => Some("No room to place the unit".to_string()),
        Event::CantPlace { player, .. } if player == human => Some("Can't build there".to_string()),
        Event::CantRepair { player, shortage } if player == human => Some(
            match shortage {
                Shortage::Lumber => "Repairs stopped: out of lumber",
                _ => "Repairs stopped: out of gold",
            }
            .to_string(),
        ),
        Event::ResearchDone { player, .. } if player == human => Some("Research complete".to_string()),
        Event::GameOver { player, outcome } => Some(match outcome {
            Outcome::Victory if player == human => "Victory!".to_string(),
            Outcome::Defeat if player == human => "You have been defeated".to_string(),
            Outcome::Defeat => format!("Player {} has been defeated", player + 1),
            _ => "The game has ended".to_string(),
        }),
        _ => None,
    }
}

/// The command panel's input state (see `panel`).
#[derive(Resource, Default)]
struct PanelUi {
    mode: panel::Mode,
    /// A panel click or key was handled this frame; the map must not also act on it.
    consumed: bool,
    /// Escape was used by the panel this frame and must not quit.
    escape_used: bool,
    /// The tooltip of the button under the cursor.
    tooltip: Option<String>,
    /// The selection the mode belongs to; a different selection ends the mode.
    last_sel: Vec<UnitId>,
}

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

#[derive(Resource, Default)]
struct Selection {
    units: Vec<UnitId>,
    /// Where a left-button drag started, in world space.
    drag_from: Option<Vec2>,
}

/// The terrain picture and what redraws its tiles.
#[derive(Resource)]
struct TerrainArt {
    image: Handle<Image>,
    tileset: tidewater_formats::tileset::Tileset,
}

/// Unit sprites: each type's sheet, and the pictures made from it so far per (type, owner, sheet frame).
#[derive(Resource)]
struct Art {
    sheets: HashMap<u8, tidewater_formats::grp::Grp>,
    palette: tidewater_formats::tileset::Palette,
    made: HashMap<(u8, u8, usize), Handle<Image>>,
}

impl Art {
    /// The picture of sheet frame `f` of `kind` in `owner`'s colours, made on first use.
    fn picture(&mut self, images: &mut Assets<Image>, kind: u8, owner: u8, f: usize) -> Option<Handle<Image>> {
        let grp = self.sheets.get(&kind)?;
        let h = self
            .made
            .entry((kind, owner, f))
            .or_insert_with(|| images.add(image(scene::frame_picture(grp, f, &self.palette, owner))));
        Some(h.clone())
    }
}

/// Links a sprite entity to its unit.
#[derive(Component)]
struct UnitSprite(UnitId);

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
    tiles: Vec<[u8; 3]>,
    /// Map size in tiles.
    map: (u32, u32),
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

    /// The map tile under the cursor if it is over the minimap.
    fn minimap_tile(&self, window: &Window) -> Option<Vec2> {
        let (l, p) = (self.layout?, self.cursor(window)?);
        l.minimap.contains(p.x, p.y).then(|| {
            let t = hud::Minimap {
                rect: l.minimap,
                map: self.map,
            }
            .tile_at((p.x, p.y));
            Vec2::new(t.0, t.1)
        })
    }
}

/// The fog overlay picture (one texel per quarter tile) and the sim state it was last drawn from.
#[derive(Resource)]
struct FogArt {
    image: Handle<Image>,
    drawn: Option<(u32, bool)>,
}

/// Above every unit (ground 1 to 4 plus a fraction for rows).
const FOG_Z: f32 = 20.0;

#[derive(Component)]
struct MapCamera;

#[derive(Component)]
struct HudSprite;

const HUD_LAYER: usize = 1;

fn main() -> Result<()> {
    let args = Args::parse();
    let mut install = Install::open(&args.install)?;
    let scene = scene::load(&mut install, &args.map)?;
    println!(
        "{:?} map, {} units, you are player {}: {}",
        scene.era,
        scene.world.units.len(),
        scene.human,
        scene.description
    );
    let race = scene.world.players[scene.human as usize].race;
    let hud_art = hud::HudArt::load(&mut install, scene.era, race, &scene.palette)?;
    let hz = 1000.0 / tidewater_sim::timing::ENHANCED_MS[tidewater_sim::timing::NORMAL as usize] as f64;
    let mut scene = scene;
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
    let speaker = Speaker {
        director: sound::Director::new(sound::SoundSet::of_race(race), scene.era, scene.seed),
        subtitles: install
            .read("rez\\network.tbl")
            .ok()
            .and_then(|d| tidewater_formats::tbl::StringTable::parse(&d).ok()),
        install,
        sources: HashMap::new(),
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
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()).set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Tidewater — {}", args.map),
                resolution: (640 * args.ui_scale, 480 * args.ui_scale).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Loaded(Some(scene)))
        .insert_resource(MapSize(size))
        .insert_resource(StartZoom(args.zoom))
        .insert_resource(Hud {
            art: hud_art,
            scale: args.ui_scale,
            layout: None,
            frame: scene::Picture {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
            },
            image: Handle::default(),
            tiles: Vec::new(),
            map: (0, 0),
        })
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .insert_resource(selection)
        .insert_resource(speaker)
        .init_resource::<PanelUi>()
        .init_resource::<InfoPanel>()
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, tick)
        .add_systems(
            Update,
            (
                hud_layout,
                minimap_input,
                panel_input,
                info_input,
                pan_and_zoom,
                select,
                order,
                play_sounds,
                sync_sprites,
                redraw_terrain,
                redraw_fog,
                draw_selection,
                update_info,
                draw_hud,
                quit_on_escape,
            )
                .chain(),
        )
        .run();
    Ok(())
}

fn image(p: scene::Picture) -> Image {
    image_with(p, RenderAssetUsages::RENDER_WORLD)
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

fn setup(
    mut commands: Commands,
    mut loaded: ResMut<Loaded>,
    mut images: ResMut<Assets<Image>>,
    size: Res<MapSize>,
    zoom: Res<StartZoom>,
    mut hud: ResMut<Hud>,
    sel: Res<Selection>,
) {
    let scene = loaded.0.take().expect("scene loaded once");
    // Start the view on the selection if there is one, else on the local player's start location (types 0x5E and
    // 0x5F), else on one of their units, else the map centre.
    let own = |u: &&tidewater_sim::Unit| u.owner == scene.human;
    let look = sel
        .units
        .first()
        .and_then(|&id| scene.world.unit(id))
        .or_else(|| {
            let mut mine = scene.world.units.iter().filter(own);
            mine.clone()
                .find(|u| matches!(u.kind, 0x5e | 0x5f))
                .or_else(|| mine.next())
        })
        .map_or(size.0 / 2.0, |u| {
            Vec2::new(u.centre_px().0 as f32, u.centre_px().1 as f32)
        });
    hud.tiles = scene.tile_colours.clone();
    hud.map = (
        scene.terrain.width / TILE_PX as u32,
        scene.terrain.height / TILE_PX as u32,
    );
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
    let (map_w, map_h) = (scene.world.terrain.width as u32, scene.world.terrain.height as u32);
    let fog = images.add(image_with(
        scene::Picture {
            width: map_w * 2,
            height: map_h * 2,
            rgba: [0, 0, 0, 255].repeat((map_w * map_h * 4) as usize),
        },
        RenderAssetUsages::default(),
    ));
    commands.spawn((
        Sprite {
            custom_size: Some(size.0),
            ..Sprite::from_image(fog.clone())
        },
        Transform::from_translation(world(size.0.x / 2.0, size.0.y / 2.0, FOG_Z)),
    ));
    commands.insert_resource(FogArt {
        image: fog,
        drawn: None,
    });
    commands.insert_resource(Art {
        sheets: scene.sheets,
        palette: scene.palette.clone(),
        made: HashMap::new(),
    });
    commands.insert_resource(Sim {
        world: scene.world,
        pending: Vec::new(),
        human: scene.human,
        message: None,
        unload_one: Vec::new(),
    });
}

fn tick(mut sim: ResMut<Sim>, mut sel: ResMut<Selection>) {
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
    // A dying unit is deselected (corpses spec).
    sel.units.retain(|&id| sim.world.unit(id).is_some_and(|u| !u.dying));
    let human = sim.human;
    let events = std::mem::take(&mut sim.world.events);
    if let Some(m) = events.iter().rev().find_map(|e| message(e, human)) {
        sim.message = Some((m, MESSAGE_TICKS));
    } else if let Some((_, left)) = &mut sim.message {
        *left -= 1;
        if *left == 0 {
            sim.message = None;
        }
    }
}

/// Redraws tiles whose shape changed (felled trees, broken walls) on the terrain picture and the minimap.
fn redraw_terrain(mut sim: ResMut<Sim>, art: Res<TerrainArt>, mut images: ResMut<Assets<Image>>, mut hud: ResMut<Hud>) {
    let changed = std::mem::take(&mut sim.world.terrain.redraw);
    if changed.is_empty() {
        return;
    }
    let Some(mut img) = images.get_mut(&art.image) else {
        return;
    };
    let width = img.width() as usize;
    let Some(data) = img.data.as_mut() else { return };
    let map_w = sim.world.terrain.width as usize;
    let t = TILE_PX as usize;
    for (x, y) in changed {
        let Some(m) = sim.world.terrain.tile(x, y) else {
            continue;
        };
        let pic = scene::tile_picture(&art.tileset, m as usize);
        for row in 0..t {
            let at = ((y as usize * t + row) * width + x as usize * t) * 4;
            data[at..at + t * 4].copy_from_slice(&pic.rgba[row * t * 4..(row + 1) * t * 4]);
        }
        if let Some(c) = hud.tiles.get_mut(y as usize * map_w + x as usize) {
            *c = hud::tile_colours(&pic, TILE_PX as u32)[0];
        }
    }
}

/// Redraws the fog overlay when the sim's explored or visible map has changed.
fn redraw_fog(sim: Option<Res<Sim>>, fog: Option<ResMut<FogArt>>, mut images: ResMut<Assets<Image>>) {
    let (Some(sim), Some(mut fog)) = (sim, fog) else { return };
    let v = &sim.world.vision;
    let key = (v.generation, sim.world.rules.reveal_map);
    if fog.drawn == Some(key) {
        return;
    }
    let Some(mut img) = images.get_mut(&fog.image) else {
        return;
    };
    let (explored, visible) = fog::maps(&sim.world);
    let px = fog::overlay(explored, visible, v.width, v.height, key.1);
    if let Some(data) = img.data.as_mut() {
        data.copy_from_slice(&px);
        fog.drawn = Some(key);
    }
}

/// Draw depth: buildings under ground units under ships under aircraft; lower on screen draws in front.
fn depth(u: &tidewater_sim::Unit) -> f32 {
    let layer = match (u.building, u.layer) {
        (true, _) => 1.0,
        (_, tidewater_sim::Layer::Land) => 2.0,
        (_, tidewater_sim::Layer::Sea) => 3.0,
        (_, tidewater_sim::Layer::Air) => 4.0,
    };
    layer + u.centre_px().1 as f32 * 1e-5
}

fn sync_sprites(
    mut commands: Commands,
    sim: Option<Res<Sim>>,
    art: Option<ResMut<Art>>,
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(Entity, &UnitSprite, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let (Some(sim), Some(mut art)) = (sim, art) else { return };
    let mut drawn = std::collections::HashSet::new();
    for (e, us, mut sprite, mut tf, mut vis) in &mut q {
        let Some(u) = sim.world.unit(us.0) else {
            commands.entity(e).despawn();
            continue;
        };
        drawn.insert(u.id);
        // Units the local player can't see aren't drawn (vision spec, "Unit visibility").
        let shown = if fog::unit_shown(&sim.world, u) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        vis.set_if_neq(shown);
        // Units inside mines, platforms, depots and transports aren't drawn.
        sprite.color = if u.inside { Color::NONE } else { Color::WHITE };
        tf.translation = world(u.centre_px().0 as f32, u.centre_px().1 as f32, depth(u));
        let Some(frames) = art.sheets.get(&u.kind).map(|g| g.frames.len()) else {
            sprite.color = Color::NONE;
            continue;
        };
        let (f, flip) = scene::sprite_frame(u, frames);
        if let Some(h) = art.picture(&mut images, u.kind, u.owner, f)
            && sprite.image != h
        {
            sprite.image = h;
        }
        sprite.flip_x = flip;
    }
    // Units created during play get a sprite; the next run places it.
    for u in sim.world.units.iter().filter(|u| !drawn.contains(&u.id)) {
        commands.spawn((
            Sprite::default(),
            Transform::default(),
            Visibility::Hidden,
            UnitSprite(u.id),
        ));
    }
}

/// The cursor position in map pixels (y down), if it is over the window.
fn cursor_map(window: &Window, cam: (&Camera, &GlobalTransform)) -> Option<Vec2> {
    let p = cam.0.viewport_to_world_2d(cam.1, window.cursor_position()?).ok()?;
    Some(Vec2::new(p.x, -p.y))
}

fn select(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<MapCamera>>,
    sim: Option<ResMut<Sim>>,
    mut sel: ResMut<Selection>,
    hud: Res<Hud>,
    ui: Res<PanelUi>,
) {
    let (Some(mut sim), Ok(window), Ok(cam)) = (sim, windows.single(), cams.single()) else {
        return;
    };
    let Some(at) = cursor_map(window, cam) else { return };
    if ui.consumed {
        return;
    }
    if buttons.just_pressed(MouseButton::Left) && hud.over_map(window) {
        sel.drag_from = Some(at);
    }
    if !buttons.just_released(MouseButton::Left) {
        return;
    }
    let Some(from) = sel.drag_from.take() else { return };
    let (lo, hi) = (from.min(at), from.max(at));
    let mine = |u: &&tidewater_sim::Unit| u.owner == sim.human && !u.building;
    let picked: Vec<UnitId> = if (hi - lo).length() < 4.0 {
        // A click: the unit whose footprint contains the point.
        let tile = ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX));
        unit_at(&sim.world, tile).into_iter().collect()
    } else {
        sim.world
            .units
            .iter()
            .filter(mine)
            .filter(|u| {
                let p = Vec2::new(u.centre_px().0 as f32, u.centre_px().1 as f32);
                p.cmpge(lo).all() && p.cmple(hi).all()
            })
            .take(9)
            .map(|u| u.id)
            .collect()
    };
    // Sounds spec, "Selecting units": the selection's first unit; in Classic rules only a single unit. Open question
    // "Which selecting actions ask for the sound": every action that changes the selection does.
    if let Some(&first) = picked.first()
        && (picked.len() == 1 || sim.world.rules.enhanced)
    {
        sim.world.select_sound(first);
    }
    sel.units = picked;
}

fn order(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<MapCamera>>,
    sim: Option<ResMut<Sim>>,
    sel: Res<Selection>,
    hud: Res<Hud>,
    ui: Res<PanelUi>,
) {
    let (Some(mut sim), Ok(window), Ok(cam)) = (sim, windows.single(), cams.single()) else {
        return;
    };
    if ui.consumed {
        return;
    }
    let human = sim.human;
    let units: Vec<UnitId> = sel
        .units
        .iter()
        .copied()
        .filter(|&id| sim.world.unit(id).is_some_and(|u| u.owner == human))
        .collect();
    let Some(&first) = units.first() else { return };
    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    // A right click on the minimap moves there; on the map view, an enemy under the cursor is attacked if any
    // selected unit can, and anything else is a smart move.
    let (to, on_map) = if let Some(t) = hud.minimap_tile(window) {
        let max = (hud.map.0 as i32 - 1, hud.map.1 as i32 - 1);
        (((t.x as i32).min(max.0), (t.y as i32).min(max.1)), false)
    } else if hud.over_map(window) {
        let Some(at) = cursor_map(window, cam) else { return };
        (
            ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX)),
            true,
        )
    } else {
        return;
    };
    let foe = unit_at(&sim.world, to).filter(|&t| {
        on_map
            && units
                .iter()
                .any(|&id| sim.world.unit(id).is_some_and(|u| sim.world.can_attack(u, t)))
    });
    // Our own choice: a right click on a damaged building or transport of the player's sends the workers to repair it.
    let mend = unit_at(&sim.world, to).filter(|&t| {
        on_map
            && sim.world.unit(t).is_some_and(|t| t.owner == human)
            && units
                .iter()
                .any(|&id| sim.world.unit(id).is_some_and(|u| sim.world.can_repair(u, t)))
    });
    let c = match (mend, foe) {
        (Some(target), _) => Command::Repair {
            player: human,
            units,
            target,
        },
        (None, Some(target)) => Command::Attack {
            player: human,
            units,
            target,
        },
        // Orders spec, "Smart move": returning goods, harvesting, following or moving, decided per unit.
        (None, None) => Command::Smart {
            player: human,
            units,
            target: unit_at(&sim.world, to).filter(|_| on_map),
            tile: to,
        },
    };
    sim.pending.push(c);
    sim.world.acknowledge(first);
}

/// The living unit whose footprint covers a tile.
fn unit_at(w: &tidewater_sim::World, (x, y): (i32, i32)) -> Option<UnitId> {
    w.units
        .iter()
        .find(|u| {
            u.present()
                && x >= u.tile.0
                && y >= u.tile.1
                && x < u.tile.0 + u.size.0 as i32
                && y < u.tile.1 + u.size.1 as i32
        })
        .map(|u| u.id)
}

fn draw_selection(
    mut gizmos: Gizmos,
    sim: Option<Res<Sim>>,
    sel: Res<Selection>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform), With<MapCamera>>,
    ui: Res<PanelUi>,
    hud: Res<Hud>,
) {
    let Some(sim) = sim else { return };
    // Placing a building: its footprint under the cursor.
    if let (panel::Mode::Place(kind), Ok(window), Ok(cam)) = (ui.mode, windows.single(), cams.single())
        && hud.over_map(window)
        && let Some(at) = cursor_map(window, cam)
    {
        let st = panel::WorldState {
            world: &sim.world,
            local: sim.human,
        };
        let (w, h) = st.footprint(kind);
        let size = Vec2::new(w as f32, h as f32) * TILE_PX as f32;
        let tile = Vec2::new(
            (at.x as i32).div_euclid(TILE_PX) as f32,
            (at.y as i32).div_euclid(TILE_PX) as f32,
        );
        let c = tile * TILE_PX as f32 + size / 2.0;
        gizmos.rect_2d(Vec2::new(c.x, -c.y), size, Color::srgb(0.9, 0.9, 0.2));
    }
    let green = Color::srgb(0.2, 0.9, 0.2);
    // Missiles have no sprites yet: draw each as a small square.
    for m in sim.world.missiles.iter().flatten() {
        let c = if m.splash {
            Color::srgb(1.0, 0.5, 0.1)
        } else {
            Color::srgb(1.0, 1.0, 0.6)
        };
        gizmos.rect_2d(Vec2::new(m.pos.0 as f32, -(m.pos.1 as f32)), Vec2::splat(4.0), c);
    }
    for &id in &sel.units {
        let Some(u) = sim.world.unit(id) else { continue };
        let size = Vec2::new(u.size.0 as f32, u.size.1 as f32) * TILE_PX as f32;
        gizmos.rect_2d(
            Vec2::new(u.centre_px().0 as f32, -(u.centre_px().1 as f32)),
            size,
            green,
        );
    }
    if let (Some(from), Ok(window), Ok(cam)) = (sel.drag_from, windows.single(), cams.single())
        && let Some(at) = cursor_map(window, cam)
    {
        let c = (from + at) / 2.0;
        gizmos.rect_2d(Vec2::new(c.x, -c.y), (at - from).abs(), green);
    }
}

fn pan_and_zoom(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    size: Res<MapSize>,
    mut cam: Query<(&Camera, &mut Transform, &mut Projection), With<MapCamera>>,
) {
    let Ok((camera, mut tf, mut proj)) = cam.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = proj.as_mut() else {
        return;
    };
    let mut zoom = 0.0;
    if keys.pressed(KeyCode::Equal) {
        zoom -= 2.0 * time.delta_secs();
    }
    if keys.pressed(KeyCode::Minus) {
        zoom += 2.0 * time.delta_secs();
    }
    zoom -= scroll.delta.y * 0.1;
    ortho.scale = (ortho.scale * (1.0 + zoom)).clamp(0.25, 8.0);

    let mut d = Vec2::ZERO;
    for (k1, v) in [
        (KeyCode::ArrowLeft, Vec2::NEG_X),
        (KeyCode::ArrowRight, Vec2::X),
        (KeyCode::ArrowUp, Vec2::Y),
        (KeyCode::ArrowDown, Vec2::NEG_Y),
    ] {
        if keys.pressed(k1) {
            d += v;
        }
    }
    let step = d * 900.0 * ortho.scale * time.delta_secs();
    // Runs every frame, so a view moved any other way (minimap, start position) is kept on the map too.
    let half = camera.logical_viewport_size().unwrap_or(Vec2::ZERO) * ortho.scale / 2.0;
    tf.translation.x = keep_on_map(tf.translation.x + step.x, half.x, size.0.x);
    tf.translation.y = -keep_on_map(-(tf.translation.y + step.y), half.y, size.0.y);
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

/// Left click or drag on the minimap centres the map view there.
fn minimap_input(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    hud: Res<Hud>,
    sel: Res<Selection>,
    mut cam: Query<&mut Transform, With<MapCamera>>,
) {
    let (Ok(window), Ok(mut tf)) = (windows.single(), cam.single_mut()) else {
        return;
    };
    if !buttons.pressed(MouseButton::Left) || sel.drag_from.is_some() {
        return;
    }
    if let Some(t) = hud.minimap_tile(window) {
        let p = t * TILE_PX as f32;
        tf.translation.x = p.x;
        tf.translation.y = -p.y;
    }
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
    cams: Query<(&Camera, &GlobalTransform), With<MapCamera>>,
    sim: Option<ResMut<Sim>>,
    sel: Res<Selection>,
    hud: Res<Hud>,
    mut ui: ResMut<PanelUi>,
) {
    ui.consumed = false;
    ui.escape_used = false;
    ui.tooltip = None;
    let (Some(mut sim), Ok(window), Ok(cam)) = (sim, windows.single(), cams.single()) else {
        return;
    };
    let human = sim.human;
    let (infos, effect, tile) = {
        let state = panel::WorldState {
            world: &sim.world,
            local: human,
        };
        let infos: Vec<panel::UnitInfo> = sel.units.iter().filter_map(|&id| state.unit(id)).collect();
        let ids: Vec<UnitId> = infos.iter().map(|u| u.id).collect();
        if ids != ui.last_sel {
            ui.last_sel = ids;
            ui.mode = panel::Mode::None;
        }
        let slots = panel::fill(panel::choose_set(ui.mode, &infos, &state), &state, &infos);
        let label = |n: u16| panel::parse_label(&hud.art.string(n));
        let mut effect = panel::Effect::Nothing;
        if let (Some(l), Some(c)) = (hud.layout, hud.cursor(window))
            && let Some(n) = panel::slot_at((l.buttons.x, l.buttons.y), (c.x, c.y))
            && let Some(en) = slots[n]
        {
            ui.tooltip = Some(panel::tooltip(en, &label(en.string), &state));
            if buttons.just_pressed(MouseButton::Left) {
                effect = panel::activate(en, &state, &infos);
                ui.consumed = true;
            }
        }
        if keys.just_pressed(KeyCode::Escape) && ui.mode != panel::Mode::None {
            ui.escape_used = true;
        }
        if !ui.consumed {
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
        let tile =
            cursor_map(window, cam).map(|at| ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX)));
        (infos, effect, tile)
    };
    match effect {
        panel::Effect::Nothing => {}
        panel::Effect::Send(c) => {
            sim.pending.push(c);
            if let Some(u) = infos.first() {
                sim.world.acknowledge(u.id);
            }
        }
        panel::Effect::Message(n) => sim.message = Some((hud.art.string(n), MESSAGE_TICKS)),
        panel::Effect::SetMode(m) => ui.mode = m,
    }
    if ui.consumed || infos.first().is_none_or(|p| p.owner != human) {
        return;
    }
    // A map click in target or placement mode gives the target; a right click leaves the mode.
    let waiting = matches!(ui.mode, panel::Mode::Target(_) | panel::Mode::Place(_));
    if waiting && buttons.just_pressed(MouseButton::Right) {
        ui.mode = panel::Mode::None;
        ui.consumed = true;
    } else if waiting && buttons.just_pressed(MouseButton::Left) && hud.over_map(window) {
        let Some(tile) = tile else { return };
        let units: Vec<UnitId> = infos.iter().filter(|u| u.owner == human).map(|u| u.id).collect();
        let command = match ui.mode {
            panel::Mode::Target(t) => panel::target_command(t, human, units, unit_at(&sim.world, tile), tile),
            panel::Mode::Place(kind) => Some(panel::build_command(human, infos[0].id, kind, tile)),
            _ => None,
        };
        if command.is_some() {
            sim.world.acknowledge(infos[0].id);
        }
        sim.pending.extend(command);
        ui.mode = panel::Mode::None;
        ui.consumed = true;
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
/// subtitles in the message line.
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
    if sim.world.sounds.cues.is_empty() {
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
    for cue in &cues {
        let heard = sp.director.hear(cue, view);
        if let Some(n) = heard.subtitle
            && let Some(text) = sp.subtitles.as_ref().and_then(|t| t.text(n as usize - 1))
        {
            sim.message = Some((text, MESSAGE_TICKS));
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
    if buttons.just_pressed(MouseButton::Left) && slot.is_some() && !ui.consumed {
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
        info::Click::SelectOnly(id) => sel.units = vec![id],
        info::Click::Remove(id) => sel.units.retain(|&u| u != id),
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

fn draw_hud(
    sim: Option<Res<Sim>>,
    sel: Res<Selection>,
    hud: Res<Hud>,
    ui: Res<PanelUi>,
    info_panel: Res<InfoPanel>,
    mut images: ResMut<Assets<Image>>,
    cam: Query<(&Camera, &Transform, &Projection), With<MapCamera>>,
) {
    let (Some(sim), Some(layout), Ok((camera, tf, proj))) = (sim, hud.layout, cam.single()) else {
        return;
    };
    let view = view_tiles(camera, tf, proj);
    let dots: Vec<hud::Dot> = sim
        .world
        .units
        .iter()
        .filter(|u| sim.world.on_minimap(u))
        .map(|u| hud::Dot {
            tile: u.tile,
            size: u.size,
            owner: u.owner,
        })
        .collect();
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
    let me = &sim.world.players[sim.human as usize];
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
        &hud.tiles,
        hud.map,
        &hud::View {
            gold: me.gold,
            lumber: me.lumber,
            oil: me.oil,
            info: hud::InfoView {
                frame: info_panel.frame,
                slots: icons,
                pressed: info_panel.pressed,
                content: info_panel.content.as_ref(),
                local: sim.human,
            },
            status: ui
                .tooltip
                .as_deref()
                .or(info_panel.hover.as_deref())
                .or(sim.message.as_ref().map(|(m, _)| m.as_str())),
            dots: &dots,
            fog: (!sim.world.rules.reveal_map).then(|| fog::maps(&sim.world)),
            view,
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
