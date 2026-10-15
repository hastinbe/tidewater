//! The window shows one picture: the 640 × 480 screen, composed on the CPU by [`compose`] each frame and drawn at the
//! window's whole-number scale with black around it (`display::Letterbox`). This is the first step of the
//! engine-neutral client core (`docs/design/client-core.md`): Bevy no longer stacks the screen's layers itself.
//!
//! The layers are the core's (`layers`, step 3 of the design): each is a picture in [`ScreenPictures`] and a
//! [`ScreenLayer`] on an entity that says where it goes, and [`compose_screen`] puts them on the screen in order. The
//! entities stay only so that the flow's states can still remove a screen's layers with it (until step 6). The map
//! view's place, centre and zoom are the core's too (`mapview`, step 2); Bevy's only camera shows the finished screen.
//!
//! | Order | Plane (`layers::Plane`) | Layers | Placed |
//! |---|---|---|---|
//! | 1 | Map | Terrain, map view, placement image (by z) | In the map view (`hud::Layout::viewport`), around the view's centre, at the zoom |
//! | 2 | Interface | The interface picture (`Hud`) | Centred on the screen, one picture pixel per screen pixel |
//! | 3 | Flow | The flow's layers (`frontend`, `gamemenu`: screens, dialogs, menus), by z | Centred on the screen at their size, with their fade |
//!
//! Outside a game the flow's screen is cleared before its layers (`frontend::FrontScreen`), so only they show there.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::compose::{self, Clip, Frame, Placed};
use crate::frontend::{FrontScreen, Persist};
use crate::layers::{Layer, PictureId, Pictures, Plane};
use crate::{GameView, Hud};

/// The render layer only the screen's own camera and picture are on.
const SCREEN_LAYER: usize = 30;

/// The composed screen: its picture for the window, and the frame itself.
#[derive(Resource)]
pub struct Screen {
    pub image: Handle<Image>,
    pub frame: Frame,
}

#[derive(Component)]
struct ScreenCamera;

#[derive(Component)]
struct ScreenSprite;

/// The screen is composed after everything that changes the layers, and before the frame is drawn.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComposeScreen;

pub fn plugin(app: &mut App) {
    app.init_resource::<ScreenPictures>()
        .add_systems(Startup, spawn_screen)
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
    commands.insert_resource(Screen {
        image,
        frame: Frame::black(),
    });
}

/// The layers' pictures (`layers::Pictures`).
#[derive(Resource, Default, Deref, DerefMut)]
pub struct ScreenPictures(pub Pictures);

/// A layer of the screen (`layers::Layer`); its picture goes when the layer does.
#[derive(Component, Deref, DerefMut)]
#[component(on_remove = free_picture)]
pub struct ScreenLayer(pub Layer);

fn free_picture(mut world: DeferredWorld, ctx: HookContext) {
    let Some(id) = world.get::<ScreenLayer>(ctx.entity).map(|l| l.picture) else {
        return;
    };
    if let Some(mut pictures) = world.get_resource_mut::<ScreenPictures>() {
        pictures.remove(id);
    }
}

/// A layer to compose: its picture and its place on the screen.
struct Placing {
    picture: PictureId,
    z: f32,
    entity: Entity,
    origin: (f64, f64),
    scale: (f64, f64),
    clip: Clip,
    tint: f32,
}

