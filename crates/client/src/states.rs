//! The flow's state machine (`docs/design/client-core.md`, migration step 6): which screen shows, the change the
//! flow asks for, and what runs when a screen is left and entered. Nothing here depends on an engine; the shell runs
//! the [`Hook`]s.
//!
//! A change asked for during a frame takes effect between the frame's input and its game cycles, at most one a frame,
//! and the last one asked for wins. A change to the screen that already shows is a change too: it leaves and enters
//! the screen again (one video after another, one game after another).
//!
//! A change runs, in this order:
//! 1. the state becomes the new one (the leaving hooks already see it);
//! 2. the leaving screen's hooks ([`on_exit`]);
//! 3. the leaving screen's own layers and sounds go, the last made first;
//! 4. the entering screen's hooks ([`on_enter`]).

/// The screen that shows.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Boot,
    /// The first-run screen (`firstrun`): there are no game files yet.
    FirstRun,
    Video,
    Title,
    MainMenu,
    NewCampaign,
    /// The Custom Scenario screen (`customsetup`).
    CustomSetup,
    Act,
    Briefing,
    Game,
    Score,
    Finale,
    Credits,
}

/// The state and the change asked for.
#[derive(Default, Clone, Debug)]
pub struct Machine {
    current: AppState,
    next: Option<AppState>,
    /// The screen shown before the current one (`Boot` at first): an entering screen's hooks see where it came from.
    previous: AppState,
}

/// A change of screen: the state left and the state entered (the same for a screen entered again).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub exited: AppState,
    pub entered: AppState,
}

impl Machine {
    /// The screen that shows.
    pub fn get(&self) -> AppState {
        self.current
    }

    /// Asks for `state` from the next change point on; a later ask in the same frame replaces it.
    pub fn set(&mut self, state: AppState) {
        self.next = Some(state);
    }

    /// A change of screen has been asked for and not yet made.
    pub fn leaving(&self) -> bool {
        self.next.is_some()
    }

    /// The screen a change asked for and not yet made goes to.
    pub fn asked(&self) -> Option<AppState> {
        self.next
    }

    /// The screen shown before the current one.
    pub fn previous(&self) -> AppState {
        self.previous
    }

    /// Takes the change asked for, if any, and makes its state the current one.
    pub fn take_transition(&mut self) -> Option<Transition> {
        let entered = self.next.take()?;
        let exited = std::mem::replace(&mut self.current, entered);
        self.previous = exited;
        Some(Transition { exited, entered })
    }
}

/// What runs when a screen is left or entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hook {
    /// The flow's music stops (before a game starts its own).
    StopMusic,
    /// The flow's screen clears to black except over the game screen.
    CameraClear,
    /// Loads the game the flow asks for.
    StartGame,
    /// Lays the game screen out and spawns its layers.
    SetupGame,
    /// Removes the game screen's layers and resources.
    EndGame,
    /// Closes the outcome dialog.
    CloseDialog,
    /// Closes the game menu.
    CloseGameMenu,
    /// Closes the main menu's load dialog.
    CloseMenuLoad,
    EnterVideo,
    EnterTitle,
    EnterMenu,
    EnterNewCampaign,
    EnterCustomSetup,
    EnterAct,
    EnterBriefing,
    EnterScore,
    EnterFinale,
    EnterCredits,
}

/// The hooks that run, in order, when `state` is left.
pub fn on_exit(state: AppState) -> &'static [Hook] {
    use Hook::*;
    match state {
        AppState::MainMenu => &[CloseMenuLoad],
        AppState::Game => &[CameraClear, CloseDialog, CloseGameMenu, EndGame],
        _ => &[],
    }
}

