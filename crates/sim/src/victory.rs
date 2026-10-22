//! Victory and defeat: the check countdown, the standard, multiplayer and mission rules, and the mission-start
//! changes. Spec: `docs/specs/game/victory.md`. Unit type numbers are those of the public PUD specification
//! (appendix A).
//!
//! Every rule is evaluated for the local player ([`Vision::local`](crate::vision::Vision)). Counts are taken by
//! scanning the unit table at check time, which gives the same numbers as the game's live counts.
//!
//! Our readings where the spec leaves room:
//! - In the mission conditions, "defeat when / unless" ends the check with defeat; only "victory when" conditions
//!   and the destroy-all check replace an earlier result. The two exceptions the spec spells out (mode L when Grom
//!   doesn't exist, and mode U's rescue test) are kept.
//! - "No result while ..." ends the check keeping the result so far, so the standard part's defeat still stands.
//! - Rescue itself (joining, the circle of power, the rescued count going up) is in `rescue.rs`. The rescuable
//!   counts are [`World::rescuable_counts`].
//! - Modes W and Z (flagged open question): the literal behaviour. Making a hero rescuable that has no mark lowers
//!   L's rescued count by 1, wrapping at 16 bits, so a count of 0 becomes 65535 and the rescue test gives victory at
//!   the next check.
//! - The multiplayer rule treats only human and computer slots as players; rescue and passive players are neither in
//!   nor out. The network game is our multiplayer game.
//! - Demo playback isn't implemented; [`Victory::for_demo`] is the countdown it will start with. It runs with the fog
//!   option on and the reveal-map cheat (`vision/fog.md`, F47), so it needs no fog rule of its own.

use tidewater_formats::udta::flags;

use crate::ai::controller;
use crate::economy::Event;
use crate::world::{Order, World};

/// The countdown's reload value; the check runs every 201 cycles ("When the check runs").
pub const RELOAD: u32 = 200;
/// Gold a player without units needs to stay in the game.
pub const SURVIVAL_GOLD: u32 = 400;
/// The hero bit in rescue counts.
pub const HERO_RESCUE: u32 = 0x1000;

/// A player's result.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    Victory,
    Defeat,
    /// The network result every used slot gets when a game ends with nobody having a result.
    GameEnded,
}

/// The rule the check applies.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rule {
    /// "The standard rule", with its two options; `rescue` holds the required count.
    Standard {
        destroy_all: bool,
        rescue: Option<u32>,
    },
    /// A lettered mission mode (A is 0, AE is 30) and its required count.
    Mission {
        mode: u8,
        required: u32,
    },
    Multiplayer,
    /// A single-player outcome found at the last check; it happens at the next one.
    Outcome(Outcome),
}

const DESTROY_ALL: Rule = Rule::Standard {
    destroy_all: true,
    rescue: None,
};

fn rescue_rule(required: u32) -> Rule {
    Rule::Standard {
        destroy_all: false,
        rescue: Some(required),
    }
}

/// The check's state.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Victory {
    pub countdown: u32,
    /// `None` until a campaign mission starts: the custom-scenario or multiplayer rule, by [`crate::Rules`].
    pub rule: Option<Rule>,
    /// The campaign mission table entry being played, set by [`World::start_campaign`]; `None` in a custom scenario
    /// or a network game. The damage spec's Dark Portal case reads it.
    pub campaign: Option<u8>,
    /// The local player has an outcome; no more checks run.
    pub over: bool,
    /// Mode K's "circle's region": the region value at the last circle of power the map lists, remembered when the
    /// map's units are created (setup spec, "Creating the map's units").
    pub circle_region: Option<u16>,
}

impl Default for Victory {
    fn default() -> Self {
        Victory {
            countdown: 1,
            rule: None,
            campaign: None,
            over: false,
            circle_region: None,
        }
    }
}

/// The countdown demo playback starts with, so its first check (which ends the demo) is the 1,401st cycle, the two
/// start-up cycles included (`game/victory.md`, "When the check runs", X4).
pub const DEMO_COUNTDOWN: u32 = 1400;

impl Victory {
    /// The state a demo playback starts with: [`DEMO_COUNTDOWN`] instead of 1.
    pub fn for_demo() -> Self {
        Victory {
            countdown: DEMO_COUNTDOWN,
            ..Default::default()
        }
    }
}

/// Lettered mission modes.
pub mod mode {
    pub const A: u8 = 0;
    pub const B: u8 = 1;
    pub const C: u8 = 2;
    pub const D: u8 = 3;
    pub const E: u8 = 4;
    pub const F: u8 = 5;
    pub const G: u8 = 6;
    pub const H: u8 = 7;
    pub const I: u8 = 8;
    pub const J: u8 = 9;
    pub const K: u8 = 10;
    pub const L: u8 = 11;
    pub const M: u8 = 12;
    pub const N: u8 = 13;
    pub const O: u8 = 14;
    pub const P: u8 = 15;
    pub const Q: u8 = 16;
    pub const R: u8 = 17;
    pub const S: u8 = 18;
    pub const T: u8 = 19;
    pub const U: u8 = 20;
    pub const V: u8 = 21;
    pub const W: u8 = 22;
    pub const X: u8 = 23;
    pub const Y: u8 = 24;
    pub const Z: u8 = 25;
    pub const AA: u8 = 26;
    pub const AB: u8 = 27;
    pub const AC: u8 = 28;
    pub const AD: u8 = 29;
    pub const AE: u8 = 30;
}

/// PUD unit types and type groups the rules name.
mod t {
    pub const PEASANT: u8 = 0x02;
    pub const KNIGHT: u8 = 0x06;
    pub const MAGE: u8 = 0x0a;
    pub const PALADIN: u8 = 0x0c;
    pub const ATTACK_PEASANT: u8 = 0x10;
    pub const ALLERIA: u8 = 0x14;
    pub const TERON: u8 = 0x15;
    pub const KURDAN: u8 = 0x16;
    pub const DENTARG: u8 = 0x17;
    pub const KHADGAR: u8 = 0x18;
    pub const GROM: u8 = 0x19;
    pub const DEATHWING: u8 = 0x23;
    pub const TURALYON: u8 = 0x2c;
    pub const DANATH: u8 = 0x2e;
    pub const KHORGATH: u8 = 0x2f;
    pub const LOTHAR: u8 = 0x32;
    pub const ORC_SHIPYARD: u8 = 0x49;
    pub const FORTRESS: u8 = 0x5b;
    pub const PORTAL: &[u8] = &[0x65];
    pub const RUNESTONE: &[u8] = &[0x66];
    pub const DAEMON: &[u8] = &[0x38];
    pub const BARRACKS: &[u8] = &[0x3c, 0x3d];
    pub const FARMS: &[u8] = &[0x3a, 0x3b];
    pub const PLATFORMS: &[u8] = &[0x56, 0x57];
    pub const REFINERIES: &[u8] = &[0x54, 0x55];
    pub const SHIPYARDS: &[u8] = &[0x48, 0x49];
    pub const CASTLES: &[u8] = &[0x5a, 0x5b];
    pub const KEEPS_UP: &[u8] = &[0x58, 0x59, 0x5a, 0x5b];
    pub const AVIARIES: &[u8] = &[0x46, 0x47];
    /// Mages, death knights, Teron Gorefiend and Khadgar: one type group.
    pub const MAGES: &[u8] = &[0x0a, 0x0b, 0x15, 0x18];
    /// Gryphon riders, dragons, Kurdan and Deathwing.
    pub const FLYERS: &[u8] = &[0x2a, 0x2b, 0x16, 0x23];
    /// Mode G's naval targets: transports, oil platforms, shipyards.
    pub const G_NAVY: &[u8] = &[0x1c, 0x1d, 0x56, 0x57, 0x48, 0x49];
    /// Mode P's: shipyards, tankers, destroyers, battleships and juggernaughts, submarines and turtles.
    pub const P_NAVY: &[u8] = &[0x48, 0x49, 0x1a, 0x1b, 0x1e, 0x1f, 0x20, 0x21, 0x26, 0x27];
    /// Units that aren't "real": flying machines and zeppelins, transports, tankers.
    pub const NOT_REAL: &[u8] = &[0x28, 0x29, 0x1c, 0x1d, 0x1a, 0x1b];
    /// Production buildings: barracks, shipyards, halls and their upgrades, guard and cannon towers.
    pub const PRODUCTION: &[u8] = &[
        0x3c, 0x3d, 0x48, 0x49, 0x4a, 0x4b, 0x58, 0x59, 0x5a, 0x5b, 0x60, 0x61, 0x62, 0x63,
    ];
}

