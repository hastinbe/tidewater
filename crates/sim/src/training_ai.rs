//! Computer buildings deciding what to train, research or upgrade into, the research requests and done marks, and
//! the no-workers research pass. Spec: `docs/specs/ai/training-research.md`. Unit types and upgrade numbers are those
//! of the public PUD specification (appendices A and B); the script's second list is in `ai/ai-scripts.md`.
//!
//! Where these run in the cycle is `ai/computer-player.md` (`ai.rs`): a building thinks from its resting order 33 in
//! phase 4, and the no-workers pass runs between passes A and B of the 50-cycle passes in phase 2.
//!
//! Our readings where the spec leaves room:
//! - Order 33's logic runs at each "step done" of a building with no job (`World::advance`); the step done at which a
//!   newly started job takes over runs the job instead. It first forgets the building's attacker, then thinks. The
//!   circle of power rests in order 59, so it neither forgets its attacker nor thinks.
//! - "In progress" for the two-level upgrades and the ranger line tests the fixed bits of the buttons
//!   (`ui/command-buttons.md`, "Upgrade and research buttons"); for spells it tests the upgrade data's "affects"
//!   bits, as "known" does.
//! - Only players 0 to 7 have script state (`World::ai_player`).

use crate::ai::{AiPlayer, BUILD_LIMIT, SCRIPT_PLAYERS};
use crate::economy::JobKind;
use crate::tech::{PALADIN_BIT, RANGER_TRAINING_LINE};
use crate::world::World;

/// The first building type in the second list.
const FIRST_LISTED: u8 = 0x3a;
/// "Never think".
const NEVER: u16 = 0xffff;
/// Entries scanned for done marks.
const LIST_MAX: usize = 64;
/// Research codes with a request flag (open question "Codes above 0x9A": higher codes are ignored).
const FIRST_CODE: u8 = 0x80;
const LAST_CODE: u8 = 0x9a;
/// Hall upgrade codes.
const KEEP_CODE: u8 = 0x98;
const CASTLE_CODE: u8 = 0x99;

/// Building types, human number (the race bit is added).
mod b {
    pub const BARRACKS: u8 = 0x3c;
    pub const CHURCH: u8 = 0x3e;
    pub const SCOUT_TOWER: u8 = 0x40;
    pub const STABLES: u8 = 0x42;
    pub const INVENTOR: u8 = 0x44;
    pub const AVIARY: u8 = 0x46;
    pub const SHIPYARD: u8 = 0x48;
    pub const TOWN_HALL: u8 = 0x4a;
    pub const LUMBER_MILL: u8 = 0x4c;
    pub const FOUNDRY: u8 = 0x4e;
    pub const MAGE_TOWER: u8 = 0x50;
    pub const BLACKSMITH: u8 = 0x52;
    pub const KEEP: u8 = 0x58;
    pub const CASTLE: u8 = 0x5a;
    pub const GUARD_TOWER: u8 = 0x60;
}

/// Unit types, human number.
mod u {
    pub const FOOTMAN: u8 = 0x00;
    pub const WORKER: u8 = 0x02;
    pub const BALLISTA: u8 = 0x04;
    pub const KNIGHT: u8 = 0x06;
    pub const ARCHER: u8 = 0x08;
    pub const MAGE: u8 = 0x0a;
    pub const PALADIN: u8 = 0x0c;
    pub const RANGER: u8 = 0x12;
    pub const TANKER: u8 = 0x1a;
    pub const TRANSPORT: u8 = 0x1c;
    pub const DESTROYER: u8 = 0x1e;
    pub const BATTLESHIP: u8 = 0x20;
    pub const SUBMARINE: u8 = 0x26;
    pub const FLYING_MACHINE: u8 = 0x28;
    pub const GRYPHON: u8 = 0x2a;
}

/// The unit group counts ("Training buildings"): complete units, groups without held units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    pub footmen: u32,
    pub archers: u32,
    pub siege: u32,
    pub knights: u32,
    pub mages: u32,
    pub gryphons: u32,
    pub flying_machines: u32,
    pub transports: u32,
    pub destroyers: u32,
    pub battleships: u32,
    pub submarines: u32,
    /// Held units included.
    pub tankers: u32,
    /// As in `ai/ai-scripts.md`, held units included.
    pub workers: u32,
}

impl AiPlayer {
    /// Sets the request flag of research code `code`; codes outside 0x80 to 0x9A are ignored.
    pub(crate) fn request(&mut self, code: u8) {
        if (FIRST_CODE..=LAST_CODE).contains(&code) {
            self.research_requests |= 1 << (code - FIRST_CODE);
        }
    }

    pub fn requested(&self, code: u8) -> bool {
        (FIRST_CODE..=LAST_CODE).contains(&code) && self.research_requests & (1 << (code - FIRST_CODE)) != 0
    }
}

/// The research item of code `code` for an orc (`SIDE` 1) or human player ("Research codes"), and whether it is a
/// spell job. `None` for the hall codes and codes never acted on.
pub fn research_item(code: u8, orc: bool) -> Option<JobKind> {
    let (human, orcish) = match code {
        0x80 => (4, 6),
        0x81 => (5, 7),
        0x82 => (24, 28),
        0x83 => (27, 31),
        0x84 => (25, 29),
        0x85 => (26, 30),
        0x86 => (0, 2),
        0x87 => (1, 3),
        0x88 => (8, 10),
        0x89 => (9, 11),
        0x8a => (22, 20),
        0x8b => (23, 21),
        0x8c => (12, 14),
        0x8d => (13, 15),
        0x8e => (16, 18),
        0x8f => (17, 19),
        0x90 => (33, 32),
        0x91 => (35, 44),
        0x92 => (36, 50),
        0x93 => (37, 45),
        0x94 => (39, 48),
        0x95 => (40, 47),
        0x96 => (41, 49),
        0x97 => (42, 51),
        _ => return None,
    };
    let n = if orc { orcish } else { human };
    Some(if code < 0x90 {
        JobKind::Research(n)
    } else {
        JobKind::Spell(n)
    })
}

