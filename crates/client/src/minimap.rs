//! The minimap as `docs/specs/ui/minimap.md` describes it: a 128 × 128 picture of palette indices, redrawn on a
//! game-cycle countdown, with a terrain picture rebuilt on every sixth redraw. The fog maps it samples are in
//! `docs/specs/vision/fog.md` ("The minimap"); the attack mark is in `docs/specs/ui/messages.md` ("Attacks on the
//! minimap"); its place on the screen is in `docs/specs/ui/screen-and-cursors.md`. The sim keeps the attack marks and
//! the remembered buildings ([`tidewater_sim::minimap`]); this module draws. No Bevy types here.
//!
//! The terrain sample uses the tileset layout described in `tidewater_formats::tileset` (megatiles of 4 × 4 minitile
//! references, minitiles of 8 × 8 palette indices).
//!
//! Maps are always square (`game/setup.md`); a pixel whose tile would lie outside the map is still drawn as colour 0,
//! as a guard. Dot order and the colour player come from the sim ([`World::minimap_units`]; in single player slot p
//! has the p-th colour). Default for the spec's open question: the terrain preference is on unless the caller turns
//! it off. The view rectangle's outer size is 14 tiles on the original screen; on a larger canvas the caller passes
//! the view's real size in tiles (our extension).

use tidewater_formats::tileset::Tileset;
use tidewater_sim::World;
use tidewater_sim::vision::Map;

/// The minimap's size in pixels, each way.
pub const SIZE: usize = 128;
/// Screen position of the minimap's top-left pixel ("Data").
pub const ORIGIN: (i32, i32) = (24, 26);
/// Cycles between redraws, and the most a scroll leaves before the next one.
pub const REDRAW_CYCLES: u32 = 40;
pub const AFTER_SCROLL: u32 = 4;
/// The terrain picture is rebuilt on every this many redraws.
pub const REBUILD_EVERY: u32 = 6;
/// The view size in tiles on the original screen.
pub const VIEW_TILES: i32 = 14;
/// The minitile pixel the terrain takes: column 7, row 6.
const SAMPLE: (usize, usize) = (7, 6);

/// Palette colours ("Data").
pub mod colour {
    /// Fogged ground, terrain turned off, and oil patches.
    pub const BLACK: u8 = 0;
    pub const OIL_PATCH: u8 = 0;
    pub const CRITTER: u8 = 247;
    pub const ATTACKED: u8 = 3;
    pub const OWN: u8 = 250;
    pub const NEUTRAL: u8 = 251;
    pub const VIEW: u8 = 246;
    /// Red, blue, green, violet, orange, black, white, yellow.
    pub const PLAYERS: [u8; 8] = [208, 1, 216, 220, 224, 228, 255, 2];
}

/// Unit types with their own dot colour.
pub const OIL_PATCH: u8 = 0x5d;
pub const CRITTER: u8 = 0x39;

// --- scale ---------------------------------------------------------------------------------------------------------

/// Tile to minimap pixel on a map `width` tiles wide ("Scale"). Also converts sizes.
pub fn to_px(width: u32, t: i32) -> i32 {
    match width {
        32 => 4 * t,
        64 => 2 * t,
        96 => (4 * t).div_euclid(3),
        _ => t,
    }
}

/// Minimap pixel to tile on a map `width` tiles wide ("Scale").
pub fn to_tile(width: u32, p: i32) -> i32 {
    match width {
        32 => p.div_euclid(4),
        64 => p.div_euclid(2),
        96 => (3 * p).div_euclid(4),
        _ => p,
    }
}

/// The tile and minitile column (0 to 3) of each of the 128 pixels along a row ("Terrain", the walk). The same walk
/// gives the tile row and minitile row of each pixel row.
pub fn walk(width: u32) -> [(u32, u8); SIZE] {
    let mut out = [(0, 0); SIZE];
    let (mut counter, mut tile, mut col) = (128i32, 0u32, 0u8);
    for o in &mut out {
        *o = (tile, col.min(3));
        counter -= width.max(1) as i32;
        if counter >= 1 {
            col += 1;
        } else {
            while counter < 1 {
                counter += 128;
            }
            tile += 1;
            col = 0;
        }
    }
    out
}