/// The hooks that run, in order, when `state` is entered.
pub fn on_enter(state: AppState) -> &'static [Hook] {
    use Hook::*;
    match state {
        AppState::Boot | AppState::FirstRun => &[],
        AppState::Video => &[EnterVideo],
        AppState::Title => &[EnterTitle],
        AppState::MainMenu => &[EnterMenu],
        AppState::NewCampaign => &[EnterNewCampaign],
        AppState::CustomSetup => &[EnterCustomSetup],
        AppState::Act => &[EnterAct],
        AppState::Briefing => &[EnterBriefing],
        // The flow's music stops before the game starts its own.
        AppState::Game => &[StopMusic, CameraClear, StartGame, SetupGame],
        AppState::Score => &[EnterScore],
        AppState::Finale => &[EnterFinale],
        AppState::Credits => &[EnterCredits],
    }
}

/// Every hook: the test checks that each is used.
#[cfg(test)]
const HOOKS: [Hook; 18] = {
    use Hook::*;
    [
        StopMusic,
        CameraClear,
        StartGame,
        SetupGame,
        EndGame,
        CloseDialog,
        CloseGameMenu,
        CloseMenuLoad,
        EnterVideo,
        EnterTitle,
        EnterMenu,
        EnterNewCampaign,
        EnterCustomSetup,
        EnterAct,
        EnterBriefing,
        EnterScore,
        EnterFinale,
        EnterCredits,
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [AppState; 13] = [
        AppState::Boot,
        AppState::FirstRun,
        AppState::Video,
        AppState::Title,
        AppState::MainMenu,
        AppState::NewCampaign,
        AppState::CustomSetup,
        AppState::Act,
        AppState::Briefing,
        AppState::Game,
        AppState::Score,
        AppState::Finale,
        AppState::Credits,
    ];

    #[test]
    fn starts_at_boot_with_nothing_asked() {
        let mut m = Machine::default();
        assert_eq!(m.get(), AppState::Boot);
        assert_eq!(m.take_transition(), None);
    }

    #[test]
    fn a_change_takes_effect_at_the_change_point() {
        let mut m = Machine::default();
        m.set(AppState::Title);
        assert_eq!(m.get(), AppState::Boot, "not before the change point");
        let t = m.take_transition();
        assert_eq!(
            t,
            Some(Transition {
                exited: AppState::Boot,
                entered: AppState::Title
            })
        );
        assert_eq!(m.get(), AppState::Title);
        assert_eq!(
            m.previous(),
            AppState::Boot,
            "the entering screen sees where it came from"
        );
        assert_eq!(m.take_transition(), None, "one change per ask");
    }

    #[test]
    fn the_last_ask_in_a_frame_wins() {
        let mut m = Machine::default();
        m.set(AppState::Title);
        m.set(AppState::MainMenu);
        assert_eq!(m.asked(), Some(AppState::MainMenu));
        assert_eq!(m.take_transition().map(|t| t.entered), Some(AppState::MainMenu));
        assert_eq!(m.asked(), None);
    }

    #[test]
    fn entering_the_same_screen_again_is_a_change() {
        let mut m = Machine::default();
        m.set(AppState::Video);
        m.take_transition();
        m.set(AppState::Video);
        assert_eq!(
            m.take_transition(),
            Some(Transition {
                exited: AppState::Video,
                entered: AppState::Video
            })
        );
    }

    #[test]
    fn a_game_stops_the_music_before_it_starts() {
        assert_eq!(
            on_enter(AppState::Game),
            [Hook::StopMusic, Hook::CameraClear, Hook::StartGame, Hook::SetupGame]
        );
    }

    #[test]
    fn leaving_a_game_ends_it() {
        assert!(on_exit(AppState::Game).contains(&Hook::EndGame));
        assert!(on_exit(AppState::Game).contains(&Hook::CloseGameMenu));
        assert_eq!(on_exit(AppState::MainMenu), [Hook::CloseMenuLoad]);
    }

    #[test]
    fn every_hook_is_listed_once_and_used() {
        for (i, h) in HOOKS.iter().enumerate() {
            assert!(!HOOKS[..i].contains(h), "{h:?} twice");
            assert!(
                ALL.iter().any(|&s| on_enter(s).contains(h) || on_exit(s).contains(h)),
                "{h:?} unused"
            );
        }
    }
}