/// The button's fixed "being researched" bit of a two-level upgrade 0 to 23 (`ui/command-buttons.md`, "Upgrade
/// bits"): weapons 2/3, missiles 0/1, shields 4/5, ship cannons 6/7, ship armour 8/9, siege 12/13.
fn two_level_bit(n: u8) -> u32 {
    1 << ([2, 0, 4, 6, 8, 12][(n / 4) as usize] + (n & 1) as u32)
}

/// The ranger line of codes 0x82 to 0x85: its level line and its fixed "being researched" bit.
pub(crate) fn ranger_line(code: u8) -> (usize, u32) {
    match code {
        0x82 => (RANGER_TRAINING_LINE, 1 << 16),
        0x83 => (10, 1 << 19),
        0x84 => (8, 1 << 17),
        _ => (9, 1 << 18),
    }
}

/// Code orders of the research buildings ("Research buildings").
const LUMBER_MILL_CODES: [u8; 6] = [0x80, 0x81, 0x82, 0x83, 0x84, 0x85];
const BLACKSMITH_CODES: [u8; 6] = [0x86, 0x88, 0x87, 0x89, 0x8a, 0x8b];
const FOUNDRY_CODES: [u8; 4] = [0x8c, 0x8e, 0x8d, 0x8f];
const CHURCH_CODES: [u8; 3] = [0x90, 0x91, 0x92];
const MAGE_TOWER_CODES: [u8; 5] = [0x93, 0x94, 0x95, 0x96, 0x97];

impl World {
    /// The second-list interval of building type `k` for player `p`; 0xFFFF ("never") for types before the list or
    /// a list running off the file.
    pub fn think_interval(&self, p: usize, k: u8) -> u16 {
        let Some(file) = &self.ai_file else { return NEVER };
        if k < FIRST_LISTED || p >= SCRIPT_PLAYERS {
            return NEVER;
        }
        file.u16(self.ai[p].second_list as usize + 2 * (k - FIRST_LISTED) as usize)
            .unwrap_or(NEVER)
    }

    /// One run of order 33's logic for building `i` ("When a building thinks"): the think counter, and the think on
    /// every (interval + 2)-th run.
    pub(crate) fn building_think_tick(&mut self, i: usize) {
        let unit = &self.units[i];
        let (owner, k) = (unit.owner, unit.kind);
        if !self.ai_player(owner) || unit.dying || unit.construction.is_some() {
            return;
        }
        let interval = self.think_interval(owner as usize, k);
        if interval == NEVER {
            return;
        }
        let c = self.units[i].think;
        self.units[i].think = c.wrapping_add(1);
        if interval < c {
            self.units[i].think = 0;
            self.think(i);
        }
    }

    /// The unit counts of player `p` ("Training buildings").
    pub fn ai_groups(&self, p: usize) -> Groups {
        let mut g = Groups {
            workers: self.ai_counts(p).workers,
            ..Default::default()
        };
        for unit in self
            .units
            .iter()
            .filter(|v| v.owner as usize == p && v.hp > 0 && !v.dying && v.construction.is_none())
        {
            if matches!(unit.kind, 0x1a | 0x1b) {
                g.tankers += 1;
            }
            if unit.held {
                continue;
            }
            let n = match unit.kind {
                0x00 | 0x01 | 0x19 | 0x2e | 0x2f => &mut g.footmen,
                0x08 | 0x09 | 0x12..=0x14 => &mut g.archers,
                0x04 | 0x05 => &mut g.siege,
                0x06 | 0x07 | 0x0c | 0x0d | 0x17 | 0x2c => &mut g.knights,
                0x0a | 0x0b | 0x15 | 0x18 => &mut g.mages,
                0x16 | 0x23 | 0x2a | 0x2b => &mut g.gryphons,
                0x28 | 0x29 => &mut g.flying_machines,
                0x1c | 0x1d => &mut g.transports,
                0x1e | 0x1f => &mut g.destroyers,
                0x20 | 0x21 => &mut g.battleships,
                0x26 | 0x27 => &mut g.submarines,
                _ => continue,
            };
            *n += 1;
        }
        g
    }

    /// [`World::ai_groups`] with the surplus a load left on the four traced groups added (`game/save-load.md` SL4 to
    /// SL6; `ai/training-research.md`, "Training buildings"). The rules compare these counts.
    pub fn ai_groups_with_surplus(&self, p: usize) -> Groups {
        let mut g = self.ai_groups(p);
        if let Some(&[f, ar, s, k]) = self.group_surplus.get(p) {
            g.footmen += f;
            g.archers += ar;
            g.siege += s;
            g.knights += k;
        }
        g
    }

    /// The no-workers research pass ("Research requests and done marks"), run every 50 cycles after pass A
    /// (`ai/computer-player.md`, "One cycle, in order", step 2): for each computer player with no workers, the first
    /// build-list entry below min(slot 0x22, 64) that is not done and is 0x80 or more gets its request set. Players 8
    /// to 15 have no script state and are left out (open question "Players 8 to 15 in the no-workers pass").
    pub fn no_workers_research_pass(&mut self) {
        for p in 0..SCRIPT_PLAYERS {
            if !self.ai_player(p as u8) || self.ai_counts(p).workers != 0 {
                continue;
            }
            let list = self.build_list(p);
            let limit = (self.ai[p].slots[BUILD_LIMIT] as usize).min(LIST_MAX);
            for (n, &e) in list.iter().enumerate().take(limit) {
                if e == 0xff {
                    break;
                }
                if e < FIRST_CODE || self.ai[p].done & (1 << n) != 0 {
                    continue;
                }
                self.ai[p].request(e);
                break;
            }
        }
    }