// --- terrain -------------------------------------------------------------------------------------------------------

/// The terrain picture: per minimap pixel, its tile (row-major index, `None` outside the map) and its colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terrain {
    pub width: u32,
    pub tiles: Vec<Option<usize>>,
    pub colours: Vec<u8>,
}

/// The palette index at a minitile's (7, 6), unmirrored, of minitile column `col` and row `row` of megatile `m`.
pub fn sample(ts: &Tileset, m: usize, col: u8, row: u8) -> u8 {
    let Some(refs) = ts.megatiles.get(m) else {
        return colour::BLACK;
    };
    let mini = (refs[row as usize * 4 + col as usize] >> 2) as usize;
    ts.minitiles
        .get(mini * 64 + SAMPLE.1 * 8 + SAMPLE.0)
        .copied()
        .unwrap_or(colour::BLACK)
}

/// Builds the terrain picture from the map's megatiles (`width` × `height`, row by row).
pub fn terrain(ts: &Tileset, megatiles: &[u16], width: u32, height: u32) -> Terrain {
    let w = walk(width);
    let mut tiles = Vec::with_capacity(SIZE * SIZE);
    let mut colours = Vec::with_capacity(SIZE * SIZE);
    for &(ty, row) in &w {
        for &(tx, col) in &w {
            let n = (tx < width && ty < height).then(|| (ty * width + tx) as usize);
            let c = n
                .and_then(|n| megatiles.get(n))
                .map_or(colour::BLACK, |&m| sample(ts, m as usize, col, row));
            tiles.push(n);
            colours.push(c);
        }
    }
    Terrain { width, tiles, colours }
}

/// What fills the minimap before the dots ("Fog and the terrain option"). `explored` and `visible` are the fog maps'
/// fogged quarters per tile.
pub fn ground(t: &Terrain, explored: &[u8], visible: &[u8], terrain_on: bool, reveal: bool) -> Vec<u8> {
    let mut out = vec![colour::BLACK; SIZE * SIZE];
    if !terrain_on {
        return out;
    }
    for (i, o) in out.iter_mut().enumerate() {
        let Some(n) = t.tiles[i] else { continue };
        let map = if (i % SIZE).is_multiple_of(2) {
            explored
        } else {
            visible
        };
        if reveal || map.get(n) == Some(&0) {
            *o = t.colours[i];
        }
    }
    out
}

// --- dots ----------------------------------------------------------------------------------------------------------

/// A unit, building or remembered building as the minimap draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dot {
    pub kind: u8,
    /// The colour player (owner at creation).
    pub colour: u8,
    pub tile: (i32, i32),
    pub size: (u8, u8),
    pub marked: bool,
}

/// The minimap colour of a player colour: the table for players 0 to 7, the neutral colour otherwise.
pub fn player_colour(p: u8) -> u8 {
    colour::PLAYERS.get(p as usize).copied().unwrap_or(colour::NEUTRAL)
}

/// A dot's colour, first rule that applies ("Dot colour"), and whether the attack mark is used up.
pub fn dot_colour(d: &Dot, local: u8, demo: bool) -> (u8, bool) {
    if d.kind == OIL_PATCH {
        (colour::OIL_PATCH, false)
    } else if d.kind == CRITTER {
        (colour::CRITTER, false)
    } else if d.marked {
        (colour::ATTACKED, true)
    } else if d.colour == local && !demo {
        (colour::OWN, false)
    } else {
        (player_colour(d.colour), false)
    }
}

/// A rectangle in minimap pixels: x, y, width, height.
pub type Px = (i32, i32, i32, i32);

