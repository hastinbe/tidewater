//! The window shows one picture: the 640 × 480 screen, composed on the CPU by [`compose`] each frame and drawn at the
//! window's whole-number scale with black around it (`display::Letterbox`). This is the first step of the
//! engine-neutral client core (`docs/design/client-core.md`): Bevy no longer stacks the screen's layers itself.
//!
//! The layers are still kept in Bevy's world as sprites, and the cameras that used to draw them still give the
//! pointer's place, but none of them draws: [`compose_screen`] reads the layers and places them on the screen the way
//! the cameras did, in the same order:
//!
//! | Order | Layer | Placed |
//! |---|---|---|
//! | 1 | Map sprites: terrain, map view, placement image (by z) | In the map view (`hud::Layout::viewport`), around the map camera's centre, at the zoom |
//! | 2 | The interface picture (`Hud`) | Centred on the screen, one texel per screen pixel |
//! | 3 | The flow's layers (`frontend`, `gamemenu`: screens, dialogs, menus), by z | Centred on the screen at their size, with their fade |
//!
//! Outside a game the flow's camera cleared the screen before its layers, so only they show there.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::compose::{self, Clip, Frame, Placed};
use crate::frontend::{FRONT_LAYER, FrontCamera, Persist};
use crate::{HUD_LAYER, Hud, MapCamera};

/// The render layer only the screen's own camera and picture are on.
const SCREEN_LAYER: usize = 30;

/// The composed screen's picture.
#[derive(Resource)]
pub struct Screen {
    pub image: Handle<Image>,
}

#[derive(Component)]
struct ScreenCamera;

#[derive(Component)]
struct ScreenSprite;

/// The screen is composed after everything that changes the layers, and before the frame is drawn.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComposeScreen;

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_screen)
        .add_systems(Last, (compose_screen, show_screen).chain().in_set(ComposeScreen));
}

