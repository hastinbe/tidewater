//! Computer workers and tankers choosing their own jobs: returning goods, new town halls, farms, the build list,
//! repair and harvest, with the keep-clear marks and the computer's site searches. Spec: `docs/specs/ai/worker-ai.md`.
//! Unit type numbers and ALOW bits are those of the public PUD specification.
//!
//! Our readings where the spec leaves room:
//! - "Computer-controlled" is the movement spec's computer player (computer slots and active rescue players).
//! - The ordinary search for a farm runs without the border; every other build-list building and the new-mine
//!   search use the border, matching the keep-clear mark each one gets.
//! - The prerequisites of a build-list entry are the build-button rules of `ui/command-buttons.md`.
//! - Coming out of a depot, a choice that ends in the wait gives no job, so the usual return to the old source goes
//!   ahead (the draw is still made).
//! - Build-list entries of 0x80 or more are recorded as research requests (acted on in `training_ai.rs`).

use tidewater_formats::udta::flags;

use crate::economy::cost;
use crate::orders::distance;
use crate::slots;
use crate::terrain::bits;
use crate::world::{Gather, Order, Unit, UnitId, World};

/// The keep-clear tile mark (occupancy spec, "Run-time marks").
pub const KEEP_CLEAR: u16 = 0x0400;
/// The computer's site-search masks: the placement masks plus the keep-clear bit.
pub const LAND_SITE: u16 = 0x0DDE;
pub const SHORE_SITE: u16 = 0x0D11;
/// Hall tries skipped after a hall is started.
const HALL_COOLDOWN: u8 = 10;
/// The build list holds at most 64 entries.
const LIST_MAX: usize = 64;

/// PUD types.
mod t {
    pub const FARM: u8 = 0x3a;
    pub const SHIPYARD: u8 = 0x48;
    pub const TOWN_HALL: u8 = 0x4a;
    pub const FOUNDRY: u8 = 0x4e;
    pub const REFINERY: u8 = 0x54;
    pub const PLATFORM: u8 = 0x56;
    pub const GOLD_MINE: u8 = 0x5c;
    pub const OIL_PATCH: u8 = 0x5d;
    pub const WALL: u8 = 0x67;
    pub const ORC_WALL: u8 = 0x68;
    pub const HALLS: &[u8] = &[0x4a, 0x4b, 0x58, 0x59, 0x5a, 0x5b];
    pub const KEEPS_UP: &[u8] = &[0x58, 0x59, 0x5a, 0x5b];
    pub const CASTLES: &[u8] = &[0x5a, 0x5b];
    pub const SHIPYARDS: &[u8] = &[0x48, 0x49];
    pub const LUMBER_MILLS: &[u8] = &[0x4c, 0x4d];
}

/// The job a computer worker holds, for the bookkeeping its release undoes.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AiJob {
    #[default]
    None,
    Repair,
    Gold,
    Lumber,
    Build {
        kind: u8,
        site: (i32, i32),
    },
}

/// What a choice came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Order(Order),
    /// The wait counter was set; no order.
    Wait,
    /// The reaction to an attacker gave the order, or (tankers) nothing happens.
    Nothing,
}

/// A building family's site search ("Ordinary search").
#[derive(Clone, Copy, Debug)]
struct Search {
    centre: (i32, i32),
    step: i32,
    /// Shipyards: the footprint must contain a tile of this water region.
    water: Option<u16>,
}

/// The tiles the keep-clear line marks from the depot's top-left tile `d` to the mine's `m` ("Keep-clear line").
pub fn keep_clear_line(d: (i32, i32), m: (i32, i32)) -> Vec<(i32, i32)> {
    if d.0 == m.0 || d.1 == m.1 {
        return Vec::new();
    }
    let (ax, ay) = ((m.0 - d.0).abs(), (m.1 - d.1).abs());
    let (sx, sy) = ((m.0 - d.0).signum(), (m.1 - d.1).signum());
    let x_major = ax >= ay;
    let (major, minor) = if x_major { (ax, ay) } else { (ay, ax) };
    let mut counter = (major / 2).max(1);
    let mut at = d;
    let mut marked = Vec::new();
    loop {
        let step_minor = {
            counter -= minor;
            counter <= 0
        };
        if step_minor {
            counter += major;
        }
        if x_major {
            at.0 += sx;
            if step_minor {
                at.1 += sy;
            }
        } else {
            at.1 += sy;
            if step_minor {
                at.0 += sx;
            }
        }
        marked.push(at);
        if at.0 == m.0 || at.1 == m.1 {
            return marked;
        }
    }
}

/// The positions of ring `k` with step `s` around `c`: clockwise from the top-left corner, corners twice.
fn ring(c: (i32, i32), k: i32, s: i32) -> impl Iterator<Item = (i32, i32)> {
    let r = k * s;
    let n = 2 * k;
    let top = (0..=n).map(move |j| (c.0 - r + j * s, c.1 - r));
    let right = (0..=n).map(move |j| (c.0 + r, c.1 - r + j * s));
    let bottom = (0..=n).map(move |j| (c.0 + r - j * s, c.1 + r));
    let left = (0..=n).map(move |j| (c.0 - r, c.1 + r - j * s));
    top.chain(right).chain(bottom).chain(left)
}

/// The lumberjacks wanted for gold `g`, lumber `l` and `w` workers ("Lumber target").
pub fn lumber_target(g: u32, l: u32, w: u32) -> u32 {
    if (g < 500 && w < 5) || l >= 2000 || (g < 1000 && l >= 500) {
        0
    } else if g < 1000 || l >= 500 {
        w / 2
    } else {
        3 * (w + 1) / 4
    }
}

