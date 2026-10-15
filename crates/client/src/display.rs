//! How the 640 × 480 screen (spec `docs/specs/ui/screen-and-cursors.md`, "Layout") goes into the window: the window
//! size, the whole-number scale and the black letterbox around it, the pointer's way back to screen pixels, and the
//! map view's zoom. No SDL types here.
//!
//! Our choices (the original runs full screen at 640 × 480, so it has none of these):
//! - **Scale.** The screen is drawn at a whole number of physical pixels per screen pixel, the largest that fits the
//!   window, and is never smaller than 640 × 480 or squashed. The rest of the window is black, centred as a full-screen
//!   game would be. The default window is the largest such scale that fits the monitor, less [`RESERVE`] points for
//!   the menu bar, title bar and task bar.
//! - **Pointer.** A pointer over the black border is taken to be at the nearest screen edge, as the original's pointer
//!   stops at the screen's edge: pushing it into the border scrolls the view.
//! - **Zoom.** The map view draws one map pixel per screen pixel, as the original does. The `--free-zoom` development
//!   option (not part of the original) zooms with the mouse wheel in powers of two, from [`MIN_ZOOM`] to [`MAX_ZOOM`]
//!   map pixels per screen pixel.
//! - **Mouse lock.** In a game the pointer is kept in the window, as a full-screen game's pointer can't leave the
//!   screen, so pushing it against an edge scrolls the view however fast the mouse moves (as StarCraft's windowed
//!   "W-Mode" and Diablo II: Resurrected's "lock cursor" option do). [`lock_wanted`] says when: only while a game runs (not while a game menu or the outcome dialog is
//!   up, which use the system's pointer), only
//!   while the window has the focus (switching away, Cmd-Tab, frees it; a click back in locks it again), and not while
//!   Cmd (the Windows key elsewhere) is held. Where the system can confine the pointer to the window, it does; macOS
//!   can't, so there the system pointer is held still and the game's own pointer moves by the mouse's motion, kept on
//!   the screen ([`Letterbox::keep_on_screen`]). `--no-mouse-lock` turns it off.

/// The screen in its own pixels.
pub const SCREEN: (u32, u32) = (640, 480);

/// Logical points of the monitor kept free for the system's bars (menu bar, title bar, dock or task bar): across and
/// down.
pub const RESERVE: (f64, f64) = (32.0, 120.0);

/// The zoom's range with `--free-zoom`: map pixels per screen pixel.
pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 8.0;

/// The largest whole number of physical pixels per screen pixel whose 640 × 480 fits `(width, height)` physical
/// pixels; at least 1.
pub fn fit_scale(size: (u32, u32)) -> u32 {
    (size.0 / SCREEN.0).min(size.1 / SCREEN.1).max(1)
}

/// The default scale in physical pixels per screen pixel for a monitor of `size` physical pixels at `factor` physical
/// pixels per logical point: the largest whose window fits the monitor less [`RESERVE`]; at least 1.
pub fn monitor_scale(size: (u32, u32), factor: f64) -> u32 {
    let factor = if factor > 0.0 { factor } else { 1.0 };
    let usable = |len: u32, reserve: f64| (len as f64 - reserve * factor).max(0.0) as u32;
    fit_scale((usable(size.0, RESERVE.0), usable(size.1, RESERVE.1)))
}

/// The window size in physical pixels for a scale.
pub fn window_size(scale: u32) -> (u32, u32) {
    (SCREEN.0 * scale, SCREEN.1 * scale)
}

/// Where the screen sits in a window: its scale and its top-left corner, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Letterbox {
    pub scale: u32,
    pub offset: (u32, u32),
}

impl Default for Letterbox {
    fn default() -> Self {
        Self::fit((SCREEN.0, SCREEN.1))
    }
}

impl Letterbox {
    /// The screen in a window of `size` physical pixels: the largest whole scale that fits, centred.
    pub fn fit(size: (u32, u32)) -> Self {
        let scale = fit_scale(size);
        let (w, h) = window_size(scale);
        Self {
            scale,
            offset: (size.0.saturating_sub(w) / 2, size.1.saturating_sub(h) / 2),
        }
    }

    /// A rectangle of screen pixels (`x`, `y`, width, height) in physical window pixels.
    pub fn rect(&self, x: i32, y: i32, w: u32, h: u32) -> (u32, u32, u32, u32) {
        let s = self.scale;
        (
            self.offset.0 + x.max(0) as u32 * s,
            self.offset.1 + y.max(0) as u32 * s,
            w * s,
            h * s,
        )
    }

    /// The whole screen in physical window pixels.
    pub fn screen(&self) -> (u32, u32, u32, u32) {
        self.rect(0, 0, SCREEN.0, SCREEN.1)
    }

