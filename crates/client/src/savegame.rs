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
//! Not yet here: the game clock and the pause counts (`ClientState::clock_ms` and `pause_counts` stay `None`; nothing
//! keeps them yet), multiplayer saves (who may save and the slot choice are in `saves`, tested but unused), and the
//! main menu's Load Game button (the entry points are ready).

use bevy::prelude::*;
use tidewater_sim::setup::Setup;

use crate::frontend::{AppState, Flow, Step};
use crate::savefile::{self, Body, ClientState, Header, Kind, Reason};
use crate::saves::SaveDir;
use crate::{Cycling, MapCamera, Sim, jukebox::Jukebox, scene};

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
    let music = world.resource::<Jukebox>().music().current();
    let screen = {
        let mut q = world.query_filtered::<&Transform, With<MapCamera>>();
        q.single(world)
            .map_or((0, 0), |t| (t.translation.x as i32, -t.translation.y as i32))
    };
    let (game, total, memory, start_controllers) = {
        let f = world.resource::<Flow>();
        (
            f.game.unwrap_or(crate::campaign::GameKind::Custom),
            f.total,
            f.memory,
            f.start_controllers,
        )
    };
    let mut sim = world.get_resource_mut::<Sim>()?;
    // "Saving with the map revealed": the cheat is off just long enough to reset the remembered buildings.
    sim.world.prepare_save();
    let enhanced = sim.world.rules.enhanced;
    let header = Header {
        name: name.to_string(),
        saved_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
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
            clock_ms: None,
            pause_counts: None,
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
pub fn load_game(flow: &mut Flow, next: &mut NextState<AppState>, header: Header, body: Body) {
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
pub fn reseed() -> u32 {
    scene::now_secs()
}
