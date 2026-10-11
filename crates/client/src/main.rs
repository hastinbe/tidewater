//! Tidewater client. Loads a PUD from the user's installation, runs the simulation at its fixed tick rate and draws it.
//!
//! Controls: left click or drag selects your units, right click orders them to move, S stops them. Arrow keys / WASD
//! pan, mouse wheel or +/- zoom, Escape quits.

mod scene;

use std::path::PathBuf;

use anyhow::Result;
use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use clap::Parser;
use std::collections::HashMap;
use tidewater_formats::install::Install;
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
    /// Development aid: order all of your units to tile `X,Y` at the start.
    #[arg(long, value_parser = parse_tile)]
    demo_move: Option<(i32, i32)>,
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
}

#[derive(Resource, Default)]
struct Selection {
    units: Vec<UnitId>,
    /// Where a left-button drag started, in world space.
    drag_from: Option<Vec2>,
}

/// Facing frames per (unit type, owner).
#[derive(Resource)]
struct Art(HashMap<(u8, u8), Vec<Handle<Image>>>);

/// Links a sprite entity to its unit.
#[derive(Component)]
struct UnitSprite(UnitId);

#[derive(Resource)]
struct StartZoom(f32);

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
    let size = Vec2::new(scene.terrain.width as f32, scene.terrain.height as f32);
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()).set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Tidewater — {}", args.map),
                resolution: (1280, 800).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Loaded(Some(scene)))
        .insert_resource(MapSize(size))
        .insert_resource(StartZoom(args.zoom))
        .insert_resource(Time::<Fixed>::from_hz(hz))
        .init_resource::<Selection>()
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, tick)
        .add_systems(
            Update,
            (
                pan_and_zoom,
                select,
                order,
                sync_sprites,
                draw_selection,
                quit_on_escape,
            ),
        )
        .run();
    Ok(())
}

fn image(p: scene::Picture) -> Image {
    Image::new(
        Extent3d {
            width: p.width,
            height: p.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        p.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
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
) {
    let scene = loaded.0.take().expect("scene loaded once");
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scale: zoom.0,
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_translation(world(size.0.x / 2.0, size.0.y / 2.0, 0.0)),
    ));
    commands.spawn((
        Sprite::from_image(images.add(image(scene.terrain))),
        Transform::from_translation(world(size.0.x / 2.0, size.0.y / 2.0, 0.0)),
    ));
    let art: HashMap<_, Vec<_>> = scene
        .art
        .into_iter()
        .map(|(k, a)| (k, a.facings.into_iter().map(|p| images.add(image(p))).collect()))
        .collect();
    for u in &scene.world.units {
        if let Some(frames) = art.get(&(u.kind, u.owner)) {
            commands.spawn((
                Sprite::from_image(frames[0].clone()),
                Transform::default(),
                UnitSprite(u.id),
            ));
        }
    }
    commands.insert_resource(Art(art));
    commands.insert_resource(Sim {
        world: scene.world,
        pending: Vec::new(),
        human: scene.human,
    });
}

fn tick(mut sim: ResMut<Sim>) {
    let cmds = std::mem::take(&mut sim.pending);
    sim.world.step(&cmds);
}

/// Draw depth: buildings under ground units under ships under aircraft; lower on screen draws in front.
fn depth(u: &tidewater_sim::Unit) -> f32 {
    let layer = match (u.building, u.layer) {
        (true, _) => 1.0,
        (_, tidewater_sim::Layer::Land) => 2.0,
        (_, tidewater_sim::Layer::Sea) => 3.0,
        (_, tidewater_sim::Layer::Air) => 4.0,
    };
    layer + u.pos.1 as f32 * 1e-5
}