/// The building type an entry or type `k` names for race `race` (the type's lowest bit).
/// Walls are the exception: human 0x67, orc 0x68.
pub(crate) fn for_race(k: u8, race: u8) -> u8 {
    if matches!(k, t::WALL | t::ORC_WALL) {
        t::WALL + race
    } else {
        (k & !1) | race
    }
}

impl World {
    fn is_ai_tanker(&self, u: &Unit) -> bool {
        let f = self.stats_of(u.kind).flags;
        f & flags::TANKER != 0 || (f & flags::PEON != 0 && f & flags::SEA != 0)
    }

    /// Whether the job choice runs for unit `i`: a computer player's worker or tanker.
    pub(crate) fn chooses_jobs(&self, i: usize) -> bool {
        let u = &self.units[i];
        let f = self.stats_of(u.kind).flags;
        (u.owner as usize) < self.ai.len()
            && self.computer_player(u.owner)
            && !u.building
            && f & (flags::PEON | flags::TANKER) != 0
    }

    /// The idle-order hook: makes the choice and gives its order.
    pub(crate) fn idle_choice(&mut self, i: usize) {
        if let Choice::Order(o) = self.worker_choice(i) {
            self.give(i, o);
        }
    }

    /// The job choice ("Worker: the steps, in order", and "Tankers").
    pub(crate) fn worker_choice(&mut self, i: usize) -> Choice {
        if self.is_ai_tanker(&self.units[i]) {
            return self.tanker_choice(i);
        }
        self.release_job(i);
        if self.react(i) {
            return Choice::Nothing;
        }
        let depot = self.nearest_depot(&self.units[i]);
        let carrying = self.units[i].carrying;
        match depot {
            Some(d) if carrying => Choice::Order(Order::ReturnGoods { depot: Some(d) }),
            None => {
                if let Some(o) = self.try_hall(i) {
                    return Choice::Order(o);
                }
                if !carrying && let Some(o) = self.try_repair(i).or_else(|| self.try_harvest(i)) {
                    return Choice::Order(o);
                }
                self.ai_wait(i)
            }
            Some(_) => {
                let job = self
                    .try_farm(i)
                    .or_else(|| self.try_build_list(i))
                    .or_else(|| self.try_repair(i))
                    .or_else(|| self.try_harvest(i));
                match job {
                    Some(o) => Choice::Order(o),
                    None => self.ai_wait(i),
                }
            }
        }
    }

    /// "The wait": (r AND 15) + 44 cycles.
    fn ai_wait(&mut self, i: usize) -> Choice {
        let r = self.rng.draw();
        self.units[i].script.wait = ((r & 15) + 44) as u8;
        Choice::Wait
    }

    /// Step 1: undo the bookkeeping of the worker's job. Also run when a computer worker dies.
    pub(crate) fn release_job(&mut self, i: usize) {
        let job = std::mem::take(&mut self.units[i].ai_job);
        let p = self.units[i].owner as usize;
        let Some(a) = self.ai.get_mut(p) else { return };
        match job {
            AiJob::None => {}
            AiJob::Repair => a.repairers = a.repairers.saturating_sub(1),
            AiJob::Gold => a.miners = a.miners.saturating_sub(1),
            AiJob::Lumber => a.lumberjacks = a.lumberjacks.saturating_sub(1),
            AiJob::Build { kind, site } => {
                a.in_progress[kind as usize] = a.in_progress[kind as usize].saturating_sub(1);
                let w = self.stats_of(kind).size.0 as i32;
                self.keep_clear(site, w, false, false);
            }
        }
    }

    /// Sets or clears the keep-clear mark on a footprint of `w` x `w` tiles, or with the border (w + 2) x (w + 2)
    /// from one tile up and left (from the site itself on a map edge).
    fn keep_clear(&mut self, site: (i32, i32), w: i32, border: bool, set: bool) {
        let (x0, y0, x1, y1) = footprint(site, w, border);
        for y in y0..y1 {
            for x in x0..x1 {
                if set {
                    self.terrain.set_bits(x, y, KEEP_CLEAR);
                } else {
                    self.terrain.clear_bits(x, y, KEEP_CLEAR);
                }
            }
        }
    }

    fn race_of(&self, i: usize) -> u8 {
        self.units[i].kind & 1
    }

    fn affordable(&self, owner: u8, kind: u8) -> bool {
        self.can_afford(owner, cost(&self.stats_of(kind)), false).is_ok()
    }

    /// Gives the build command for `kind` at `site` (computer players pay at once) and records the job.
    fn start_building(&mut self, i: usize, kind: u8, site: (i32, i32), border: bool) -> Option<Order> {
        let o = self.build_command(i, kind, site)?;
        let p = self.units[i].owner as usize;
        self.ai[p].in_progress[kind as usize] += 1;
        let w = self.stats_of(kind).size.0 as i32;
        self.keep_clear(site, w, border, true);
        self.units[i].ai_job = AiJob::Build { kind, site };
        Some(o)
    }

