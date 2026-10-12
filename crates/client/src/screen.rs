//! The in-game screen's rules from `docs/specs/ui/screen-and-cursors.md`: the resource bar (its counting, the food
//! item and the large-number placeholder), the menu and pause buttons, scrolling at the screen edge and with the keys,
//! which pointer shows when, and the placement site under the pointer's hot spot. The saved views F2 to F4 are game
//! setup's `Started::saved_views`, kept by the client.
//! The 640 × 480 layout itself is [`crate::hud::Layout`]. No Bevy types here.
//!
//! Our readings where the spec leaves room:
//! - On a canvas larger than 640 × 480 the right and bottom edge zones are the last 10 pixels of the canvas, as they
//!   are at 640 × 480 (x ≥ 630, y ≥ 470), and the view's size in tiles is its real size (14 × 14 on the original
//!   screen).
//! - Gold, lumber and oil count once per game cycle (the "interface update").
//! - While Ctrl or Alt is held the key countdown keeps running; only the keys aren't read.
//! - While scrolling is blocked (paused, a menu open) neither tick clock nor countdown moves.
//! - While the game is paused the pointer is the normal arrow, scrolling edge or not.
//! - Default scroll speeds (open question): 3 for the mouse and 3 for the keys.
//! - The large-number placeholder (open question) is our own six-character word, [`PLACEHOLDER`].

use crate::hud::Rect;

/// Tile size in pixels.
#[cfg_attr(not(test), expect(dead_code, reason = "only `site_tile` uses it"))]
pub const TILE: i32 = 32;

// --- resource bar --------------------------------------------------------------------------------------------------

/// Values at or above this show [`PLACEHOLDER`].
pub const LARGE: i64 = 1_000_000;
/// Our text for a value too large to show.
pub const PLACEHOLDER: &str = "Plenty";
/// The food supply shown is at most this.
pub const FOOD_CAP: u32 = 200;
/// The text starts this far right of an item's left edge.
pub const TEXT_DX: i32 = 18;

/// The resource bar's items, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Gold,
    Lumber,
    Oil,
    Food,
}

impl Item {
    pub const ALL: [Item; 4] = [Item::Gold, Item::Lumber, Item::Oil, Item::Food];

    /// The item's frame of `Art\icons.grp`.
    pub fn icon(self) -> usize {
        match self {
            Item::Gold => 0,
            Item::Lumber => 1,
            Item::Oil => 2,
            Item::Food => 4,
        }
    }

    /// The item's area, x offsets from the bar's left edge, edges included.
    pub fn area(self, multiplayer: bool) -> (i32, i32) {
        match (self, multiplayer) {
            (Item::Gold, false) => (36, 108),
            (Item::Lumber, false) => (109, 181),
            (Item::Oil, false) => (182, 254),
            (Item::Food, false) => (255, 327),
            (Item::Gold, true) => (8, 77),
            (Item::Lumber, true) => (78, 147),
            (Item::Oil, true) => (148, 217),
            (Item::Food, true) => (218, 287),
        }
    }
}

/// One counting step of a shown value toward the real one ("Counting up and down").
pub fn count_step(shown: i64, real: i64) -> i64 {
    let d = real - shown;
    if d == 0 { shown } else { shown + d / 16 + d.signum() }
}

/// The shown gold, lumber and oil.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counter {
    pub shown: [i64; 3],
}

impl Counter {
    /// When the game screen opens: each value one below the real one.
    pub fn open(real: [u32; 3]) -> Self {
        Self {
            shown: real.map(|r| r as i64 - 1),
        }
    }

    pub fn update(&mut self, real: [u32; 3]) {
        for (s, r) in self.shown.iter_mut().zip(real) {
            *s = count_step(*s, r as i64);
        }
    }
}

/// A gold, lumber or oil value as text.
pub fn amount_text(v: i64) -> String {
    if v >= LARGE {
        PLACEHOLDER.to_string()
    } else {
        v.to_string()
    }
}

