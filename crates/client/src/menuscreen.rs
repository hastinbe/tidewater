//! The menu screens' rules (spec `docs/specs/ui/screen-and-cursors.md`, "Menu screens"; the fades in
//! `docs/specs/ui/palette-effects.md`, "Screen fades"): which screens share the menu picture
//! (`campaign::MENU_PICTURE`), when it is loaded again, which switches fade, and where the version line shows; and
//! how their buttons and text are drawn in the picture's palette entries ("Text colours", "Buttons"). SDL-free and
//! engine-free; `frontend` opens the screens by these rules.
//!
//! The main menu and every menu reached from it without starting a game are drawn on one picture in its own palette.
//! A switch between them replaces the buttons and text only: the picture isn't loaded again and nothing fades. The
//! custom-scenario setup loads the picture again, fades out and in over 200 ms and has no version line; the main
//! menu, opened from any screen not on the picture, fades in over 200 ms. Tidewater has only the main menu and the
//! campaign menu yet; the other rows are kept whole for the screens to come.

use tidewater_formats::fnt::Font;
use tidewater_formats::tileset::Palette;

use crate::hud::{self, Rect, TextStyle};
use crate::palette::fades;
use crate::scene::Picture;

/// A screen drawn on the menu picture ("Menu screens", "Which screens").
#[allow(dead_code)] // the spec's table is kept whole; Tidewater has only the main menu and the campaign menu yet
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuScreen {
    Main,
    SinglePlayer,
    /// The campaign menu (Tides of Darkness or Beyond the Dark Portal, orc or human); ours is `AppState::NewCampaign`.
    Campaign,
    LoadGame,
    /// The multiplayer menus up to the direct-connection setup (Enhanced or Classic choice, connection method).
    Multiplayer,
    /// A small dialog over these menus (name entry, "Unable to locate IPX network.").
    SmallDialog,
    /// The custom-scenario setup ("Custom Game Setup").
    CustomSetup,
}

/// How a menu screen opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Opening {
    /// The menu picture is loaded (again), with its full palette.
    pub reload: bool,
    /// The fade-in's length in ms; 0 for none.
    pub fade_in_ms: u32,
    /// The version line is drawn over the picture.
    pub version_line: bool,
}

impl MenuScreen {
    /// Whether the screen shows on the shared picture without loading it again: every menu screen but the
    /// custom-scenario setup.
    pub(crate) fn shares_picture(self) -> bool {
        self != MenuScreen::CustomSetup
    }

    /// How the screen opens after `from`, the menu screen before it (`None`: a screen not on the menu picture, such
    /// as the title, the credits or a game).
    pub(crate) fn opening(self, from: Option<MenuScreen>) -> Opening {
        let switch = from.is_some_and(|f| !switch_fades(f, self));
        Opening {
            reload: !switch,
            fade_in_ms: if switch { 0 } else { fades::MENUS },
            version_line: self != MenuScreen::CustomSetup,
        }
    }
}

/// Whether a switch from one menu screen to another fades out and in (200 ms each): only to or from the
/// custom-scenario setup ("Which screens"; `ui/palette-effects.md`, "Screen fades").
pub(crate) fn switch_fades(from: MenuScreen, to: MenuScreen) -> bool {
    !(from.shares_picture() && to.shares_picture())
}

/// Whether the pointer stays shown on the frame a screen asks to switch from `from` to `to`: on a switch between menus
/// on the shared picture, which has no fade (spec "Open questions", "Pointer during menu switches without a fade":
/// the default, the orc arrow stays shown). Every other switch hides it from that frame ("Pointer and fades").
pub(crate) fn pointer_stays(from: Option<MenuScreen>, to: Option<MenuScreen>) -> bool {
    matches!((from, to), (Some(f), Some(t)) if !switch_fades(f, t))
}

// ---------------------------------------------------------------------------------------------------------------
// Buttons and text ("Menu screens", "Text colours" and "Buttons")