    /// A window point in logical points (`factor` physical pixels each) in screen pixels, not kept on the screen: a
    /// point on the black border is outside 640 × 480.
    pub fn screen_of(&self, point: (f32, f32), factor: f32) -> (f32, f32) {
        let s = self.scale as f32;
        let on = |p: f32, offset: u32| (p * factor - offset as f32) / s;
        (on(point.0, self.offset.0), on(point.1, self.offset.1))
    }

    /// A window point in logical points (`factor` physical pixels each) in screen pixels, kept on the screen: a point
    /// on the black border goes to the nearest screen edge.
    pub fn screen_point(&self, point: (f32, f32), factor: f32) -> (f32, f32) {
        let (x, y) = self.screen_of(point, factor);
        (
            x.clamp(0.0, SCREEN.0 as f32 - 0.001),
            y.clamp(0.0, SCREEN.1 as f32 - 0.001),
        )
    }

    /// A window point in logical points (`factor` physical pixels each) kept on the screen, still in logical points:
    /// a point on the black border or outside the window goes to the nearest screen edge. The locked pointer lives here.
    pub fn keep_on_screen(&self, point: (f32, f32), factor: f32) -> (f32, f32) {
        let s = self.scale as f32;
        let on = |p: f32, offset: u32, len: u32| {
            let lo = offset as f32 / factor;
            p.clamp(lo, lo + (len as f32 - 0.001) * s / factor)
        };
        (
            on(point.0, self.offset.0, SCREEN.0),
            on(point.1, self.offset.1, SCREEN.1),
        )
    }

    /// The middle of the screen in logical points.
    pub fn centre(&self, factor: f32) -> (f32, f32) {
        let s = self.scale as f32;
        (
            (self.offset.0 as f32 + SCREEN.0 as f32 * s / 2.0) / factor,
            (self.offset.1 as f32 + SCREEN.1 as f32 * s / 2.0) / factor,
        )
    }
}

/// What decides the mouse lock.
#[derive(Clone, Copy, Debug, Default)]
pub struct LockInput {
    /// Not turned off with `--no-mouse-lock`.
    pub enabled: bool,
    /// A game is on screen, drawing its own pointer.
    pub in_game: bool,
    pub focused: bool,
    /// Cmd (the Windows key elsewhere) is held: the pointer is free while it is.
    pub held_free: bool,
}

/// Whether the pointer is locked to the window (module doc, "Mouse lock").
pub fn lock_wanted(i: LockInput) -> bool {
    i.enabled && i.in_game && i.focused && !i.held_free
}

/// The zoom after `steps` mouse-wheel steps (positive zooms in: fewer map pixels per screen pixel): powers of two
/// from [`MIN_ZOOM`] to [`MAX_ZOOM`], so map pixels stay whole blocks of screen pixels when zoomed in. A zoom that
/// isn't a power of two goes to the nearest one first.
pub fn zoom_step(zoom: f32, steps: i32) -> f32 {
    let k = zoom.max(f32::MIN_POSITIVE).log2().round() as i32 - steps;
    2f32.powi(k).clamp(MIN_ZOOM, MAX_ZOOM)
}

/// Mouse-wheel steps from a scroll amount in lines (`pixels` false) or in pixels (trackpads), with the remainder kept
/// in `acc` for the next frame. One line or [`WHEEL_PIXELS`] pixels make a step.
pub fn wheel_steps(acc: &mut f32, delta: f32, pixels: bool) -> i32 {
    *acc += if pixels { delta / WHEEL_PIXELS } else { delta };
    let steps = acc.trunc();
    *acc -= steps;
    steps as i32
}