    /// Marks every entry `code` of the first 64 build-list entries (up to an 0xFF entry) done. Returns whether one
    /// was found.
    fn mark_code_done(&mut self, p: usize, code: u8) -> bool {
        let list = self.build_list(p);
        let mut found = false;
        for (n, &e) in list.iter().enumerate().take(LIST_MAX) {
            if e == 0xff {
                break;
            }
            if e == code {
                self.ai[p].done |= 1 << n;
                found = true;
            }
        }
        found
    }

    fn clear_request(&mut self, p: usize, code: u8) {
        if self.ai[p].requested(code) {
            self.ai[p].research_requests &= !(1 << (code - FIRST_CODE));
        }
    }

    /// Starts a job silently; a computer decision ("Starting a job").
    fn ai_start(&mut self, i: usize, job: JobKind) -> bool {
        self.try_job(i, job, false)
    }

    fn has_pair(&self, p: usize, kind: u8) -> bool {
        self.has_complete(p, &[kind, kind + 1])
    }

    /// A 1-in-`m` chance: one simulation draw r, success when r mod m = 1.
    fn chance(&mut self, m: u32) -> bool {
        self.rng.draw() % m == 1
    }

    /// A building's think. Does nothing if it is not complete or is busy.
    fn think(&mut self, i: usize) {
        let unit = &self.units[i];
        if unit.building_state() != Some(crate::economy::BuildingState::Resting) {
            return;
        }
        let (p, k) = (unit.owner as usize, unit.kind);
        let race = k & 1;
        let g = self.ai_groups_with_surplus(p);
        match k & !1 {
            b::LUMBER_MILL => {
                if self.slot(p, 0x15) != 0 && g.archers == 0 {
                    return;
                }
                self.research_codes(i, &LUMBER_MILL_CODES);
            }
            b::BLACKSMITH => {
                if self.knights_and_footmen_hold(p, &g) {
                    return;
                }
                self.research_codes(i, &BLACKSMITH_CODES);
            }
            b::FOUNDRY => {
                if self.slot(p, 0x19) != 0 && g.destroyers == 0 {
                    return;
                }
                self.research_codes(i, &FOUNDRY_CODES);
            }
            b::CHURCH => {
                if self.slot(p, 0x17) != 0 && g.knights == 0 {
                    return;
                }
                self.research_codes(i, &CHURCH_CODES);
            }
            b::MAGE_TOWER => {
                if self.advanced_hold(p, &g) {
                    return;
                }
                let wanted = self.slot(p, 0x1d);
                if wanted != 0 && g.mages == 0 {
                    self.ai_start(i, JobKind::Train(u::MAGE | race));
                    return;
                }
                self.research_codes(i, &MAGE_TOWER_CODES);
                if g.mages < wanted {
                    self.ai_start(i, JobKind::Train(u::MAGE | race));
                }
            }
            b::AVIARY => {
                if !self.advanced_hold(p, &g) && g.gryphons < self.slot(p, 0x20) {
                    self.ai_start(i, JobKind::Train(u::GRYPHON | race));
                }
            }
            b::INVENTOR => {
                if !self.advanced_hold(p, &g)
                    && g.flying_machines < self.slot(p, 0x1e)
                    && self.has_pair(p, b::LUMBER_MILL)
                {
                    self.ai_start(i, JobKind::Train(u::FLYING_MACHINE | race));
                }
            }
            b::BARRACKS => {
                if let Some(t) = self.barracks_choice(p, &g, race) {
                    self.ai_start(i, JobKind::Train(t));
                }
            }
            b::SHIPYARD => {
                if let Some(t) = self.shipyard_choice(p, &g) {
                    self.ai_start(i, JobKind::Train(t | race));
                }
            }
            b::SCOUT_TOWER => {
                self.ai_start(i, JobKind::Upgrade(b::GUARD_TOWER | race));
            }
            b::TOWN_HALL | b::KEEP | b::CASTLE => self.hall_think(i, p, k, &g),
            _ => {}
        }
    }

    fn slot(&self, p: usize, n: usize) -> u32 {
        self.ai[p].slots[n] as u32
    }

    /// The blacksmith's hold-back: slot 0x17 ≠ 0, no knights, slot 0x14 ≠ 0 and no footmen.
    fn knights_and_footmen_hold(&self, p: usize, g: &Groups) -> bool {
        self.slot(p, 0x17) != 0 && g.knights == 0 && self.slot(p, 0x14) != 0 && g.footmen == 0
    }

    /// The mage tower's, aviary's and inventor's hold-back: the blacksmith's, and a barracks complete (open question
    /// "Hold-back of the mage tower, aviary and inventor": implemented literally).
    fn advanced_hold(&self, p: usize, g: &Groups) -> bool {
        self.has_pair(p, b::BARRACKS) && self.knights_and_footmen_hold(p, g)
    }

    /// Tries each code of a research building in order, starting each one that is requested and whose conditions
    /// hold; only the first start can succeed.
    fn research_codes(&mut self, i: usize, codes: &[u8]) {
        let p = self.units[i].owner as usize;
        let orc = self.players[p].race == 1;
        for &code in codes {
            if !self.ai[p].requested(code) || !self.code_ready(p, code, orc) {
                continue;
            }
            let Some(job) = research_item(code, orc) else { continue };
            if self.ai_start(i, job) {
                self.mark_code_done(p, code);
                self.clear_request(p, code);
            }
        }
    }