/// A font's shades 1 to 4 as entries of the menu palette; shade 5, the shadow, is always entry 0 ("Text colours").
pub(crate) type ShadeMap = [u8; 4];

/// Button labels and dialog text: gold.
pub(crate) const GOLD: ShadeMap = [200, 199, 197, 192];
/// The label's hotkey letter, the whole label while the pointer is over the button, and the screen headings: white.
pub(crate) const WHITE: ShadeMap = [246, 246, 108, 104];
/// A disabled button's label: grey.
pub(crate) const GREY: ShadeMap = [108, 108, 105, 102];
/// The shadow's entry (shade 5), and the button's inner ring: black.
pub(crate) const BLACK: u8 = 0;

/// The focused button's outer ring.
pub(crate) const FOCUS_RING: u8 = 251;
/// A disabled button's outer ring.
pub(crate) const DISABLED_RING: u8 = 247;
/// The outer ring of a button neither focused nor disabled: one of the gold-brown entries 129 to 135 the spec names.
/// Which entry each ring pixel takes isn't known (the button face isn't in the game data, spec "Menu button face"); ours
/// is 132 all round.
pub(crate) const RING: u8 = 132;
/// A disabled button's face: one of the grey entries 97 to 108 the spec names; ours is 102 all over.
pub(crate) const DISABLED_FACE: u8 = 102;
/// The colour our button face takes the nearest menu palette entry to. The original's face is a red marble whose
/// pixels are known only from captures (spec "Menu button face"); ours is flat.
pub(crate) const FACE_NEAR: [u8; 3] = [111, 0, 0];

/// The pixels a space leaves between the glyphs either side of it in `font12x` ("Text colours").
pub(crate) const SPACE_12X: u32 = 10;

/// The main menu's buttons ("Buttons", from `rez\MainMenu.bin`): 224 × 28, left edge 208, tops 36 apart from 240.
pub(crate) const BUTTON_W: u32 = 224;
pub(crate) const BUTTON_H: u32 = 28;
pub(crate) const BUTTON_X: i32 = 208;
pub(crate) const BUTTON_TOP: i32 = 240;
pub(crate) const BUTTON_PITCH: i32 = 36;

/// The advance of a space that leaves `gap` pixels: a glyph already leaves 1 pixel after itself.
pub(crate) fn space_advance(gap: u32) -> u32 {
    gap.saturating_sub(1)
}

/// A shade map's eight colours by font shade in `pal`: shades 1 to 4 from the map, the shadow (5) and the unused shades
/// black (entry 0).
pub(crate) fn colours(pal: &Palette, map: ShadeMap) -> [[u8; 3]; 8] {
    let mut c = [pal.0[BLACK as usize]; 8];
    for (i, &e) in map.iter().enumerate() {
        c[i + 1] = pal.0[e as usize];
    }
    c
}

/// The entry of `pal` nearest to `rgb` (the lowest on a tie).
pub(crate) fn nearest(pal: &Palette, rgb: [u8; 3]) -> u8 {
    let d = |c: &[u8; 3]| (0..3).map(|i| (c[i] as i32 - rgb[i] as i32).pow(2)).sum::<i32>();
    (0..=255u8).min_by_key(|&i| d(&pal.0[i as usize])).unwrap_or(0)
}

/// The rectangles of a menu's `n` buttons: the main menu's column, widened (and kept centred on the screen) where a
/// label `widest` pixels wide wouldn't fit with 16 pixels either side. The campaign menu's layout isn't specified; it
/// takes the main menu's.
pub(crate) fn button_rects(n: usize, widest: u32) -> Vec<Rect> {
    let w = BUTTON_W.max(widest + 32);
    let x = BUTTON_X - (w - BUTTON_W) as i32 / 2;
    (0..n)
        .map(|i| Rect::new(x, BUTTON_TOP + i as i32 * BUTTON_PITCH, w, BUTTON_H))
        .collect()
}