/// A dot's rectangle ("Dot position and size"): at least 2 × 2, and one pixel narrower (or shorter) when it would
/// pass the right (or bottom) edge.
pub fn dot_rect(width: u32, tile: (i32, i32), size: (u8, u8)) -> Px {
    let (x, y) = (to_px(width, tile.0), to_px(width, tile.1));
    let mut w = to_px(width, size.0 as i32).max(2);
    let mut h = to_px(width, size.1 as i32).max(2);
    if x + w > SIZE as i32 {
        w -= 1;
    }
    if y + h > SIZE as i32 {
        h -= 1;
    }
    (x, y, w, h)
}

/// The view's size in minimap pixels for a view `n` tiles across: 14 tiles are 14, 28 and 56 pixels on 128-, 64- and
/// 32-wide maps, and 19 on 96 (floor(59 / 3), so rounded up there, unlike a dot's size).
pub fn view_px(width: u32, n: i32) -> i32 {
    match width {
        96 => (4 * n + 3).div_euclid(3),
        _ => to_px(width, n),
    }
}

/// The view rectangle ("The view rectangle"): the view position converted, with an outer size of `tiles` tiles.
pub fn view_rect(width: u32, view: (i32, i32), tiles: (i32, i32)) -> Px {
    (
        to_px(width, view.0),
        to_px(width, view.1),
        view_px(width, tiles.0),
        view_px(width, tiles.1),
    )
}

fn put(pic: &mut [u8], x: i32, y: i32, c: u8) {
    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
        pic[y as usize * SIZE + x as usize] = c;
    }
}

/// Fills a rectangle, clipped to the minimap.
pub fn fill(pic: &mut [u8], r: Px, c: u8) {
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            put(pic, x, y, c);
        }
    }
}

/// A one-pixel outline, clipped to the minimap.
pub fn outline(pic: &mut [u8], r: Px, c: u8) {
    if r.2 <= 0 || r.3 <= 0 {
        return;
    }
    let (x1, y1) = (r.0 + r.2 - 1, r.1 + r.3 - 1);
    for x in r.0..=x1 {
        put(pic, x, r.1, c);
        put(pic, x, y1, c);
    }
    for y in r.1..=y1 {
        put(pic, r.0, y, c);
        put(pic, x1, y, c);
    }
}

// --- the minimap pixel under the pointer ---------------------------------------------------------------------------

/// The new view position for a "move the view" press or drag at screen point `pointer` ("Moving the view with the
/// minimap"): centred on the pointer, then clamped to `0..=max` (the view range). `view_tiles` is the view's size in
/// tiles (14 on the original screen).
pub fn view_for_press(width: u32, pointer: (i32, i32), view_tiles: (i32, i32), max: (i32, i32)) -> (i32, i32) {
    let half = |n: i32| view_px(width, n) / 2;
    let px = (pointer.0 - ORIGIN.0 - half(view_tiles.0)).clamp(0, SIZE as i32 - 1);
    let py = (pointer.1 - ORIGIN.1 - half(view_tiles.1)).clamp(0, SIZE as i32 - 1);
    (
        to_tile(width, px).clamp(0, max.0.max(0)),
        to_tile(width, py).clamp(0, max.1.max(0)),
    )
}

/// The tile a minimap press that gives a tile (a target or a right-click command) uses: the pixel under the pointer,
/// with no half-view offset.
pub fn tile_for_press(width: u32, pointer: (i32, i32)) -> (i32, i32) {
    let px = (pointer.0 - ORIGIN.0).clamp(0, SIZE as i32 - 1);
    let py = (pointer.1 - ORIGIN.1).clamp(0, SIZE as i32 - 1);
    (to_tile(width, px), to_tile(width, py))
}

// --- redraws -------------------------------------------------------------------------------------------------------

/// The redraw countdown ("When it is redrawn").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Countdown {
    pub left: u32,
    /// The view position at the last redraw.
    pub drawn_at: Option<(i32, i32)>,
}

impl Default for Countdown {
    /// When the game screen opens: the first cycle redraws.
    fn default() -> Self {
        Self {
            left: 1,
            drawn_at: None,
        }
    }
}