fn spawn_screen(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let frame = Frame::black();
    let image = images.add(Image::new(
        bevy::render::render_resource::Extent3d {
            width: compose::W as u32,
            height: compose::H as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        frame.rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderLayers::layer(SCREEN_LAYER),
        ScreenCamera,
        Persist,
    ));
    commands.spawn((
        Sprite::from_image(image.clone()),
        RenderLayers::layer(SCREEN_LAYER),
        ScreenSprite,
        Persist,
    ));
    commands.insert_resource(Screen { image });
}

/// A layer to compose: its picture and its place on the screen.
struct Layer {
    image: AssetId<Image>,
    z: f32,
    entity: Entity,
    origin: (f64, f64),
    scale: (f64, f64),
    clip: Clip,
    tint: f32,
}

/// The size a sprite is drawn at and its picture's size.
fn sizes(sprite: &Sprite, img: &Image, tf: &GlobalTransform) -> (Vec2, Vec2) {
    let tex = Vec2::new(img.width() as f32, img.height() as f32);
    let (scale, _, _) = tf.to_scale_rotation_translation();
    (sprite.custom_size.unwrap_or(tex) * scale.truncate(), tex)
}

#[allow(clippy::type_complexity)]
fn compose_screen(
    mut images: ResMut<Assets<Image>>,
    screen: Res<Screen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    hud: Option<Res<Hud>>,
    map_cam: Query<(&GlobalTransform, &Projection), With<MapCamera>>,
    front_cam: Query<&Camera, With<FrontCamera>>,
    sprites: Query<
        (
            Entity,
            &Sprite,
            &GlobalTransform,
            &InheritedVisibility,
            Option<&RenderLayers>,
        ),
        Without<ScreenSprite>,
    >,
) {
    let Ok(window) = windows.single() else { return };
    let lb = crate::letterbox(window);
    let factor = window.scale_factor() as f64;
    // Outside a game the flow's camera clears the screen first (`frontend::camera_clear`).
    let front_clears = front_cam
        .iter()
        .any(|c| !matches!(c.clear_color, ClearColorConfig::None));
    let (center_x, center_y) = (compose::W as f64 / 2.0, compose::H as f64 / 2.0);
    let mut map = Vec::new();
    let mut interface = Vec::new();
    let mut front = Vec::new();
    // The map view: its rectangle on the screen, the map pixel at its centre, and map pixels per screen pixel.
    let view = match (hud.as_ref().and_then(|h| h.layout), map_cam.single()) {
        (Some(layout), Ok((tf, Projection::Orthographic(o)))) => {
            let v = layout.viewport;
            let t = tf.translation();
            let per_px = o.scale as f64 * lb.scale as f64 / factor;
            Some((
                Clip::new(v.x.max(0), v.y.max(0), v.w.max(1) as i32, v.h.max(1) as i32),
                (t.x as f64, -t.y as f64),
                per_px,
            ))
        }
        _ => None,
    };
    for (entity, sprite, tf, vis, layers) in &sprites {
        if !vis.get() {
            continue;
        }
        let Some(img) = images.get(&sprite.image) else { continue };
        let (size, tex) = sizes(sprite, img, tf);
        let t = tf.translation();
        let tint = sprite.color.to_srgba().red;
        let layers = layers.cloned().unwrap_or_default();
        let mut layer = Layer {
            image: sprite.image.id(),
            z: t.z,
            entity,
            origin: (0.0, 0.0),
            scale: (1.0, 1.0),
            clip: Clip::SCREEN,
            tint,
        };
        if layers.intersects(&RenderLayers::layer(0)) {
            let Some((clip, centre, per_px)) = view else { continue };
            let (vx, vy) = (
                clip.x0 as f64 + (clip.x1 - clip.x0) as f64 / 2.0,
                clip.y0 as f64 + (clip.y1 - clip.y0) as f64 / 2.0,
            );
            // The sprite's top-left corner in map pixels (y down).
            let (mx, my) = (t.x as f64 - size.x as f64 / 2.0, -t.y as f64 - size.y as f64 / 2.0);
            layer.origin = (vx + (mx - centre.0) / per_px, vy + (my - centre.1) / per_px);
            layer.scale = (
                size.x as f64 / tex.x as f64 / per_px,
                size.y as f64 / tex.y as f64 / per_px,
            );
            layer.clip = clip;
            map.push(layer);
        } else if layers.intersects(&RenderLayers::layer(HUD_LAYER)) {
            // Drawn at the window's scale by its camera: one texel per screen pixel, centred.
            layer.origin = (center_x - tex.x as f64 / 2.0, center_y - tex.y as f64 / 2.0);
            interface.push(layer);
        } else if layers.intersects(&RenderLayers::layer(FRONT_LAYER)) {
            layer.origin = (
                center_x + t.x as f64 - size.x as f64 / 2.0,
                center_y - t.y as f64 - size.y as f64 / 2.0,
            );
            layer.scale = (size.x as f64 / tex.x as f64, size.y as f64 / tex.y as f64);
            front.push(layer);
        }
    }
    let by_z = |a: &Layer, b: &Layer| a.z.total_cmp(&b.z).then(a.entity.cmp(&b.entity));
    map.sort_by(by_z);
    interface.sort_by(by_z);
    front.sort_by(by_z);
    let order: Vec<&Layer> = if front_clears {
        front.iter().collect()
    } else {
        map.iter().chain(&interface).chain(&front).collect()
    };
    let mut frame = Frame::black();
    for l in order {
        let Some(img) = images.get(l.image) else { continue };
        let Some(data) = img.data.as_deref() else { continue };
        frame.draw(&Placed {
            rgba: data,
            width: img.width(),
            height: img.height(),
            origin: l.origin,
            scale: l.scale,
            clip: l.clip,
            tint: l.tint,
        });
    }
    if let Some(mut img) = images.get_mut(&screen.image) {
        img.data = Some(frame.rgba);
    }
}

/// Puts the screen's picture on the screen's place in the window, at the whole-number scale.
fn show_screen(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cam: Query<&mut Camera, With<ScreenCamera>>,
    mut sprite: Query<&mut Transform, With<ScreenSprite>>,
) {
    let (Ok(window), Ok(mut cam), Ok(mut tf)) = (windows.single(), cam.single_mut(), sprite.single_mut()) else {
        return;
    };
    let lb = crate::letterbox(window);
    let (x, y, w, h) = lb.screen();
    let (pos, size) = (UVec2::new(x, y), UVec2::new(w.max(1), h.max(1)));
    if cam
        .viewport
        .as_ref()
        .is_none_or(|v| v.physical_position != pos || v.physical_size != size)
    {
        cam.viewport = Some(Viewport {
            physical_position: pos,
            physical_size: size,
            ..default()
        });
    }
    let k = lb.scale as f32 / window.scale_factor();
    let scale = Vec3::new(k, k, 1.0);
    if tf.scale != scale {
        tf.scale = scale;
    }
}
