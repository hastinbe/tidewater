//! Collecting a running game into a saved game and starting a game from one (spec `docs/specs/game/save-load.md`, "What a
//! saved game keeps" and "What a loaded game starts afresh"). The file layout is `savefile`, the folder and dialog rules
//! `saves`, the dialogs `savedlg`; the game menu (`gamemenu.rs`) calls the entry points here:
//!
//! - [`capture`]: the running game's bytes as a saved game; `main.rs`'s `save_game` gathers the parts and writes them
//!   to the save folder under a name (single player).
//! - [`load_game`]: ends the running game, or the main menu, and starts the saved one. The flow goes through the
//!   `Boot` state, which takes the next step, a `Play` of the saved game's map; `main.rs`'s `load_game` finds the
//!   [`Resume`] in the flow and puts the stored state over the freshly built game.
//!
//! What the client keeps beside the world: the colour cycles that are on (the pulse included), the music track, the
//! screen's map position and the flow's record of the game (kind, campaign total, starting controllers, the acts
//! shown), see [`crate::savefile::ClientState`]. Settings (volumes, scroll speeds, mouse style, preferences) are not
//! part of a saved game: they stay in [`crate::gamemenu::Stored`].
//!
//! The game clock and the pause counts are [`GameClock`] and [`PauseCounts`]; Enhanced saves keep them.
//!
//! Not yet here: multiplayer saves (who may save and the slot choice are in `saves`, tested but unused). The main
//! menu's Load Game (`frontend`) uses [`load_game`] too.

use std::time::Duration;

use tidewater_sim::setup::Setup;

use crate::frontend::{AppState, Flow, Step};
use crate::savefile::{self, Body, ClientState, Header, Kind};
use crate::states::Machine;

/// How the running game was started, so a save can say how to build the same game again.
#[derive(Clone, Debug)]
pub struct GameOrigin {
    pub map: String,
    pub setup: Setup,
    /// The start seed the slot set-up drew from.
    pub seed: u32,
}

/// A saved game waiting to be put over the game that starts next.
#[derive(Debug)]
pub struct Resume {
    pub header: Header,
    pub body: Body,
}

/// The map position a loaded game starts the view on, until the game screen has used it.
pub struct ResumeLook(pub (f32, f32));

/// The game clock (`ui/game-menu.md`, "Battle.net's quit button"): real time since this game's clock started at game
/// start (read on the core's clock, [`crate::clock::FrameClock::elapsed`], so a scripted run's clock is the same every
/// time), plus the time already played before a loaded game was saved; it stops when the game ends. Enhanced saves
/// keep it (`game/save-load.md`); a Classic save doesn't, and its game starts the clock at 0 (the spec's open-question
/// default, "Classic clock").
#[derive(Clone, Copy, Debug)]
pub struct GameClock {
    /// Milliseconds played before this start (a loaded game's saved clock).
    base_ms: u32,
    started: Duration,
    /// The reading at the game's end, once it has ended.
    stopped_ms: Option<u32>,
}

impl GameClock {
    pub fn start(base_ms: u32, now: Duration) -> Self {
        GameClock {
            base_ms,
            started: now,
            stopped_ms: None,
        }
    }

    /// The clock in milliseconds at `now`.
    pub fn ms(&self, now: Duration) -> u32 {
        self.stopped_ms.unwrap_or_else(|| {
            let run = now.saturating_sub(self.started).as_millis();
            self.base_ms.saturating_add(run.min(u32::MAX as u128) as u32)
        })
    }

    /// The game has ended: the clock keeps its reading from here on.
    pub fn stop(&mut self, now: Duration) {
        if self.stopped_ms.is_none() {
            self.stopped_ms = Some(self.ms(now));
        }
    }
}

/// The pauses each player starts a game with (`ui/game-menu.md`, "Pausing").
pub const START_PAUSES: u8 = 3;

/// Each player's pauses left. Only Enhanced multiplayer games use them (`ui/game-menu.md`, "Pausing"), and this client
/// has none, so nothing spends them yet: the counts are kept so Enhanced saves carry them (`game/save-load.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PauseCounts(pub [u8; 16]);

impl Default for PauseCounts {
    fn default() -> Self {
        PauseCounts([START_PAUSES; 16])
    }
}

/// The game clock and pause counts a game starts with: a loaded game's saved ones, else a fresh clock and full counts.
pub fn clock_and_pauses(
    clock_ms: Option<u32>,
    pause_counts: Option<[u8; 16]>,
    now: Duration,
) -> (GameClock, PauseCounts) {
    let base = clock_ms.unwrap_or(0);
    let pauses = pause_counts.map_or_else(PauseCounts::default, PauseCounts);
    (GameClock::start(base, now), pauses)
}