/// The food item: used food, the supply as shown, and whether the used number takes text colour 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Food {
    pub used: u32,
    pub supply: u32,
    pub over: bool,
}

impl Food {
    /// The used number's text colour, then the slash and supply's.
    pub fn colours(&self) -> (u8, u8) {
        (if self.over { 3 } else { 2 }, 1)
    }
}

/// The food item, if it is drawn: never under Classic rules, and not while the supply is 0.
pub fn food(enhanced: bool, used: u32, supply: u32) -> Option<Food> {
    if !enhanced || supply == 0 {
        return None;
    }
    let supply = supply.min(FOOD_CAP);
    Some(Food {
        used,
        supply,
        over: used > supply,
    })
}

// --- menu buttons --------------------------------------------------------------------------------------------------

/// The two buttons above the minimap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuButton {
    Menu,
    Pause,
}

impl MenuButton {
    /// The button's screen area.
    pub fn rect(self) -> Rect {
        match self {
            MenuButton::Menu => Rect::new(4, 0, 80, 24),
            MenuButton::Pause => Rect::new(88, 0, 80, 24),
        }
    }
}

/// Whether the pause button shows: not for a player with no pauses left in Enhanced multiplayer.
pub fn pause_shown(enhanced: bool, multiplayer: bool, pauses_left: u32) -> bool {
    !(enhanced && multiplayer && pauses_left == 0)
}

/// The button under screen point `p`, among those shown.
pub fn menu_button_at(p: (f32, f32), pause: bool) -> Option<MenuButton> {
    [MenuButton::Menu, MenuButton::Pause]
        .into_iter()
        .filter(|&b| b == MenuButton::Menu || pause)
        .find(|b| b.rect().contains(p.0, p.1))
}

// --- scrolling -----------------------------------------------------------------------------------------------------

/// Tiles per scroll step, on each axis.
pub const STEP: i32 = 2;
/// The least time between ticks of one tick clock, in milliseconds.
pub const TICK_MS: u64 = 20;
/// The default scroll speeds (`ui/game-menu.md`, "Stored settings").
pub const DEFAULT_SPEED: u8 = 4;

/// The countdown reload W for a speed setting 0 to 6.
pub fn wait(speed: u8) -> u8 {
    6 - speed.min(6)
}

/// The view position and what limits it, in tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewGeom {
    /// The tile at the map view's top-left corner.
    pub pos: (i32, i32),
    /// The view's size in tiles (14 × 14 on the original screen).
    pub tiles: (i32, i32),
    /// The map's size in tiles.
    pub map: (i32, i32),
}

impl ViewGeom {
    /// The largest view position on each axis.
    pub fn max(&self) -> (i32, i32) {
        ((self.map.0 - self.tiles.0).max(0), (self.map.1 - self.tiles.1).max(0))
    }

    /// `p` clamped to the view range.
    pub fn clamp(&self, p: (i32, i32)) -> (i32, i32) {
        let m = self.max();
        (p.0.clamp(0, m.0), p.1.clamp(0, m.1))
    }

    /// The view after one step in direction `d` (each axis −1, 0 or 1), clamped.
    pub fn step(&self, d: (i32, i32)) -> (i32, i32) {
        self.clamp((self.pos.0 + STEP * d.0, self.pos.1 + STEP * d.1))
    }
}

/// The scrolling edge the pointer at screen point `p` is at, as a direction (each axis −1, 0 or 1; (0, 0) for none),
/// on a canvas `canvas` pixels big. The bottom test compares view y with the map's **width** less the view's height,
/// as the spec says to keep.
pub fn edge(p: (i32, i32), canvas: (i32, i32), v: &ViewGeom) -> (i32, i32) {
    let dx = if p.0 <= 5 && v.pos.0 > 0 {
        -1
    } else if p.0 >= canvas.0 - 10 && v.pos.0 < v.map.0 - v.tiles.0 {
        1
    } else {
        0
    };
    let dy = if p.1 <= 5 && v.pos.1 > 0 {
        -1
    } else if p.1 >= canvas.1 - 10 && v.pos.1 < v.map.0 - v.tiles.1 {
        1
    } else {
        0
    };
    (dx, dy)
}