    /// Step 5: a new town hall when no depot is reachable.
    fn try_hall(&mut self, i: usize) -> Option<Order> {
        let owner = self.units[i].owner;
        let p = owner as usize;
        if self.players[p].units_allowed & (1 << 26) == 0 {
            return None;
        }
        if self.ai[p].hall_cooldown > 0 {
            self.ai[p].hall_cooldown -= 1;
            return None;
        }
        let hall = for_race(t::TOWN_HALL, self.race_of(i));
        if self.ai[p].in_progress[hall as usize] > 0 || !self.affordable(owner, hall) {
            return None;
        }
        let search = self.mine_centred(i)?;
        let (site, border) = [true, false]
            .into_iter()
            .find_map(|b| self.site_search(i, hall, search, b).map(|s| (s, b)))?;
        let o = self.start_building(i, hall, site, border)?;
        self.ai[p].hall_cooldown = HALL_COOLDOWN;
        Some(o)
    }

    /// "Farm".
    fn try_farm(&mut self, i: usize) -> Option<Order> {
        let owner = self.units[i].owner;
        let p = &self.players[owner as usize];
        if !p.had_hall || p.units_allowed & (1 << 16) == 0 {
            return None;
        }
        let farm = for_race(t::FARM, self.race_of(i));
        let supply = self.food_supply(owner);
        let in_progress = self.ai[owner as usize].in_progress[farm as usize] as u32;
        if supply >= 200 || supply + 4 * in_progress > self.food_used(owner) + 3 || !self.affordable(owner, farm) {
            return None;
        }
        let search = self.family_search(i, farm)?;
        let site = self.site_search(i, farm, search, false)?;
        self.start_building(i, farm, site, false)
    }

    /// The build-button rules (`ui/command-buttons.md`, "Build buttons") for building type `k`.
    fn may_build(&self, owner: u8, k: u8) -> bool {
        let p = &self.players[owner as usize];
        let hall = p.had_hall;
        let has = |kinds: &[u8]| self.has_complete(owner as usize, kinds);
        if matches!(k, t::WALL | t::ORC_WALL) {
            return p.units_allowed & (1 << 31) != 0 && hall && self.rules.multiplayer;
        }
        let (bit, also) = match k & !1 {
            0x3a => (16, hall),
            0x3c => (17, hall),
            0x4a => (26, true),
            0x4c => (18, hall),
            0x52 => (29, hall),
            0x40 => (25, hall),
            0x48 => (30, hall && has(t::LUMBER_MILLS)),
            0x4e => (21, hall && has(t::SHIPYARDS)),
            0x54 => (22, hall && has(t::SHIPYARDS)),
            0x44 => (23, has(t::KEEPS_UP)),
            0x42 => (19, has(t::KEEPS_UP)),
            0x3e => (24, has(t::CASTLES)),
            0x50 => (20, has(t::CASTLES)),
            0x46 => (15, has(t::CASTLES)),
            _ => return false,
        };
        p.units_allowed & (1 << bit) != 0 && also
    }

    /// The player's build list (`ai/ai-scripts.md`), read from the script file.
    pub(crate) fn build_list(&self, p: usize) -> Vec<u8> {
        let Some(file) = &self.ai_file else {
            return Vec::new();
        };
        let at = self.ai[p].build_list as usize;
        (0..LIST_MAX).map_while(|n| file.u8(at + n)).collect()
    }

    /// "Build list".
    fn try_build_list(&mut self, i: usize) -> Option<Order> {
        let owner = self.units[i].owner;
        let p = owner as usize;
        let race = self.race_of(i);
        let list = self.build_list(p);
        let limit = (self.ai[p].slots[crate::ai::BUILD_LIMIT] as usize).min(LIST_MAX);
        for (n, &e) in list.iter().enumerate().take(limit) {
            if e == 0xff {
                break;
            }
            if self.ai[p].done & (1 << n) != 0 {
                continue;
            }
            if e >= 0x80 {
                self.ai[p].request(e);
                return None;
            }
            let kind = for_race(e, race);
            if !self.may_build(owner, kind) || !self.affordable(owner, kind) {
                return None;
            }
            if self.ai[p].in_progress[kind as usize] > 0 {
                continue;
            }
            let farm = kind & !1 == t::FARM;
            let site = if kind & !1 == t::TOWN_HALL {
                self.new_mine_site(i, kind)
            } else {
                self.family_search(i, kind)
                    .and_then(|s| self.site_search(i, kind, s, false))
            };
            if let Some(site) = site {
                return self.start_building(i, kind, site, !farm);
            }
        }
        None
    }

    /// The done-mark rule when a computer player's building of type `k` is completed (`true`) or destroyed.
    pub(crate) fn mark_build_list(&mut self, owner: u8, k: u8, completed: bool) {
        let p = owner as usize;
        if p >= self.ai.len() || !self.computer_player(owner) {
            return;
        }
        let list = self.build_list(p);
        let a = &mut self.ai[p];
        for (n, &e) in list.iter().enumerate() {
            if e == 0xff {
                break;
            }
            if e >= 0x80 || for_race(e, k & 1) != k {
                continue;
            }
            let done = a.done & (1 << n) != 0;
            if completed && !done {
                a.done |= 1 << n;
                return;
            }
            if !completed && done {
                a.done &= !(1 << n);
                return;
            }
        }
    }