/// What the client keeps of the running game beside the world (`savefile::ClientState`), from the flow's record and
/// the screen's state.
pub fn client_state(
    flow: &Flow,
    cycles_on: u8,
    music: Option<u8>,
    screen: (i32, i32),
    clock_ms: Option<u32>,
    pause_counts: Option<[u8; 16]>,
) -> ClientState {
    ClientState {
        cycles_on,
        music,
        screen,
        game: flow.game.unwrap_or(crate::campaign::GameKind::Custom),
        total: flow.total,
        memory: flow.memory,
        start_controllers: flow.start_controllers,
        clock_ms,
        pause_counts,
    }
}

/// The bytes of the running game `world` as a saved game named `name`, saved at `saved_at` (whole seconds since
/// 1970).
pub fn capture(
    world: &mut tidewater_sim::World,
    origin: &GameOrigin,
    client: ClientState,
    name: &str,
    saved_at: u64,
) -> Vec<u8> {
    // "Saving with the map revealed": the cheat is off just long enough to reset the remembered buildings.
    world.prepare_save();
    let enhanced = world.rules.enhanced;
    let header = Header {
        name: name.to_string(),
        saved_at,
        enhanced,
        kind: Kind::Single,
        map: origin.map.clone(),
        setup: origin.setup,
        seed: origin.seed,
        race: world.start.race_setting,
    };
    let body = Body {
        world: world.save_state(),
        client,
    }
    .for_rules(enhanced);
    savefile::encode(&header, &body)
}

impl Body {
    fn for_rules(mut self, enhanced: bool) -> Self {
        self.client = self.client.for_rules(enhanced);
        self
    }
}

/// Starts the game in `file`: the flow plays its map next, and the state to enter is `Boot`, which takes that step.
pub fn load_game(flow: &mut Flow, next: &mut Machine, header: Header, body: Body) {
    flow.steps.clear();
    flow.steps.push_back(Step::Play {
        map: header.map.clone(),
        setup: Some(header.setup),
    });
    flow.resume = Some(Box::new(Resume { header, body }));
    next.set(AppState::Boot);
}

/// What a game started from a saved game has to put back besides the world: applied by `main.rs` once the game is built.
pub fn apply_client(flow: &mut Flow, c: &ClientState) {
    flow.game = Some(c.game);
    flow.total = c.total;
    flow.memory = c.memory;
    flow.start_controllers = c.start_controllers;
}

/// The seed a loaded game's generator starts from: the time in whole seconds, as for a new game.
pub fn reseed(wall_secs: u64) -> u32 {
    wall_secs as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_runs_on_from_the_saved_time_and_stops_at_the_end() {
        let t0 = Duration::from_secs(7);
        let (mut clock, _) = clock_and_pauses(Some(61_000), None, t0);
        assert_eq!(clock.ms(t0 + Duration::from_millis(1500)), 62_500);
        clock.stop(t0 + Duration::from_secs(4));
        assert_eq!(clock.ms(t0 + Duration::from_secs(60)), 65_000);
        clock.stop(t0 + Duration::from_secs(90));
        assert_eq!(clock.ms(t0 + Duration::from_secs(90)), 65_000);
    }

    /// X7 (`game/save-load.md`, "What the client saves"; `game/campaign-flow.md`, "Campaign total"): the campaign total
    /// is saved, and a load takes the saved value (and the value at the start of the mission) rather than what a game
    /// start would give. A game start zeroes it for a first mission, so `record_game`'s order matters: it starts the
    /// game, then applies the saved state.
    #[test]
    fn a_load_takes_the_saved_campaign_total() {
        use crate::campaign::{CampaignTotal, GameKind};
        let mut saved = Flow::default();
        saved.game = Some(GameKind::Campaign(0));
        saved.total = CampaignTotal {
            total: 900,
            at_start: 400,
        };
        let state = client_state(&saved, 0, None, (0, 0), None, None);
        assert_eq!(state.total, saved.total);
        // The program that loads it was playing something else and starts the saved game the way `record_game` does.
        let mut flow = Flow::default();
        flow.total = CampaignTotal {
            total: 50,
            at_start: 50,
        };
        flow.total.start_game(GameKind::Campaign(0));
        assert_eq!(flow.total.total, 0, "a first mission starts at 0");
        apply_client(&mut flow, &state);
        assert_eq!(flow.total, saved.total);
        assert_eq!(flow.game, Some(GameKind::Campaign(0)));
    }

    #[test]
    fn a_new_game_or_a_classic_save_starts_the_clock_at_0_with_full_pauses() {
        let t0 = Duration::from_secs(7);
        // A new game and a Classic save look the same here: neither has a clock or counts.
        let (clock, pauses) = clock_and_pauses(None, None, t0);
        assert_eq!(clock.ms(t0), 0);
        assert_eq!(pauses, PauseCounts([START_PAUSES; 16]));
        let mut left = [START_PAUSES; 16];
        left[2] = 1;
        let (_, pauses) = clock_and_pauses(Some(5), Some(left), t0);
        assert_eq!(pauses.0, left);
    }
}