    /// The "Also needs" column of the research code table.
    fn code_ready(&self, p: usize, code: u8, orc: bool) -> bool {
        let player = &self.players[p];
        let Some(job) = research_item(code, orc) else {
            return false;
        };
        let rangers = player.levels[RANGER_TRAINING_LINE] > 0;
        match job {
            JobKind::Research(n) if n < 24 => {
                self.level_ok(p as u8, n) && player.upgrades_researching & two_level_bit(n) == 0
            }
            JobKind::Research(_) => {
                let (line, bit) = ranger_line(code);
                let before = if code == 0x82 {
                    self.has_complete(p, &[b::KEEP, b::KEEP + 1, b::CASTLE, b::CASTLE + 1])
                } else {
                    rangers
                };
                before && player.levels[line] == 0 && player.upgrades_researching & bit == 0
            }
            JobKind::Spell(n) => {
                let bits = self.upgrade_data.0.get(n as usize).map_or(0, |e| e.affect);
                let church_ok = !matches!(code, 0x91 | 0x92) || player.spells_known & PALADIN_BIT != 0;
                church_ok && player.spells_known & bits == 0 && player.spells_researching & bits == 0
            }
            _ => false,
        }
    }

    /// The barracks rules; the unit type with the race bit added, or `None` (rule 10).
    fn barracks_choice(&mut self, p: usize, g: &Groups, race: u8) -> Option<u8> {
        let (lm, bs, st) = (
            self.has_pair(p, b::LUMBER_MILL),
            self.has_pair(p, b::BLACKSMITH),
            self.has_pair(p, b::STABLES),
        );
        let player = &self.players[p];
        let archer = if player.levels[RANGER_TRAINING_LINE] > 0 {
            u::RANGER
        } else {
            u::ARCHER
        } | race;
        let knight = if player.spells_known & PALADIN_BIT != 0 {
            u::PALADIN
        } else {
            u::KNIGHT
        } | race;
        let (footman, ballista) = (u::FOOTMAN | race, u::BALLISTA | race);
        let (s14, s15, s16, s17) = (
            self.slot(p, 0x14),
            self.slot(p, 0x15),
            self.slot(p, 0x16),
            self.slot(p, 0x17),
        );
        // Each condition is tested left to right, so a draw happens only once the tests before it hold.
        if s14 != 0 && g.footmen == 0 {
            Some(footman)
        } else if s17 != 0 && g.knights == 0 && bs && st {
            Some(knight)
        } else if (s15 != 0 && g.archers == 0 && lm) || (g.archers < s15 && self.chance(4) && lm) {
            Some(archer)
        } else if g.siege < s16 && self.chance(8) && bs && lm {
            Some(ballista)
        } else if g.knights < s17 && bs && st {
            Some(knight)
        } else if g.footmen < s14 {
            Some(footman)
        } else if s16 != 0 && g.siege == 0 && bs && lm {
            Some(ballista)
        } else if g.archers < s15 && lm {
            Some(archer)
        } else if g.siege < s16 && bs && lm {
            Some(ballista)
        } else {
            None
        }
    }

    /// The shipyard rules; the human unit type, or `None`.
    fn shipyard_choice(&mut self, p: usize, g: &Groups) -> Option<u8> {
        let f = self.has_pair(p, b::FOUNDRY);
        let inv = self.has_pair(p, b::INVENTOR);
        let (s18, s19, s1a, s1b, s1c) = (
            self.slot(p, 0x18),
            self.slot(p, 0x19),
            self.slot(p, 0x1a),
            self.slot(p, 0x1b),
            self.slot(p, 0x1c),
        );
        if s18 != 0 && g.tankers == 0 {
            Some(u::TANKER)
        } else if s19 != 0 && g.destroyers == 0 {
            Some(u::DESTROYER)
        } else if s1a != 0 && g.transports == 0 && f {
            Some(u::TRANSPORT)
        } else if g.destroyers < s19 && self.chance(4) {
            Some(u::DESTROYER)
        } else if g.battleships < s1b && self.chance(8) && f {
            Some(u::BATTLESHIP)
        } else if g.submarines < s1c && self.chance(8) && inv {
            Some(u::SUBMARINE)
        } else if g.transports < s1a && self.chance(8) && f {
            Some(u::TRANSPORT)
        } else if g.tankers < s18 {
            Some(u::TANKER)
        } else if g.destroyers < s19 {
            Some(u::DESTROYER)
        } else if g.battleships < s1b && f {
            Some(u::BATTLESHIP)
        } else if g.transports < s1a && f {
            Some(u::TRANSPORT)
        } else if g.submarines < s1c && inv {
            Some(u::SUBMARINE)
        } else {
            None
        }
    }