/// A tick clock: ticks at least [`TICK_MS`] apart, by the system clock in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    last: Option<u64>,
}

impl Clock {
    /// Whether a tick happens at `now`.
    pub fn tick(&mut self, now: u64) -> bool {
        if self.last.is_some_and(|l| now.saturating_sub(l) < TICK_MS) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

/// A scrolling key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    PageUp,
    End,
    PageDown,
    /// A numeric keypad digit.
    Pad(u8),
}

/// The direction a scrolling key gives; the keypad only while no chat line is being typed.
pub fn key_direction(k: ScrollKey, chat: bool) -> Option<(i32, i32)> {
    use ScrollKey::*;
    if chat && matches!(k, Pad(_)) {
        return None;
    }
    Some(match k {
        Up | Pad(8) => (0, -1),
        Down | Pad(2) => (0, 1),
        Left | Pad(4) => (-1, 0),
        Right | Pad(6) => (1, 0),
        Home | Pad(7) => (-1, -1),
        PageUp | Pad(9) => (1, -1),
        End | Pad(1) => (-1, 1),
        PageDown | Pad(3) => (1, 1),
        Pad(_) => return None,
    })
}

/// The held keys' directions added up, each axis down to its sign.
pub fn keys_direction(keys: &[ScrollKey], chat: bool) -> (i32, i32) {
    let s = keys
        .iter()
        .filter_map(|&k| key_direction(k, chat))
        .fold((0, 0), |a, d| (a.0 + d.0, a.1 + d.1));
    (s.0.signum(), s.1.signum())
}

/// What scrolling reads each time input is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollInput {
    /// Milliseconds by the system clock.
    pub now: u64,
    /// The scrolling edge from [`edge`].
    pub edge: (i32, i32),
    /// The held keys from [`keys_direction`].
    pub keys: (i32, i32),
    pub ctrl_or_alt: bool,
    /// Paused, a menu open or the attract-mode demo.
    pub blocked: bool,
}

/// The scrolling state: two tick clocks, two countdowns and the two speed settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scroller {
    edge_clock: Clock,
    key_clock: Clock,
    pub edge_countdown: u8,
    pub key_countdown: u8,
    pub mouse_speed: u8,
    pub key_speed: u8,
}

impl Default for Scroller {
    fn default() -> Self {
        Self::new(DEFAULT_SPEED, DEFAULT_SPEED)
    }
}

impl Scroller {
    /// Both countdowns start at 0 when the program starts.
    pub fn new(mouse_speed: u8, key_speed: u8) -> Self {
        Self {
            edge_clock: Clock::default(),
            key_clock: Clock::default(),
            edge_countdown: 0,
            key_countdown: 0,
            mouse_speed: mouse_speed.min(6),
            key_speed: key_speed.min(6),
        }
    }

    /// One input read: the direction of the step to make, if any.
    pub fn update(&mut self, i: ScrollInput) -> Option<(i32, i32)> {
        if i.blocked {
            return None;
        }
        if i.edge != (0, 0) {
            if !self.edge_clock.tick(i.now) {
                return None;
            }
            if self.edge_countdown == 0 {
                self.edge_countdown = wait(self.mouse_speed);
                return Some(i.edge);
            }
            self.edge_countdown -= 1;
            return None;
        }
        if !self.key_clock.tick(i.now) {
            return None;
        }
        if self.key_countdown != 0 {
            self.key_countdown -= 1;
            return None;
        }
        self.key_countdown = wait(self.key_speed);
        (!i.ctrl_or_alt && i.keys != (0, 0)).then_some(i.keys)
    }
}

// --- pointers ------------------------------------------------------------------------------------------------------

/// A pointer shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pointer {
    Arrow,
    /// Target cross, "no unit" version.
    TargetNone,
    /// Target cross, "other" version.
    TargetOther,
    /// Target cross, "own" version.
    TargetOwn,
    Magnify,
    Watch,
    /// The arrow of a scrolling edge (each axis −1, 0 or 1).
    Edge(i8, i8),
}