impl Countdown {
    /// One game cycle, after the units are updated, with the view at `view`. True when the minimap is redrawn.
    pub fn cycle(&mut self, view: (i32, i32)) -> bool {
        self.left = self.left.saturating_sub(1);
        if self.left == 0 {
            self.left = REDRAW_CYCLES;
            self.drawn_at = Some(view);
            return true;
        }
        if self.drawn_at.is_some_and(|v| v != view) && self.left > AFTER_SCROLL {
            self.left = AFTER_SCROLL;
        }
        false
    }
}

/// Everything one redraw reads, already in drawing order.
pub struct Scene<'a> {
    pub megatiles: &'a [u16],
    pub height: u32,
    pub explored: &'a [u8],
    pub visible: &'a [u8],
    pub reveal: bool,
    pub terrain_on: bool,
    pub local: u8,
    pub demo: bool,
    /// Remembered buildings, then units and buildings (player 15 first, player 0 last).
    pub dots: &'a [Dot],
    pub view: (i32, i32),
    pub view_tiles: (i32, i32),
}

/// The minimap's state: the countdown, the terrain picture and the last picture drawn.
#[derive(Clone, Debug)]
pub struct Minimap {
    pub width: u32,
    pub countdown: Countdown,
    /// Redraws so far.
    pub redraws: u32,
    pub terrain: Option<Terrain>,
    /// The reveal-map cheat at the last terrain rebuild.
    pub reveal_at_rebuild: bool,
    /// The last picture drawn: 128 × 128 palette indices.
    pub pixels: Vec<u8>,
}

impl Minimap {
    pub fn new(width: u32) -> Self {
        Self {
            width,
            countdown: Countdown::default(),
            redraws: 0,
            terrain: None,
            reveal_at_rebuild: false,
            pixels: vec![colour::BLACK; SIZE * SIZE],
        }
    }

    /// Whether this redraw rebuilds the terrain: every sixth redraw (the first included), or when the reveal-map
    /// cheat changed since the last rebuild.
    pub fn rebuilds(&self, reveal: bool) -> bool {
        self.terrain.is_none() || self.redraws.is_multiple_of(REBUILD_EVERY) || reveal != self.reveal_at_rebuild
    }

    /// Redraws the picture. Returns, per dot, whether its attack mark was used up (to be cleared).
    pub fn redraw(&mut self, ts: &Tileset, s: &Scene) -> Vec<bool> {
        if self.rebuilds(s.reveal) {
            self.terrain = Some(terrain(ts, s.megatiles, self.width, s.height));
            self.reveal_at_rebuild = s.reveal;
        }
        self.redraws += 1;
        let t = self.terrain.as_ref().expect("built above");
        let mut pic = ground(t, s.explored, s.visible, s.terrain_on, s.reveal);
        let mut used = Vec::with_capacity(s.dots.len());
        for d in s.dots {
            let (c, u) = dot_colour(d, s.local, s.demo);
            fill(&mut pic, dot_rect(self.width, d.tile, d.size), c);
            used.push(u);
        }
        outline(&mut pic, view_rect(self.width, s.view, s.view_tiles), colour::VIEW);
        self.pixels = pic;
        used
    }

    /// One game cycle of the client: counts down and, when due, redraws from the world and clears the attack marks
    /// of the units drawn in colour 3. True when it redrew.
    pub fn cycle(
        &mut self,
        world: &mut World,
        ts: &Tileset,
        view: (i32, i32),
        view_tiles: (i32, i32),
        terrain_on: bool,
    ) -> bool {
        if !self.countdown.cycle(view) {
            return false;
        }
        let (dots, ids) = dots(world);
        let local = world.vision.local;
        let scene = Scene {
            megatiles: &world.terrain.tiles,
            height: world.terrain.height as u32,
            explored: world.vision.flags(Map::Explored),
            visible: world.vision.flags(Map::Visible),
            reveal: world.rules.reveal_map,
            terrain_on,
            local,
            demo: world.sounds.demo,
            dots: &dots,
            view,
            view_tiles,
        };
        let used = self.redraw(ts, &scene);
        for (id, used) in ids.iter().zip(used) {
            if let (Some(id), true) = (id, used)
                && let Some(u) = world.units.iter_mut().find(|u| u.id == *id)
            {
                u.attack_mark.0 = false;
            }
        }
        true
    }
}

