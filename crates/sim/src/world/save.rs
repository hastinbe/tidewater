//! Saving the simulation's state and putting it back (`docs/specs/game/save-load.md`, "What a saved game keeps" and
//! "What a loaded game starts afresh").
//!
//! The spec describes behaviour, not a file layout, so the encoding is our own: [`postcard`] (a compact, stable serde
//! format, <https://docs.rs/postcard>) over a struct of the fields below, one field after the other. The container
//! (magic, version, kind, name, client state) is in the client's `savefile` module; this module only produces and reads
//! the world's part.
//!
//! **What is stored and what isn't.** The data a map's start rebuilds the same way every time is not stored: the unit
//! table, scripts, movement strides, the default and map upgrade data and the computer scripts' file. A load builds a
//! *template* world from the same map and set-up ([`World::from_pud`] then `start_ai`), and [`World::restore`] puts the
//! stored fields over it. Not stored either, because they aren't game state: the message and sound queues, spell
//! counts, the start-view record. The selection and control groups are stored: the spec's open question "Selection and
//! groups" defaults to "the selection is empty and groups are kept only if they're part of the units' state", and the
//! synced selection and groups are part of the world here, so they are kept as they are (a choice recorded here).
//!
//! **Afresh on load.** The generator is not stored; [`World::restore`] takes the seed to start it from (the caller
//! passes the time in seconds, as for a new game, `core/timing-and-rng.md` "Seeding"). The cheat flags are not stored;
//! a loaded game keeps the template's flags masked to the lasting ones ([`crate::cheats::Cheats::across_games`]). A
//! Classic save ([`Rules::enhanced`] false) drops the cycle count, which restarts at 0, and the score counters.
//! The game clock and the pause counts are not in the simulation (the client keeps them), so the client's save state
//! carries them.

use serde::{Deserialize, Serialize};

use super::*;

/// Why a stored world couldn't be read back.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RestoreError {
    #[error("the stored world can't be read: {0}")]
    Decode(String),
    #[error("the stored world is for a different map")]
    WrongMap,
}

macro_rules! saved {
    ($($f:ident : $t:ty),* $(,)?) => {
        #[derive(Serialize)]
        struct SavedRef<'a> { $($f: &'a $t),* }
        #[derive(Deserialize)]
        struct Saved { $($f: $t),* }
        impl World {
            fn saved_ref(&self) -> SavedRef<'_> { SavedRef { $($f: &self.$f),* } }
        }
        impl Saved {
            fn put(self, w: &mut World) { $(w.$f = self.$f;)* }
        }
    };
}

saved! {
    tick: u32,
    rules: Rules,
    players: [Player; PLAYERS],
    units: Vec<Unit>,
    missiles: Vec<Option<Missile>>,
    slots: Slots,
    slot_of: BTreeMap<UnitId, u16>,
    ai: [AiPlayer; SCRIPT_PLAYERS],
    army: crate::attack_ai::Army,
    next_id: u32,
    vision: Vision,
    victory: crate::victory::Victory,
    occupancy: [Vec<Option<UnitId>>; 2],
    ylist: Vec<UnitId>,
    runes: Vec<Option<crate::spells::Rune>>,
    stale: BTreeMap<u16, crate::caster_ai::Stale>,
    selections: [crate::select::PlayerSelection; PLAYERS],
    speed: u8,
    paused: bool,
    remembered: crate::minimap::Memory,
}

/// The terrain's changing part; the tile-change tables come from the template.
#[derive(Serialize)]
struct TerrainRef<'a> {
    size: (u16, u16),
    words: &'a [u16],
    tiles: &'a [u16],
    regions: &'a [u16],
    wall_damage: &'a [u8],
}

#[derive(Deserialize)]
struct TerrainOwned {
    size: (u16, u16),
    words: Vec<u16>,
    tiles: Vec<u16>,
    regions: Vec<u16>,
    wall_damage: Vec<u8>,
}

impl World {
    /// Gets the world ready to be written: with the reveal-map cheat on, it is switched off just long enough to reset
    /// the remembered look of every building that stands wholly on unexplored ground ("Saving with the map revealed",
    /// `vision/fog.md` open questions), then switched back on. The reset stays in the running game.
    pub fn prepare_save(&mut self) {
        if !self.rules.reveal_map {
            return;
        }
        self.rules.reveal_map = false;
        let vision = &self.vision;
        for u in self.units.iter_mut().filter(|u| u.building && u.look.0.is_some()) {
            let (x, y) = u.tile;
            let all_dark =
                (0..u.size.1 as i32).all(|dy| (0..u.size.0 as i32).all(|dx| vision.unexplored(x + dx, y + dy)));
            if all_dark {
                u.look = Default::default();
            }
        }
        self.rules.reveal_map = true;
    }

    /// The world's part of a saved game.
    pub fn save_state(&self) -> Vec<u8> {
        let t = &self.terrain;
        let terrain = TerrainRef {
            size: (t.width, t.height),
            words: &t.words,
            tiles: &t.tiles,
            regions: &t.regions,
            wall_damage: &t.wall_damage,
        };
        postcard::to_allocvec(&(self.saved_ref(), terrain)).expect("encoding to memory")
    }

    /// Puts a stored world over `self`, which must be the template the same map and set-up build. `seed` starts the
    /// random generator afresh. Cheats start as for a new game. A Classic save restarts the cycle count at 0 and
    /// clears the score counters.
    pub fn restore(&mut self, bytes: &[u8], seed: u32) -> Result<(), RestoreError> {
        let (saved, terrain): (Saved, TerrainOwned) =
            postcard::from_bytes(bytes).map_err(|e| RestoreError::Decode(e.to_string()))?;
        let t = &mut self.terrain;
        let n = t.width as usize * t.height as usize;
        if terrain.size != (t.width, t.height)
            || [
                terrain.words.len(),
                terrain.tiles.len(),
                terrain.regions.len(),
                terrain.wall_damage.len(),
            ] != [n; 4]
        {
            return Err(RestoreError::WrongMap);
        }
        t.words = terrain.words;
        t.tiles = terrain.tiles;
        t.regions = terrain.regions;
        t.wall_damage = terrain.wall_damage;
        // The whole map is drawn again after a load.
        t.redraw = (0..t.height as i32)
            .flat_map(|y| (0..t.width as i32).map(move |x| (x, y)))
            .collect();
        let template = self.rules.clone();
        let kept = self.cheats.flags.for_new_game();
        saved.put(self);
        self.rng = Rng::new(seed);
        // Cheats: the flags aren't stored, so the effects the stored rules carry are cleared and the flags that carry
        // across games ("Flags across games") are set again.
        self.rules.reveal_map = template.reveal_map;
        self.rules.build_speed_cheat = template.build_speed_cheat;
        self.rules.invulnerable_cheat = template.invulnerable_cheat;
        self.rules.lumber_cheat = template.lumber_cheat;
        self.rules.all_spells_cheat = template.all_spells_cheat;
        self.rules.instant_victory = template.instant_victory;
        self.rules.instant_defeat = template.instant_defeat;
        self.rules.disable_victory = template.disable_victory;
        self.cheats = Default::default();
        self.start_cheats(kept);
        self.events.clear();
        self.loading = false;
        if !self.rules.enhanced {
            self.tick = 0;
            for p in &mut self.players {
                p.kills = 0;
                p.razings = 0;
                p.score = 0;
                p.units_lost = 0;
                p.buildings_lost = 0;
                p.gathered = [0; 3];
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