    /// The nearest of the player's gold-storing buildings, complete or not, in the worker's region, strictly nearest.
    fn nearest_gold_store(&self, i: usize) -> Option<(i32, i32)> {
        let u = &self.units[i];
        let region = self.terrain.region(u.tile.0, u.tile.1);
        let mut best: Option<(i32, (i32, i32))> = None;
        for b in self.owned(u.owner) {
            if b.building
                && !b.dying
                && self.stats_of(b.kind).flags & flags::GOLD_STORAGE != 0
                && self.terrain.region(b.tile.0, b.tile.1) == region
            {
                let d = distance(u.tile, b);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, b.tile));
                }
            }
        }
        best.map(|(_, t)| t)
    }

    /// The units of `owner` in owner-list order.
    fn owned(&self, owner: u8) -> impl Iterator<Item = &Unit> {
        self.slots
            .list(owner)
            .filter_map(|s| self.units.binary_search_by_key(&s, |u| u.slot).ok())
            .map(|n| &self.units[n])
    }

    /// The search for a town hall not from the build list: around the nearest gold mine with trips left in the
    /// worker's region.
    fn mine_centred(&self, i: usize) -> Option<Search> {
        let u = &self.units[i];
        let region = self.terrain.region(u.tile.0, u.tile.1);
        let mut best: Option<(i32, (i32, i32))> = None;
        for m in self.owned(slots::NEUTRAL) {
            if self.is_live_mine(m) && self.terrain.region(m.tile.0, m.tile.1) == region {
                let d = distance(u.tile, m);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, m.tile));
                }
            }
        }
        best.map(|(_, centre)| Search {
            centre,
            step: 1,
            water: None,
        })
    }

    fn is_live_mine(&self, m: &Unit) -> bool {
        m.kind == t::GOLD_MINE && !m.dying && m.resources > 0
    }

    /// The ordinary search's centre and step for building type `k` (the family table).
    fn family_search(&self, i: usize, k: u8) -> Option<Search> {
        let here = self.units[i].tile;
        let owner = self.units[i].owner;
        let store = || self.nearest_gold_store(i).unwrap_or(here);
        let plain = |step| {
            Some(Search {
                centre: store(),
                step,
                water: None,
            })
        };
        match k & !1 {
            t::FARM => plain(2),
            t::FOUNDRY | t::REFINERY => {
                if self.has_complete(owner as usize, t::SHIPYARDS) {
                    plain(1)
                } else {
                    None
                }
            }
            t::SHIPYARD => {
                let hall = self
                    .owned(owner)
                    .filter(|b| t::HALLS.contains(&b.kind) && !b.dying)
                    .min_by_key(|b| distance(here, b))?;
                let patch = self
                    .units
                    .iter()
                    .filter(|p| p.kind == t::OIL_PATCH && !p.dying)
                    .min_by_key(|p| distance(hall.tile, p))?;
                Some(Search {
                    centre: hall.tile,
                    step: 1,
                    water: self.terrain.region(patch.tile.0, patch.tile.1),
                })
            }
            t::TOWN_HALL => self.mine_centred(i),
            _ => plain(6),
        }
    }

    /// The ordinary search: rings of radius k * s around the centre, the first accepted position.
    fn site_search(&self, i: usize, k: u8, s: Search, border: bool) -> Option<(i32, i32)> {
        let region = {
            let u = &self.units[i];
            self.terrain.region(u.tile.0, u.tile.1)?
        };
        let n = self.terrain.width as i32;
        let (cx, cy) = s.centre;
        let reach = cx.max(n - cx).max(cy).max(n - cy) - 1;
        let mut ring_k = 1;
        while (2 * ring_k + 1) * s.step < reach {
            let mut in_region = false;
            for (x, y) in ring(s.centre, ring_k, s.step) {
                if self.terrain.region(x, y) != Some(region) {
                    continue;
                }
                in_region = true;
                if self.site_accepted(k, (x, y), border, s.water) {
                    return Some((x, y));
                }
            }
            if !in_region {
                return None;
            }
            ring_k += 1;
        }
        None
    }

    fn site_accepted(&self, k: u8, site: (i32, i32), border: bool, water: Option<u16>) -> bool {
        let st = self.stats_of(k);
        let w = st.size.0 as i32;
        let (x0, y0, x1, y1) = footprint(site, w, border);
        let (n, m) = (self.terrain.width as i32, self.terrain.height as i32);
        if x0 < 0 || y0 < 0 || x1 > n || y1 > m {
            return false;
        }
        let mask = if st.flags & flags::SHORE_BUILDING != 0 {
            SHORE_SITE
        } else {
            LAND_SITE
        };
        let clear = (y0..y1).all(|y| (x0..x1).all(|x| self.terrain.word(x, y) & mask == 0));
        let wet = water.is_none_or(|r| {
            (site.1..site.1 + w).any(|y| (site.0..site.0 + w).any(|x| self.terrain.region(x, y) == Some(r)))
        });
        clear && wet && self.site_ok(k, site).is_some()
    }

    /// "New-mine search": the mine whose site is nearest it (at most 10 tiles), earliest on ties.
    fn new_mine_site(&self, i: usize, hall: u8) -> Option<(i32, i32)> {
        let stores: Vec<(i32, i32)> = self
            .units
            .iter()
            .filter(|b| b.building && !b.dying && self.stats_of(b.kind).flags & flags::GOLD_STORAGE != 0)
            .map(|b| b.tile)
            .collect();
        let mut best: Option<(i32, (i32, i32))> = None;
        for m in self.owned(slots::NEUTRAL) {
            if m.kind != t::GOLD_MINE || m.dying || m.resources < 50 {
                continue;
            }
            let (mx, my) = m.tile;
            if stores
                .iter()
                .any(|&(x, y)| (mx - 5..=mx + 7).contains(&x) && (my - 5..=my + 7).contains(&y))
            {
                continue;
            }
            let s = Search {
                centre: m.tile,
                step: 1,
                water: None,
            };
            let Some(site) = self.site_search(i, hall, s, false) else {
                continue;
            };
            let d = distance(site, m);
            if d <= 10 && best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, site));
            }
        }
        best.map(|(_, site)| site)
    }

    /// "Repair".
    fn try_repair(&mut self, i: usize) -> Option<Order> {
        let owner = self.units[i].owner;
        let p = owner as usize;
        let workers = self.ai_counts(p).workers as i32;
        if self.ai[p].repairers as i32 > (workers - 1) / 2 {
            return None;
        }
        let low = self.players[p].gold < 200 && self.players[p].lumber < 100;
        let here = self.units[i].tile;
        let region = self.terrain.region(here.0, here.1);
        let mut best: Option<(u8, UnitId)> = None;
        for b in self.owned(owner) {
            let st = self.stats_of(b.kind);
            let ok = b.building
                && b.construction.is_none()
                && !b.dying
                && !b.removing
                && !b.inside
                && st.flags & flags::OIL_SOURCE == 0
                && self.terrain.region(b.tile.0, b.tile.1) == region
                && (b.hp as u32) < 3 * st.hit_points as u32 / 4
                && (!low || st.flags & flags::GOLD_STORAGE != 0);
            if ok && st.annoy_computer > best.map_or(0, |(v, _)| v) && self.repair_safe(b) {
                best = Some((st.annoy_computer, b.id));
            }
        }
        let (_, target) = best?;
        let o = self.repair_command(i, target)?;
        self.ai[p].repairers += 1;
        self.units[i].ai_job = AiJob::Repair;
        Some(o)
    }

    /// No unit that can attack, of a player the owner doesn't count as allied, in the 9 x 9 area.
    fn repair_safe(&self, b: &Unit) -> bool {
        let allies = self.players[b.owner as usize].allies | (1 << b.owner);
        let (x, y) = b.tile;
        for ty in y - 3..=y + 5 {
            for tx in x - 3..=x + 5 {
                for layer in [crate::world::Layer::Land, crate::world::Layer::Air] {
                    let Some(o) = self.occupant(layer, tx, ty).and_then(|o| self.unit(o)) else {
                        continue;
                    };
                    let foe = o.owner >= 8 || allies & (1 << o.owner) == 0;
                    if foe && !o.dying && self.stats_of(o.kind).flags & flags::CAN_ATTACK != 0 {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// "Harvest".
    fn try_harvest(&mut self, i: usize) -> Option<Order> {
        let owner = self.units[i].owner;
        let p = owner as usize;
        let w = self.ai_counts(p).workers;
        let want = lumber_target(self.players[p].gold, self.players[p].lumber, w);
        if (self.ai[p].lumberjacks as u32) < want
            && let Some(o) = self.try_lumber(i)
        {
            return Some(o);
        }
        self.units[i].gather = Gather::Gold;
        let here = self.units[i].tile;
        let region = self.terrain.region(here.0, here.1);
        let depot = self.nearest_depot(&self.units[i]).and_then(|d| self.unit(d));
        let mine = depot.and_then(|d| {
            let mut best: Option<(i32, UnitId)> = None;
            for m in self.owned(slots::NEUTRAL) {
                if self.is_live_mine(m) && self.terrain.region(m.tile.0, m.tile.1) == region {
                    let v = distance(d.tile, m) + distance(here, m) / 2;
                    if best.is_none_or(|(bv, _)| v < bv) {
                        best = Some((v, m.id));
                    }
                }
            }
            best.map(|(_, m)| (d.tile, m))
        });
        let Some((depot_tile, mine)) = mine else {
            return self.try_lumber(i);
        };
        let mine_tile = self.unit(mine).map(|m| m.tile)?;
        let o = self.harvest_command(i, Some(mine), mine_tile)?;
        self.ai[p].miners += 1;
        self.units[i].ai_job = AiJob::Gold;
        for (x, y) in keep_clear_line(depot_tile, mine_tile) {
            self.terrain.set_bits(x, y, KEEP_CLEAR);
        }
        Some(o)
    }

    fn try_lumber(&mut self, i: usize) -> Option<Order> {
        self.units[i].gather = Gather::Lumber;
        // Rings of radius k while 2k + 1 < floor(3N / 4), from the worker's tile (the depot's top-left tile while
        // the worker is still inside it).
        let rings = (self.terrain.width as i32 * 3 / 4 - 2) / 2;
        let tree = self.nearest_tree_within(self.units[i].tile, rings)?;
        let o = self.harvest_command(i, None, tree)?;
        let p = self.units[i].owner as usize;
        self.ai[p].lumberjacks += 1;
        self.units[i].ai_job = AiJob::Lumber;
        Some(o)
    }

    /// "Tankers".
    fn tanker_choice(&mut self, i: usize) -> Choice {
        if self.react(i) {
            return Choice::Nothing;
        }
        let Some(depot) = self.nearest_depot(&self.units[i]).and_then(|d| self.unit(d)) else {
            return Choice::Nothing;
        };
        let (depot_id, depot_tile) = (depot.id, depot.tile);
        if self.units[i].carrying {
            return Choice::Order(Order::ReturnGoods { depot: Some(depot_id) });
        }
        let owner = self.units[i].owner;
        let mut best: Option<(i32, UnitId, (i32, i32))> = None;
        for p in self.owned(owner) {
            if self.stats_of(p.kind).flags & flags::OIL_SOURCE != 0 && p.building && !p.dying && p.resources > 0 {
                let v = distance(depot_tile, p) + distance(p.tile, &self.units[i]) / 2;
                if best.is_none_or(|(bv, ..)| v < bv) {
                    best = Some((v, p.id, p.tile));
                }
            }
        }
        if let Some((_, platform, tile)) = best {
            return match self.harvest_command(i, Some(platform), tile) {
                Some(o) => Choice::Order(o),
                None => Choice::Nothing,
            };
        }
        let patch = self.units.iter().filter(|p| p.kind == t::OIL_PATCH && !p.dying).fold(
            None::<(i32, (i32, i32))>,
            |best, p| {
                let d = distance(depot_tile, p);
                if best.is_none_or(|(bd, _)| d < bd) {
                    Some((d, p.tile))
                } else {
                    best
                }
            },
        );
        let platform = for_race(t::PLATFORM, self.race_of(i));
        match patch {
            Some((_, tile)) if self.affordable(owner, platform) => match self.build_command(i, platform, tile) {
                Some(o) => Choice::Order(o),
                None => Choice::Nothing,
            },
            _ => Choice::Nothing,
        }
    }

    /// The keep-clear and occupancy word bit, for tests and tools.
    pub fn keep_clear_at(&self, x: i32, y: i32) -> bool {
        self.terrain.word(x, y) & KEEP_CLEAR != 0 && self.terrain.word(x, y) != u16::MAX
    }
}

/// The footprint corners (x0, y0) inclusive to (x1, y1) exclusive.
fn footprint(site: (i32, i32), w: i32, border: bool) -> (i32, i32, i32, i32) {
    if border {
        let x0 = if site.0 == 0 { 0 } else { site.0 - 1 };
        let y0 = if site.1 == 0 { 0 } else { site.1 - 1 };
        (x0, y0, site.0 + w + 1, site.1 + w + 1)
    } else {
        (site.0, site.1, site.0 + w, site.1 + w)
    }
}

const _: () = assert!(bits::BUILDING & KEEP_CLEAR == 0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::terrain::region;
    use crate::world::tests::field;
    use std::sync::Arc;
    use tidewater_formats::aibin::AiFile;
    use tidewater_formats::udta::UnitStats;

    const PEASANT: u8 = 0x02;
    const BARRACKS: u8 = 0x3c;

    fn building(hp: u16, size: u16, f: u32, annoy: u8) -> UnitStats {
        UnitStats {
            hit_points: hp,
            size: (size, size),
            flags: flags::BUILDING | f,
            gold: 500,
            lumber: 250,
            annoy_computer: annoy,
            ..Default::default()
        }
    }

    /// A 64 x 64 field (on a 32-tile map a step-6 search has no ring to try): player 1 a computer player with a town hall at (14, 14), plenty of stock, everything allowed
    /// and a hall in its past. Returns the world and the hall.
    fn town() -> (World, UnitId) {
        let mut w = field(64, 64);
        w.players[1].controller = controller::COMPUTER;
        w.stats.0[PEASANT as usize] = UnitStats {
            hit_points: 30,
            size: (1, 1),
            flags: flags::LAND | flags::PEON,
            ..Default::default()
        };
        w.stats.0[t::TOWN_HALL as usize] = building(1200, 4, flags::GOLD_STORAGE | flags::WOOD_STORAGE, 45);
        w.stats.0[t::FARM as usize] = building(400, 2, 0, 45);
        w.stats.0[BARRACKS as usize] = building(800, 3, 0, 35);
        w.stats.0[t::GOLD_MINE as usize] = building(25500, 3, flags::MINE, 0);
        let p = &mut w.players[1];
        p.gold = 5000;
        p.lumber = 5000;
        p.units_allowed = !0;
        p.had_hall = true;
        let hall = w.spawn(t::TOWN_HALL, 1, (14, 14), 0).unwrap();
        (w, hall)
    }

    fn peasants(w: &mut World, n: i32) -> Vec<usize> {
        let ids: Vec<UnitId> = (0..n).map(|k| w.spawn(PEASANT, 1, (10 + k, 10), 0).unwrap()).collect();
        ids.into_iter().map(|id| w.index(id).unwrap()).collect()
    }

    fn with_list(w: &mut World, list: &[u8]) {
        let mut data = vec![0u8; 256];
        data.extend_from_slice(list);
        w.ai_file = Some(Arc::new(AiFile { data }));
        w.ai[1].build_list = 256;
        w.ai[1].slots[crate::ai::BUILD_LIMIT] = 0xff;
    }

    #[test]
    fn a_farm_when_fewer_than_4_food_are_spare() {
        let (mut w, _) = town();
        // Supply 5 (hall 1 + a farm 4), food in use 3.
        w.spawn(t::FARM, 1, (2, 2), 0).unwrap();
        let i = peasants(&mut w, 3)[0];
        let gold = w.players[1].gold;
        let Choice::Order(Order::Build { kind, .. }) = w.worker_choice(i) else {
            panic!("no farm");
        };
        assert_eq!(kind, t::FARM);
        assert_eq!(w.ai[1].in_progress[t::FARM as usize], 1);
        assert_eq!(w.players[1].gold, gold - 500, "paid at once");
        // With one in progress, 5 + 4 > 3 + 3: nothing else to do, so the wait.
        let i = w
            .units
            .iter()
            .position(|u| u.kind == PEASANT && u.ai_job == AiJob::None)
            .unwrap();
        assert_eq!(w.worker_choice(i), Choice::Wait);
        assert!((44..=59).contains(&w.units[i].script.wait));
    }

    #[test]
    fn the_build_list_skips_work_in_progress_and_stops_at_research() {
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        with_list(&mut w, &[BARRACKS, 0x85, t::FARM, 0xff]);
        let ps = peasants(&mut w, 2);
        let a = w.units[ps[0]].id;
        let i = w.index(a).unwrap();
        assert!(matches!(
            w.worker_choice(i),
            Choice::Order(Order::Build { kind: BARRACKS, .. })
        ));
        let i = w.units.iter().position(|u| u.kind == PEASANT && u.id != a).unwrap();
        assert_eq!(w.worker_choice(i), Choice::Wait);
        assert_eq!(w.ai[1].research_requests, 1 << 5);
        assert_eq!(w.ai[1].in_progress[t::FARM as usize], 0);
    }

    #[test]
    fn an_unaffordable_entry_ends_the_scan() {
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        with_list(&mut w, &[BARRACKS, t::FARM, 0xff]);
        w.stats.0[BARRACKS as usize].gold = 6000;
        let i = peasants(&mut w, 1)[0];
        assert_eq!(w.worker_choice(i), Choice::Wait);
    }

    #[test]
    fn done_marks_follow_completion_and_loss() {
        let (mut w, _) = town();
        with_list(&mut w, &[BARRACKS, BARRACKS, 0xff]);
        w.mark_build_list(1, BARRACKS + 1, true);
        w.mark_build_list(1, BARRACKS + 1, true);
        assert_eq!(w.ai[1].done, 0b11, "an orc barracks matches after the race change");
        w.mark_build_list(1, BARRACKS, false);
        assert_eq!(w.ai[1].done, 0b10);
    }

    #[test]
    fn repair_takes_the_most_annoying_safe_building() {
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        let farm = w.spawn(t::FARM, 1, (2, 24), 0).unwrap();
        let barracks = w.spawn(BARRACKS, 1, (16, 24), 0).unwrap();
        let set = |w: &mut World, id, hp| {
            let n = w.index(id).unwrap();
            w.units[n].hp = hp;
        };
        set(&mut w, farm, 300);
        set(&mut w, barracks, 500);
        let i = peasants(&mut w, 1)[0];
        // The farm at 300/400 isn't below 300; the barracks is.
        assert_eq!(w.worker_choice(i), Choice::Order(Order::Repair { target: barracks }));
        let i = w.units.iter().position(|u| u.kind == PEASANT).unwrap();
        w.release_job(i);
        set(&mut w, farm, 299);
        assert_eq!(w.worker_choice(i), Choice::Order(Order::Repair { target: farm }));
        w.release_job(i);
        // Low stock: only gold stores are repaired.
        w.players[1].gold = 150;
        w.players[1].lumber = 50;
        assert_eq!(w.worker_choice(i), Choice::Wait);
        // An enemy that can attack, 4 tiles right of the farm's top-left: not safe.
        w.players[1].gold = 5000;
        w.spawn(0x00, 0, (6, 24), 0).unwrap();
        let i = w.units.iter().position(|u| u.kind == PEASANT).unwrap();
        assert_eq!(w.worker_choice(i), Choice::Order(Order::Repair { target: barracks }));
    }

    #[test]
    fn the_hall_cooldown_counts_down_and_a_carrier_waits() {
        let (mut w, hall) = town();
        let n = w.index(hall).unwrap();
        w.kill(n);
        let i = peasants(&mut w, 1)[0];
        w.units[i].carrying = true;
        w.units[i].gather = Gather::Gold;
        w.ai[1].hall_cooldown = 10;
        assert_eq!(w.worker_choice(i), Choice::Wait);
        assert_eq!(w.ai[1].hall_cooldown, 9);
    }

    #[test]
    fn gold_goes_to_the_mine_with_the_best_sum_and_marks_the_line() {
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        let near = w.spawn(t::GOLD_MINE, slots::NEUTRAL, (22, 20), 100).unwrap();
        w.spawn(t::GOLD_MINE, slots::NEUTRAL, (2, 26), 100).unwrap();
        let i = peasants(&mut w, 1)[0];
        assert!(matches!(
            w.worker_choice(i),
            Choice::Order(Order::Harvest { source: Some(m), .. }) if m == near
        ));
        assert_eq!(w.ai[1].miners, 1);
        for (x, y) in keep_clear_line((14, 14), (22, 20)) {
            assert!(w.keep_clear_at(x, y));
        }
    }

    #[test]
    fn keep_clear_line_vectors() {
        assert_eq!(keep_clear_line((10, 10), (14, 11)), [(11, 10), (12, 11)]);
        assert_eq!(
            keep_clear_line((10, 10), (13, 16)),
            [(11, 11), (11, 12), (12, 13), (12, 14), (13, 15)]
        );
        assert!(keep_clear_line((10, 10), (10, 20)).is_empty());
    }

    #[test]
    fn lumber_targets() {
        assert_eq!(lumber_target(400, 300, 7), 3);
        assert_eq!(lumber_target(400, 0, 4), 0);
        assert_eq!(lumber_target(1500, 100, 8), 6);
        assert_eq!(lumber_target(1500, 2500, 8), 0);
    }

    #[test]
    fn rings_go_clockwise_from_the_top_left_with_corners_twice() {
        let r: Vec<_> = ring((5, 5), 1, 2).collect();
        assert_eq!(
            r,
            [
                (3, 3),
                (5, 3),
                (7, 3),
                (7, 3),
                (7, 5),
                (7, 7),
                (7, 7),
                (5, 7),
                (3, 7),
                (3, 7),
                (3, 5),
                (3, 3)
            ]
        );
    }

    #[test]
    fn race_change_keeps_walls() {
        assert_eq!(for_race(0x3c, 1), 0x3d);
        assert_eq!(for_race(0x3b, 0), 0x3a);
        assert_eq!(for_race(t::WALL, 1), t::ORC_WALL);
        assert_eq!(for_race(t::ORC_WALL, 0), t::WALL);
    }

    /// A computer player 1 on an `n` x `n` land field with one peasant at `at`; returns the world and the peasant.
    fn lumber_field(n: u16, at: (i32, i32)) -> (World, usize) {
        let mut w = field(n, n);
        w.players[1].controller = controller::COMPUTER;
        w.stats.0[PEASANT as usize] = UnitStats {
            hit_points: 30,
            size: (1, 1),
            flags: flags::LAND | flags::PEON,
            ..Default::default()
        };
        let id = w.spawn(PEASANT, 1, at, 0).unwrap();
        let i = w.index(id).unwrap();
        (w, i)
    }

    fn tree(w: &mut World, x: i32, y: i32) {
        let n = w.terrain.at(x, y).unwrap();
        w.terrain.regions[n] = region::FOREST;
        w.terrain.words[n] |= bits::OBSTACLE;
    }

    #[test]
    fn the_job_choice_finds_a_forest_up_to_radius_47_on_a_128_map() {
        // A worker coming out of a town hall whose top-left is (12, 70): it is still at that tile.
        let (mut w, i) = lumber_field(128, (12, 70));
        assert_eq!(w.units[i].tile, (12, 70));
        tree(&mut w, 32, 70);
        assert!(matches!(w.try_lumber(i), Some(Order::Harvest { tree: (32, 70), .. })));
        assert_eq!(w.ai[1].lumberjacks, 1);
        // The edge of the limit: radius 47 is found, 48 is not.
        let (mut w, i) = lumber_field(128, (12, 70));
        tree(&mut w, 12 + 48, 70);
        assert_eq!(w.try_lumber(i), None);
        let (mut w, i) = lumber_field(128, (12, 70));
        tree(&mut w, 12 + 47, 70);
        assert!(w.try_lumber(i).is_some());
    }

    #[test]
    fn the_job_choice_reaches_radius_11_on_a_32_map() {
        let (mut w, i) = lumber_field(32, (4, 4));
        tree(&mut w, 16, 4);
        assert_eq!(w.try_lumber(i), None, "radius 12 is out of reach");
        let (mut w, i) = lumber_field(32, (4, 4));
        tree(&mut w, 15, 4);
        assert!(w.try_lumber(i).is_some());
        // The order 23 search keeps radius 6.
        let (mut w, _) = lumber_field(32, (4, 4));
        tree(&mut w, 10, 4);
        assert_eq!(w.nearest_tree((4, 4)), Some((10, 4)));
        let (mut w, _) = lumber_field(32, (4, 4));
        tree(&mut w, 11, 4);
        assert_eq!(w.nearest_tree((4, 4)), None);
    }

    #[test]
    fn the_ring_limit_per_map_size() {
        for (n, largest) in [(32, 11), (64, 23), (96, 35), (128, 47)] {
            let (mut w, i) = lumber_field(n, (0, 0));
            tree(&mut w, largest + 1, 0);
            assert_eq!(w.try_lumber(i), None, "N = {n}, radius {}", largest + 1);
            tree(&mut w, largest, 0);
            // The nearer tree is found now.
            assert!(w.try_lumber(i).is_some(), "N = {n}, radius {largest}");
        }
    }

    #[test]
    fn an_unfinished_gold_store_is_the_site_search_centre() {
        let (mut w, hall) = town();
        let n = w.index(hall).unwrap();
        w.units[n].construction = Some(Default::default());
        let i = peasants(&mut w, 1)[0];
        let s = w.family_search(i, t::FARM).unwrap();
        assert_eq!((s.centre, s.step), ((14, 14), 2));
        let s = w.family_search(i, BARRACKS).unwrap();
        assert_eq!((s.centre, s.step), ((14, 14), 6));
    }

    #[test]
    fn map_placed_halls_get_no_keep_clear_mark() {
        let (mut w, _) = town();
        with_list(&mut w, &[0xff]);
        for y in 0..64 {
            for x in 0..64 {
                assert!(!w.keep_clear_at(x, y), "({x}, {y})");
            }
        }
        w.mark_game_start(1);
        for y in 0..64 {
            for x in 0..64 {
                assert!(!w.keep_clear_at(x, y), "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_carrier_with_no_path_is_sent_to_the_same_depot_again_without_waiting() {
        let (mut w, hall) = town();
        let (x, y) = (30, 30);
        let id = w.spawn(PEASANT, 1, (x, y), 0).unwrap();
        let i = w.index(id).unwrap();
        w.stride[PEASANT as usize] = 1;
        assert_eq!(w.units[i].tile, (x, y));
        w.units[i].carrying = true;
        w.units[i].gather = Gather::Gold;
        // Walled in by obstacles; the hall is in the same region.
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) != (0, 0) {
                    let n = w.terrain.at(x + dx, y + dy).unwrap();
                    w.terrain.words[n] |= bits::OBSTACLE;
                }
            }
        }
        let gold = w.players[1].gold;
        let mut gave = 0;
        for _ in 0..60 {
            w.step(&[]);
            let u = w.unit(id).unwrap();
            if u.order == (Order::ReturnGoods { depot: Some(hall) }) {
                gave += 1;
            }
            assert!(u.script.wait < 44, "no wait: {}", u.script.wait);
            assert_eq!(u.tile, (x, y));
        }
        assert!(gave > 5, "command 24 given again and again: {gave}");
        assert_eq!(w.players[1].gold, gold);
        assert_eq!(w.ai[1].hall_cooldown, 0);
    }
}
