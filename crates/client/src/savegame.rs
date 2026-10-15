//! Collecting a running game into a saved game and starting a game from one (spec `docs/specs/game/save-load.md`, "What a
//! saved game keeps" and "What a loaded game starts afresh"). The file layout is `savefile`, the folder and dialog rules
//! `saves`, the dialogs `savedlg`; the game menu (`gamemenu.rs`) calls the entry points here:
//!
//! - [`save_game`]: writes the running game to the save folder under a name (single player).
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

use std::time::Instant;

use bevy::prelude::*;
use tidewater_sim::setup::Setup;

use crate::frontend::{AppState, Flow, Step};
use crate::savefile::{self, Body, ClientState, Header, Kind, Reason};
use crate::saves::SaveDir;
use crate::states::Machine;
use crate::{Cycling, GameView, Sim, jukebox::Jukebox};

/// The save folder in use.
#[derive(Resource, Clone, Debug)]
pub struct SaveFolder(pub SaveDir);

/// How the running game was started, so a save can say how to build the same game again.
#[derive(Resource, Clone, Debug)]
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
#[derive(Resource)]
pub struct ResumeLook(pub Vec2);

/// The game clock (`ui/game-menu.md`, "Battle.net's quit button"): real time since this game's clock started at game
/// start, plus the time already played before a loaded game was saved; it stops when the game ends. Enhanced saves
/// keep it (`game/save-load.md`); a Classic save doesn't, and its game starts the clock at 0 (the spec's open-question
/// default, "Classic clock").
#[derive(Resource, Clone, Copy, Debug)]
pub struct GameClock {
    /// Milliseconds played before this start (a loaded game's saved clock).
    base_ms: u32,
    started: Instant,
    /// The reading at the game's end, once it has ended.
    stopped_ms: Option<u32>,
}

impl GameClock {
    pub fn start(base_ms: u32, now: Instant) -> Self {
        GameClock {
            base_ms,
            started: now,
            stopped_ms: None,
        }
    }

    /// The clock in milliseconds at `now`.
    pub fn ms(&self, now: Instant) -> u32 {
        self.stopped_ms.unwrap_or_else(|| {
            let run = now.saturating_duration_since(self.started).as_millis();
            self.base_ms.saturating_add(run.min(u32::MAX as u128) as u32)
        })
    }

    /// The game has ended: the clock keeps its reading from here on.
    pub fn stop(&mut self, now: Instant) {
        if self.stopped_ms.is_none() {
            self.stopped_ms = Some(self.ms(now));
        }
    }
}

/// The pauses each player starts a game with (`ui/game-menu.md`, "Pausing").
pub const START_PAUSES: u8 = 3;

/// Each player's pauses left. Only Enhanced multiplayer games use them (`ui/game-menu.md`, "Pausing"), and this client
/// has none, so nothing spends them yet: the counts are kept so Enhanced saves carry them (`game/save-load.md`).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
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
    now: Instant,
) -> (GameClock, PauseCounts) {
    let base = clock_ms.unwrap_or(0);
    let pauses = pause_counts.map_or_else(PauseCounts::default, PauseCounts);
    (GameClock::start(base, now), pauses)
}

/// Writes the running game as `name` (the typed name; `stem` is the file name without the extension).
pub fn save_game(world: &mut World, stem: &str, name: &str) -> Result<(), Reason> {
    let folder = world.resource::<SaveFolder>().0.clone();
    let bytes = capture(world, name).ok_or(Reason::WriteFailed)?;
    folder.write(stem, Kind::Single, &bytes)
}

/// The bytes of the running game as a saved game, or `None` outside a game.
pub fn capture(world: &mut World, name: &str) -> Option<Vec<u8>> {
    let origin = world.get_resource::<GameOrigin>()?.clone();
    let cycles_on = world.get_resource::<Cycling>()?.state.cycler().on_set();
    let clock_ms = world.get_resource::<GameClock>().map(|c| c.ms(Instant::now()));
    let pause_counts = world.get_resource::<PauseCounts>().map(|p| p.0);
    let music = world.resource::<Jukebox>().music().current();
    let screen = world
        .get_resource::<GameView>()
        .map_or((0, 0), |v| (v.centre.0 as i32, v.centre.1 as i32));
    let (game, total, memory, start_controllers) = {
        let f = world.resource::<Flow>();
        (
            f.game.unwrap_or(crate::campaign::GameKind::Custom),
            f.total,
            f.memory,
            f.start_controllers,
        )
    };
    let saved_at = world.resource::<crate::WallClock>().secs();
    let mut sim = world.get_resource_mut::<Sim>()?;
    // "Saving with the map revealed": the cheat is off just long enough to reset the remembered buildings.
    sim.world.prepare_save();
    let enhanced = sim.world.rules.enhanced;
    let header = Header {
        name: name.to_string(),
        saved_at,
        enhanced,
        kind: Kind::Single,
        map: origin.map,
        setup: origin.setup,
        seed: origin.seed,
        race: sim.world.start.race_setting,
    };
    let body = Body {
        world: sim.world.save_state(),
        client: ClientState {
            cycles_on,
            music,
            screen,
            game,
            total,
            memory,
            start_controllers,
            clock_ms,
            pause_counts,
        },
    }
    .for_rules(enhanced);
    Some(savefile::encode(&header, &body))
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
pub fn reseed(wall: &crate::WallClock) -> u32 {
    wall.secs() as u32
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn the_clock_runs_on_from_the_saved_time_and_stops_at_the_end() {
        let t0 = Instant::now();
        let (mut clock, _) = clock_and_pauses(Some(61_000), None, t0);
        assert_eq!(clock.ms(t0 + Duration::from_millis(1500)), 62_500);
        clock.stop(t0 + Duration::from_secs(4));
        assert_eq!(clock.ms(t0 + Duration::from_secs(60)), 65_000);
        clock.stop(t0 + Duration::from_secs(90));
        assert_eq!(clock.ms(t0 + Duration::from_secs(90)), 65_000);
    }

    #[test]
    fn a_new_game_or_a_classic_save_starts_the_clock_at_0_with_full_pauses() {
        let t0 = Instant::now();
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
