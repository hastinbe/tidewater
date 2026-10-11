//! Tidewater client: currently a map viewer. Loads a PUD from the user's installation and shows its terrain and units.
//!
//! Controls: arrow keys / WASD pan, mouse wheel or +/- zoom, Escape quits.

mod scene;

use std::path::PathBuf;

use anyhow::Result;
use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use clap::Parser;
use tidewater_formats::install::Install;

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
}

#[derive(Resource)]
struct Loaded(Option<scene::Scene>);

#[derive(Resource)]
struct MapSize(Vec2);

#[derive(Resource)]
struct StartZoom(f32);

fn main() -> Result<()> {
    let args = Args::parse();
    let mut install = Install::open(&args.install)?;
    let scene = scene::load(&mut install, &args.map)?;
    println!(
        "{:?} map, {} units: {}",
        scene.era,
        scene.units.len(),
        scene.description
    );
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
        .add_systems(Startup, setup)
        .add_systems(Update, (pan_and_zoom, quit_on_escape))
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
    let handles: std::collections::HashMap<_, _> = scene
        .pictures
        .into_iter()
        .map(|(k, p)| (k, images.add(image(p))))
        .collect();
    for (i, u) in scene.units.iter().enumerate() {
        // Later units in the file draw over earlier ones on the same layer.
        let z = u.layer + i as f32 * 1e-4;
        commands.spawn((
            Sprite::from_image(handles[&u.picture].clone()),
            Transform::from_translation(world(u.center.0, u.center.1, z)),
        ));
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
