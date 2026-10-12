//! Fog of war as the client shows it (vision spec, "The fog maps" and "The minimap"). The sim owns the maps; this turns
//! them into a quarter-tile overlay picture and decides which units to draw. No Bevy types here.
//!
//! The shading is our own design (the spec leaves it to rendering): unexplored quarters are black, explored quarters
//! that are not visible are darkened.

use tidewater_sim::vision::{Map, bit, quarter};
use tidewater_sim::{Unit, World};

/// Opacity of the darkening over explored but not visible ground.
pub const DIM: u8 = 120;

/// The overlay picture: two texels per tile on each axis, one per quarter tile, black with a per-texel opacity.
/// `explored` and `visible` hold each tile's fogged quarters, row by row (`World::vision.flags`). With `reveal` (the
/// reveal-map cheat) nothing is covered.
pub fn overlay(explored: &[u8], visible: &[u8], width: usize, height: usize, reveal: bool) -> Vec<u8> {
    let w2 = width * 2;
    let mut out = vec![0u8; w2 * height * 2 * 4];
    if reveal {
        return out;
    }
    for ty in 0..height {
        for tx in 0..width {
            let (e, v) = (explored[ty * width + tx], visible[ty * width + tx]);
            for (flag, qx, qy) in [
                (quarter::TL, 0, 0),
                (quarter::TR, 1, 0),
                (quarter::BL, 0, 1),
                (quarter::BR, 1, 1),
            ] {
                let alpha = if e & flag != 0 {
                    255
                } else if v & flag != 0 {
                    DIM
                } else {
                    0
                };
                out[((ty * 2 + qy) * w2 + tx * 2 + qx) * 4 + 3] = alpha;
            }
        }
    }
    out
}

/// Whether the local player's view draws a unit. Its own units always show. Another unit shows when it is detected and
/// not hidden from the local player; a building also shows once any tile of it has been explored, since buildings stay
/// where they were seen (the original's last-seen drawing is an open question in the spec).
pub fn unit_shown(world: &World, u: &Unit) -> bool {
    let me = world.vision.local;
    if u.owner == me || world.rules.reveal_map {
        return true;
    }
    if u.detected_by & bit(me) == 0 {
        return false;
    }
    if !u.building {
        return u.hidden_from & bit(me) == 0;
    }
    (0..u.size.1 as i32).any(|dy| (0..u.size.0 as i32).any(|dx| !world.vision.unexplored(u.tile.0 + dx, u.tile.1 + dy)))
}

/// What the minimap samples for terrain: the explored map on even columns and the visible map on odd ones, each
/// showing the colour only where the tile is fully clear.
pub fn minimap_terrain_shown(explored: &[u8], visible: &[u8], column: i32, tile: usize) -> bool {
    let map = if column % 2 == 0 { explored } else { visible };
    map.get(tile).is_none_or(|&f| f == 0)
}

/// The explored and visible maps' flags for the minimap.
pub fn maps(world: &World) -> (&[u8], &[u8]) {
    (world.vision.flags(Map::Explored), world.vision.flags(Map::Visible))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_has_one_texel_per_quarter() {
        // Two tiles side by side: the first unexplored, the second explored with only its top-left quarter visible.
        let explored = [15, 0];
        let visible = [15, 14];
        let px = overlay(&explored, &visible, 2, 1, false);
        let a = |x: usize, y: usize| px[(y * 4 + x) * 4 + 3];
        assert_eq!([a(0, 0), a(1, 0), a(0, 1), a(1, 1)], [255; 4]);
        assert_eq!([a(2, 0), a(3, 0), a(2, 1), a(3, 1)], [0, DIM, DIM, DIM]);
        assert_eq!(px.len(), 4 * 2 * 4);
        assert!(overlay(&explored, &visible, 2, 1, true).iter().all(|&b| b == 0));
    }

    #[test]
    fn minimap_columns_alternate_between_the_maps() {
        let (explored, visible) = ([0u8], [15u8]);
        assert!(minimap_terrain_shown(&explored, &visible, 0, 0));
        assert!(!minimap_terrain_shown(&explored, &visible, 1, 0));
        assert!(
            !minimap_terrain_shown(&[8], &[0], 2, 0),
            "a partly fogged tile is black"
        );
    }
}