/// The mission table entry of campaign mission `n` (from 1): human and orc missions interleave, and *Beyond the Dark
/// Portal* starts at 28 ("Mission rules").
pub fn mission_entry(expansion: bool, orc: bool, n: u8) -> u8 {
    2 * n.saturating_sub(1) + orc as u8 + if expansion { 28 } else { 0 }
}

/// The rule of mission table entry `entry` (0 to 51).
pub fn mission_rule(entry: u8) -> Rule {
    use mode::*;
    let (mode, required) = match entry {
        0 | 1 => (A, 0),
        2 => return rescue_rule(1),
        3 | 11 | 16 => return rescue_rule(HERO_RESCUE + 1),
        4 | 5 => (B, 0),
        12 => (C, 0),
        14 => (D, 0),
        15 => (J, 0),
        17 => (K, 0),
        18 => (E, 4),
        20 => (F, 0),
        22 => (G, 0),
        24 => (H, 0),
        26 => (I, 0),
        28 => (U, 3),
        29 => (L, 0),
        30 => (V, 0),
        31 => (M, 0),
        32 => (W, 1),
        34 => (X, 0),
        35 | 39 => (N, 0),
        36 => (Y, 0),
        37 => (O, 0),
        38 => (Z, 2),
        40 => (AA, 0),
        41 => (P, 0),
        44 => (AB, 0),
        45 => (Q, 0),
        46 => (AC, 0),
        47 => (R, 0),
        48 => (AD, 0),
        49 => (S, 0),
        50 => (AE, 0),
        51 => (T, 0),
        // 6-10, 13, 19, 21, 23, 25, 27, 33, 42, 43
        _ => return DESTROY_ALL,
    };
    Rule::Mission { mode, required }
}

/// Players 0 to 7.
fn players_0_7() -> std::ops::Range<usize> {
    0..8
}

impl World {
    /// Phase 8: the countdown, and the check when it runs out.
    pub(crate) fn victory_phase(&mut self) {
        if self.victory.over || (self.rules.disable_victory && !self.rules.multiplayer) {
            return;
        }
        if self.victory.countdown > 0 {
            self.victory.countdown -= 1;
            return;
        }
        self.victory.countdown = RELOAD;
        self.check();
    }

    /// Starts campaign mission table entry `entry`: sets its rule and makes its mission-start changes.
    pub fn start_campaign(&mut self, entry: u8) {
        let rule = mission_rule(entry);
        self.victory.rule = Some(rule);
        self.victory.campaign = Some(entry);
        self.mission_start();
    }