impl Pointer {
    /// Every shape, for loading.
    pub fn all() -> Vec<Pointer> {
        let mut v = vec![
            Pointer::Arrow,
            Pointer::TargetNone,
            Pointer::TargetOther,
            Pointer::TargetOwn,
            Pointer::Magnify,
            Pointer::Watch,
        ];
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) != (0, 0) {
                    v.push(Pointer::Edge(dx, dy));
                }
            }
        }
        v
    }

    /// The pointer file in `Art\`, by the local race for the race pointers.
    pub fn file(self, orc: bool) -> &'static str {
        let race = |h, o| if orc { o } else { h };
        match self {
            Pointer::Arrow => race("h_arrow.cur", "o_arrow.cur"),
            Pointer::TargetNone => race("htarget.cur", "otarget.cur"),
            Pointer::TargetOther => race("htarget2.cur", "otarget2.cur"),
            Pointer::TargetOwn => race("htarget3.cur", "otarget3.cur"),
            Pointer::Magnify => "select.cur",
            Pointer::Watch => "watch.cur",
            Pointer::Edge(0, -1) => "ptr_n.cur",
            Pointer::Edge(1, -1) => "ptr_ne.cur",
            Pointer::Edge(1, 0) => "ptr_e.cur",
            Pointer::Edge(1, 1) => "ptr_se.cur",
            Pointer::Edge(0, 1) => "ptr_s.cur",
            Pointer::Edge(-1, 1) => "ptr_sw.cur",
            Pointer::Edge(-1, 0) => "ptr_w.cur",
            Pointer::Edge(..) => "ptr_nw.cur",
        }
    }
}

/// Where the pointer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    MapView,
    Minimap,
    /// The panel or anywhere else.
    Other,
}

/// The interface mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Target,
    Place,
}

/// The neutral player.
pub const NEUTRAL: u8 = 15;

/// What decides the pointer shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerInput<'a> {
    /// Multiplayer, and waiting for another player's data longer than one retry period.
    pub waiting: bool,
    /// The scrolling edge from [`edge`].
    pub edge: (i32, i32),
    /// Paused or a menu open.
    pub paused: bool,
    pub over: Over,
    pub mode: Mode,
    /// The owners of the units under the pointer (the hover list).
    pub hovered: &'a [u8],
    pub local: u8,
}

/// The pointer shape ("Pointer shapes"): the first rule that applies wins.
pub fn pointer(i: &PointerInput) -> Pointer {
    if i.waiting {
        return Pointer::Watch;
    }
    if i.paused {
        return Pointer::Arrow;
    }
    if i.edge != (0, 0) {
        return Pointer::Edge(i.edge.0 as i8, i.edge.1 as i8);
    }
    match (i.over, i.mode) {
        (Over::Other, _) => Pointer::Arrow,
        (Over::Minimap, Mode::Target) => Pointer::TargetNone,
        (Over::Minimap, _) => Pointer::Arrow,
        (Over::MapView, Mode::Place) => Pointer::Arrow,
        (Over::MapView, Mode::Target) => {
            if i.hovered.is_empty() {
                Pointer::TargetNone
            } else if i.hovered.contains(&i.local) {
                Pointer::TargetOwn
            } else if i.hovered.contains(&NEUTRAL) {
                Pointer::TargetNone
            } else {
                Pointer::TargetOther
            }
        }
        (Over::MapView, Mode::Normal) if !i.hovered.is_empty() => Pointer::Magnify,
        (Over::MapView, Mode::Normal) => Pointer::Arrow,
    }
}

