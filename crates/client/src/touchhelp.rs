//! The Touch Controls help (ours, `docs/design/touch.md`, "Help"): one page that says what the fingers and the side
//! buttons do. The game menu shows it as a dialog like Keystroke Help (`gamemenu`, `Screen::TouchHelp`, from the Help
//! menu), the main menu over its picture (`frontend`, the Touch controls link), and a phone or tablet shows it by itself
//! the first time a game starts (the `touch_help_seen` line of the stored settings, `prefs`). SDL-free.

use tidewater_formats::fnt::Font;
use tidewater_formats::tileset::Palette;

use crate::hud::{self, Rect};
use crate::menuscreen::{self, GOLD, SPACE_10X, WHITE};
use crate::scene::Picture;

/// The page's title.
pub const TITLE: &str = "Touch Controls";

/// The page, a gesture or button and what it does on each line (as Keystroke Help lays out its keys).
pub const LINES: [&str; 15] = [
    "Drag   Scroll the map",
    "Hold, then drag   Select with a box",
    "Tap   Select; with units selected, order them",
    "Two taps   Select all of that type",
    "Long press   Right click",
    "Two fingers   Scroll; pinch to zoom on a phone",
    "Three fingers   Game menu",
    "Minimap   Tap or drag to look, hold to order",
    "Building   Drag the outline, tap it to place",
    "Left bar   Menu, pause, Shift, Ctrl, Alt,",
    "   L: the next tap selects, K: keyboard",
    "Right bar   Groups: a tap recalls,",
    "   two taps centre, a hold makes the group",
    "Settings   The Touch page changes all this",
    "Help menu   Shows this page again",
];

/// The main menu's box for the page, in screen pixels.
pub(crate) const BOX: Rect = Rect::new(60, 60, 520, 360);

/// The line under the page on the main menu.
pub(crate) const CLOSE_TEXT: &str = "Tap anywhere to close";

/// The rows of text from the box's top: the title's, then the lines' and the close line's.
const TITLE_Y: i32 = 14;
const LINES_Y: i32 = 48;
const LINE_H: i32 = 18;

/// Draws the page over a menu picture: the picture darkened, a box with a gold rule, the title in `font`, the lines
/// and the close line in `small`.
pub(crate) fn draw(dst: &mut Picture, font: &Font, small: &Font, pal: &Palette) {
    for px in dst.rgba.chunks_exact_mut(4) {
        for ch in &mut px[..3] {
            *ch /= 3;
        }
    }
    let gold = menuscreen::colours(pal, GOLD)[0];
    let b = BOX;
    hud::fill(dst, b, [12, 8, 4]);
    for r in [
        Rect::new(b.x, b.y, b.w, 2),
        Rect::new(b.x, b.y + b.h as i32 - 2, b.w, 2),
        Rect::new(b.x, b.y, 2, b.h),
        Rect::new(b.x + b.w as i32 - 2, b.y, 2, b.h),
    ] {
        hud::fill(dst, r, gold);
    }
    menuscreen::draw_heading(dst, font, pal, Rect::new(b.x, b.y + TITLE_Y, b.w, 20), TITLE);
    for (i, line) in LINES.iter().enumerate() {
        let (left, right) = split(line);
        let y = b.y + LINES_Y + i as i32 * LINE_H;
        menuscreen::draw_text(dst, small, pal, (b.x + 24, y), left, GOLD, SPACE_10X);
        menuscreen::draw_text(dst, small, pal, (b.x + 190, y), right, WHITE, SPACE_10X);
    }
    let w = menuscreen::text_width(small, CLOSE_TEXT, SPACE_10X) as i32;
    let at = (b.x + (b.w as i32 - w) / 2, b.y + b.h as i32 - 26);
    menuscreen::draw_text(dst, small, pal, at, CLOSE_TEXT, WHITE, SPACE_10X);
}

/// A line's two columns: the gesture before its three spaces, what it does after (a continuation line, starting with
/// the spaces, has only the second).
fn split(line: &str) -> (&str, &str) {
    match line.split_once("   ") {
        Some((a, b)) => (a, b),
        None => ("", line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_has_two_columns_or_continues_the_one_above() {
        for line in LINES {
            let (a, b) = split(line);
            assert!(!b.is_empty(), "{line:?}");
            assert!(!b.starts_with(' '), "{line:?}");
            if a.is_empty() {
                assert!(line.starts_with("   "), "{line:?}");
            }
        }
        assert!(!split(LINES[0]).0.is_empty());
    }

    #[test]
    fn the_page_fits_its_box() {
        let bottom = LINES_Y + LINES.len() as i32 * LINE_H;
        assert!(bottom <= BOX.h as i32 - 30, "the lines end at {bottom}");
        assert!(BOX.x >= 0 && BOX.x + BOX.w as i32 <= 640);
        assert!(BOX.y >= 0 && BOX.y + BOX.h as i32 <= 480);
        // The right column has room for the longest line at about 7 pixels a character (`font10x`).
        let longest = LINES.iter().map(|l| split(l).1.len()).max().unwrap_or(0) as i32;
        assert!(190 + longest * 7 <= BOX.w as i32, "{longest}");
    }
}