    /// The mission start's changes for the rule in force (`game/victory.md`, "Mission start"). A new campaign mission
    /// runs it once through [`World::start_campaign`]; a load runs it again from the saved rule (G24, `game/save-load.md`).
    /// Modes E and H switch the rule when they run, so a restored game no longer holds their mode and they don't repeat.
    pub(crate) fn mission_start(&mut self) {
        use mode::*;
        let Some(Rule::Mission { mode, required }) = self.victory.rule else {
            return;
        };
        let l = self.vision.local;
        let know = |w: &mut World, p: u8, bits: &[u32]| {
            for b in bits {
                w.players[p as usize].spells_known |= 1 << b;
            }
        };
        const AC_SPELLS: &[u32] = &[0, 1, 3, 4, 5, 6, 7, 8, 9];
        match mode {
            D | E => {
                let peasants: Vec<usize> = (0..self.units.len())
                    .filter(|&i| self.units[i].owner == 4 && self.units[i].kind == t::PEASANT)
                    .collect();
                self.convert(4, t::PEASANT, t::ATTACK_PEASANT);
                if mode == D {
                    for i in peasants {
                        if self.units[i].kind == t::ATTACK_PEASANT {
                            self.units[i].rescuable = false;
                        }
                    }
                    if l < 8 {
                        self.players[l as usize].allies &= !(1 << 4);
                        self.players[4].allies &= !(1 << l);
                    }
                } else {
                    self.victory.rule = Some(rescue_rule(required));
                }
            }
            H => {
                for u in &mut self.units {
                    if u.owner == 3 && u.kind == t::LOTHAR && !u.dying && !u.removing {
                        u.rescuable = false;
                    }
                }
                self.victory.rule = Some(DESTROY_ALL);
            }
            M => know(self, l, &[10, 11]),
            Q => {
                know(self, 1, &[1, 3, 20]);
                self.convert(1, t::KNIGHT, t::PALADIN);
            }
            T => {
                for u in &mut self.units {
                    if u.owner == 15 && t::PORTAL.contains(&u.kind) && !u.dying && !u.removing {
                        u.rescuable = true;
                    }
                }
                know(self, l, &[10, 11, 13, 14, 15, 16, 17, 18, 19]);
            }
            U => know(self, l, &[1, 3]),
            W | Z => know(self, l, &[0, 1, 3]),
            AA => {
                know(self, l, &[1, 3, 4, 5, 6, 7, 9, 20]);
                self.convert(l, t::KNIGHT, t::PALADIN);
                self.convert(3, t::KNIGHT, t::PALADIN);
            }
            AB => {
                for u in &mut self.units {
                    if (u.owner as usize) < 8 && t::RUNESTONE.contains(&u.kind) {
                        u.hp = 400;
                    }
                }
            }
            AC => know(self, l, AC_SPELLS),
            AD => {
                know(self, l, AC_SPELLS);
                self.convert(1, t::KNIGHT, t::PALADIN);
                self.convert(0, t::KNIGHT, t::PALADIN);
            }
            AE => know(self, l, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
            _ => {}
        }
    }

    /// The required rescue count of the current rule (0 without one).
    pub(crate) fn required(&self) -> u32 {
        match self.victory.rule {
            Some(Rule::Standard { rescue: Some(r), .. }) => r,
            Some(Rule::Mission { required, .. }) => required,
            _ => 0,
        }
    }

    fn check(&mut self) {
        // The check is made for the local player alone (it finishes the game for them and knocks the others out), so
        // in a network game each machine reaches a different state once a player is out.
        // Network games: open question (see docs/design/multiplayer-determinism.md)
        let l = self.vision.local as usize;
        let rule = self.victory.rule.unwrap_or(if self.rules.multiplayer {
            Rule::Multiplayer
        } else {
            DESTROY_ALL
        });
        let found = match rule {
            Rule::Outcome(o) => return self.finish(l, o),
            Rule::Multiplayer => return self.multiplayer(l),
            Rule::Standard { destroy_all, rescue } => self.standard(l, destroy_all, rescue),
            Rule::Mission { mode, required } => self.mission(l, mode, required),
        };
        if let Some(o) = found {
            self.rule_before_outcome = self.victory.rule;
            self.victory.rule = Some(Rule::Outcome(o));
        }
    }

    /// A player's Surrender or Quit (draw) from the game menu (`ui/game-menu.md`, "Ending the scenario"): the player is
    /// defeated and the game is over at once, in the cycle the command runs in. A slot that is not a player is ignored
    /// (a remote peer sent it). It pushes no [`Event::GameOver`]: the client's outcome dialog follows from the result.
    pub(crate) fn surrender_command(&mut self, player: u8) {
        let Some(p) = self.players.get_mut(player as usize) else {
            return;
        };
        p.result = Some(Outcome::Defeat);
        self.victory.over = true;
    }

    fn finish(&mut self, p: usize, o: Outcome) {
        self.players[p].result = Some(o);
        self.events.push(Event::GameOver {
            player: p as u8,
            outcome: o,
        });
        self.victory.over = true;
    }

    fn cheat(&self) -> Option<Outcome> {
        if self.rules.instant_victory {
            Some(Outcome::Victory)
        } else if self.rules.instant_defeat {
            Some(Outcome::Defeat)
        } else {
            None
        }
    }

    /// Living units of player `p` of the given kinds: units, and complete buildings.
    fn count(&self, p: usize, kinds: &[u8]) -> usize {
        self.units
            .iter()
            .filter(|u| {
                u.owner as usize == p && !u.dying && !u.removing && kinds.contains(&u.kind) && u.construction.is_none()
            })
            .count()
    }

    fn has(&self, p: usize, kinds: &[u8]) -> bool {
        self.count(p, kinds) > 0
    }

    fn others_have(&self, l: usize, kinds: &[u8]) -> bool {
        players_0_7().filter(|&q| q != l).any(|q| self.has(q, kinds))
    }

    /// Non-building units, and of those the real ones ("Terms").
    fn units_of(&self, p: usize) -> (usize, usize) {
        let mut all = 0;
        let mut real = 0;
        for u in &self.units {
            if u.owner as usize == p && !u.building && !u.dying && !u.removing {
                all += 1;
                if !t::NOT_REAL.contains(&u.kind) {
                    real += 1;
                }
            }
        }
        (all, real)
    }

    /// All buildings, complete or not.
    fn buildings_of(&self, p: usize) -> usize {
        self.units
            .iter()
            .filter(|u| u.owner as usize == p && u.building && !u.dying && !u.removing)
            .count()
    }

    /// At least one real unit or one building.
    fn present(&self, p: usize) -> bool {
        self.units_of(p).1 > 0 || self.buildings_of(p) > 0
    }

    /// The multiplayer sense of "in": present, and still connected if a human player.
    fn is_in(&self, p: usize) -> bool {
        self.present(p) && (self.players[p].controller != controller::HUMAN || !self.players[p].disconnected)
    }

    fn wiped_out(&self, p: usize) -> bool {
        self.buildings_of(p) == 0 && self.units_of(p).1 == 0
    }

    /// The first `n` units of the given kinds in slot order, whoever owns them: whether `n` were found and all alive.
    fn all_alive(&self, kinds: &[u8], n: usize) -> bool {
        let found: Vec<bool> = self
            .units
            .iter()
            .filter(|u| kinds.contains(&u.kind))
            .take(n)
            .map(|u| !u.dying && !u.removing)
            .collect();
        found.len() == n && found.iter().all(|&a| a)
    }

    /// Modes W and Z: makes the first unit of `kind` found rescuable. A hero that didn't have the mark also lowers
    /// L's rescued count by 1, unsigned 16-bit.
    fn make_rescuable(&mut self, l: usize, kind: u8) {
        let hero = self.stats_of(kind).flags & flags::HERO != 0;
        if let Some(u) = self.units.iter_mut().find(|u| u.kind == kind)
            && !u.rescuable
        {
            u.rescuable = true;
            if hero {
                let p = &mut self.players[l];
                p.rescued = p.rescued.wrapping_sub(1);
            }
        }
    }

    /// "The standard rule".
    fn standard(&self, l: usize, destroy_all: bool, rescue: Option<u32>) -> Option<Outcome> {
        if let Some(o) = self.cheat() {
            return Some(o);
        }
        let p = &self.players[l];
        if p.in_training == 0 && p.under_construction == 0 {
            if self.units_of(l).0 == 0 {
                if !self.has(l, t::PRODUCTION) || p.gold < SURVIVAL_GOLD {
                    return Some(Outcome::Defeat);
                }
            } else if destroy_all {
                let won = players_0_7().filter(|&q| q != l).all(|q| self.wiped_out(q));
                return won.then_some(Outcome::Victory);
            }
        }
        self.rescue_test(l, rescue?)
    }

    /// Step 3 of the standard rule.
    fn rescue_test(&self, l: usize, required: u32) -> Option<Outcome> {
        let got = self.players[l].rescued as u32;
        if got >= required {
            return Some(Outcome::Victory);
        }
        let (left, heroes) = self.rescuable_counts();
        let hero = if heroes > 0 || got & HERO_RESCUE != 0 {
            HERO_RESCUE
        } else {
            0
        };
        let possible = (got & !HERO_RESCUE) + left + hero;
        (possible < required).then_some(Outcome::Defeat)
    }

    /// Mode K: L owns a complete orc shipyard and a complete fortress that each stand at least partly in the region
    /// of the circle of power.
    fn in_circle_region(&self, l: usize) -> bool {
        let Some(region) = self.victory.circle_region else {
            return false;
        };
        let region = Some(region);
        let touches = |kind: u8| {
            self.units.iter().any(|u| {
                u.owner as usize == l
                    && u.kind == kind
                    && !u.dying
                    && u.construction.is_none()
                    && (0..u.size.1 as i32).any(|dy| {
                        (0..u.size.0 as i32).any(|dx| self.terrain.region(u.tile.0 + dx, u.tile.1 + dy) == region)
                    })
            })
        };
        touches(t::ORC_SHIPYARD) && touches(t::FORTRESS)
    }

    /// The lettered modes ("Mission rules").
    fn mission(&mut self, l: usize, m: u8, required: u32) -> Option<Outcome> {
        use Outcome::{Defeat, Victory};
        use mode::*;
        let win = |c: bool, r: &mut Option<Outcome>| {
            if c {
                *r = Some(Victory);
            }
        };
        let mut r = match m {
            D | M | N => self.standard(l, true, None),
            AD => None,
            _ => self.standard(l, false, None),
        };
        match m {
            A => win(self.has(l, t::BARRACKS) && self.count(l, t::FARMS) >= 4, &mut r),
            B => win(self.count(l, t::PLATFORMS) >= 4, &mut r),
            C => win(!self.others_have(l, t::REFINERIES), &mut r),
            D => {
                if r == Some(Victory) && !self.has(l, t::CASTLES) {
                    r = None;
                }
            }
            F => {
                if self.players[l].converted > 0 {
                    self.victory.rule = Some(DESTROY_ALL);
                }
            }
            G => win(!self.others_have(l, t::G_NAVY), &mut r),
            I => win(!self.has(15, t::PORTAL), &mut r),
            J => {
                if !players_0_7().any(|q| self.has(q, t::RUNESTONE)) {
                    return Some(Defeat);
                }
                win(!self.others_have(l, t::CASTLES) && self.has(l, t::RUNESTONE), &mut r);
            }
            K => win(
                self.has(l, t::CASTLES) && self.has(l, t::SHIPYARDS) && self.in_circle_region(l),
                &mut r,
            ),
            L => {
                if !self.units.iter().any(|u| u.kind == t::GROM) {
                    r = Some(Defeat);
                } else if !self.all_alive(&[t::GROM], 1) {
                    return Some(Defeat);
                }
                win(!self.has(6, t::MAGES) && self.buildings_of(6) == 0, &mut r);
            }
            M => {
                if !self.all_alive(&[t::KHORGATH, t::DENTARG], 2) {
                    return Some(Defeat);
                }
            }
            N => {
                if !self.all_alive(&[t::TERON], 1) {
                    return Some(Defeat);
                }
            }
            O => {
                if !self.has(2, t::AVIARIES) && !self.has(l, t::AVIARIES) {
                    return Some(Defeat);
                }
                win(self.has(l, t::AVIARIES) && !self.has(2, t::FLYERS), &mut r);
            }
            P => win(
                self.count(l, t::SHIPYARDS) >= 5 && !self.others_have(l, t::P_NAVY),
                &mut r,
            ),
            Q => win(!self.has(1, t::DAEMON), &mut r),
            R => {
                if r == Some(Defeat) {
                    return r;
                }
                let mage = self
                    .units
                    .iter()
                    .find(|u| u.kind == t::MAGE && (u.owner == 4 || u.owner as usize == l));
                let Some(mage) = mage.filter(|u| !u.dying && !u.removing) else {
                    return Some(Defeat);
                };
                let rescued = !mage.rescuable;
                if self.present(6) || self.present(0) || self.present(2) {
                    return r;
                }
                win(rescued, &mut r);
            }
            S => win(self.wiped_out(3), &mut r),
            T => {
                if r.is_some() {
                    return r;
                }
                if !self.has(15, t::PORTAL) && !self.has(l, t::PORTAL) {
                    return Some(Defeat);
                }
                r = self.standard(l, true, None);
                if r == Some(Victory) && !self.has(l, t::PORTAL) {
                    r = None;
                }
            }
            U => {
                if !self.all_alive(&[t::DANATH, t::TURALYON, t::ALLERIA], 3) {
                    r = Some(Defeat);
                }
                if let Some(o) = self.rescue_test(l, required) {
                    r = Some(o);
                }
            }
            V => {
                if !self.all_alive(&[t::DANATH], 1) {
                    return Some(Defeat);
                }
                r = self.standard(l, true, None);
            }
            W => {
                if !self.all_alive(&[t::TURALYON], 1) {
                    return Some(Defeat);
                }
                if !players_0_7().any(|q| q != l && q != 4 && self.has(q, t::KEEPS_UP)) {
                    self.make_rescuable(l, t::TURALYON);
                    self.victory.rule = Some(rescue_rule(required));
                }
            }
            X => {
                if players_0_7().any(|q| q != l && q != 7 && self.is_in(q)) {
                    return r;
                }
                r = self.has(l, t::CASTLES).then_some(Victory);
            }
            Y => win(
                self.count(l, t::SHIPYARDS) >= 3 && !self.others_have(l, t::SHIPYARDS),
                &mut r,
            ),
            Z => {
                if !self.all_alive(&[t::TURALYON, t::DANATH], 2) {
                    return Some(Defeat);
                }
                if self.is_in(4) {
                    return r;
                }
                self.make_rescuable(l, t::TURALYON);
                self.make_rescuable(l, t::DANATH);
                self.victory.rule = Some(rescue_rule(required));
            }
            AA => {
                if !self.all_alive(&[t::ALLERIA, t::KHADGAR, t::KURDAN], 3) {
                    return Some(Defeat);
                }
                let deathwing = self
                    .units
                    .iter()
                    .any(|u| u.kind == t::DEATHWING && !u.dying && !u.removing);
                win(!deathwing && !self.others_have(l, t::AVIARIES), &mut r);
            }
            AB => win(!self.has(5, t::CASTLES) && !self.has(4, t::RUNESTONE), &mut r),
            AC => {
                if !self.all_alive(&[t::DANATH, t::KHADGAR, t::TURALYON, t::ALLERIA], 4) {
                    return Some(Defeat);
                }
                r = self.standard(l, true, None);
                win(
                    players_0_7()
                        .filter(|&q| q != l)
                        .all(|q| self.players[q].gold < SURVIVAL_GOLD),
                    &mut r,
                );
            }
            AD => {
                if !self.all_alive(&[t::KHADGAR, t::TURALYON, t::ALLERIA], 3) {
                    return Some(Defeat);
                }
                r = self.standard(l, true, None);
            }
            AE => {
                if !self.all_alive(&[t::KHADGAR], 1) {
                    return Some(Defeat);
                }
                win(!self.has(15, t::PORTAL), &mut r);
            }
            _ => {}
        }
        r
    }

    fn is_player_slot(&self, q: usize) -> bool {
        matches!(self.players[q].controller, controller::HUMAN | controller::COMPUTER)
    }

    /// A player leaves a chat-room game (`net/leave-and-drop.md`, "Chat-room game: carrying out a leave", steps 3 and
    /// 4, Enhanced): its slot becomes "nobody", it leaves everyone's shared-vision set and its own becomes just itself.
    /// Its units keep their orders. No result is set by the leave itself. The notice, the score row and the new host
    /// are the caller's.
    pub fn leave_game(&mut self, q: usize) {
        if q >= self.players.len() {
            return;
        }
        self.players[q].controller = controller::NOBODY;
        for p in &mut self.players {
            p.shared_vision &= !(1 << q);
        }
        self.players[q].shared_vision = 1 << q;
    }

    /// `Command::Leave`: a human's leave, carried out in the turn. Enhanced rules clear the leaver from the
    /// shared-vision sets; Classic mode leaves them alone (`net/leave-and-drop.md`, "Chat-room game", step 4, LV5 and
    /// LV6). A slot that is not a human (a computer, or one that already left) is ignored, so a repeated leave or a
    /// remote peer's wrong slot changes nothing.
    pub(crate) fn leave_command(&mut self, player: u8) {
        let q = player as usize;
        if self.players.get(q).is_none_or(|p| p.controller != controller::HUMAN) {
            return;
        }
        if self.rules.enhanced {
            self.leave_game(q);
        } else {
            self.players[q].controller = controller::NOBODY;
        }
    }

    /// A player who is out: in a network game its slot becomes "nobody", it leaves everyone's shared vision and its
    /// own shared-vision set becomes just itself (`vision/fog.md`, "Shared vision", X10), and its living units take
    /// order 32 (the docked wait, which does nothing).
    fn knock_out(&mut self, q: usize) {
        self.players[q].result = Some(Outcome::Defeat);
        if !self.rules.multiplayer {
            return;
        }
        self.players[q].controller = controller::NOBODY;
        for p in &mut self.players {
            p.shared_vision &= !(1 << q);
        }
        self.players[q].shared_vision = 1 << q;
        for i in 0..self.units.len() {
            if self.units[i].owner as usize == q && !self.units[i].dying {
                self.give(i, Order::Docked);
            }
        }
    }

    /// "The multiplayer rule".
    fn multiplayer(&mut self, l: usize) {
        let cheat = self.cheat();
        if cheat != Some(Outcome::Victory) {
            if cheat == Some(Outcome::Defeat) || !self.is_in(l) {
                self.knock_out(l);
                return self.finish(l, Outcome::Defeat);
            }
            for q in players_0_7().filter(|&q| q != l) {
                if self.is_player_slot(q) && self.players[q].result.is_none() && !self.is_in(q) {
                    self.knock_out(q);
                    self.events.push(Event::GameOver {
                        player: q as u8,
                        outcome: Outcome::Defeat,
                    });
                }
            }
            if players_0_7().any(|q| self.players[q].controller == controller::COMPUTER && self.is_in(q)) {
                return;
            }
            let humans: Vec<usize> = players_0_7()
                .filter(|&q| self.players[q].controller == controller::HUMAN && self.is_in(q))
                .collect();
            let allied = |a: usize, b: usize| a == b || self.players[a].allies & (1 << b) != 0;
            let together = humans.iter().all(|&a| {
                (humans.len() == 1 || self.players[a].allied_victory)
                    && humans.iter().all(|&b| allied(a, b) && allied(b, a))
            });
            if !together {
                return;
            }
        }
        if players_0_7().all(|q| self.players[q].result.is_none()) {
            let used: Vec<usize> = players_0_7().filter(|&q| q != l && self.is_player_slot(q)).collect();
            for q in used {
                self.players[q].result = Some(Outcome::GameEnded);
            }
        }
        self.finish(l, Outcome::Victory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use crate::world::tests::field;
    use tidewater_formats::udta::UnitStats;

    /// A 32 x 32 field where every type the tests use has stats: 2 x 2 buildings and 1 x 1 land units.
    fn world(_: u16, _: u16) -> World {
        let mut w = field(32, 32);
        for k in [0x3a, 0x3c, 0x48, 0x4a, 0x60, 0x66] {
            w.stats.0[k] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING,
                ..Default::default()
            };
        }
        for k in [0x00, 0x02, 0x10, 0x14, 0x1c, 0x29] {
            w.stats.0[k] = UnitStats {
                hit_points: 60,
                size: (1, 1),
                flags: flags::LAND,
                ..Default::default()
            };
        }
        w
    }

    /// Runs until cycle `c` (1-based) has finished.
    fn run_to(w: &mut World, c: u32) {
        while w.tick < c {
            w.step(&[]);
        }
    }

    fn outcomes(w: &World) -> Vec<(u8, Outcome)> {
        w.events
            .iter()
            .filter_map(|e| match *e {
                Event::GameOver { player, outcome } => Some((player, outcome)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn checks_run_every_201_cycles_from_the_second() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        w.spawn(0x00, 1, (20, 20), 0).unwrap();
        let mut checks = Vec::new();
        for _ in 0..700 {
            let before = w.victory.countdown;
            w.step(&[]);
            if before == 0 {
                checks.push(w.tick);
            }
        }
        assert_eq!(checks, [2, 203, 404, 605]);
    }

    /// Core timing spec, scheduler rule 1, and game/victory, "When the check runs": the countdown counts the two
    /// start-up cycles, so the check runs in the second start-up cycle, then at counter 201, 402, 603.
    #[test]
    fn checks_run_in_the_second_start_up_cycle_then_at_counters_201_402_603() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        w.spawn(0x00, 1, (20, 20), 0).unwrap();
        let (mut records, mut counters) = (Vec::new(), Vec::new());
        for n in 1..=2 {
            let before = w.victory.countdown;
            w.step(&[]);
            if before == 0 {
                records.push(n);
            }
        }
        assert_eq!(records, [2], "the check runs in the second start-up cycle only");
        w.end_start_up();
        for _ in 0..700 {
            let before = w.victory.countdown;
            w.step(&[]);
            if before == 0 {
                counters.push(w.tick);
            }
        }
        assert_eq!(counters, [201, 402, 603]);
        assert_eq!(w.victory.countdown, 200 - (700 - 603), "reloaded to 200 at 603");
    }

    /// Runs normal cycles until the cycle counter reaches `c` (call `start_up` first).
    fn run_to_counter(w: &mut World, c: u32) {
        while w.tick < c {
            w.step(&[]);
        }
    }

    #[test]
    fn counter_vectors_destroy_all_found_at_603_and_shown_at_804() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        let foe = w.spawn(0x3a, 1, (10, 10), 0).unwrap();
        w.start_up();
        run_to_counter(&mut w, 500);
        let i = w.index(foe).unwrap();
        w.units[i].dying = true;
        run_to_counter(&mut w, 602);
        assert_eq!(w.victory.rule, None);
        run_to_counter(&mut w, 603);
        assert_eq!(w.victory.rule, Some(Rule::Outcome(Outcome::Victory)));
        run_to_counter(&mut w, 803);
        assert!(outcomes(&w).is_empty());
        run_to_counter(&mut w, 804);
        assert_eq!(outcomes(&w), [(0, Outcome::Victory)]);
    }

    #[test]
    fn counter_vectors_training_until_700_found_at_804_and_shown_at_1005() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        let foe = w.spawn(0x3a, 1, (10, 10), 0).unwrap();
        w.players[0].in_training = 1;
        w.start_up();
        run_to_counter(&mut w, 500);
        let i = w.index(foe).unwrap();
        w.units[i].dying = true;
        run_to_counter(&mut w, 603);
        assert_eq!(w.victory.rule, None, "training blocks the 603 check");
        run_to_counter(&mut w, 700);
        w.players[0].in_training = 0;
        run_to_counter(&mut w, 803);
        assert_eq!(w.victory.rule, None);
        run_to_counter(&mut w, 804);
        assert_eq!(w.victory.rule, Some(Rule::Outcome(Outcome::Victory)));
        run_to_counter(&mut w, 1004);
        assert!(outcomes(&w).is_empty());
        run_to_counter(&mut w, 1005);
        assert_eq!(outcomes(&w), [(0, Outcome::Victory)]);
    }

    #[test]
    fn counter_vectors_fourth_farm_at_300_found_at_402_and_shown_at_603() {
        let mut w = world(24, 24);
        w.victory.rule = Some(mission_rule(0));
        w.spawn(0x00, 0, (1, 1), 0).unwrap();
        w.spawn(0x3c, 0, (4, 1), 0).unwrap();
        for x in 0..3 {
            w.spawn(0x3a, 0, (4 + 3 * x, 8), 0).unwrap();
        }
        w.start_up();
        run_to_counter(&mut w, 300);
        assert_eq!(w.victory.rule, Some(mission_rule(0)), "no result at the 201 check");
        // The fourth farm completes in cycle 300.
        w.spawn(0x3a, 0, (13, 8), 0).unwrap();
        run_to_counter(&mut w, 401);
        assert_eq!(w.victory.rule, Some(mission_rule(0)));
        run_to_counter(&mut w, 402);
        assert_eq!(w.victory.rule, Some(Rule::Outcome(Outcome::Victory)));
        run_to_counter(&mut w, 602);
        assert!(outcomes(&w).is_empty());
        run_to_counter(&mut w, 603);
        assert_eq!(outcomes(&w), [(0, Outcome::Victory)]);
    }

    #[test]
    fn entries_interleave_races_and_campaigns() {
        assert_eq!(mission_entry(false, false, 8), 14);
        assert_eq!(
            mission_rule(14),
            Rule::Mission {
                mode: mode::D,
                required: 0
            }
        );
        assert_eq!(mission_entry(true, true, 1), 29);
        assert_eq!(
            mission_rule(29),
            Rule::Mission {
                mode: mode::L,
                required: 0
            }
        );
        assert_eq!(mission_rule(3), rescue_rule(0x1001));
        assert_eq!(mission_rule(42), DESTROY_ALL);
    }

    #[test]
    fn destroy_all_victory_shows_at_the_next_check() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        let foe = w.spawn(0x3a, 1, (10, 10), 0).unwrap();
        run_to(&mut w, 500);
        let i = w.index(foe).unwrap();
        w.units[i].dying = true;
        run_to(&mut w, 604);
        assert_eq!(w.victory.rule, None);
        run_to(&mut w, 605);
        assert_eq!(w.victory.rule, Some(Rule::Outcome(Outcome::Victory)));
        assert!(outcomes(&w).is_empty());
        run_to(&mut w, 806);
        assert_eq!(outcomes(&w), [(0, Outcome::Victory)]);
        assert_eq!(w.players[0].result, Some(Outcome::Victory));
    }

    #[test]
    fn training_delays_victory_and_leftover_transports_dont_count() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        w.spawn(0x1c, 1, (10, 10), 0).ok();
        w.spawn(0x29, 1, (12, 12), 0).ok();
        w.players[0].in_training = 1;
        assert_eq!(w.standard(0, true, None), None);
        w.players[0].in_training = 0;
        assert_eq!(w.standard(0, true, None), Some(Outcome::Victory));
        // An ally's units still count.
        w.spawn(0x00, 2, (6, 6), 0).unwrap();
        assert_eq!(w.standard(0, true, None), None);
    }