    /// "Halls": a worker, else the keep or castle upgrade.
    fn hall_think(&mut self, i: usize, p: usize, k: u8, g: &Groups) {
        let race = k & 1;
        if self.slot(p, 0x13) > g.workers {
            self.ai_start(i, JobKind::Train(u::WORKER | race));
            return;
        }
        let (code, to, ready) = match k & !1 {
            b::TOWN_HALL => (KEEP_CODE, b::KEEP, self.has_pair(p, b::BARRACKS)),
            b::KEEP => (
                CASTLE_CODE,
                b::CASTLE,
                self.has_pair(p, b::STABLES) && self.has_pair(p, b::BLACKSMITH) && self.has_pair(p, b::LUMBER_MILL),
            ),
            _ => return,
        };
        if self.ai[p].requested(code)
            && ready
            && self.ai_start(i, JobKind::Upgrade(to | race))
            && self.mark_code_done(p, code)
        {
            self.clear_request(p, code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::world::UnitId;
    use crate::world::tests::{field, run};
    use std::sync::Arc;
    use tidewater_formats::aibin::AiFile;
    use tidewater_formats::udta::{UnitStats, flags};

    const P: u8 = 1;

    /// A 40 x 40 field: player 1 a computer human player with plenty of stock and food, every building type a
    /// 2 x 2 building, every unit type a cheap 1 x 1 land unit. The second list gives every type interval 1 and the
    /// build list is `list`.
    fn base(list: &[u8]) -> World {
        let mut w = field(40, 40);
        w.players[P as usize].controller = controller::COMPUTER;
        for k in 0..0x3a {
            w.stats.0[k] = UnitStats {
                hit_points: 10,
                size: (1, 1),
                flags: flags::LAND,
                gold: 10,
                build_time: 10,
                ..Default::default()
            };
        }
        for k in 0x3a..0x6a {
            w.stats.0[k] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING,
                gold: 10,
                build_time: 10,
                ..Default::default()
            };
        }
        let p = &mut w.players[P as usize];
        (p.gold, p.lumber, p.oil) = (50_000, 50_000, 50_000);
        let mut data = vec![0u8; 256];
        data.extend_from_slice(list);
        data.resize(512, 0xff);
        // The second list at 512: interval 1 for every type from 0x3A.
        for _ in 0x3a..0x6a {
            data.extend(1u16.to_le_bytes());
        }
        w.ai_file = Some(Arc::new(AiFile { data }));
        w.ai[P as usize].build_list = 256;
        w.ai[P as usize].second_list = 512;
        w.ai[P as usize].slots[BUILD_LIMIT] = 0xff;
        // Farms for food.
        for x in 0..4 {
            w.spawn(0x3a, P, (30 + 2 * (x % 2), 30 + 2 * (x / 2)), 0).unwrap();
        }
        w
    }

    /// Places a complete building of type `k` for player 1; spots by type keep them apart.
    fn put(w: &mut World, k: u8) -> UnitId {
        let n = (k - 0x3a) as i32;
        w.spawn(k, P, ((n % 8) * 3, (n / 8) * 3), 0).unwrap()
    }

    fn at(w: &mut World, id: UnitId) -> &mut crate::world::Unit {
        let i = w.index(id).unwrap();
        &mut w.units[i]
    }

    fn tick(w: &mut World, id: UnitId) {
        let i = w.index(id).unwrap();
        w.building_think_tick(i);
    }

    /// `n` units of type `k` for player 1 in row `y`.
    fn units(w: &mut World, k: u8, n: i32, y: i32) {
        for x in 0..n {
            w.spawn(k, P, (x, 25 + y), 0).unwrap();
        }
    }

    fn slots(w: &mut World, s: &[(usize, u8)]) {
        for &(n, v) in s {
            w.ai[P as usize].slots[n] = v;
        }
    }

    fn job(w: &World, id: UnitId) -> Option<JobKind> {
        w.unit(id).unwrap().job.map(|j| j.kind)
    }

    /// Runs one think of building `i` and returns its job and the number of draws made.
    fn think(w: &mut World, id: UnitId) -> (Option<JobKind>, u32) {
        let before = w.rng;
        w.think(w.index(id).unwrap());
        let mut r = before;
        let mut n = 0;
        while r != w.rng {
            r.draw();
            n += 1;
            assert!(n < 100);
        }
        (job(w, id), n)
    }

    #[test]
    fn interval_1_thinks_on_every_third_run_and_0xffff_never() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::BARRACKS);
        slots(&mut w, &[(0x14, 9)]);
        let mut thinks = vec![];
        for run in 1..=9 {
            tick(&mut w, i);
            if at(&mut w, i).job.is_some() {
                thinks.push(run);
                at(&mut w, i).job = None;
            }
        }
        assert_eq!(thinks, [3, 6, 9]);
        // 0xFFFF: never, and the counter doesn't move.
        let mut data = (**w.ai_file.as_ref().unwrap()).clone();
        data.data[512..514].copy_from_slice(&NEVER.to_le_bytes());
        data.data[514 + 2..514 + 4].copy_from_slice(&NEVER.to_le_bytes());
        w.ai_file = Some(Arc::new(data));
        assert_eq!(w.think_interval(1, b::BARRACKS), NEVER);
        let c = at(&mut w, i).think;
        for _ in 0..10 {
            tick(&mut w, i);
        }
        assert_eq!((at(&mut w, i).job, at(&mut w, i).think), (None, c));
    }