/// Where a label whose drawn pixels fill `ink` (`(left, top, right, bottom)` from its cell's top-left, inclusive) has
/// its cell's top-left on the button `r`: the drawn pixels centred horizontally and vertically, rounding up and left
/// (spec "Open questions", "Menu label placement": the default). A label that draws nothing sits at the button's
/// top-left.
pub(crate) fn label_cell(r: Rect, ink: Option<(i32, i32, i32, i32)>) -> (i32, i32) {
    let Some((l, t, rt, b)) = ink else { return (r.x, r.y) };
    let (w, h) = (rt - l + 1, b - t + 1);
    (r.x + (r.w as i32 - w) / 2 - l, r.y + (r.h as i32 - h) / 2 - t)
}

/// The byte index of a label's hotkey letter: its first letter equal to `key`, ignoring case.
pub(crate) fn hotkey_at(label: &str, key: char) -> Option<usize> {
    label
        .char_indices()
        .find(|(_, c)| c.eq_ignore_ascii_case(&key))
        .map(|(i, _)| i)
}

/// A menu button's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ButtonState {
    pub enabled: bool,
    /// The pointer is over it.
    pub hover: bool,
    /// It has the focus: the yellow ring, which stays when the pointer is over another button.
    pub focused: bool,
}

/// The label's shade map ("Text colours"): grey when disabled, white while the pointer is over it, gold otherwise.
pub(crate) fn label_map(s: ButtonState) -> ShadeMap {
    match s {
        ButtonState { enabled: false, .. } => GREY,
        ButtonState { hover: true, .. } => WHITE,
        _ => GOLD,
    }
}

/// The outer ring's entry ("Buttons"): yellow when focused, light grey when disabled, gold-brown otherwise.
pub(crate) fn ring(s: ButtonState) -> u8 {
    match s {
        ButtonState { focused: true, .. } => FOCUS_RING,
        ButtonState { enabled: false, .. } => DISABLED_RING,
        _ => RING,
    }
}

/// One line of `font12x` text in a shade map.
fn style<'a>(colours: &'a [[u8; 3]; 8], gap: u32) -> TextStyle<'a> {
    TextStyle {
        colours,
        space: Some(space_advance(gap)),
        rows: (i32::MIN, i32::MAX),
    }
}

/// The width of `s` in `font` with spaces that leave `gap` pixels: the advances, less the last glyph's trailing pixel.
pub(crate) fn text_width(font: &Font, s: &str, gap: u32) -> u32 {
    style(&[[0; 3]; 8], gap).width(font, s).saturating_sub(1)
}

/// Draws `s` with its cell's top-left at `at` in the shade map, spaces leaving `gap` pixels.
pub(crate) fn draw_text(
    dst: &mut Picture,
    font: &Font,
    pal: &Palette,
    at: (i32, i32),
    s: &str,
    map: ShadeMap,
    gap: u32,
) {
    let c = colours(pal, map);
    hud::text_styled(dst, font, at, s, &style(&c, gap));
}

/// Draws a heading in the white map, centred on the 640-pixel screen with its cell top at `y`.
pub(crate) fn draw_heading(dst: &mut Picture, font: &Font, pal: &Palette, y: i32, s: &str) {
    let x = (dst.width as i32 - text_width(font, s, SPACE_12X) as i32) / 2;
    draw_text(dst, font, pal, (x, y), s, WHITE, SPACE_12X);
}