#[allow(clippy::too_many_arguments)]
fn compose_screen(
    mut images: ResMut<Assets<Image>>,
    pictures: Res<ScreenPictures>,
    mut screen: ResMut<Screen>,
    windows: Query<&Window, With<PrimaryWindow>>,
    hud: Option<Res<Hud>>,
    map_view: Option<Res<GameView>>,
    front_screen: Res<FrontScreen>,
    layers: Query<(Entity, &ScreenLayer)>,
) {
    if windows.single().is_err() {
        return;
    }
    // Outside a game the flow's screen clears the screen first (`frontend::camera_clear`).
    let front_clears = front_screen.clears;
    let (center_x, center_y) = (compose::W as f64 / 2.0, compose::H as f64 / 2.0);
    let mut map = Vec::new();
    let mut interface = Vec::new();
    let mut front = Vec::new();
    // The map view: its rectangle on the screen, the map pixel at its centre, and map pixels per screen pixel.
    let view = match (hud.as_ref().and_then(|h| h.layout), map_view) {
        (Some(layout), Some(mv)) => {
            let v = layout.viewport;
            Some((
                Clip::new(v.x.max(0), v.y.max(0), v.w.max(1) as i32, v.h.max(1) as i32),
                (mv.centre.0 as f64, mv.centre.1 as f64),
                mv.zoom as f64,
            ))
        }
        _ => None,
    };
    for (entity, l) in &layers {
        if !l.visible {
            continue;
        }
        let Some(pic) = pictures.get(l.picture) else { continue };
        let (w, h) = l.drawn_size(pic);
        let (tw, th) = (pic.width as f64, pic.height as f64);
        let mut layer = Placing {
            picture: l.picture,
            z: l.z,
            entity,
            origin: (0.0, 0.0),
            scale: (1.0, 1.0),
            clip: Clip::SCREEN,
            tint: l.tint,
        };
        match l.plane {
            Plane::Map => {
                let Some((clip, centre, per_px)) = view else { continue };
                let (vx, vy) = (
                    clip.x0 as f64 + (clip.x1 - clip.x0) as f64 / 2.0,
                    clip.y0 as f64 + (clip.y1 - clip.y0) as f64 / 2.0,
                );
                // The picture's top-left corner in map pixels.
                let (mx, my) = (l.at.0 as f64 - w as f64 / 2.0, l.at.1 as f64 - h as f64 / 2.0);
                layer.origin = (vx + (mx - centre.0) / per_px, vy + (my - centre.1) / per_px);
                layer.scale = (w as f64 / tw / per_px, h as f64 / th / per_px);
                layer.clip = clip;
                map.push(layer);
            }
            Plane::Interface => {
                // One picture pixel per screen pixel, centred.
                layer.origin = (center_x - tw / 2.0, center_y - th / 2.0);
                interface.push(layer);
            }
            Plane::Flow => {
                layer.origin = (
                    center_x + l.at.0 as f64 - w as f64 / 2.0,
                    center_y + l.at.1 as f64 - h as f64 / 2.0,
                );
                layer.scale = (w as f64 / tw, h as f64 / th);
                front.push(layer);
            }
        }
    }
    let by_z = |a: &Placing, b: &Placing| a.z.total_cmp(&b.z).then(a.entity.cmp(&b.entity));
    map.sort_by(by_z);
    interface.sort_by(by_z);
    front.sort_by(by_z);
    let order: Vec<&Placing> = if front_clears {
        front.iter().collect()
    } else {
        map.iter().chain(&interface).chain(&front).collect()
    };
    let mut frame = Frame::black();
    for l in order {
        let Some(pic) = pictures.get(l.picture) else { continue };
        frame.draw(&Placed {
            rgba: &pic.rgba,
            width: pic.width,
            height: pic.height,
            origin: l.origin,
            scale: l.scale,
            clip: l.clip,
            tint: l.tint,
        });
    }
    if let Some(mut img) = images.get_mut(&screen.image) {
        img.data = Some(frame.rgba.clone());
    }
    screen.frame = frame;
}

/// The frame the window shows this frame, at one window pixel per screen pixel: what a scripted run records. `None`
/// without a window, or when the window shows the screen at a larger scale (the run's frames are then Bevy's own).
pub fn shown_frame(world: &mut World) -> Option<Frame> {
    let size = {
        let mut q = world.query_filtered::<&Window, With<PrimaryWindow>>();
        q.single(world).ok()?.physical_size()
    };
    if crate::display::Letterbox::fit((size.x, size.y)).scale != 1 {
        return None;
    }
    Some(world.resource::<Screen>().frame.clone())
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