    #[test]
    fn a_resting_barracks_thinks_from_its_order_logic() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::BARRACKS);
        slots(&mut w, &[(0x14, 1)]);
        // The still animation has a "step done" every 5 cycles; the 3rd run starts a footman.
        run(&mut w, 30);
        assert_eq!(job(&w, i), Some(JobKind::Train(u::FOOTMAN)));
        // A human player's barracks never thinks.
        let mut w = base(&[0xff]);
        w.players[P as usize].controller = controller::HUMAN;
        let i = put(&mut w, b::BARRACKS);
        slots(&mut w, &[(0x14, 1)]);
        run(&mut w, 60);
        assert_eq!((job(&w, i), at(&mut w, i).think), (None, 0));
    }

    /// Steps the world once per entry and records the building's wait after each step (one trace record each).
    fn waits(w: &mut World, id: UnitId, n: usize) -> Vec<u8> {
        (0..n)
            .map(|_| {
                w.step(&[]);
                w.unit(id).unwrap().script.wait
            })
            .collect()
    }

    #[test]
    fn a_new_buildings_resting_order_takes_over_at_its_first_step_done() {
        // Orders spec, "Taking over", Human01 vector: wait 1 at setup takes over at n = 1, wait 2 at n = 2; then
        // a "step done" every 5 cycles.
        let mut w = base(&[0xff]);
        let first = put(&mut w, b::BARRACKS);
        let second = put(&mut w, b::LUMBER_MILL);
        at(&mut w, first).script.wait = 1;
        at(&mut w, second).script.wait = 2;
        assert!(at(&mut w, first).pending.is_some());
        w.step(&[]);
        assert_eq!((at(&mut w, first).pending, at(&mut w, first).script.wait), (None, 3));
        assert_eq!(at(&mut w, second).script.wait, 1);
        assert!(at(&mut w, second).pending.is_some());
        w.step(&[]);
        assert_eq!((at(&mut w, second).pending, at(&mut w, second).script.wait), (None, 3));
        // n = 3 on: the first building's step done at n = 4 (the second 1), then every 5 cycles.
        assert_eq!(at(&mut w, first).script.wait, 2);
        assert_eq!(waits(&mut w, first, 8), [1, 1, 4, 3, 2, 1, 1, 4]);
    }

    #[test]
    fn a_take_over_replaces_the_logic_run_so_a_hall_thinks_at_n_21() {
        // Orders spec, "Taking over", Human04 great hall vector: created with wait 8, the resting order takes over at
        // n = 8 (wait 3); its logic runs at n = 11, 16, 21 and the third run thinks; the job takes over at n = 26.
        let mut w = base(&[0xff]);
        let hall = put(&mut w, b::BARRACKS);
        slots(&mut w, &[(0x14, 9)]);
        at(&mut w, hall).script.wait = 8;
        let mut runs = vec![];
        let mut thought = 0;
        let mut working = 0;
        for n in 1..=30 {
            let before = at(&mut w, hall).think;
            w.step(&[]);
            let u = at(&mut w, hall);
            if n == 8 {
                assert_eq!((u.pending, u.script.wait), (None, 3), "take-over at n = 8");
            }
            if u.think != before || (thought == 0 && u.job.is_some()) {
                runs.push(n);
            }
            if thought == 0 && u.job.is_some() {
                thought = n;
            }
            if working == 0 && u.job.is_some_and(|j| j.running) {
                working = n;
            }
        }
        assert_eq!((runs, thought, working), (vec![11, 16, 21], 21, 26));
    }

    #[test]
    fn after_training_the_resting_order_takes_over_at_once() {
        // Human04 vector: the peon appears at n = 489; order 33 takes over at once with wait 3, its logic runs at
        // n = 492, 497, 502, and it thinks at the third run.
        let mut w = base(&[0xff]);
        let hall = put(&mut w, b::BARRACKS);
        slots(&mut w, &[(0x14, 9)]);
        let units = |w: &World| w.units.len();
        let mut n0 = units(&w);
        let mut out = 0;
        for n in 1..=200 {
            w.step(&[]);
            if units(&w) > n0 {
                out = n;
                break;
            }
            n0 = units(&w);
        }
        assert!(out > 0, "a footman was trained");
        let u = at(&mut w, hall);
        assert_eq!((u.job, u.pending, u.script.wait), (None, None, 3));
        let mut thought = 0;
        for n in out + 1..=out + 20 {
            w.step(&[]);
            if at(&mut w, hall).job.is_some() {
                thought = n;
                break;
            }
        }
        assert_eq!(thought, out + 13);
    }

    #[test]
    fn barracks_footman_first_without_a_draw() {
        let mut w = base(&[0xff]);
        put(&mut w, b::LUMBER_MILL);
        let i = put(&mut w, b::BARRACKS);
        slots(&mut w, &[(0x14, 2), (0x15, 2)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::FOOTMAN)), 0));
    }

    #[test]
    fn barracks_archers_now_without_a_draw() {
        let mut w = base(&[0xff]);
        put(&mut w, b::LUMBER_MILL);
        let i = put(&mut w, b::BARRACKS);
        units(&mut w, u::FOOTMAN, 1, 0);
        slots(&mut w, &[(0x14, 2), (0x15, 2)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::ARCHER)), 0));
    }

    #[test]
    fn barracks_without_a_lumber_mill_draws_once_and_trains_nothing() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::BARRACKS);
        units(&mut w, u::FOOTMAN, 1, 0);
        slots(&mut w, &[(0x14, 1), (0x15, 2)]);
        assert_eq!(think(&mut w, i), (None, 1));
        // With rangers researched and a lumber mill: a ranger.
        put(&mut w, b::LUMBER_MILL);
        w.players[P as usize].levels[RANGER_TRAINING_LINE] = 1;
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::RANGER)), 0));
    }

    #[test]
    fn barracks_knight_choice() {
        for (race, paladin, want) in [(0, false, 0x06), (0, true, 0x0c), (1, false, 0x07), (1, true, 0x0d)] {
            let mut w = base(&[0xff]);
            w.players[P as usize].race = race;
            put(&mut w, b::BLACKSMITH);
            put(&mut w, b::STABLES);
            let i = put(&mut w, b::BARRACKS + race);
            units(&mut w, u::FOOTMAN | race, 2, 0);
            slots(&mut w, &[(0x14, 2), (0x17, 1)]);
            if paladin {
                w.players[P as usize].spells_known |= PALADIN_BIT;
            }
            assert_eq!(think(&mut w, i), (Some(JobKind::Train(want)), 0));
        }
    }

    #[test]
    fn barracks_draws_of_rules_3_and_4_in_one_think() {
        // Archers below the slot and siege below the slot: draw for rule 3, and if it fails, rule 4.
        let mut w = base(&[0xff]);
        put(&mut w, b::LUMBER_MILL);
        put(&mut w, b::BLACKSMITH);
        let i = put(&mut w, b::BARRACKS);
        units(&mut w, u::ARCHER, 1, 0);
        slots(&mut w, &[(0x15, 2), (0x16, 1)]);
        let mut r = w.rng;
        let (a, b) = (r.draw(), r.draw());
        let (got, draws) = think(&mut w, i);
        if a % 4 == 1 {
            assert_eq!((got, draws), (Some(JobKind::Train(u::ARCHER)), 1));
        } else if b % 8 == 1 {
            assert_eq!((got, draws), (Some(JobKind::Train(u::BALLISTA)), 2));
        } else {
            // Rule 7: slot 0x16 ≠ 0 and no siege.
            assert_eq!((got, draws), (Some(JobKind::Train(u::BALLISTA)), 2));
        }
        // Seeds where the draws fail: rule 7 still gives a ballista, rule 8 an archer once siege is there.
        let mut w = base(&[0xff]);
        put(&mut w, b::LUMBER_MILL);
        put(&mut w, b::BLACKSMITH);
        let i = put(&mut w, b::BARRACKS);
        units(&mut w, u::ARCHER, 1, 0);
        units(&mut w, u::BALLISTA, 1, 1);
        slots(&mut w, &[(0x15, 2), (0x16, 1)]);
        w.rng = crate::Rng::new(0); // first draw 0: 0 mod 4 ≠ 1; rule 4 doesn't apply (1 siege)
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::ARCHER)), 1));
    }

    #[test]
    fn shipyard_vectors() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::SHIPYARD);
        units(&mut w, u::TANKER, 1, 0);
        slots(&mut w, &[(0x18, 1), (0x19, 2)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::DESTROYER)), 0));
        // Draws 2 then 9: rule 4 fails, rule 5 gives a battleship. Seed the generator to give them.
        let mut w = base(&[0xff]);
        put(&mut w, b::FOUNDRY);
        let i = put(&mut w, b::SHIPYARD);
        units(&mut w, u::DESTROYER, 1, 0);
        slots(&mut w, &[(0x19, 2), (0x1b, 1)]);
        w.rng = seed_for(&[(4, 2), (8, 1)]);
        let mut r = w.rng;
        assert_eq!((r.draw() % 4, r.draw() % 8), (2, 1));
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::BATTLESHIP)), 2));
        // Orc shipyard: the race bit is added.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::SHIPYARD + 1);
        slots(&mut w, &[(0x18, 1)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::TANKER + 1)), 0));
    }

    /// A generator state whose next draws give the residues asked for, `(modulus, residue)`.
    fn seed_for(want: &[(u32, u32)]) -> crate::Rng {
        (0..u32::MAX)
            .map(crate::Rng::new)
            .find(|r| {
                let mut r = *r;
                want.iter().all(|&(m, v)| r.draw() % m == v)
            })
            .unwrap()
    }

    #[test]
    fn town_hall_upgrades_to_a_keep_on_request() {
        let mut w = base(&[0x3c, 0x98, 0x98, 0xff]);
        put(&mut w, b::BARRACKS);
        let i = put(&mut w, b::TOWN_HALL);
        units(&mut w, u::WORKER, 5, 0);
        slots(&mut w, &[(0x13, 5)]);
        w.ai[1].request(0x98);
        assert_eq!(think(&mut w, i), (Some(JobKind::Upgrade(b::KEEP)), 0));
        assert_eq!(w.ai[1].done, 0b110);
        assert!(!w.ai[1].requested(0x98));
        // One more worker wanted: a peasant, no keep.
        let mut w = base(&[0x98, 0xff]);
        put(&mut w, b::BARRACKS);
        let i = put(&mut w, b::TOWN_HALL);
        units(&mut w, u::WORKER, 5, 0);
        slots(&mut w, &[(0x13, 6)]);
        w.ai[1].request(0x98);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::WORKER)), 0));
        assert!(w.ai[1].requested(0x98));
        // No entry in the list: the upgrade starts but the request stays.
        let mut w = base(&[0xff]);
        put(&mut w, b::BARRACKS);
        let i = put(&mut w, b::TOWN_HALL);
        w.ai[1].request(0x98);
        assert_eq!(think(&mut w, i), (Some(JobKind::Upgrade(b::KEEP)), 0));
        assert!(w.ai[1].requested(0x98));
    }

    #[test]
    fn keep_needs_stables_blacksmith_and_lumber_mill_for_the_castle() {
        let mut w = base(&[0x99, 0xff]);
        let i = put(&mut w, b::KEEP + 1);
        w.players[P as usize].race = 1;
        w.ai[1].request(0x99);
        put(&mut w, b::STABLES);
        put(&mut w, b::BLACKSMITH);
        assert_eq!(think(&mut w, i), (None, 0));
        put(&mut w, b::LUMBER_MILL + 1);
        assert_eq!(think(&mut w, i), (Some(JobKind::Upgrade(b::CASTLE + 1)), 0));
        assert_eq!(w.ai[1].done, 1);
        // Castles only train workers.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::CASTLE);
        assert_eq!(think(&mut w, i), (None, 0));
        slots(&mut w, &[(0x13, 1)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::WORKER)), 0));
    }

    #[test]
    fn lumber_mill_research() {
        let mut w = base(&[0x80, 0x3c, 0x80, 0xff]);
        let i = put(&mut w, b::LUMBER_MILL);
        w.ai[1].request(0x80);
        w.ai[1].request(0x81);
        assert_eq!(think(&mut w, i), (Some(JobKind::Research(4)), 0));
        assert_eq!(w.ai[1].done, 0b101);
        assert!(!w.ai[1].requested(0x80) && w.ai[1].requested(0x81));
        // Hold-back: slot 0x15 = 3 and no archers.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::LUMBER_MILL);
        w.ai[1].request(0x80);
        slots(&mut w, &[(0x15, 3)]);
        assert_eq!(think(&mut w, i), (None, 0));
        // Rangers need a keep or castle.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::LUMBER_MILL);
        w.ai[1].request(0x82);
        assert_eq!(think(&mut w, i), (None, 0));
        put(&mut w, b::CASTLE + 1);
        assert_eq!(think(&mut w, i), (Some(JobKind::Research(24)), 0));
        // Orc player: the orc item.
        let mut w = base(&[0xff]);
        w.players[P as usize].race = 1;
        let i = put(&mut w, b::LUMBER_MILL + 1);
        w.ai[1].request(0x81);
        w.players[P as usize].levels[0] = 1;
        assert_eq!(think(&mut w, i), (Some(JobKind::Research(7)), 0));
    }

    #[test]
    fn arrow_2_waits_for_level_1_and_not_in_progress() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::LUMBER_MILL);
        w.ai[1].request(0x81);
        assert_eq!(think(&mut w, i), (None, 0));
        w.players[P as usize].levels[0] = 1;
        w.players[P as usize].upgrades_researching = 1 << 1;
        assert_eq!(think(&mut w, i), (None, 0));
        w.players[P as usize].upgrades_researching = 1;
        assert_eq!(think(&mut w, i), (Some(JobKind::Research(5)), 0));
    }

    #[test]
    fn blacksmith_order_and_hold_back() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::BLACKSMITH);
        for c in [0x87, 0x88, 0x8a] {
            w.ai[1].request(c);
        }
        w.players[P as usize].levels[1] = 1;
        // 0x86 not requested; 0x88 (shield 1) comes before 0x87 (sword 2).
        assert_eq!(think(&mut w, i), (Some(JobKind::Research(8)), 0));
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::BLACKSMITH);
        w.ai[1].request(0x86);
        slots(&mut w, &[(0x17, 1), (0x14, 1)]);
        assert_eq!(think(&mut w, i), (None, 0));
        units(&mut w, u::FOOTMAN, 1, 0);
        assert_eq!(think(&mut w, i), (Some(JobKind::Research(0)), 0));
    }

    #[test]
    fn church_needs_paladins_for_its_spells() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::CHURCH);
        w.ai[1].request(0x91);
        assert_eq!(think(&mut w, i), (None, 0));
        w.players[P as usize].spells_known = PALADIN_BIT;
        assert_eq!(think(&mut w, i), (Some(JobKind::Spell(35)), 0));
        // Hold-back: knights wanted, none.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::CHURCH);
        w.ai[1].request(0x90);
        slots(&mut w, &[(0x17, 1)]);
        assert_eq!(think(&mut w, i), (None, 0));
    }

    #[test]
    fn mage_tower_trains_before_and_after_research() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::MAGE_TOWER);
        w.ai[1].request(0x94);
        slots(&mut w, &[(0x1d, 2)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::MAGE)), 0));
        assert!(w.ai[1].requested(0x94));
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::MAGE_TOWER);
        units(&mut w, u::MAGE, 1, 0);
        w.ai[1].request(0x94);
        slots(&mut w, &[(0x1d, 2)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Spell(39)), 0));
        // No research: the second mage.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::MAGE_TOWER);
        units(&mut w, u::MAGE, 1, 0);
        slots(&mut w, &[(0x1d, 2)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::MAGE)), 0));
        // Hold-back needs a barracks.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::MAGE_TOWER);
        slots(&mut w, &[(0x1d, 2), (0x17, 1), (0x14, 1)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::MAGE)), 0));
        at(&mut w, i).job = None;
        put(&mut w, b::BARRACKS);
        assert_eq!(think(&mut w, i), (None, 0));
    }

    #[test]
    fn aviary_inventor_and_scout_tower() {
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::AVIARY + 1);
        slots(&mut w, &[(0x20, 1)]);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::GRYPHON + 1)), 0));
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::INVENTOR);
        slots(&mut w, &[(0x1e, 1)]);
        assert_eq!(think(&mut w, i), (None, 0));
        put(&mut w, b::LUMBER_MILL);
        assert_eq!(think(&mut w, i), (Some(JobKind::Train(u::FLYING_MACHINE)), 0));
        // Scout tower: guard tower at its first think, if it may.
        let mut w = base(&[0xff]);
        let i = put(&mut w, b::SCOUT_TOWER);
        assert_eq!(think(&mut w, i), (None, 0));
        put(&mut w, b::LUMBER_MILL);
        for _ in 0..3 {
            tick(&mut w, i);
        }
        assert_eq!(job(&w, i), Some(JobKind::Upgrade(b::GUARD_TOWER)));
    }

    #[test]
    fn held_units_leave_the_groups_but_not_the_workers_and_tankers() {
        let mut w = base(&[0xff]);
        units(&mut w, 0x2e, 1, 0);
        units(&mut w, u::WORKER, 2, 1);
        units(&mut w, u::TANKER, 1, 2);
        units(&mut w, 0x13, 1, 3);
        for unit in &mut w.units {
            if unit.kind != 0x13 {
                unit.held = !unit.building;
            }
        }
        let g = w.ai_groups(1);
        assert_eq!((g.footmen, g.archers, g.workers, g.tankers), (0, 1, 2, 1));
    }

    #[test]
    fn no_workers_pass_requests_the_first_open_code() {
        let mut w = base(&[0x4a, 0x3c, 0x80, 0x81, 0xff]);
        w.ai[1].done = 0b1;
        w.no_workers_research_pass();
        assert_eq!(w.ai[1].research_requests, 1);
        // Done codes are passed over; a worker stops the pass; the limit cuts the list.
        w.ai[1].research_requests = 0;
        w.ai[1].done = 0b101;
        w.no_workers_research_pass();
        assert_eq!(w.ai[1].research_requests, 0b10);
        w.ai[1].research_requests = 0;
        slots(&mut w, &[(BUILD_LIMIT, 3)]);
        w.no_workers_research_pass();
        assert_eq!(w.ai[1].research_requests, 0);
        slots(&mut w, &[(BUILD_LIMIT, 0xff)]);
        units(&mut w, u::WORKER, 1, 0);
        w.no_workers_research_pass();
        assert_eq!(w.ai[1].research_requests, 0);
    }

    #[test]
    fn codes_above_0x9a_are_ignored() {
        let mut a = AiPlayer::default();
        a.request(0x9b);
        a.request(0x7f);
        assert_eq!(a.research_requests, 0);
        a.request(0x9a);
        assert!(a.requested(0x9a) && !a.requested(0x9b));
        assert_eq!(research_item(0x98, false), None);
        assert_eq!(research_item(0x97, true), Some(JobKind::Spell(51)));
    }
}