fn sync_sprites(
    sim: Option<Res<Sim>>,
    art: Option<Res<Art>>,
    mut q: Query<(&UnitSprite, &mut Sprite, &mut Transform)>,
) {
    let (Some(sim), Some(art)) = (sim, art) else { return };
    for (us, mut sprite, mut tf) in &mut q {
        let Some(u) = sim.world.unit(us.0) else { continue };
        tf.translation = world(u.pos.0 as f32, u.pos.1 as f32, depth(u));
        let Some(frames) = art.0.get(&(u.kind, u.owner)) else {
            continue;
        };
        let (f, flip) = if frames.len() > 1 {
            scene::facing_frame(u.facing)
        } else {
            (0, false)
        };
        if sprite.image != frames[f] {
            sprite.image = frames[f].clone();
        }
        sprite.flip_x = flip;
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
    cams: Query<(&Camera, &GlobalTransform)>,
    sim: Option<Res<Sim>>,
    mut sel: ResMut<Selection>,
) {
    let (Some(sim), Ok(window), Ok(cam)) = (sim, windows.single(), cams.single()) else {
        return;
    };
    let Some(at) = cursor_map(window, cam) else { return };
    if buttons.just_pressed(MouseButton::Left) {
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
        sim.world
            .units
            .iter()
            .filter(|u| {
                let (x, y) = ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX));
                x >= u.tile.0 && y >= u.tile.1 && x < u.tile.0 + u.size.0 as i32 && y < u.tile.1 + u.size.1 as i32
            })
            .take(1)
            .map(|u| u.id)
            .collect()
    } else {
        sim.world
            .units
            .iter()
            .filter(mine)
            .filter(|u| {
                let p = Vec2::new(u.pos.0 as f32, u.pos.1 as f32);
                p.cmpge(lo).all() && p.cmple(hi).all()
            })
            .take(9)
            .map(|u| u.id)
            .collect()
    };
    sel.units = picked;
}

fn order(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform)>,
    sim: Option<ResMut<Sim>>,
    sel: Res<Selection>,
) {
    let (Some(mut sim), Ok(window), Ok(cam)) = (sim, windows.single(), cams.single()) else {
        return;
    };
    let human = sim.human;
    let units: Vec<UnitId> = sel
        .units
        .iter()
        .copied()
        .filter(|&id| sim.world.unit(id).is_some_and(|u| u.owner == human))
        .collect();
    if units.is_empty() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyS) && !keys.pressed(KeyCode::ShiftLeft) {
        sim.pending.push(Command::Stop { player: human, units });
        return;
    }
    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let Some(at) = cursor_map(window, cam) else { return };
    let to = ((at.x as i32).div_euclid(TILE_PX), (at.y as i32).div_euclid(TILE_PX));
    sim.pending.push(Command::Move {
        player: human,
        units,
        to,
    });
}

fn draw_selection(
    mut gizmos: Gizmos,
    sim: Option<Res<Sim>>,
    sel: Res<Selection>,
    windows: Query<&Window>,
    cams: Query<(&Camera, &GlobalTransform)>,
) {
    let Some(sim) = sim else { return };
    let green = Color::srgb(0.2, 0.9, 0.2);
    for &id in &sel.units {
        let Some(u) = sim.world.unit(id) else { continue };
        let size = Vec2::new(u.size.0 as f32, u.size.1 as f32) * TILE_PX as f32;
        gizmos.rect_2d(Vec2::new(u.pos.0 as f32, -(u.pos.1 as f32)), size, green);
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
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut tf, mut proj)) = cam.single_mut() else {
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
    for (k1, k2, v) in [
        (KeyCode::ArrowLeft, KeyCode::KeyA, Vec2::NEG_X),
        (KeyCode::ArrowRight, KeyCode::KeyD, Vec2::X),
        (KeyCode::ArrowUp, KeyCode::KeyW, Vec2::Y),
        (KeyCode::ArrowDown, KeyCode::KeyS, Vec2::NEG_Y),
    ] {
        if keys.pressed(k1) || keys.pressed(k2) {
            d += v;
        }
    }
    let step = d * 900.0 * ortho.scale * time.delta_secs();
    tf.translation.x = (tf.translation.x + step.x).clamp(0.0, size.0.x);
    tf.translation.y = (tf.translation.y + step.y).clamp(-size.0.y, 0.0);
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