/// Trackpad pixels per wheel step.
pub const WHEEL_PIXELS: f32 = 50.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_scale_picks_the_largest_that_fits() {
        // A Retina laptop: 1512 × 982 points; 3 physical px per screen pixel is a 960 × 720-point window.
        assert_eq!(monitor_scale((3024, 1964), 2.0), 3);
        assert_eq!(window_size(3), (1920, 1440));
        // 1080p at 1: 1280 × 960 fits under the bars.
        assert_eq!(monitor_scale((1920, 1080), 1.0), 2);
        assert_eq!(monitor_scale((2560, 1440), 1.0), 2);
        assert_eq!(monitor_scale((3840, 2160), 1.0), 4);
        assert_eq!(monitor_scale((3840, 2160), 2.0), 4);
        // Small screens get 1, even when 640 × 480 doesn't quite fit.
        assert_eq!(monitor_scale((1024, 768), 1.0), 1);
        assert_eq!(monitor_scale((800, 600), 1.0), 1);
        assert_eq!(monitor_scale((640, 480), 1.0), 1);
        assert_eq!(monitor_scale((0, 0), 0.0), 1);
    }

    #[test]
    fn fit_is_whole_and_never_smaller_than_the_screen() {
        assert_eq!(fit_scale((1920, 1440)), 3);
        assert_eq!(fit_scale((1919, 1440)), 2);
        assert_eq!(fit_scale((3024, 1964)), 4);
        assert_eq!(fit_scale((100, 100)), 1);
    }

    #[test]
    fn letterbox_centres_the_screen() {
        // Exactly the scaled screen: no border.
        assert_eq!(
            Letterbox::fit((1920, 1440)),
            Letterbox {
                scale: 3,
                offset: (0, 0)
            }
        );
        // Full screen on the Retina laptop: scale 4 (2560 × 1920), 232 px left and right, 22 above and below.
        let lb = Letterbox::fit((3024, 1964));
        assert_eq!(
            lb,
            Letterbox {
                scale: 4,
                offset: (232, 22)
            }
        );
        assert_eq!(lb.screen(), (232, 22, 2560, 1920));
        // A window too wide keeps its scale and gets side bars.
        assert_eq!(Letterbox::fit((2000, 960)).offset, (360, 0));
        // The map view (screen 176, 16, 448 × 448) in the window.
        assert_eq!(lb.rect(176, 16, 448, 448), (232 + 704, 22 + 64, 1792, 1792));
    }

    #[test]
    fn pointer_goes_through_the_offset() {
        let lb = Letterbox::fit((3024, 1964));
        // Logical points at factor 2: physical (232, 22) is screen (0, 0).
        assert_eq!(lb.screen_point((116.0, 11.0), 2.0), (0.0, 0.0));
        // Physical (232 + 4 × 176, 22 + 4 × 16) is the map view's top-left pixel.
        assert_eq!(lb.screen_point((468.0, 43.0), 2.0), (176.0, 16.0));
        // Half a screen pixel in.
        assert_eq!(lb.screen_point((469.0, 44.0), 2.0), (176.5, 16.5));
        // On the black border: the nearest edge.
        let (x, y) = lb.screen_point((10.0, 10.0), 2.0);
        assert_eq!((x, y), (0.0, 0.0));
        let (x, y) = lb.screen_point((1500.0, 980.0), 2.0);
        assert_eq!((x.floor(), y.floor()), (639.0, 479.0));
        // No border at factor 1.
        let lb = Letterbox::fit((1280, 960));
        assert_eq!(lb.screen_point((300.0, 200.0), 1.0), (150.0, 100.0));
    }

    #[test]
    fn locked_pointer_stays_on_the_screen() {
        // Full screen on the Retina laptop at factor 2: the screen is logical 116..1396 across, 11..971 down.
        let lb = Letterbox::fit((3024, 1964));
        assert_eq!(lb.keep_on_screen((500.0, 300.0), 2.0), (500.0, 300.0));
        let (x, y) = lb.keep_on_screen((-40.0, 2000.0), 2.0);
        assert_eq!(x, 116.0);
        assert!(y < 971.0 && y > 970.99);
        // Kept points are screen pixels 0 and 479, never past the edge.
        assert_eq!(lb.screen_point((x, y), 2.0).1.floor(), 479.0);
        let (x, _) = lb.keep_on_screen((5000.0, 0.0), 2.0);
        assert_eq!(lb.screen_point((x, 0.0), 2.0).0.floor(), 639.0);
        assert_eq!(lb.centre(2.0), (756.0, 491.0));
        assert_eq!(lb.screen_point(lb.centre(2.0), 2.0), (320.0, 240.0));
    }

    #[test]
    fn lock_only_in_a_focused_game() {
        let on = LockInput {
            enabled: true,
            in_game: true,
            focused: true,
            held_free: false,
        };
        assert!(lock_wanted(on));
        assert!(!lock_wanted(LockInput { enabled: false, ..on }));
        assert!(!lock_wanted(LockInput { in_game: false, ..on }));
        assert!(!lock_wanted(LockInput { focused: false, ..on }));
        assert!(!lock_wanted(LockInput { held_free: true, ..on }));
    }

    #[test]
    fn wheel_zoom_steps_in_powers_of_two() {
        assert_eq!(zoom_step(1.0, 1), 0.5);
        assert_eq!(zoom_step(1.0, -1), 2.0);
        assert_eq!(zoom_step(0.25, 1), MIN_ZOOM);
        assert_eq!(zoom_step(8.0, -3), MAX_ZOOM);
        // An odd start goes to the nearest power of two.
        assert_eq!(zoom_step(1.3, 0), 1.0);
        assert_eq!(zoom_step(3.0, 1), 2.0);
        let mut acc = 0.0;
        assert_eq!(wheel_steps(&mut acc, 1.0, false), 1);
        assert_eq!(wheel_steps(&mut acc, -2.0, false), -2);
        assert_eq!(wheel_steps(&mut acc, 30.0, true), 0);
        assert_eq!(wheel_steps(&mut acc, 30.0, true), 1);
        assert!((acc - 0.2).abs() < 1e-6);
    }
}
