//! The frame the window shows, composed on the CPU from Bevy's cameras and sprites with [`compose`]: each active
//! camera in order clears the window if it clears, then draws the visible sprites on its render layers, back to front,
//! inside its viewport. This is how the screen is built while its layers are still Bevy sprites; it lets a run without
//! a window (the frame snapshot) see what the window would show, and a run with one check it pixel for pixel against
//! Bevy's own screenshot (`script`). It covers what the client uses: centred, unrotated sprites, a uniform scale, a
//! grey tint and clear colours.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;

use crate::compose::{Clip, Frame, H, Placed, W};

/// A sprite's place in the composition, before it is drawn.
struct Item {
    z: f32,
    entity: Entity,
    image: AssetId<Image>,
    origin: (f64, f64),
    scale: (f64, f64),
    tint: f32,
}

/// The screen as the window shows it: the 640 × 480 screen's place in the window (`display::Letterbox`) at one
/// physical pixel per screen pixel. `None` until the window and cameras are ready.
pub fn compose_world(world: &mut World) -> Option<Frame> {
    let (size, factor) = {
        let mut q = world.query_filtered::<&Window, With<PrimaryWindow>>();
        let w = q.single(world).ok()?;
        (w.physical_size(), w.scale_factor() as f64)
    };
    let lb = crate::display::Letterbox::fit((size.x, size.y));
    if lb.scale != 1 {
        return None;
    }
    let off = (lb.offset.0 as f64, lb.offset.1 as f64);
    let clear_default = world.get_resource::<ClearColor>().map_or(Color::BLACK, |c| c.0);
    let mut cams: Vec<(isize, Entity, Camera, GlobalTransform, RenderLayers)> = {
        let mut q = world.query::<(Entity, &Camera, &GlobalTransform, Option<&RenderLayers>, &RenderTarget)>();
        q.iter(world)
            .filter(|(_, c, _, _, t)| c.is_active && matches!(t, RenderTarget::Window(_)))
            .map(|(e, c, g, l, _)| (c.order, e, c.clone(), *g, l.cloned().unwrap_or_default()))
            .collect()
    };
    cams.sort_by_key(|c| (c.0, c.1));
    let mut sprites = world.query::<(
        Entity,
        &Sprite,
        &GlobalTransform,
        &InheritedVisibility,
        Option<&RenderLayers>,
        &Anchor,
    )>();
    let images = world.resource::<Assets<Image>>();
    let mut frame = Frame::black();
    for (_, _, cam, cam_tf, layers) in &cams {
        let rect = cam.physical_viewport_rect()?;
        let clip = Clip::new(
            rect.min.x as i32 - lb.offset.0 as i32,
            rect.min.y as i32 - lb.offset.1 as i32,
            rect.width() as i32,
            rect.height() as i32,
        );
        // A clearing camera clears its whole target, not only its viewport.
        let clear = match cam.clear_color {
            ClearColorConfig::Default => Some(clear_default),
            ClearColorConfig::Custom(c) => Some(c),
            ClearColorConfig::None => None,
        };
        if let Some(c) = clear {
            let c = c.to_srgba();
            let rgb = [c.red, c.green, c.blue].map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
            frame.fill(Clip::SCREEN, rgb);
        }
        let mut items = Vec::new();
        for (e, sprite, tf, vis, sl, anchor) in sprites.iter(world) {
            if !vis.get() || !layers.intersects(sl.unwrap_or(&RenderLayers::default())) {
                continue;
            }
            let Some(img) = images.get(&sprite.image) else { continue };
            let tex = Vec2::new(img.width() as f32, img.height() as f32);
            let size = sprite.custom_size.unwrap_or(tex);
            let a = anchor.as_vec();
            let corner =
                |sx: f32, sy: f32| tf.transform_point(Vec3::new((sx - a.x) * size.x, (sy - a.y) * size.y, 0.0));
            let (Ok(tl), Ok(br)) = (
                cam.world_to_viewport(cam_tf, corner(-0.5, 0.5)),
                cam.world_to_viewport(cam_tf, corner(0.5, -0.5)),
            ) else {
                continue;
            };
            let (x0, y0) = (tl.x as f64 * factor - off.0, tl.y as f64 * factor - off.1);
            let (x1, y1) = (br.x as f64 * factor - off.0, br.y as f64 * factor - off.1);
            let tint = sprite.color.to_srgba();
            items.push(Item {
                z: tf.translation().z,
                entity: e,
                image: sprite.image.id(),
                origin: (x0, y0),
                scale: ((x1 - x0) / tex.x as f64, (y1 - y0) / tex.y as f64),
                tint: tint.red,
            });
        }
        items.sort_by(|a, b| a.z.total_cmp(&b.z).then(a.entity.cmp(&b.entity)));
        for it in &items {
            let Some(img) = images.get(it.image) else { continue };
            let Some(data) = img.data.as_deref() else { continue };
            frame.draw(&Placed {
                rgba: data,
                width: img.width(),
                height: img.height(),
                origin: it.origin,
                scale: it.scale,
                clip,
                tint: it.tint,
            });
        }
    }
    debug_assert_eq!(frame.rgba.len(), W * H * 4);
    Some(frame)
}
