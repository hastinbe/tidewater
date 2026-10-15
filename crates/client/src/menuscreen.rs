//! The menu screens' rules (spec `docs/specs/ui/screen-and-cursors.md`, "Menu screens"; the fades in
//! `docs/specs/ui/palette-effects.md`, "Screen fades"): which screens share the menu picture
//! (`campaign::MENU_PICTURE`), when it is loaded again, which switches fade, and where the version line shows.
//! SDL-free and engine-free; `frontend` opens the screens by these rules.
//!
//! The main menu and every menu reached from it without starting a game are drawn on one picture in its own palette.
//! A switch between them replaces the buttons and text only: the picture isn't loaded again and nothing fades. The
//! custom-scenario setup loads the picture again, fades out and in over 200 ms and has no version line; the main
//! menu, opened from any screen not on the picture, fades in over 200 ms. Tidewater has only the main menu and the
//! campaign menu yet; the other rows are kept whole for the screens to come.

use crate::palette::fades;

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

#[cfg(test)]
mod tests {
    use super::*;
    use MenuScreen::*;

    const SHARED: [MenuScreen; 6] = [Main, SinglePlayer, Campaign, LoadGame, Multiplayer, SmallDialog];

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