/// The dots of a world in drawing order ("Units and buildings"), with the unit each came from (`None` for a
/// remembered building).
pub fn dots(world: &World) -> (Vec<Dot>, Vec<Option<tidewater_sim::UnitId>>) {
    let mut dots = Vec::new();
    let mut ids = Vec::new();
    for c in world.remembered.copies() {
        dots.push(Dot {
            kind: c.kind,
            colour: c.colour,
            tile: c.tile,
            size: c.size,
            marked: false,
        });
        ids.push(None);
    }
    for u in world.minimap_units() {
        dots.push(Dot {
            kind: u.kind,
            colour: u.colour,
            tile: u.tile,
            size: u.size,
            marked: u.attack_mark.0,
        });
        ids.push(Some(u.id));
    }
    (dots, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_formats::tileset::Palette;

    /// A screen point's minimap pixel.
    fn screen(r: Px) -> Px {
        (r.0 + ORIGIN.0, r.1 + ORIGIN.1, r.2, r.3)
    }

    fn dot(kind: u8, colour: u8, tile: (i32, i32), size: (u8, u8)) -> Dot {
        Dot {
            kind,
            colour,
            tile,
            size,
            marked: false,
        }
    }

    #[test]
    fn scale_table() {
        assert_eq!([32, 64, 96, 128, 100].map(|w| to_px(w, 3)), [12, 6, 4, 3, 3]);
        assert_eq!([32, 64, 96, 128].map(|w| to_tile(w, 7)), [1, 3, 5, 7]);
        assert_eq!(to_px(96, 14), 18);
        assert_eq!(to_px(96, 96), 128);
        assert_eq!(to_tile(96, 127), 95);
    }

    #[test]
    fn the_walk_by_width() {
        let w = walk(128);
        assert!(w.iter().enumerate().all(|(i, &t)| t == (i as u32, 0)));
        let w = walk(64);
        assert_eq!(&w[..4], &[(0, 0), (0, 1), (1, 0), (1, 1)]);
        assert_eq!(w[127], (63, 1));
        let w = walk(32);
        assert_eq!(&w[..5], &[(0, 0), (0, 1), (0, 2), (0, 3), (1, 0)]);
        assert_eq!(w[127], (31, 3));
        // 96: every 3 tiles, the first gets columns 0 and 1, the next two column 0.
        let w = walk(96);
        assert_eq!(
            &w[..8],
            &[(0, 0), (0, 1), (1, 0), (2, 0), (3, 0), (3, 1), (4, 0), (5, 0)]
        );
        assert_eq!(w[127], (95, 0));
    }

    /// A tileset with megatile `m` whose 16 minitiles are numbered `m * 16 + k`, each mirrored both ways, and
    /// minitile `n` filled with `n` except its (7, 6) pixel, which is `n + 100`.
    fn tileset(megatiles: usize) -> Tileset {
        let minis = megatiles * 16;
        let mut minitiles = Vec::new();
        for n in 0..minis {
            let mut px = [n as u8; 64];
            px[6 * 8 + 7] = n as u8 + 100;
            minitiles.extend(px);
        }
        Tileset {
            palette: Palette([[0; 3]; 256]),
            groups: Vec::new(),
            megatiles: (0..megatiles)
                .map(|m| std::array::from_fn(|k| (((m * 16 + k) as u16) << 2) | 3))
                .collect(),
            minitiles,
            changes: None,
        }
    }

    #[test]
    fn terrain_takes_minitile_pixel_7_6_unmirrored() {
        let ts = tileset(2);
        // A 32-wide map: tile (0, 0) is megatile 1, the rest megatile 0.
        let mut tiles = vec![0u16; 32 * 32];
        tiles[0] = 1;
        let t = terrain(&ts, &tiles, 32, 32);
        // Pixel (2, 1) of tile 0 is its minitile column 2, row 1: minitile 16 + 4 + 2.
        assert_eq!(t.colours[SIZE + 2], 100 + 22);
        assert_eq!(t.colours[0], 116);
        // Pixel (4, 0) is tile 1, column 0: minitile 0 of megatile 0.
        assert_eq!(t.colours[4], 100);
        assert_eq!(t.tiles[4], Some(1));
        assert_eq!(t.tiles[4 * SIZE], Some(32));
    }

    #[test]
    fn tiles_past_the_map_are_black() {
        let ts = tileset(1);
        // Maps are square; the guard still keeps a short tile list from being read past its end.
        let t = terrain(&ts, &vec![0u16; 64 * 32], 64, 32);
        assert_eq!(t.tiles[63 * SIZE], Some(31 * 64));
        assert_eq!(t.tiles[64 * SIZE], None);
        let g = ground(&t, &[], &[], true, true);
        assert_eq!(g[64 * SIZE], 0);
        assert_ne!(g[63 * SIZE], 0);
    }

    #[test]
    fn fog_alternates_by_pixel_column() {
        // 64-wide map, tile (5, 0) explored but not visible.
        let ts = tileset(1);
        let t = terrain(&ts, &vec![0u16; 64 * 64], 64, 64);
        let mut explored = vec![15u8; 64 * 64];
        let visible = vec![15u8; 64 * 64];
        explored[5] = 0;
        let g = ground(&t, &explored, &visible, true, false);
        assert_eq!(g[10], t.colours[10], "column 10 samples the explored map");
        assert_ne!(g[10], 0);
        assert_eq!(g[11], 0, "column 11 samples the visible map");
        // A partly fogged tile is black.
        explored[5] = 8;
        assert_eq!(ground(&t, &explored, &visible, true, false)[10], 0);
        // The reveal-map cheat shows everything.
        assert_eq!(ground(&t, &explored, &visible, true, true)[11], t.colours[11]);
    }

    #[test]
    fn terrain_preference_off_is_all_black() {
        let ts = tileset(1);
        let t = terrain(&ts, &vec![0u16; 32 * 32], 32, 32);
        assert!(ground(&t, &[0; 1024], &[0; 1024], false, true).iter().all(|&c| c == 0));
        // Dots and the rectangle are still drawn.
        let mut m = Minimap::new(32);
        let dots = [dot(0, 1, (5, 5), (1, 1))];
        m.redraw(&ts, &scene(&[0; 1024], 32, false, &dots, (0, 0)));
        assert_eq!(m.pixels[20 * SIZE + 20], 1);
        assert_eq!(m.pixels[0], colour::VIEW);
        assert_eq!(m.pixels[SIZE + 1], 0);
    }

    fn scene<'a>(tiles: &'a [u16], height: u32, terrain_on: bool, dots: &'a [Dot], view: (i32, i32)) -> Scene<'a> {
        Scene {
            megatiles: tiles,
            height,
            explored: &[],
            visible: &[],
            reveal: true,
            terrain_on,
            local: 0,
            demo: false,
            dots,
            view,
            view_tiles: (VIEW_TILES, VIEW_TILES),
        }
    }

    #[test]
    fn dot_vectors() {
        // 128-wide map, own footman at (10, 20): colour 250 at screen (34, 46), 2 × 2.
        let d = dot(2, 0, (10, 20), (1, 1));
        assert_eq!(dot_colour(&d, 0, false), (250, false));
        assert_eq!(screen(dot_rect(128, d.tile, d.size)), (34, 46, 2, 2));
        // 64-wide map, blue town hall (4 × 4) at (10, 20): colour 1 at (44, 66), 8 × 8.
        let d = dot(0x3a, 1, (10, 20), (4, 4));
        assert_eq!(dot_colour(&d, 0, false).0, 1);
        assert_eq!(screen(dot_rect(64, d.tile, d.size)), (44, 66, 8, 8));
        // 96-wide map, neutral farm (2 × 2) at (30, 30): 251 at (64, 66), 2 × 2.
        let d = dot(0x3a, 15, (30, 30), (2, 2));
        assert_eq!(dot_colour(&d, 0, false).0, 251);
        assert_eq!(screen(dot_rect(96, d.tile, d.size)), (64, 66, 2, 2));
        // 32-wide map, footman at (31, 31): (148, 150), 4 × 4, not trimmed.
        assert_eq!(screen(dot_rect(32, (31, 31), (1, 1))), (148, 150, 4, 4));
        // 128-wide map, unit at (127, 0): (151, 26), 1 wide, 2 high.
        assert_eq!(screen(dot_rect(128, (127, 0), (1, 1))), (151, 26, 1, 2));
        // The trim happens once: a 4-wide dot 2 pixels from the edge is still clipped.
        assert_eq!(dot_rect(128, (126, 126), (4, 4)), (126, 126, 3, 3));
    }

    #[test]
    fn colour_rules() {
        // A neutral gold mine.
        assert_eq!(dot_colour(&dot(0x5c, 15, (0, 0), (3, 3)), 0, false).0, 251);
        // A sheep of anyone, even marked.
        let mut sheep = dot(CRITTER, 0, (0, 0), (1, 1));
        sheep.marked = true;
        assert_eq!(dot_colour(&sheep, 0, false), (247, false));
        assert_eq!(dot_colour(&dot(OIL_PATCH, 15, (0, 0), (2, 2)), 0, false).0, 0);
        // Marked: colour 3, and the mark is used up.
        let mut d = dot(2, 0, (0, 0), (1, 1));
        d.marked = true;
        assert_eq!(dot_colour(&d, 0, false), (3, true));
        // Own colour only outside demo playback; the player table by colour player.
        let d = dot(2, 0, (0, 0), (1, 1));
        assert_eq!(dot_colour(&d, 0, true).0, 208);
        assert_eq!((0..8).map(player_colour).collect::<Vec<_>>(), colour::PLAYERS);
        assert_eq!(player_colour(15), 251);
        assert_eq!(dot_colour(&dot(2, 7, (0, 0), (1, 1)), 0, false).0, 2);
    }

    #[test]
    fn press_vectors() {
        let max = |w: i32| (w - VIEW_TILES, w - VIEW_TILES);
        let v = (VIEW_TILES, VIEW_TILES);
        assert_eq!(view_for_press(128, (100, 100), v, max(128)), (69, 67));
        assert_eq!(view_for_press(64, (100, 100), v, max(64)), (31, 30));
        assert_eq!(view_for_press(32, (151, 153), v, max(32)), (18, 18));
        assert_eq!(view_for_press(96, (88, 90), v, max(96)), (41, 41));
        assert_eq!(view_for_press(96, (24, 26), v, max(96)), (0, 0));
        assert_eq!(tile_for_press(128, (100, 100)), (76, 74));
        // Outside the minimap clamps to its edge.
        assert_eq!(tile_for_press(64, (500, 0)), (63, 0));
    }

    #[test]
    fn view_rectangle_vectors() {
        let v = (VIEW_TILES, VIEW_TILES);
        assert_eq!(screen(view_rect(128, (50, 60), v)), (74, 86, 14, 14));
        assert_eq!(screen(view_rect(96, (12, 12), v)), (40, 42, 19, 19));
        assert_eq!(view_rect(64, (0, 0), v).2, 28);
        assert_eq!(view_rect(32, (0, 0), v).2, 56);
        // Half the view, for the press: 7, 14, 28 and 9.
        assert_eq!([128, 64, 32, 96].map(|w| view_px(w, VIEW_TILES) / 2), [7, 14, 28, 9]);
        let mut pic = vec![0u8; SIZE * SIZE];
        outline(&mut pic, view_rect(128, (50, 60), v), colour::VIEW);
        assert_eq!(pic[60 * SIZE + 50], 246);
        assert_eq!(pic[73 * SIZE + 63], 246);
        assert_eq!(pic[61 * SIZE + 51], 0, "only the outline");
        // Clipped at the edge.
        outline(&mut pic, view_rect(128, (120, 120), v), colour::VIEW);
        assert_eq!(pic[127 * SIZE + 125], 0, "the bottom side is off the minimap");
        assert_eq!(pic[127 * SIZE + 120], 246, "the left side runs to the edge");
        assert_eq!(pic[127 * SIZE + 127], 0);
        assert_eq!(pic[120 * SIZE + 127], 246);
    }

    #[test]
    fn countdown() {
        let mut c = Countdown::default();
        // The first cycle redraws; then every 40.
        assert!(c.cycle((0, 0)));
        let cycles: Vec<bool> = (0..80).map(|_| c.cycle((0, 0))).collect();
        assert_eq!(cycles.iter().filter(|&&r| r).count(), 2);
        assert!(cycles[39] && cycles[79]);
        // Countdown at 30; the view scrolls in cycle C: the redraw is in C + 4.
        c.left = 30;
        assert!(!c.cycle((1, 0)));
        assert_eq!(c.left, 4);
        assert_eq!(
            (0..4).map(|_| c.cycle((1, 0))).collect::<Vec<_>>(),
            [false, false, false, true]
        );
        // Not lengthened when fewer than 4 are left.
        c.left = 3;
        assert!(!c.cycle((5, 5)));
        assert_eq!(c.left, 2);
    }

    #[test]
    fn terrain_rebuilds_every_sixth_redraw_or_on_the_cheat() {
        let ts = tileset(2);
        let mut tiles = vec![0u16; 32 * 32];
        let mut m = Minimap::new(32);
        m.redraw(&ts, &scene(&tiles, 32, true, &[], (0, 0)));
        let before = m.pixels[SIZE * 64 + 64];
        // A tree is cut down: the change shows only at the next rebuild.
        tiles[16 * 32 + 16] = 1;
        for _ in 1..6 {
            m.redraw(&ts, &scene(&tiles, 32, true, &[], (0, 0)));
            assert_eq!(m.pixels[SIZE * 64 + 64], before);
        }
        m.redraw(&ts, &scene(&tiles, 32, true, &[], (0, 0)));
        assert_ne!(
            m.pixels[SIZE * 64 + 64],
            before,
            "the sixth redraw after the first rebuilds"
        );
        // A cheat change rebuilds at once.
        tiles[16 * 32 + 16] = 0;
        let mut s = scene(&tiles, 32, true, &[], (0, 0));
        s.reveal = false;
        s.explored = &[0; 1024];
        s.visible = &[0; 1024];
        m.redraw(&ts, &s);
        assert_eq!(m.pixels[SIZE * 64 + 64], before);
    }

    #[test]
    fn dots_draw_in_order_and_report_used_marks() {
        let ts = tileset(1);
        let tiles = vec![0u16; 128 * 128];
        let mut m = Minimap::new(128);
        let mut marked = dot(2, 0, (10, 10), (1, 1));
        marked.marked = true;
        // A remembered building first, then units: the later one is on top.
        let dots = [dot(0x3a, 1, (10, 10), (3, 3)), marked];
        let used = m.redraw(&ts, &scene(&tiles, 128, false, &dots, (100, 100)));
        assert_eq!(used, [false, true]);
        assert_eq!(m.pixels[10 * SIZE + 10], 3);
        assert_eq!(m.pixels[11 * SIZE + 11], 3);
        assert_eq!(m.pixels[12 * SIZE + 12], 1);
        assert_eq!(m.pixels[13 * SIZE + 13], 0);
        // The next redraw, with the mark cleared, draws the unit in the own colour again.
        let dots = [dot(0x3a, 1, (10, 10), (3, 3)), dot(2, 0, (10, 10), (1, 1))];
        assert_eq!(
            m.redraw(&ts, &scene(&tiles, 128, false, &dots, (100, 100))),
            [false, false]
        );
        assert_eq!(m.pixels[11 * SIZE + 11], 250);
        assert_eq!(m.pixels[12 * SIZE + 12], 1);
    }
}
