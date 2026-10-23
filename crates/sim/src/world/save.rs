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
//! groups" defaults to "the selection is empty and groups are kept only if they're part of the units' state". The
//! control groups are part of the world here, so they are stored and kept; the selected units are not kept (a choice
//! recorded here).
//!
//! **Afresh on load.** The generator is not stored; [`World::restore`] takes the seed to start it from (the caller
//! passes the time in seconds, as for a new game, `core/timing-and-rng.md` "Seeding"). The cheat flags are not stored;
//! a loaded game keeps the template's flags masked to the lasting ones ([`Flags::for_new_game`]), and leaves the
//! cheat-used mark as the template has it. A Classic save ([`Rules::enhanced`] false) drops the score counters.
//! What a load starts afresh in either mode (cycle counter, pause, countdowns, creation numbers) is
//! [`World::starts_afresh`]. The game clock and the pause counts are not in the simulation (the client keeps them), so
//! the client's save state carries them.

use serde::{Deserialize, Serialize};

use super::*;
use crate::training_ai::DOUBLED_GROUPS;

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
    next_serial: u32,
    rule_before_outcome: Option<crate::victory::Rule>,
    vision: Vision,
    victory: crate::victory::Victory,
    occupancy: [Vec<Option<UnitId>>; 2],
    ylist: Vec<UnitId>,
    runes: Vec<Option<crate::spells::Rune>>,
    stale: BTreeMap<u16, crate::caster_ai::Stale>,
    selections: [crate::select::PlayerSelection; PLAYERS],
    speed: u8,
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

    /// The world's part of a saved game: the stored fields, the terrain's changing part and the training group count
    /// surplus.
    pub fn save_state(&self) -> Vec<u8> {
        let mut bytes = self.core_state();
        bytes.extend(postcard::to_allocvec(&self.group_surplus).expect("encoding to memory"));
        bytes.extend(postcard::to_allocvec(&self.extras_saved()).expect("encoding to memory"));
        bytes.extend(postcard::to_allocvec(&self.army_surplus).expect("encoding to memory"));
        // The players left out of their own shared-vision sets (`game/diplomacy.md`, DP11); the sets and the
        // allied-victory settings themselves are in the stored `players`.
        bytes.push(self.vision_outsiders);
        bytes
    }

    /// The optional improvements and what they keep, the saved game's last part (`extras`).
    pub(crate) fn extras_saved(&self) -> crate::extras::Saved {
        crate::extras::Saved {
            extras: self.rules.extras,
            state: self.ext.clone(),
        }
    }

    /// The stored fields and the terrain's changing part: [`World::save_state`] without its trailing surplus.
    fn core_state(&self) -> Vec<u8> {
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

    /// A fingerprint of the whole simulation state, for snapshot tests that pin behaviour across refactors: FNV-1a
    /// over the saved state ([`World::save_state`], which covers every stored field including the private ones and
    /// the terrain's changing part), the sync hash ([`World::state_hash`]), the generator, the cheat state, the sound
    /// state and the spell counts. Every part is ordered (vectors, arrays and `BTreeMap`s), so the value doesn't
    /// depend on hash-map order or addresses. It follows the representation: renaming, adding or reordering a stored
    /// field changes it even when behaviour doesn't. The queues the client drains (`events`, `sounds.cues`) are
    /// left to the caller.
    pub fn fingerprint(&self) -> u64 {
        let fnv = |h: u64, bytes: &[u8]| {
            bytes
                .iter()
                .fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
        };
        let mut h = fnv(0xcbf2_9ce4_8422_2325, &self.core_state());
        // The surplus a load leaves counts only once there is one, so a game that was never loaded keeps its fingerprint
        // from before the surplus was stored.
        if self.group_surplus != [[0u32; DOUBLED_GROUPS]; SCRIPT_PLAYERS] {
            h = fnv(
                h,
                &postcard::to_allocvec(&self.group_surplus).expect("encoding to memory"),
            );
        }
        if self.army_surplus != [[0u32; 3]; SCRIPT_PLAYERS] {
            h = fnv(
                h,
                &postcard::to_allocvec(&self.army_surplus).expect("encoding to memory"),
            );
        }
        // The same for the optional improvements: they count once one is on.
        if !self.rules.extras.is_default() || !self.ext.is_empty() {
            h = fnv(
                h,
                &postcard::to_allocvec(&self.extras_saved()).expect("encoding to memory"),
            );
        }
        // The same for a player left out of its own shared-vision set.
        if self.vision_outsiders != 0 {
            h = fnv(h, &[self.vision_outsiders]);
        }
        h = fnv(h, &self.state_hash().to_le_bytes());
        h = fnv(h, &self.rng.state.to_le_bytes());
        h = fnv(
            h,
            format!("{:?}|{:?}|{:?}", self.cheats, self.sounds, self.casts).as_bytes(),
        );
        h
    }

    /// Puts a stored world over `self`, which must be the template the same map and set-up build. `seed` starts the
    /// random generator afresh. Cheats start as for a new game. A load starts the counters afresh ([`World::starts_afresh`])
    /// and a Classic save clears the score counters.
    pub fn restore(&mut self, bytes: &[u8], seed: u32) -> Result<(), RestoreError> {
        let ((saved, terrain, surplus), rest): ((Saved, TerrainOwned, [[u32; DOUBLED_GROUPS]; SCRIPT_PLAYERS]), _) =
            postcard::take_from_bytes(bytes).map_err(|e| RestoreError::Decode(e.to_string()))?;
        let (extras, rest): (crate::extras::Saved, _) =
            postcard::take_from_bytes(rest).map_err(|e| RestoreError::Decode(e.to_string()))?;
        let (army_surplus, rest): ([[u32; 3]; SCRIPT_PLAYERS], _) =
            postcard::take_from_bytes(rest).map_err(|e| RestoreError::Decode(e.to_string()))?;
        let [outsiders] = *rest else {
            return Err(RestoreError::Decode("the stored world ends wrongly".into()));
        };
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
        let kept = self.cheats.flags.for_new_game();
        // The cheat-used mark isn't saved and a load doesn't clear it: it stays as the running program left it
        // (`game/save-load.md`, "Cheat-used mark"; traced, `human1-human01-cheat-load-game-50`). The caller hands it
        // over in the template; the client carries it from the game before.
        let used = self.cheats.used;
        saved.put(self);
        self.group_surplus = surplus;
        self.vision_outsiders = outsiders;
        self.army_surplus = army_surplus;
        self.rules.extras = extras.extras;
        self.ext = extras.state;
        self.rng = Rng::new(seed);
        // Cheats: the flags aren't stored, so the effects the stored rules carry are cleared (`game/save-load.md`, "Cheats":
        // the instant-cheat outcome is not found again; traced, `human1-human01-unite-saveload-game-650`) and the flags
        // that carry across games ("Flags across games") are set again.
        self.rules.reveal_map = false;
        self.rules.build_speed_cheat = false;
        self.rules.invulnerable_cheat = false;
        self.rules.lumber_cheat = false;
        self.rules.all_spells_cheat = false;
        self.rules.instant_victory = false;
        self.rules.instant_defeat = false;
        self.rules.disable_victory = false;
        self.cheats = Default::default();
        self.cheats.used = used;
        self.start_cheats(kept);
        self.events.clear();
        self.loading = false;
        // The selection starts empty after a load; the control groups stay (spec open question "Selection and
        // groups").
        for sel in &mut self.selections {
            sel.places = Default::default();
        }
        self.starts_afresh();
        // A load runs the mission start again from the saved rule, after the generator is seeded (G24).
        self.mission_start();
        if !self.rules.enhanced {
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

    /// What a loaded game starts afresh, whatever was saved (`game/save-load.md`, "What a loaded game starts afresh";
    /// `game/lifecycle.md`, "How a game starts"):
    ///
    /// - the cycle counter is 0 (the start-up cycles end by setting it to 0 again, [`World::start_up`]);
    /// - the game is not paused (the pause isn't saved);
    /// - the victory countdown is 1, and an outcome a check had found but not yet shown is lost: the rule in force
    ///   before the check found it is back ([`World::rule_before_outcome`]), so a later check finds the outcome again
    ///   only if the game state still gives it;
    /// - the fog refresh countdown is 1, so the first start-up cycle refreshes the fog;
    /// - the 50-cycle pass counter is 1 again (it isn't saved), so a loaded game runs the passes in its first start-up
    ///   cycle whatever phase the save was in (`game/save-load.md` SL1 to SL3; `ai/attack-ai.md`, "Timing");
    /// - the footmen-to-submarines counts of each computer player double: the surplus becomes the saved surplus plus
    ///   the counts at the save (SL4 to SL6, SL13 to SL15); the worker and tanker counts are worked out afresh;
    /// - the land, sea and air army counts the script conditions read gain the live counts the same way (SL34 to
    ///   SL37; `World::army_surplus`);
    /// - each player's miner, lumberjack and repairer counts, building-in-progress counts and hall cooldown are 0,
    ///   while the units keep their jobs and harvest modes (SL22 to SL25);
    /// - every research request is clear and each computer player's done marks are made again, with the load's
    ///   building marks (SL9 to SL12);
    /// - the frozen and about-to-freeze marks of workers are cleared (static study, a labelled gap-fill);
    /// - every dead body's target is cleared, and no other unit's (SL28 to SL30), and so is its stored goal (SL33,
    ///   static study; the one stored goal we hold is [`Unit::goal_x`]);
    /// - every unit's creation number is its slot number plus one, and the next unit created gets the unit-slot count
    ///   plus 26: 1626 in Enhanced (SL7; the Classic value 626 is the spec's gap-fill).
    fn starts_afresh(&mut self) {
        self.tick = 0;
        self.paused = false;
        self.victory.countdown = 1;
        if matches!(self.victory.rule, Some(crate::victory::Rule::Outcome(_))) {
            self.victory.rule = self.rule_before_outcome;
        }
        self.rule_before_outcome = None;
        self.vision.countdown = 1;
        self.army.countdown = 1;
        for p in 0..self.ai.len() {
            if !self.ai_player(p as u8) {
                continue;
            }
            let g = self.ai_groups(p);
            let s = &mut self.group_surplus[p];
            for (s, n) in s.iter_mut().zip(g.doubled()) {
                *s += n;
            }
            let live = self.ai_live_counts(p);
            let a = &mut self.army_surplus[p];
            a[0] += live.land;
            a[1] += live.sea;
            a[2] += live.air;
        }
        // Research requests and done marks aren't kept: every request is clear and the marks are made again
        // (`game/save-load.md` SL9 to SL12).
        for p in 0..self.ai.len() {
            self.ai[p].research_requests = 0;
            self.ai[p].done = 0;
            // The job choice's counts start at 0 at every game start, a load included; the units keep their saved
            // jobs, so the first release of each wraps (`game/save-load.md` SL22 to SL25).
            let a = &mut self.ai[p];
            a.miners = 0;
            a.lumberjacks = 0;
            a.repairers = 0;
            a.in_progress = [0; 128];
            a.hall_cooldown = 0;
            self.mark_load(p);
        }
        for u in &mut self.units {
            u.serial = super::Serial(u.slot as u32 + 1);
            u.freeze = Default::default();
            // A dead body (type 0x69: a unit's remains or a ship wreck, any decay stage, any owner) has no target after
            // a load, whatever it had at the save; every other unit keeps its target, a dying one included
            // (`game/save-load.md` SL28 to SL30). During play a body still keeps its target.
            if u.kind == crate::corpses::DEAD_BODY {
                u.dying_target = None;
                // Its stored goal is (0, 0) too, whatever it was at the save (SL33; no rule reads a body's goal).
                u.goal_x = 0;
            }
        }
        self.next_serial = self.slots.count() as u32 + 26;
        // The owner lists are built again from scratch: every list is in descending slot order (`core/unit-slots.md`,
        // "Saved games").
        self.slots.rebuild_descending();
        // The vertical list is built again too, unit by unit in slot order (`orders/orders.md`, "Vertical order").
        self.rebuild_vertical();
    }
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod tests;