    #[test]
    fn no_units_needs_a_production_building_and_400_gold() {
        let mut w = world(16, 16);
        let farm = w.spawn(0x3a, 0, (2, 2), 0).unwrap();
        assert_eq!(w.standard(0, false, None), Some(Outcome::Defeat));
        let i = w.index(farm).unwrap();
        w.units[i].kind = 0x4a;
        w.players[0].gold = 399;
        assert_eq!(w.standard(0, false, None), Some(Outcome::Defeat));
        w.players[0].gold = 400;
        assert_eq!(w.standard(0, false, None), None);
        let i = w.index(farm).unwrap();
        w.units[i].kind = 0x60;
        w.players[0].gold = 500;
        assert_eq!(w.standard(0, false, None), None);
    }

    /// A unit of player 2 with the rescuable mark.
    fn marked(w: &mut World, k: u8, tile: (i32, i32)) -> usize {
        let id = w.spawn(k, 2, tile, 0).unwrap();
        let i = w.index(id).unwrap();
        w.units[i].rescuable = true;
        i
    }

    #[test]
    fn rescue_counts_heroes_with_bit_0x1000() {
        let mut w = world(16, 16);
        w.players[2].controller = controller::RESCUE_PASSIVE;
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        // Entry 18: required 4, three rescued, one left.
        w.players[0].rescued = 3;
        let last = marked(&mut w, 0x00, (8, 8));
        assert_eq!(w.rescue_test(0, 4), None);
        w.units[last].dying = true;
        assert_eq!(w.rescue_test(0, 4), Some(Outcome::Defeat));
        // Entry 3: a hero rescued first.
        let mut w = world(16, 16);
        w.stats.0[0x14].flags |= flags::HERO;
        w.players[0].rescued = 0x1001;
        assert_eq!(w.rescue_test(0, 0x1001), Some(Outcome::Victory));
        // An ordinary unit rescued, the hero lost: possible = 1.
        let mut w = world(16, 16);
        w.stats.0[0x14].flags |= flags::HERO;
        let hero = marked(&mut w, 0x14, (8, 8));
        w.players[0].rescued = 1;
        assert_eq!(w.rescue_test(0, 0x1001), None, "the hero is still left");
        w.units[hero].dying = true;
        assert_eq!(w.rescue_test(0, 0x1001), Some(Outcome::Defeat));
    }