/// The placement site's top-left tile for the pointer's hot spot at screen point `p` with the view at `view`, if the
/// pointer is in the map view `area` ("Placement"). The sea buildings' odd-tile rule is applied after this
/// (`tidewater_sim::World::site_at`).
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the client's zoomable view (our extension) maps the pointer through the camera, which gives this tile at the original scale"
    )
)]
pub fn site_tile(p: (i32, i32), area: Rect, view: (i32, i32)) -> Option<(i32, i32)> {
    area.contains(p.0 as f32, p.1 as f32)
        .then(|| ((p.0 - area.x) / TILE + view.0, (p.1 - area.y) / TILE + view.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i32, i32) = (640, 480);

    fn geom(pos: (i32, i32), map: (i32, i32)) -> ViewGeom {
        ViewGeom {
            pos,
            tiles: (14, 14),
            map,
        }
    }

    fn input(now: u64, edge: (i32, i32), keys: (i32, i32)) -> ScrollInput {
        ScrollInput {
            now,
            edge,
            keys,
            ..ScrollInput::default()
        }
    }

    /// The tick numbers (from 1) on which a step is made, over `n` ticks 20 ms apart.
    fn steps(s: &mut Scroller, n: u64, edge: (i32, i32), keys: (i32, i32)) -> Vec<u64> {
        (1..=n)
            .filter(|t| s.update(input(t * 20, edge, keys)).is_some())
            .collect()
    }

    #[test]
    fn edge_needs_room_to_scroll() {
        let v = geom((0, 0), (128, 128));
        assert_eq!(edge((2, 240), SCREEN, &v), (0, 0), "view x is 0");
        let v = geom((50, 50), (128, 128));
        assert_eq!(edge((2, 2), SCREEN, &v), (-1, -1));
        assert_eq!(edge((6, 6), SCREEN, &v), (0, 0));
        assert_eq!(edge((630, 470), SCREEN, &v), (1, 1));
        assert_eq!(edge((629, 469), SCREEN, &v), (0, 0));
    }

    #[test]
    fn north_west_at_full_speed() {
        let v = geom((50, 50), (128, 128));
        let e = edge((2, 2), SCREEN, &v);
        let mut s = Scroller::new(6, 3);
        let d = s.update(input(1000, e, (0, 0))).unwrap();
        let v = ViewGeom { pos: v.step(d), ..v };
        assert_eq!(v.pos, (48, 48));
        assert_eq!(s.update(input(1010, e, (0, 0))), None, "under 20 ms");
        let d = s.update(input(1020, e, (0, 0))).unwrap();
        assert_eq!(v.step(d), (46, 46));
    }

    #[test]
    fn edge_speed_3_steps_every_4_ticks() {
        let mut s = Scroller::new(3, 3);
        assert_eq!(steps(&mut s, 9, (1, 0), (0, 0)), [1, 5, 9]);
        let mut v = geom((0, 0), (128, 128));
        let mut s = Scroller::new(3, 3);
        let mut xs = Vec::new();
        for t in 1..=9 {
            if let Some(d) = s.update(input(t * 20, (1, 0), (0, 0))) {
                v.pos = v.step(d);
                xs.push(v.pos.0);
            }
        }
        assert_eq!(xs, [2, 4, 6]);
    }

    #[test]
    fn a_left_over_countdown_carries() {
        let mut s = Scroller::new(3, 3);
        s.edge_countdown = 2;
        assert_eq!(steps(&mut s, 11, (1, 0), (0, 0)), [3, 7, 11]);
        // Leaving the edge doesn't reset it.
        let mut s = Scroller::new(3, 3);
        steps(&mut s, 2, (1, 0), (0, 0));
        assert_eq!(s.edge_countdown, 2);
    }

    #[test]
    fn wide_map_bottom_edge_blocks_but_does_not_move() {
        // 128 wide, 64 high, view y 50 is the bottom.
        let v = geom((0, 50), (128, 64));
        assert_eq!(edge((300, 475), SCREEN, &v), (0, 1));
        assert_eq!(v.step((0, 1)), (0, 50));
        assert_eq!(
            pointer(&PointerInput {
                waiting: false,
                edge: (0, 1),
                paused: false,
                over: Over::MapView,
                mode: Mode::Normal,
                hovered: &[],
                local: 0,
            }),
            Pointer::Edge(0, 1)
        );
        assert_eq!(
            crate::mouse::action(
                crate::mouse::ModeKind::Normal,
                crate::mouse::Style::First,
                crate::mouse::Region::MapView,
                crate::mouse::Button::Left,
                false,
                true
            ),
            crate::mouse::Action::Nothing,
            "clicks blocked"
        );
    }

    #[test]
    fn tall_map_bottom_edge_stops_short_but_keys_go_on() {
        // 64 wide, 128 high: 50 = 64 − 14.
        let v = geom((0, 50), (64, 128));
        assert_eq!(edge((300, 475), SCREEN, &v), (0, 0));
        let mut s = Scroller::new(3, 6);
        let d = s
            .update(input(20, (0, 0), keys_direction(&[ScrollKey::Down], false)))
            .unwrap();
        assert_eq!(v.step(d), (0, 52));
    }

    #[test]
    fn right_edge_clamps_then_stops() {
        let v = geom((113, 0), (128, 128));
        assert_eq!(edge((635, 240), SCREEN, &v), (1, 0));
        let v = ViewGeom {
            pos: v.step((1, 0)),
            ..v
        };
        assert_eq!(v.pos.0, 114);
        assert_eq!(edge((635, 240), SCREEN, &v), (0, 0));
    }

    #[test]
    fn keys() {
        let up_left = keys_direction(&[ScrollKey::Up, ScrollKey::Left], false);
        let mut s = Scroller::new(3, 6);
        assert_eq!(steps(&mut s, 3, (0, 0), up_left), [1, 2, 3], "speed 6: every tick");
        assert_eq!(up_left, (-1, -1));
        let mut s = Scroller::new(3, 0);
        assert_eq!(steps(&mut s, 15, (0, 0), (0, -1)), [1, 8, 15]);
        assert_eq!(keys_direction(&[ScrollKey::Up, ScrollKey::Down], false), (0, 0));
        assert_eq!(keys_direction(&[ScrollKey::Up, ScrollKey::Home], false), (-1, -1));
        let mut s = Scroller::new(3, 6);
        let ctrl = ScrollInput {
            ctrl_or_alt: true,
            ..input(20, (0, 0), (0, -1))
        };
        assert_eq!(s.update(ctrl), None, "Ctrl held");
        assert_eq!(keys_direction(&[ScrollKey::Pad(8)], true), (0, 0), "chat line open");
        assert_eq!(keys_direction(&[ScrollKey::Up], true), (0, -1));
        assert_eq!(keys_direction(&[ScrollKey::Pad(9)], false), (1, -1));
        assert_eq!(keys_direction(&[ScrollKey::Pad(5)], false), (0, 0));
        // Keys aren't read at a scrolling edge.
        let mut s = Scroller::new(0, 6);
        assert_eq!(s.update(input(20, (1, 0), (0, -1))), Some((1, 0)));
        // Nothing while blocked.
        let mut s = Scroller::new(6, 6);
        let blocked = ScrollInput {
            blocked: true,
            ..input(20, (1, 0), (0, -1))
        };
        assert_eq!(s.update(blocked), None);
    }

    #[test]
    fn speeds() {
        assert_eq!((0..=6).map(wait).collect::<Vec<_>>(), [6, 5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn counting() {
        let mut shown = 100;
        let mut seen = Vec::new();
        while shown != 400 {
            shown = count_step(shown, 400);
            seen.push(shown);
        }
        assert_eq!(&seen[..3], [119, 137, 154]);
        let mut shown = 400;
        let down: Vec<i64> = (0..5)
            .map(|_| {
                shown = count_step(shown, 395);
                shown
            })
            .collect();
        assert_eq!(down, [399, 398, 397, 396, 395]);
        let mut c = Counter::open([500, 0, 7]);
        assert_eq!(c.shown, [499, -1, 6]);
        c.update([500, 0, 7]);
        assert_eq!(c.shown, [500, 0, 7]);
    }

    #[test]
    fn amounts_and_food() {
        assert_eq!(amount_text(999_999), "999999");
        assert_eq!(amount_text(1_000_000), PLACEHOLDER);
        assert_eq!(PLACEHOLDER.len(), 6);
        let f = food(true, 7, 4).unwrap();
        assert_eq!((f.used, f.supply, f.colours()), (7, 4, (3, 1)));
        assert_eq!(food(true, 3, 4).unwrap().colours(), (2, 1));
        assert_eq!(food(true, 10, 220).unwrap().supply, 200);
        assert_eq!(food(false, 7, 4), None, "Classic rules");
        assert_eq!(food(true, 0, 0), None);
    }

    #[test]
    fn bar_items() {
        assert_eq!(Item::Gold.area(false), (36, 108));
        assert_eq!(Item::Food.area(true), (218, 287));
        assert_eq!(Item::ALL.map(Item::icon), [0, 1, 2, 4]);
    }

    #[test]
    fn menu_buttons() {
        assert_eq!(menu_button_at((4.0, 10.0), true), Some(MenuButton::Menu));
        assert_eq!(menu_button_at((83.0, 23.0), true), Some(MenuButton::Menu));
        assert_eq!(menu_button_at((85.0, 10.0), true), None);
        assert_eq!(menu_button_at((167.0, 10.0), true), Some(MenuButton::Pause));
        assert_eq!(menu_button_at((100.0, 10.0), false), None);
        assert!(!pause_shown(true, true, 0));
        assert!(pause_shown(true, false, 0));
        assert!(pause_shown(false, true, 0));
    }

    fn over_map(mode: Mode, hovered: &[u8]) -> Pointer {
        pointer(&PointerInput {
            waiting: false,
            edge: (0, 0),
            paused: false,
            over: Over::MapView,
            mode,
            hovered,
            local: 0,
        })
    }

    #[test]
    fn pointer_shapes() {
        assert_eq!(over_map(Mode::Target, &[NEUTRAL]), Pointer::TargetNone, "gold mine");
        assert_eq!(over_map(Mode::Target, &[1]), Pointer::TargetOther, "enemy grunt");
        assert_eq!(
            over_map(Mode::Target, &[0, 1]),
            Pointer::TargetOwn,
            "own footman by a grunt"
        );
        assert_eq!(over_map(Mode::Target, &[]), Pointer::TargetNone);
        assert_eq!(over_map(Mode::Normal, &[0]), Pointer::Magnify);
        assert_eq!(over_map(Mode::Normal, &[]), Pointer::Arrow);
        assert_eq!(over_map(Mode::Place, &[0]), Pointer::Arrow);
        assert_eq!(Pointer::Arrow.file(true), "o_arrow.cur", "orc player on empty ground");
        let base = PointerInput {
            waiting: false,
            edge: (0, 0),
            paused: false,
            over: Over::Minimap,
            mode: Mode::Target,
            hovered: &[],
            local: 0,
        };
        assert_eq!(pointer(&base), Pointer::TargetNone);
        assert_eq!(
            pointer(&PointerInput {
                over: Over::Other,
                ..base
            }),
            Pointer::Arrow
        );
        assert_eq!(pointer(&PointerInput { edge: (-1, -1), ..base }), Pointer::Edge(-1, -1));
        assert_eq!(Pointer::Edge(-1, -1).file(false), "ptr_nw.cur");
        assert_eq!(
            pointer(&PointerInput {
                paused: true,
                edge: (1, 0),
                ..base
            }),
            Pointer::Arrow
        );
        assert_eq!(pointer(&PointerInput { waiting: true, ..base }), Pointer::Watch);
        assert_eq!(Pointer::all().len(), 14);
    }

    #[test]
    fn placement_site() {
        let area = Rect::new(176, 16, 448, 448);
        assert_eq!(site_tile((300, 100), area, (10, 20)), Some((13, 22)));
        assert_eq!(site_tile((100, 100), area, (10, 20)), None);
    }
}