/// Draws a menu button: the outer ring, a black ring inside it, the face, and the label in `font12x` centred on it, its
/// hotkey letter (`key`) in white on an enabled button.
pub(crate) fn draw_button(
    dst: &mut Picture,
    font: &Font,
    pal: &Palette,
    r: Rect,
    label: &str,
    key: Option<char>,
    s: ButtonState,
) {
    let rgb = |e: u8| pal.0[e as usize];
    hud::outline(dst, r, rgb(ring(s)));
    let inner = Rect::new(r.x + 1, r.y + 1, r.w.saturating_sub(2), r.h.saturating_sub(2));
    hud::outline(dst, inner, rgb(BLACK));
    let face = if s.enabled {
        nearest(pal, FACE_NEAR)
    } else {
        DISABLED_FACE
    };
    hud::fill(
        dst,
        Rect::new(r.x + 2, r.y + 2, r.w.saturating_sub(4), r.h.saturating_sub(4)),
        rgb(face),
    );
    let map = label_map(s);
    let plain = style(&[[0; 3]; 8], SPACE_12X);
    let at = label_cell(r, hud::ink_box(font, label, &plain));
    draw_text(dst, font, pal, at, label, map, SPACE_12X);
    if let Some(i) = key
        .filter(|_| s.enabled && map != WHITE)
        .and_then(|k| hotkey_at(label, k))
    {
        let x = at.0 + plain.width(font, &label[..i]) as i32;
        let end = i + label[i..].chars().next().map_or(1, char::len_utf8);
        draw_text(dst, font, pal, (x, at.1), &label[i..end], WHITE, SPACE_12X);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use MenuScreen::*;

    const SHARED: [MenuScreen; 6] = [Main, SinglePlayer, Campaign, LoadGame, Multiplayer, SmallDialog];

    /// A synthetic palette whose every entry is a different colour.
    fn palette() -> Palette {
        let mut p = Palette([[0; 3]; 256]);
        for (i, c) in p.0.iter_mut().enumerate() {
            *c = [i as u8, (i / 2) as u8, 255 - i as u8];
        }
        p
    }

    /// A synthetic font: every glyph from '!' a 2 × 3 box at (0, 0) in its cell with shades 1 2 / 3 4 / 5 5; the space
    /// has no glyph.
    fn font() -> Font {
        let g = tidewater_formats::fnt::Glyph {
            width: 2,
            height: 3,
            x: 0,
            y: 0,
            shades: vec![1, 2, 3, 4, 5, 5],
        };
        let mut glyphs = vec![None];
        glyphs.extend((33..=126).map(|_| Some(g.clone())));
        Font {
            first: 32,
            max_width: 2,
            height: 5,
            glyphs,
        }
    }

    fn px(p: &Picture, x: i32, y: i32) -> [u8; 3] {
        let i = ((y as u32 * p.width + x as u32) * 4) as usize;
        [p.rgba[i], p.rgba[i + 1], p.rgba[i + 2]]
    }

    fn blank(w: u32, h: u32) -> Picture {
        Picture {
            width: w,
            height: h,
            rgba: vec![0; (w * h * 4) as usize],
        }
    }

    const NORMAL: ButtonState = ButtonState {
        enabled: true,
        hover: false,
        focused: false,
    };

    #[test]
    fn the_text_colours_are_the_menu_palettes_entries() {
        // "Text colours": shades 1 to 4 of each kind of text; shade 5 is entry 0.
        assert_eq!(GOLD, [200, 199, 197, 192], "button labels, dialog text");
        assert_eq!(WHITE, [246, 246, 108, 104], "hotkey letter, hovered label, headings");
        assert_eq!(GREY, [108, 108, 105, 102], "disabled label");
        let pal = palette();
        let c = colours(&pal, GOLD);
        assert_eq!(c[1..5], [pal.0[200], pal.0[199], pal.0[197], pal.0[192]]);
        assert_eq!(c[5], pal.0[0], "the black shadow");
        assert_eq!(space_advance(SPACE_12X), 9, "a space leaves 10 pixels in font12x");
    }

    #[test]
    fn a_buttons_label_and_ring_follow_its_state() {
        assert_eq!((label_map(NORMAL), ring(NORMAL)), (GOLD, RING));
        let hover = ButtonState { hover: true, ..NORMAL };
        assert_eq!(label_map(hover), WHITE, "the whole label white under the pointer");
        let focused = ButtonState {
            focused: true,
            ..NORMAL
        };
        assert_eq!(
            (label_map(focused), ring(focused)),
            (GOLD, 251),
            "the yellow ring stays on the focused button with the pointer elsewhere"
        );
        let disabled = ButtonState {
            enabled: false,
            ..NORMAL
        };
        assert_eq!((label_map(disabled), ring(disabled)), (GREY, 247));
        assert!((129..=135).contains(&RING) && (97..=108).contains(&DISABLED_FACE));
    }

    #[test]
    fn the_main_menus_buttons_are_its_dialog_files() {
        // "Buttons": 224 × 28 at x 208, y 240, 276, 312, 348 and 384.
        let r = button_rects(5, 100);
        assert_eq!(r.iter().map(|r| r.y).collect::<Vec<_>>(), [240, 276, 312, 348, 384]);
        assert!(r.iter().all(|r| (r.x, r.w, r.h) == (208, 224, 28)));
        // Ours: a label too wide for 224 widens the column (16 pixels either side), kept centred on the screen.
        let wide = button_rects(1, 300)[0];
        assert_eq!((wide.x, wide.w), (154, 332));
        assert_eq!(wide.x * 2 + wide.w as i32, 640);
    }

    #[test]
    fn labels_are_centred_on_their_button() {
        // Single-player menu, "Load Game" (font12x: drawn pixels 92 × 14 from its cell's top-left) on the button at
        // (208, 276): the measured cell top is 283, as centring gives. The measured left is 275; centring gives 274
        // (the rule isn't measured; spec "Menu label placement").
        assert_eq!(
            label_cell(Rect::new(208, 276, 224, 28), Some((0, 0, 91, 13))),
            (274, 283)
        );
        // The cell's offset to its drawn pixels is taken off.
        assert_eq!(label_cell(Rect::new(0, 0, 20, 10), Some((1, 2, 4, 5))), (7, 1));
        assert_eq!(label_cell(Rect::new(5, 6, 20, 10), None), (5, 6));
    }

    #[test]
    fn the_hotkey_letter_is_the_first_match_ignoring_case() {
        assert_eq!(hotkey_at("Load Game", 'L'), Some(0));
        assert_eq!(hotkey_at("Show Credits", 'C'), Some(5));
        assert_eq!(hotkey_at("Quit", 'x'), None);
    }

    #[test]
    fn nearest_takes_the_closest_entry() {
        let pal = palette();
        assert_eq!(nearest(&pal, [100, 50, 155]), 100);
        assert_eq!(nearest(&pal, [101, 50, 154]), 101);
    }

    #[test]
    fn a_button_is_drawn_in_the_menu_palette() {
        let (pal, f) = (palette(), font());
        let r = Rect::new(10, 10, 40, 20);
        let draw = |s: ButtonState| {
            let mut p = blank(64, 40);
            draw_button(&mut p, &f, &pal, r, "AB", Some('b'), s);
            p
        };
        // "AB" draws 5 × 3 pixels (A at 0, B at 3), so its cell is at (10 + 35 / 2, 10 + 17 / 2) = (27, 18).
        let p = draw(ButtonState {
            focused: true,
            ..NORMAL
        });
        assert_eq!(px(&p, 10, 10), pal.0[251], "focus ring");
        assert_eq!(px(&p, 49, 29), pal.0[251]);
        assert_eq!(px(&p, 11, 11), pal.0[0], "black inner ring");
        assert_eq!(px(&p, 12, 12), pal.0[nearest(&pal, FACE_NEAR) as usize], "face");
        assert_eq!(px(&p, 27, 18), pal.0[200], "A: gold shade 1");
        assert_eq!(px(&p, 27, 19), pal.0[197], "A: gold shade 3");
        assert_eq!(px(&p, 27, 20), pal.0[0], "shadow");
        assert_eq!(px(&p, 30, 18), pal.0[246], "B, the hotkey: white shade 1");
        assert_eq!(px(&p, 31, 19), pal.0[104], "B: white shade 4");
        let p = draw(NORMAL);
        assert_eq!(px(&p, 10, 10), pal.0[RING as usize]);
        let p = draw(ButtonState { hover: true, ..NORMAL });
        assert_eq!(px(&p, 27, 18), pal.0[246], "hovered: the whole label white");
        let p = draw(ButtonState {
            enabled: false,
            ..NORMAL
        });
        assert_eq!(px(&p, 10, 10), pal.0[247]);
        assert_eq!(px(&p, 12, 12), pal.0[102]);
        assert_eq!(px(&p, 27, 18), pal.0[108], "grey label");
        assert_eq!(px(&p, 31, 19), pal.0[102]);
        assert_eq!(px(&p, 30, 18), pal.0[108], "no white hotkey on a disabled button");
    }

    #[test]
    fn a_heading_is_white_and_centred() {
        let (pal, f) = (palette(), font());
        let mut p = blank(64, 20);
        // "AB" is 5 pixels wide (its advances less the last pixel): left edge (64 - 5) / 2 = 29.
        draw_heading(&mut p, &f, &pal, 4, "AB");
        assert_eq!(px(&p, 29, 4), pal.0[246]);
        assert_eq!(px(&p, 29, 5), pal.0[108]);
        assert_eq!(px(&p, 28, 4), [0, 0, 0]);
    }

    #[test]
    fn menus_on_the_shared_picture_switch_without_a_fade_or_a_reload() {
        // Round 73: main menu to single player or multiplayer, campaign menu back to single player, single player to
        // load game: same picture, no darker frame, the version line stays.
        for from in SHARED {
            for to in SHARED {
                assert!(!switch_fades(from, to), "{from:?} -> {to:?}");
                assert_eq!(
                    to.opening(Some(from)),
                    Opening {
                        reload: false,
                        fade_in_ms: 0,
                        version_line: true
                    },
                    "{from:?} -> {to:?}"
                );
                assert!(pointer_stays(Some(from), Some(to)), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn the_main_menu_fades_in_from_other_screens() {
        // After the title, the credits or a game: loaded with its palette and a 200 ms fade-in, version line shown.
        assert_eq!(
            Main.opening(None),
            Opening {
                reload: true,
                fade_in_ms: 200,
                version_line: true
            }
        );
        // Custom-scenario setup, Cancel: fades out and back into the main menu.
        assert_eq!(Main.opening(Some(CustomSetup)), Main.opening(None));
        assert!(switch_fades(CustomSetup, Main));
    }

    #[test]
    fn the_custom_scenario_setup_reloads_the_picture_and_fades_with_no_version_line() {
        for from in SHARED.map(Some).into_iter().chain([None]) {
            assert_eq!(
                CustomSetup.opening(from),
                Opening {
                    reload: true,
                    fade_in_ms: 200,
                    version_line: false
                },
                "{from:?}"
            );
        }
        assert!(
            switch_fades(SinglePlayer, CustomSetup),
            "Custom Scenario on the single-player menu"
        );
        assert!(!CustomSetup.shares_picture());
    }

    #[test]
    fn the_pointer_hides_on_every_switch_that_fades() {
        // A menu button that changes screen with a fade (Custom Scenario; Show Credits leaves the menus).
        assert!(!pointer_stays(Some(SinglePlayer), Some(CustomSetup)));
        assert!(!pointer_stays(Some(CustomSetup), Some(Main)));
        assert!(!pointer_stays(Some(Main), None), "Show Credits");
        assert!(!pointer_stays(None, Some(Main)));
        assert!(!pointer_stays(None, None));
    }
}