    #[test]
    fn modes_w_and_z_lower_the_rescued_count() {
        // Mode W (entry 32): no other player has a keep; Turalyon, a hero, becomes rescuable and L's count wraps.
        let mut w = world(24, 24);
        w.stats.0[t::TURALYON as usize] = w.stats.0[0x00];
        w.stats.0[t::TURALYON as usize].flags |= flags::HERO;
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        let tur = w.spawn(t::TURALYON, 0, (4, 4), 0).unwrap();
        w.start_campaign(32);
        let Some(Rule::Mission { mode: m, required }) = w.victory.rule else {
            panic!()
        };
        assert_eq!(m, mode::W);
        assert_eq!(
            w.mission(0, m, required),
            None,
            "the rescue test first runs at the next check"
        );
        assert!(w.unit(tur).unwrap().rescuable);
        assert_eq!(w.players[0].rescued, 0xffff);
        assert_eq!(w.victory.rule, Some(rescue_rule(1)));
        assert_eq!(w.standard(0, false, Some(1)), Some(Outcome::Victory));
        // Already marked: no second drop.
        assert_eq!(w.mission(0, mode::W, 1), None);
        assert_eq!(w.players[0].rescued, 0xffff);
        // Mode Z: two heroes, two drops.
        let mut w = world(24, 24);
        for k in [t::TURALYON, t::DANATH] {
            w.stats.0[k as usize] = w.stats.0[0x00];
            w.stats.0[k as usize].flags |= flags::HERO;
            w.spawn(k, 0, (4 + k as i32 % 8, 4), 0).unwrap();
        }
        w.players[0].rescued = 5;
        assert_eq!(w.mission(0, mode::Z, 2), None);
        assert_eq!(w.players[0].rescued, 3);
        assert_eq!(w.rescuable_counts(), (2, 2));
    }

    #[test]
    fn mode_f_reads_the_converted_tally() {
        let mut w = world(24, 24);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        w.spawn(0x00, 1, (20, 20), 0).unwrap();
        w.victory.rule = Some(mission_rule(20));
        assert_eq!(w.mission(0, mode::F, 0), None);
        assert_ne!(w.victory.rule, Some(DESTROY_ALL));
        w.players[0].converted = 1;
        w.mission(0, mode::F, 0);
        assert_eq!(w.victory.rule, Some(DESTROY_ALL));
    }

    #[test]
    fn mode_a_counts_only_complete_farms() {
        let mut w = world(24, 24);
        w.victory.rule = Some(mission_rule(0));
        w.spawn(0x00, 0, (1, 1), 0).unwrap();
        w.spawn(0x3c, 0, (4, 1), 0).unwrap();
        let mut farms = Vec::new();
        for x in 0..4 {
            farms.push(w.spawn(0x3a, 0, (4 + 3 * x, 8), 0).unwrap());
        }
        let i = w.index(farms[3]).unwrap();
        w.units[i].construction = Some(Default::default());
        assert_eq!(w.mission(0, mode::A, 0), None);
        let i = w.index(farms[3]).unwrap();
        w.units[i].construction = None;
        assert_eq!(w.mission(0, mode::A, 0), Some(Outcome::Victory));
    }

    #[test]
    fn mode_y_needs_three_shipyards() {
        let mut w = world(24, 24);
        w.spawn(0x00, 0, (1, 1), 0).unwrap();
        for x in 0..2 {
            w.spawn(0x48, 0, (4 + 4 * x, 8), 0).unwrap();
        }
        assert_eq!(w.mission(0, mode::Y, 0), None);
        w.spawn(0x48, 0, (16, 8), 0).unwrap();
        assert_eq!(w.mission(0, mode::Y, 0), Some(Outcome::Victory));
    }

    #[test]
    fn mission_starts() {
        // Mode D: player 4's peasants become attack peasants and lose the mark; L and 4 stop being allied.
        let mut w = world(24, 24);
        w.players[4].controller = controller::RESCUE_PASSIVE;
        w.players[0].allies = 1 << 4;
        w.players[4].allies = 1;
        for x in 0..5 {
            w.spawn(0x02, 4, (2 + 2 * x, 2), 0).unwrap();
        }
        w.start_campaign(14);
        assert!(w.units.iter().all(|u| u.kind == 0x10 && !u.rescuable));
        assert_eq!((w.players[0].allies, w.players[4].allies), (0, 0));
        // Mode AB: runestones are set to 400 hit points.
        let mut w = world(24, 24);
        let stone = w.spawn(0x66, 4, (4, 4), 0).unwrap();
        let i = w.index(stone).unwrap();
        w.units[i].hp = 5000;
        w.start_campaign(44);
        assert_eq!(w.unit(stone).unwrap().hp, 400);
        // Mode AC: spells known.
        let mut w = world(8, 8);
        w.start_campaign(46);
        assert_eq!(w.players[0].spells_known, 0b11_1111_1011);
    }

    /// G24 (`game/victory.md`): a load runs the mission start again from the saved rule.
    #[test]
    fn a_load_runs_the_mission_start_again() {
        // Mode AB: a runestone damaged after the start heals at the load.
        let mut w = world(24, 24);
        let stone = w.spawn(0x66, 4, (4, 4), 0).unwrap();
        w.start_campaign(44);
        let i = w.index(stone).unwrap();
        w.units[i].hp = 4;
        let bytes = w.save_state();
        let mut b = world(24, 24);
        b.spawn(0x66, 4, (4, 4), 0).unwrap();
        b.restore(&bytes, 1).unwrap();
        assert_eq!(b.unit(stone).unwrap().hp, 400);
        // Mode D: peasants that player 4 has gained since the start are converted at the load.
        let mut w = world(24, 24);
        w.start_campaign(14);
        let p = w.spawn(0x02, 4, (2, 2), 0).unwrap();
        let bytes = w.save_state();
        let mut b = world(24, 24);
        b.spawn(0x02, 4, (2, 2), 0).unwrap();
        b.restore(&bytes, 1).unwrap();
        assert_eq!(b.unit(p).unwrap().kind, 0x10);
        // Mode E switches its rule at the start, so the loaded game no longer holds the mode and doesn't repeat it.
        let mut w = world(24, 24);
        w.start_campaign(18);
        let rule = w.victory.rule;
        assert!(matches!(rule, Some(Rule::Standard { .. })));
        let bytes = w.save_state();
        let mut b = world(24, 24);
        b.restore(&bytes, 1).unwrap();
        assert_eq!(b.victory.rule, rule);
    }

    /// `Command::Leave` (`net/leave-and-drop.md`, "Chat-room game", steps 3 and 4, LV5 and LV6): the slot becomes
    /// nobody; Enhanced rules also clear it from the shared-vision sets, Classic mode leaves them. A slot that is no
    /// human, or has already gone, is ignored.
    #[test]
    fn a_leave_command_empties_the_slot_and_enhanced_rules_clear_shared_vision() {
        for enhanced in [true, false] {
            let mut w = world(24, 24);
            w.rules.multiplayer = true;
            w.rules.enhanced = enhanced;
            w.players[0].controller = controller::HUMAN;
            w.players[1].controller = controller::HUMAN;
            w.players[2].controller = controller::COMPUTER;
            w.players[0].shared_vision = 0b011;
            w.players[1].shared_vision = 0b011;
            w.step(&[Command::Leave {
                player: 1,
                dropped: false,
            }]);
            assert_eq!(w.players[1].controller, controller::NOBODY);
            // Enhanced: the leaver leaves everyone's set and its own is itself. Classic: nothing else changes.
            let (seen_by_0, own) = if enhanced { (0b001, 0b010) } else { (0b011, 0b011) };
            assert_eq!(
                (w.players[0].shared_vision, w.players[1].shared_vision),
                (seen_by_0, own)
            );
            // A computer's slot, a slot with nobody, and a second leave change nothing.
            for p in [2u8, 5, 1] {
                let before = w.players[p as usize].controller;
                w.step(&[Command::Leave {
                    player: p,
                    dropped: true,
                }]);
                assert_eq!(w.players[p as usize].controller, before, "slot {p}");
            }
        }
    }

    #[test]
    fn multiplayer_needs_allied_humans_and_no_computer() {
        let mut w = world(24, 24);
        w.rules.multiplayer = true;
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::HUMAN;
        w.players[2].controller = controller::COMPUTER;
        w.spawn(0x00, 0, (1, 1), 0).unwrap();
        w.spawn(0x00, 1, (8, 8), 0).unwrap();
        let ai = w.spawn(0x00, 2, (16, 16), 0).unwrap();
        w.multiplayer(0);
        assert!(!w.victory.over);
        let i = w.index(ai).unwrap();
        w.units[i].dying = true;
        w.multiplayer(0);
        assert!(!w.victory.over, "players 0 and 1 aren't allied");
        assert_eq!(w.players[2].result, Some(Outcome::Defeat));
        assert_eq!(w.players[2].controller, controller::NOBODY);
        for (p, q) in [(0, 1), (1, 0)] {
            w.players[p].allies = 1 << q;
            w.players[p].allied_victory = true;
        }
        w.multiplayer(0);
        assert_eq!(w.players[0].result, Some(Outcome::Victory));
    }

    /// X10 follows the multiplayer rule's two places (victory spec, "The multiplayer rule", steps 2 and 3): the
    /// local player out, and another player out. A game that isn't a network game leaves the sets alone.
    /// Core timing spec, scheduler rule 4 (G15), and game/victory, "When the check runs" (G02, X1): a paused cycle moves
    /// neither the fog refresh countdown nor the victory countdown, but the cycle counter counts it, so every later
    /// refresh and check falls that many counter values later.
    #[test]
    fn paused_cycles_move_neither_countdown_but_the_counter_counts_them() {
        let run = |pause: Option<(u32, u32)>| {
            let mut w = world(16, 16);
            w.spawn(0x00, 0, (2, 2), 0).unwrap();
            w.spawn(0x00, 1, (20, 20), 0).unwrap();
            w.start_up();
            let (mut refreshes, mut checks) = (Vec::new(), Vec::new());
            let mut left = 0;
            while w.tick < 700 {
                if let Some((at, n)) = pause
                    && w.tick == at
                    && left == 0
                {
                    w.step(&[crate::Command::Pause { player: 0 }]);
                    left = n;
                }
                let (fog, vic) = (w.vision.countdown, w.victory.countdown);
                w.step(&[]);
                if left > 0 {
                    // A paused cycle: the countdowns stay where they were, the counter counted.
                    assert_eq!((w.vision.countdown, w.victory.countdown), (fog, vic));
                    left -= 1;
                    if left == 0 {
                        w.step(&[crate::Command::Pause { player: 0 }]);
                        assert!(!w.paused);
                    }
                    continue;
                }
                if fog == 1 {
                    refreshes.push(w.tick);
                }
                if vic == 0 {
                    checks.push(w.tick);
                }
            }
            (refreshes, checks)
        };
        let (refreshes, checks) = run(None);
        assert_eq!(&refreshes[..3], [99, 199, 299]);
        assert_eq!(&checks[..3], [201, 402, 603]);
        // G02: the cycles spent paused (the 10 asked for plus the one that carries the pause command: 11 here) move every
        // later refresh and check by that many counter values.
        let (refreshes, checks) = run(Some((100, 10)));
        assert_eq!(&refreshes[..3], [99, 210, 310]);
        assert_eq!(&checks[..2], [212, 413]);
    }

    /// X5 (G04, `game/victory.md` "Outcome timing" and `game/cheats.md`): the instant victory and defeat cheats are
    /// found by the next check and, in single player, shown at the one after.
    #[test]
    fn the_instant_cheats_are_found_at_the_next_check_and_shown_at_the_one_after() {
        use crate::cheats::{Flags, flag};
        for (cheat, want) in [
            (flag::INSTANT_VICTORY, Outcome::Victory),
            (flag::INSTANT_DEFEAT, Outcome::Defeat),
        ] {
            let mut w = world(16, 16);
            w.spawn(0x00, 0, (2, 2), 0).unwrap();
            w.spawn(0x00, 1, (20, 20), 0).unwrap();
            w.start_up();
            run_to_counter(&mut w, 100);
            w.cheat_command(0, Flags(cheat));
            run_to_counter(&mut w, 200);
            assert_eq!((w.victory.rule, outcomes(&w)), (None, vec![]), "before the check");
            run_to_counter(&mut w, 201);
            assert_eq!(w.victory.rule, Some(Rule::Outcome(want)), "found at counter 201");
            assert!(outcomes(&w).is_empty() && !w.victory.over);
            run_to_counter(&mut w, 401);
            assert!(outcomes(&w).is_empty());
            run_to_counter(&mut w, 402);
            assert_eq!(outcomes(&w), [(0, want)], "shown at counter 402");
            assert!(w.victory.over);
        }
    }

    /// X4 (G03, `game/victory.md` "When the check runs"): demo playback starts the countdown at 1,400, so its first
    /// check, which ends the demo, is the 1,401st cycle, the two start-up cycles included.
    #[test]
    fn the_demos_first_check_is_the_1401st_cycle() {
        let mut w = world(16, 16);
        w.spawn(0x00, 0, (2, 2), 0).unwrap();
        w.spawn(0x00, 1, (20, 20), 0).unwrap();
        w.victory = Victory::for_demo();
        assert_eq!(w.victory.countdown, 1400);
        let mut cycles = 0;
        loop {
            let before = w.victory.countdown;
            w.step(&[]);
            cycles += 1;
            if before == 0 {
                break;
            }
            assert!(cycles < 2000);
        }
        assert_eq!(cycles, 1401);
        // The two start-up cycles are the first two of them, so the counter reads 1,399 after the check.
        assert_eq!(w.victory.countdown, RELOAD);
    }

    #[test]
    fn a_player_who_is_out_gets_a_set_of_just_itself() {
        let sets = |w: &World| (0..3).map(|p| w.players[p].shared_vision).collect::<Vec<_>>();
        let game = |multiplayer: bool| {
            let mut w = world(24, 24);
            w.rules.multiplayer = multiplayer;
            for p in 0..3 {
                w.players[p].controller = controller::HUMAN;
                w.players[p].shared_vision = 0b111;
                w.spawn(0x00, p as u8, (2 + 6 * p as i32, 2), 0).unwrap();
            }
            w
        };
        // Step 2: the local player (0) is out, and the rest keep sharing with each other.
        let mut w = game(true);
        w.players[0].disconnected = true;
        w.multiplayer(0);
        assert_eq!(sets(&w), [0b001, 0b110, 0b110]);
        // Step 3: another player is out.
        let mut w = game(true);
        w.players[2].disconnected = true;
        w.multiplayer(0);
        assert_eq!(sets(&w), [0b011, 0b011, 0b100]);
        // Not a network game: nothing changes.
        let mut w = game(false);
        w.knock_out(2);
        assert_eq!(sets(&w), [0b111; 3]);
    }

    #[test]
    fn a_disconnected_human_is_knocked_out() {
        let mut w = world(24, 24);
        w.rules.multiplayer = true;
        for p in 0..3 {
            w.players[p].controller = controller::HUMAN;
            w.players[p].shared_vision = 0b111;
            w.spawn(0x00, p as u8, (2 + 6 * p as i32, 2), 0).unwrap();
        }
        w.players[1].disconnected = true;
        w.multiplayer(0);
        assert_eq!(w.players[1].result, Some(Outcome::Defeat));
        assert_eq!(w.players[1].controller, controller::NOBODY);
        // X10: the player leaves everyone's shared-vision set, and its own set becomes just itself.
        assert_eq!(w.players[1].shared_vision, 0b10);
        assert!(
            w.players
                .iter()
                .enumerate()
                .all(|(q, p)| q == 1 || p.shared_vision & 0b10 == 0)
        );
        assert_eq!((w.players[0].shared_vision, w.players[2].shared_vision), (0b101, 0b101));
        assert_eq!(w.shared_set(1), 0b10);
        let b = w.units.iter().find(|u| u.owner == 1).unwrap();
        assert_eq!(b.pending, Some(Order::Docked));
        assert!(!w.victory.over);
        // C wiped out too: only L is in. B and C have results, so nobody gets "game ended".
        let c = w.units.iter().position(|u| u.owner == 2).unwrap();
        w.units[c].dying = true;
        w.multiplayer(0);
        assert_eq!(w.players[0].result, Some(Outcome::Victory));
        assert_eq!(w.players[2].result, Some(Outcome::Defeat));
    }
}
