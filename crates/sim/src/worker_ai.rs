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
    fn chooses_jobs(&self, i: usize) -> bool {
        let u = &self.units[i];
        let f = self.stats_of(u.kind).flags;
        self.ai_player(u.owner) && !u.building && f & (flags::PEON | flags::TANKER) != 0
    }

    /// The job choice where it runs (`ai/computer-player.md`, "Where the job choice runs": from idle, in the other
    /// acquiring orders, and at a come-out): for a computer player's worker or tanker, the order it chose, if any. Any other unit makes no
    /// choice.
    pub(crate) fn job_choice(&mut self, i: usize) -> Option<Order> {
        if !self.chooses_jobs(i) {
            return None;
        }
        match self.worker_choice(i) {
            Choice::Order(o) => Some(o),
            Choice::Wait | Choice::Nothing => None,
        }
    }

    /// The job choice from the idle logic (order 2): its order takes over at once, in the same update, without
    /// running its logic (orders spec, "Taking over": a computer worker's job choice from idle).
    pub(crate) fn idle_choice(&mut self, i: usize) {
        if let Some(o) = self.job_choice(i) {
            // A computer player's command, an ordinary one (round 68, code Q7; orders spec, "What every command
            // does", and `ai/worker-ai.md`): the goal pulled back, the command steps (blocked counter cleared, route
            // dropped: round-54 errata `human12-25000` tanker 1412, n = 12951) and the target check.
            self.command_at_once(i, o);
        }
    }

    /// The job choice in an acquiring order other than idle (`ai/worker-ai.md`, "When a job is chosen": orders 4 and
    /// 12 traced, round 83 and 84; the other acquiring orders a static gap-fill; `ai/computer-player.md`, CP5, CP12 and
    /// CP13): its order is an ordinary command, pending until the unit's next step done (traced:
    /// `xhuman5-xhuman05-worker-all-out-goals-1100` peon 1575, pending at n = 804, taking over at n = 826). The goal
    /// is left as it is: nothing here clears an attack goal (`ai/attack-ai.md`, "Attack (2)"). With no job a worker
    /// gets the wait and keeps its order ("The wait"). Returns whether the choice gave a command.
    pub(crate) fn moving_choice(&mut self, i: usize) -> bool {
        match self.job_choice(i) {
            Some(o) => {
                self.command(i, o);
                true
            }
            None => false,
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

    /// Sets or clears a keep-clear rectangle ("Keep-clear rectangle", round-38 errata) for a footprint of `w` x `w`
    /// tiles at `site`. With the border it starts one tile up and left (a coordinate already 0 stays 0) with size
    /// w + 2; without, at the site with size w. The width is cut to N - 1 - x where x + s reaches N, the height the
    /// same, and each row after the first starts s - (width marked) tiles further left, counted in map rows.
    fn keep_clear(&mut self, site: (i32, i32), w: i32, border: bool, set: bool) {
        let (n, m) = (self.terrain.width as i32, self.terrain.height as i32);
        let (x, y, s) = if border {
            ((site.0 - 1).max(0), (site.1 - 1).max(0), w + 2)
        } else {
            (site.0, site.1, w)
        };
        let wide = if x + s >= n { n - 1 - x } else { s };
        let high = if y + s >= m { m - 1 - y } else { s };
        let mut start = y * n + x;
        for _ in 0..high {
            for c in 0..wide {
                let at = start + c;
                if at >= 0 {
                    self.keep_clear_tile(at % n, at / n, set);
                }
            }
            start += n - (s - wide);
        }
    }

    fn keep_clear_tile(&mut self, x: i32, y: i32, set: bool) {
        if set {
            self.terrain.set_bits(x, y, KEEP_CLEAR);
        } else {
            self.terrain.clear_bits(x, y, KEEP_CLEAR);
        }
    }

    /// "Creation marks" (round-38 errata): a building created for a computer player, by the map or in play, gets
    /// the bordered keep-clear rectangle or the shore square by its type; other types and other players get nothing.
    pub(crate) fn creation_mark(&mut self, i: usize) {
        let u = &self.units[i];
        if !self.computer_player(u.owner) {
            return;
        }
        let (k, site) = (u.kind, u.tile);
        match k {
            0x3c..=0x3f | 0x42..=0x47 | 0x4a..=0x4d | 0x50..=0x53 | 0x58..=0x5b => {
                let w = self.stats_of(k).size.0 as i32;
                self.keep_clear(site, w, true, true);
            }
            0x48 | 0x49 | 0x4e | 0x4f | 0x54 | 0x55 => self.shore_mark(site),
            _ => {}
        }
    }

    /// The shore square of a shipyard, foundry or refinery: every shore tile (movement flag 0x0002) in the 13 x 13
    /// square from (x - 6, y - 6), clipped at 0 and so that column and row N - 1 are never marked; no skew.
    fn shore_mark(&mut self, site: (i32, i32)) {
        let span = |start: i32, n: i32| {
            let (mut a, mut len) = (start, 13);
            if a < 0 {
                len += a;
                a = 0;
            }
            if a + len >= n {
                len = n - 1 - a;
            }
            a..a + len
        };
        let (n, m) = (self.terrain.width as i32, self.terrain.height as i32);
        for y in span(site.1 - 6, m) {
            for x in span(site.0 - 6, n) {
                if self.terrain.word(x, y) & bits::SHORE != 0 {
                    self.terrain.set_bits(x, y, KEEP_CLEAR);
                }
            }
        }
    }

    fn race_of(&self, i: usize) -> u8 {
        self.units[i].kind & 1
    }

    fn affordable(&self, owner: u8, kind: u8) -> bool {
        self.can_afford(owner, self.unit_cost(kind), false).is_ok()
    }

    /// Gives the build command for `kind` at `site` (computer players pay at once) and records the job.
    ///
    /// "The goal tile of a build job" (round 85, erratum 7; traced for a farm job, CP14): the site's top-left tile
    /// becomes the worker's goal tile, whatever its goal kind, which isn't changed.
    fn start_building(&mut self, i: usize, kind: u8, site: (i32, i32), border: bool) -> Option<Order> {
        let o = self.build_command(i, kind, site)?;
        let p = self.units[i].owner as usize;
        self.ai[p].in_progress[kind as usize] += 1;
        let w = self.stats_of(kind).size.0 as i32;
        self.keep_clear(site, w, border, true);
        self.units[i].ai_job = AiJob::Build { kind, site };
        self.units[i].goal.tile = Some(site);
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
        let search = self.mine_centred(i);
        let site = [true, false]
            .into_iter()
            .find_map(|b| self.site_search(i, hall, search, b))?;
        // Round 85 (static vector, erratum 11): the keep-clear mark has the border whichever search found the site.
        let o = self.start_building(i, hall, site, true)?;
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
        if !self.ai_player(owner) {
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
    ///
    /// Round 85 (static gap-fill, erratum 1; kept in round 86): the distance is the larger axis difference between
    /// the building's top-left tile and the worker's tile, with no alive test.
    fn nearest_gold_store(&self, i: usize) -> Option<(i32, i32)> {
        let u = &self.units[i];
        let region = self.terrain.region(u.tile.0, u.tile.1);
        let mut best: Option<(i32, (i32, i32))> = None;
        for b in self.owned(u.owner) {
            if b.building
                && self.stats_of(b.kind).flags & flags::GOLD_STORAGE != 0
                && self.terrain.region(b.tile.0, b.tile.1) == region
            {
                let d = crate::movement::chebyshev(b.tile, u.tile);
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
    ///
    /// Round 85 (static gap-fill, erratum 3): the mine is measured from its top-left tile (the larger axis
    /// difference), and with no such mine the search centres on the worker's own tile.
    fn mine_centred(&self, i: usize) -> Search {
        let u = &self.units[i];
        let region = self.terrain.region(u.tile.0, u.tile.1);
        let mut best: Option<(i32, (i32, i32))> = None;
        for m in self.owned(slots::NEUTRAL) {
            if self.is_live_mine(m) && self.terrain.region(m.tile.0, m.tile.1) == region {
                let d = crate::movement::chebyshev(m.tile, u.tile);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, m.tile));
                }
            }
        }
        Search {
            centre: best.map_or(u.tile, |(_, centre)| centre),
            step: 1,
            water: None,
        }
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
            t::TOWN_HALL => Some(self.mine_centred(i)),
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
                if self.site_accepted(i, k, (x, y), border, s.water) {
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

    /// Whether the site search accepts a position. The last test is the full site check of `ui/placing-buildings.md` (round-39 errata),
    /// with worker `i` as the builder: a computer worker is never the local player's, so neither the "unexplored"
    /// outcome nor the fog exception applies.
    fn site_accepted(&self, i: usize, k: u8, site: (i32, i32), border: bool, water: Option<u16>) -> bool {
        let st = self.stats_of(k);
        let w = st.size.0 as i32;
        let (x0, y0, x1, y1) = footprint(site, w, border);
        let (n, m) = (self.terrain.width as i32, self.terrain.height as i32);
        // Round-42 errata (`xorc7` farm 1415): the footprint, bordered or not, must end before the map's last column
        // and last row.
        if x0 < 0 || y0 < 0 || x1 >= n || y1 >= m {
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
        clear && wet && self.check_site(k, site, self.units[i].id) == crate::site::SiteCheck::Allowed
    }

    /// "New-mine search": the mine whose site is nearest it (at most 10 tiles), earliest on ties.
    ///
    /// Round-65 errata (`xorc4-30000` n = 27870): a gold store counts if any tile of its footprint is in the mine's
    /// 13 × 13 square, not only its top-left tile.
    fn new_mine_site(&self, i: usize, hall: u8) -> Option<(i32, i32)> {
        // Each store's footprint as (left, top, right, bottom), inclusive.
        let stores: Vec<(i32, i32, i32, i32)> = self
            .units
            .iter()
            .filter(|b| b.building && !b.dying && self.stats_of(b.kind).flags & flags::GOLD_STORAGE != 0)
            .map(|b| {
                let (x, y) = b.tile;
                (x, y, x + b.size.0 as i32 - 1, y + b.size.1 as i32 - 1)
            })
            .collect();
        let mut best: Option<(i32, (i32, i32))> = None;
        for m in self.owned(slots::NEUTRAL) {
            if m.kind != t::GOLD_MINE || m.dying || m.resources < 50 {
                continue;
            }
            let (mx, my) = m.tile;
            if stores
                .iter()
                .any(|&(l, t, r, b)| l <= mx + 7 && r >= mx - 5 && t <= my + 7 && b >= my - 5)
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

    /// "Safe": no tile of the 9 x 9 area holds an enemy that can attack (a unit of a player the owner doesn't count as
    /// allied; the occupant isn't tested for dying). Round 85 (erratum 2, traced: `xorc10-xorc10-farm-repair-1700`,
    /// CP15): a tile whose ground layer is empty is safe and its air layer isn't looked at; a ground enemy makes the
    /// building unsafe; otherwise the air unit there, if any, is tested the same way.
    fn repair_safe(&self, b: &Unit) -> bool {
        use crate::world::Layer;
        let allies = self.players[b.owner as usize].allies | (1 << b.owner);
        let enemy = |o: &Unit| {
            let foe = o.owner >= 8 || allies & (1 << o.owner) == 0;
            foe && self.stats_of(o.kind).flags & flags::CAN_ATTACK != 0
        };
        let (x, y) = b.tile;
        for ty in y - 3..=y + 5 {
            for tx in x - 3..=x + 5 {
                let Some(ground) = self.occupant(Layer::Land, tx, ty).and_then(|o| self.unit(o)) else {
                    continue;
                };
                if enemy(ground) {
                    return false;
                }
                if self
                    .occupant(Layer::Air, tx, ty)
                    .and_then(|o| self.unit(o))
                    .is_some_and(enemy)
                {
                    return false;
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
                    // Round-58 errata: the worker term is the larger axis distance from the mine's top-left tile
                    // (`mp-gold-rush-bne-25000`, n = 6461), not the distance to the mine's nearest tile.
                    let v = distance(d.tile, m) + crate::movement::chebyshev(here, m.tile) / 2;
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
        // Round 85 (static gap-fill, erratum 10): the patches in the neutral list's order (newest first), only a
        // strictly nearer one replacing the best, so the first in the list wins a tie.
        let patch = self
            .owned(slots::NEUTRAL)
            .filter(|p| p.kind == t::OIL_PATCH && !p.dying)
            .fold(None::<(i32, (i32, i32))>, |best, p| {
                let d = distance(depot_tile, p);
                if best.is_none_or(|(bd, _)| d < bd) {
                    Some((d, p.tile))
                } else {
                    best
                }
            });
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
        let mut w = bare();
        let hall = w.spawn(t::TOWN_HALL, 1, (14, 14), 0).unwrap();
        (w, hall)
    }

    /// [`town`] without its hall.
    fn bare() -> World {
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
        w
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

    /// A farm-ready town whose only free peasant is walking in `order`, with an attack goal as the all-out attack
    /// gives; the others are busy, so only this one could choose the farm.
    fn walking_worker(order: Order) -> (World, UnitId) {
        let (mut w, _) = town();
        w.spawn(t::FARM, 1, (2, 2), 0).unwrap();
        let ps = peasants(&mut w, 3);
        let id = w.units[ps[0]].id;
        w.units[ps[0]].goal = crate::attack_ai::Goal {
            kind: crate::attack_ai::GoalKind::Attack,
            tile: Some((30, 10)),
            ..Default::default()
        };
        w.take_over(ps[0], order);
        // No 50-cycle pass in the test's span, so pass B doesn't give the goal's command 4.
        w.army.countdown = 1000;
        for &p in &ps[1..] {
            w.units[p].order = Order::Move {
                goal: (10, 40),
                target: None,
            };
        }
        (w, id)
    }

    /// CP5 (`ai/computer-player.md`; `ai/worker-ai.md`, "When a job is chosen", round 83; traced:
    /// `xhuman5-xhuman05-worker-all-out-goals-1100` peon 1575, command 4 at the pass of n = 801, return goods pending at
    /// n = 804, order 24 at n = 826): a computer worker in order 4 makes its job choice at its next order-logic run in
    /// that order. The chosen command is pending, not taken over in the same update; it takes over at the next step
    /// done, before the worker reaches the goal tile. Its attack goal is left as it is.
    #[test]
    fn a_worker_in_order_4_makes_its_job_choice_at_its_next_order_logic_run() {
        let out = Order::MoveAttack {
            goal: (30, 10),
            target: None,
        };
        let (mut w, id) = walking_worker(out);
        let gold = w.players[1].gold;
        let mut n = 0;
        while w.unit(id).unwrap().pending.is_none() {
            assert_eq!(w.unit(id).unwrap().order, out);
            w.step(&[]);
            n += 1;
            assert!(n < 50, "no job choice in order 4");
        }
        let u = w.unit(id).unwrap();
        assert_eq!(u.order, out, "pending, not taken over in the same update");
        assert!(
            matches!(u.pending, Some(Order::Build { kind: t::FARM, .. })),
            "{:?}",
            u.pending
        );
        assert_eq!(w.players[1].gold, gold - 500, "paid at the choice");
        let chosen = n;
        while w.unit(id).unwrap().order == out {
            w.step(&[]);
            n += 1;
            assert!(n < 100);
        }
        let u = w.unit(id).unwrap();
        assert!(n > chosen);
        let Order::Build {
            kind: t::FARM, site, ..
        } = u.order
        else {
            panic!("{:?}", u.order)
        };
        assert_ne!(u.tile, (30, 10), "it didn't walk the whole way");
        // Round 85 (CP14): the farm job wrote its site into the goal tile; the kind stays attack.
        assert_eq!(
            (u.goal.kind, u.goal.tile),
            (crate::attack_ai::GoalKind::Attack, Some(site)),
            "the attack goal stays, measured from the site"
        );
    }

    /// CP14 (`ai/computer-player.md`; `ai/worker-ai.md`, "The goal tile of a build job", round 85, erratum 7; traced:
    /// `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500` peon 1575, attack goal (9, 20), farm job at site (6, 19)
    /// at n = 1004): every build job (the new town hall, the farm and the build list) writes the site's top-left tile
    /// into the worker's goal tile and leaves the kind, whatever it is.
    #[test]
    fn a_build_job_writes_its_site_into_the_goal_tile() {
        use crate::attack_ai::{Goal, GoalKind};
        let check = |w: &mut World, i: usize, kind: u8, goal: GoalKind| {
            w.units[i].goal = Goal {
                kind: goal,
                tile: Some((30, 10)),
                ..Default::default()
            };
            let Choice::Order(Order::Build { kind: k, site, .. }) = w.worker_choice(i) else {
                panic!("no build job")
            };
            assert_eq!(k, kind);
            assert_eq!((w.units[i].goal.kind, w.units[i].goal.tile), (goal, Some(site)));
        };
        // The farm (traced), with an attack goal from the all-out attack.
        let (mut w, _) = town();
        let i = peasants(&mut w, 1)[0];
        check(&mut w, i, t::FARM, GoalKind::Attack);
        // The build list.
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        with_list(&mut w, &[BARRACKS, 0xff]);
        let i = peasants(&mut w, 1)[0];
        check(&mut w, i, BARRACKS, GoalKind::None);
        // The new town hall with no depot.
        let (mut w, hall) = town();
        let n = w.index(hall).unwrap();
        w.kill(n);
        w.spawn(t::GOLD_MINE, slots::NEUTRAL, (40, 33), 20_000).unwrap();
        let i = peasants(&mut w, 1)[0];
        check(&mut w, i, t::TOWN_HALL, GoalKind::Hold);
    }

    /// Round-85 vectors (`ai/worker-ai.md`, "Ordinary search", static gap-fill, erratum 3): a computer worker with no
    /// depot, every mine in its region exhausted, its player able to afford a town hall. The hall search centres on
    /// the worker's tile, a site near it is used, and there is no wait draw.
    #[test]
    fn with_no_mine_left_the_hall_search_centres_on_the_worker() {
        let mut w = bare();
        w.spawn(t::GOLD_MINE, slots::NEUTRAL, (40, 40), 0).unwrap();
        let id = w.spawn(PEASANT, 1, (20, 20), 0).unwrap();
        let i = w.index(id).unwrap();
        let s = w.mine_centred(i);
        assert_eq!((s.centre, s.step), ((20, 20), 1));
        let rng = w.rng;
        let Choice::Order(Order::Build { kind, site, .. }) = w.worker_choice(i) else {
            panic!("no hall")
        };
        assert_eq!(kind, t::TOWN_HALL);
        assert!(
            crate::movement::chebyshev(site, (20, 20)) <= 3,
            "{site:?} is near the worker"
        );
        assert_eq!(w.rng, rng, "no wait draw");
        // A mine with trips left is still the centre, measured from its top-left tile.
        w.spawn(t::GOLD_MINE, slots::NEUTRAL, (30, 30), 1000).unwrap();
        let i = w.index(id).unwrap();
        assert_eq!(w.mine_centred(i).centre, (30, 30));
    }

    /// Round-85 vectors (`ai/worker-ai.md`, "Ordinary search", the "Most buildings" row, static gap-fill, erratum 1):
    /// the nearest gold store is measured from the store's top-left tile to the worker's tile (larger axis
    /// difference), and a store in its death animation still counts. A 4 x 4 hall at (14, 20) has its right column
    /// 3 from a worker at (20, 20) but its top-left tile 6 away; a hall at (25, 20) is 5 away: the later hall wins.
    #[test]
    fn the_worker_side_gold_store_is_measured_from_its_top_left_tile() {
        let mut w = bare();
        w.spawn(t::TOWN_HALL, 1, (14, 20), 0).unwrap();
        let far = w.spawn(t::TOWN_HALL, 1, (25, 20), 0).unwrap();
        let id = w.spawn(PEASANT, 1, (20, 20), 0).unwrap();
        let i = w.index(id).unwrap();
        assert_eq!(w.nearest_gold_store(i), Some((25, 20)));
        // No alive test: the dying hall still counts.
        let f = w.index(far).unwrap();
        w.units[f].dying = true;
        assert_eq!(w.nearest_gold_store(i), Some((25, 20)));
    }

    /// Round-85 vectors (`ai/worker-ai.md`, "Worker", step 5, static vector, erratum 11): a no-depot hall whose
    /// bordered search fails and whose unbordered search finds (20, 20), a 4 x 4 hall, marks the keep-clear tiles
    /// (19, 19) to (24, 24): the border whichever search found the site.
    #[test]
    fn a_second_pass_hall_is_marked_with_the_border() {
        let mut w = bare();
        // Only (20, 20) to (23, 23) is free of the placement mask, so only the unbordered search can succeed.
        for y in 0..64 {
            for x in 0..64 {
                if !(20..24).contains(&x) || !(20..24).contains(&y) {
                    w.terrain.set_bits(x, y, bits::BUILDING);
                }
            }
        }
        let id = w.spawn(PEASANT, 1, (18, 18), 0).unwrap();
        let i = w.index(id).unwrap();
        let Choice::Order(Order::Build { kind, site, .. }) = w.worker_choice(i) else {
            panic!("no hall")
        };
        assert_eq!((kind, site), (t::TOWN_HALL, (20, 20)));
        for y in 17..27 {
            for x in 17..27 {
                let inside = (19..=24).contains(&x) && (19..=24).contains(&y);
                assert_eq!(w.keep_clear_at(x, y), inside, "({x}, {y})");
            }
        }
    }

    /// Round-85 vectors (`ai/worker-ai.md`, "Tankers", step 6, static gap-fill, erratum 10): with two oil patches at
    /// the same distance from a computer tanker's depot, the patch first in the neutral player's list (the one
    /// created later) is chosen, whichever side it is on.
    #[test]
    fn a_tanker_breaks_an_oil_patch_tie_by_the_neutral_list() {
        const TANKER: u8 = 0x1a;
        for later_east in [false, true] {
            let mut w = bare();
            w.stats.0[TANKER as usize] = UnitStats {
                hit_points: 90,
                size: (1, 1),
                flags: flags::LAND | flags::TANKER,
                ..Default::default()
            };
            w.stats.0[t::SHIPYARD as usize] = building(1100, 3, flags::OIL_STORAGE, 30);
            w.stats.0[t::PLATFORM as usize] = building(650, 3, flags::OIL_SOURCE, 30);
            w.stats.0[t::OIL_PATCH as usize] = building(0, 2, flags::OIL_PATCH, 0);
            w.spawn(t::SHIPYARD, 1, (20, 20), 0).unwrap();
            // Both 3 tiles from the yard's top-left tile (20, 20): columns 16-17 and 23-24.
            let (first, second) = if later_east {
                ((16, 20), (23, 20))
            } else {
                ((23, 20), (16, 20))
            };
            w.spawn(t::OIL_PATCH, slots::NEUTRAL, first, 5000).unwrap();
            w.spawn(t::OIL_PATCH, slots::NEUTRAL, second, 5000).unwrap();
            let id = w.spawn(TANKER, 1, (20, 25), 0).unwrap();
            let i = w.index(id).unwrap();
            let Choice::Order(Order::Build { kind, site, .. }) = w.worker_choice(i) else {
                panic!("no platform")
            };
            assert_eq!((kind, site), (t::PLATFORM, second), "the patch created later");
        }
    }

    /// `ai/worker-ai.md`, "When a job is chosen" (round 84, static gap-fill): the other acquiring orders run the job
    /// choice at the same point as order 4: patrol (order 5), attack-move (order 10, which acquires only in Enhanced
    /// mode) and stand ground (order 13, which doesn't walk). Each gives the farm as a pending command; the walking
    /// orders step toward the site in the same update.
    #[test]
    fn the_other_acquiring_orders_make_the_job_choice_too() {
        for out in [
            Order::Patrol {
                to: (30, 10),
                back: (10, 10),
            },
            Order::AttackMove { goal: (30, 10) },
            Order::StandGround,
        ] {
            let (mut w, id) = walking_worker(out);
            w.rules.enhanced = true;
            let gold = w.players[1].gold;
            let start = w.unit(id).unwrap().tile;
            let mut n = 0;
            while w.unit(id).unwrap().pending.is_none() {
                assert_eq!(w.unit(id).unwrap().order, out);
                w.step(&[]);
                n += 1;
                assert!(n < 50, "{out:?}: no job choice");
            }
            let u = w.unit(id).unwrap();
            assert!(
                matches!(u.pending, Some(Order::Build { kind: t::FARM, .. })),
                "{out:?}: {:?}",
                u.pending
            );
            assert_eq!((u.order, w.players[1].gold), (out, gold - 500), "pending, paid");
            if out == Order::StandGround {
                assert_eq!(u.tile, start, "order 13 doesn't walk");
            }
        }
    }

    /// `ai/worker-ai.md`, "When a job is chosen": attack-move acquires only in Enhanced mode ("Walking orders", step
    /// 1), so in Classic mode a computer worker in order 10 makes no job choice on the way; nor does a plain move
    /// (order 3, which doesn't acquire) in either mode.
    #[test]
    fn a_worker_in_a_non_acquiring_walk_makes_no_job_choice_on_the_way() {
        for (out, enhanced) in [
            (Order::AttackMove { goal: (30, 10) }, false),
            (
                Order::Move {
                    goal: (30, 10),
                    target: None,
                },
                true,
            ),
        ] {
            let (mut w, id) = walking_worker(out);
            w.rules.enhanced = enhanced;
            let gold = w.players[1].gold;
            let mut n = 0;
            while w.unit(id).unwrap().tile != (30, 10) {
                let u = w.unit(id).unwrap();
                assert_eq!((u.pending, w.players[1].gold), (None, gold), "{out:?}: no job choice");
                w.step(&[]);
                n += 1;
                assert!(n < 400);
            }
        }
    }

    /// `orders/orders.md`, "Special cases" (round 83): only a computer player's workers and tankers make the job
    /// choice in order 4; a human player's worker in order 4 makes none.
    #[test]
    fn a_human_players_worker_makes_no_job_choice_in_order_4() {
        let out = Order::MoveAttack {
            goal: (30, 10),
            target: None,
        };
        let (mut w, id) = walking_worker(out);
        w.players[1].controller = controller::HUMAN;
        let gold = w.players[1].gold;
        for _ in 0..60 {
            w.step(&[]);
        }
        let u = w.unit(id).unwrap();
        assert_eq!((u.order, u.pending, w.players[1].gold), (out, None, gold));
    }

    /// A town with food to spare (hall and two farms) and one computer peasant at `at`, walking in order 4 toward
    /// `goal` with nothing pending; no 50-cycle pass in the test's span.
    fn order_4_worker(at: (i32, i32), goal: (i32, i32)) -> (World, UnitId) {
        let (mut w, _) = town();
        w.spawn(t::FARM, 1, (2, 2), 0).unwrap();
        w.spawn(t::FARM, 1, (2, 6), 0).unwrap();
        let id = w.spawn(PEASANT, 1, at, 0).unwrap();
        let i = w.index(id).unwrap();
        w.take_over(i, Order::MoveAttack { goal, target: None });
        w.army.countdown = 1000;
        (w, id)
    }

    /// `ai/worker-ai.md`, "When a job is chosen" (round 84; traced: `xhuman5-xhuman05-worker-all-out-goals-3000` peon
    /// 1575, n = 2410): the order-4 choice is the walk's step 1. The walk goes on in the same update, its step heading
    /// for the job's target (its command stored it) rather than the order's goal, while the job waits as the pending
    /// order.
    #[test]
    fn the_walk_after_an_order_4_job_choice_heads_for_the_job() {
        // Walking east, away from the hall at (14, 14), carrying gold: the choice gives return goods.
        let (mut w, id) = order_4_worker((24, 15), (50, 15));
        let i = w.index(id).unwrap();
        w.units[i].carrying = true;
        let mut n = 0;
        let before = loop {
            let tile = w.unit(id).unwrap().tile;
            w.step(&[]);
            n += 1;
            assert!(n < 50, "no job choice");
            if w.unit(id).unwrap().pending.is_some() {
                break tile;
            }
        };
        let u = w.unit(id).unwrap();
        assert!(
            matches!(u.pending, Some(Order::ReturnGoods { depot: Some(_) })),
            "{:?}",
            u.pending
        );
        assert!(matches!(u.order, Order::MoveAttack { .. }));
        assert!(
            u.tile.0 < before.0,
            "stepped toward the hall in the update of the choice"
        );
        assert_eq!(u.script.anim, tidewater_formats::script::anim::MOVE);
    }

    /// AI11 (`ai/worker-ai.md`, "When a job is chosen", round 84; traced: `xhuman5-xhuman05-worker-all-out-goals-3000`
    /// peon 1575 beside gold mine 1573, n = 2464): when the job's target is already in reach, the walk's arrival test
    /// ends the walk and the job's order takes over in the same update (its animation, wait 3), instead of order 4's
    /// idle and instead of waiting for the next step done.
    #[test]
    fn an_order_4_job_already_in_reach_takes_over_in_the_same_update() {
        let (mut w, id) = order_4_worker((18, 20), (50, 50));
        let mine = w.spawn(t::GOLD_MINE, slots::NEUTRAL, (19, 19), 1000).unwrap();
        let mut n = 0;
        while matches!(w.unit(id).unwrap().order, Order::MoveAttack { .. }) {
            assert_eq!(w.unit(id).unwrap().tile, (18, 20), "no step");
            w.step(&[]);
            n += 1;
            assert!(n < 50, "no job choice");
        }
        let u = w.unit(id).unwrap();
        assert!(
            matches!(u.order, Order::Harvest { source: Some(m), .. } if m == mine),
            "{:?}",
            u.order
        );
        assert_eq!((u.pending, u.tile), (None, (18, 20)));
        assert_eq!((u.script.anim, u.script.wait), (crate::orders::order_anim(u.order), 3));
    }

    /// A computer player with no depot and no money for a hall, and one of its peasants at `at` (can target land),
    /// carrying, in `order`; a human player's peasant at (40, 10). No 50-cycle pass in the test's span.
    fn depotless_worker(at: (i32, i32), order: impl Fn(UnitId) -> Order) -> (World, UnitId) {
        let mut w = field(64, 64);
        w.players[1].controller = controller::COMPUTER;
        w.stats.0[PEASANT as usize] = UnitStats {
            hit_points: 30,
            size: (1, 1),
            flags: flags::LAND | flags::PEON,
            can_target: 1,
            ..Default::default()
        };
        w.stats.0[t::TOWN_HALL as usize] = building(1200, 4, flags::GOLD_STORAGE | flags::WOOD_STORAGE, 45);
        (w.players[1].gold, w.players[1].lumber) = (0, 0);
        w.players[1].units_allowed = !0;
        let enemy = w.spawn(PEASANT, 0, (40, 10), 0).unwrap();
        let id = w.spawn(PEASANT, 1, at, 0).unwrap();
        let i = w.index(id).unwrap();
        w.units[i].carrying = true;
        w.take_over(i, order(enemy));
        w.army.countdown = 1000;
        (w, id)
    }

    /// CP12 (`ai/worker-ai.md`, "When a job is chosen" and "The wait", round 84; traced:
    /// `xhuman5-xhuman05-worker-order12-goals-3000` peon 1575, full of lumber with no depot, in order 12 from
    /// n = 2502): the job choice runs at each order-12 run, a step boundary of its walk. With no job the worker draws
    /// the wait and keeps order 12, and the walk's step goes on in the same update. At the first run the step starts
    /// the move animation, which overwrites the drawn wait (n = 2505: wait 1); from then on the wait stands (n = 2527:
    /// 59), so it chases one tile per wait.
    #[test]
    fn a_worker_in_order_12_with_no_job_waits_before_each_step() {
        let (mut w, id) = depotless_worker((10, 10), |e| Order::Chase { target: e });
        let mut waits = vec![];
        let mut last = 0;
        for n in 0..400 {
            let tile = w.unit(id).unwrap().tile;
            let rng = w.rng;
            w.step(&[]);
            let u = w.unit(id).unwrap();
            assert!(
                matches!(u.order, Order::Chase { .. }) && u.pending.is_none(),
                "{:?}",
                u.order
            );
            if u.tile != tile {
                assert_ne!(w.rng, rng, "the wait's draw");
                assert_eq!(u.script.anim, tidewater_formats::script::anim::MOVE);
                waits.push((n - last, u.script.wait));
                last = n;
            }
        }
        assert!(waits.len() >= 4, "{waits:?}");
        assert!(
            !(44..=59).contains(&waits[0].1),
            "the move animation's start overwrites the wait: {waits:?}"
        );
        // The step after the overwritten wait comes as soon as the move animation's step is done (n = 2527, 22
        // cycles after n = 2505); each later one waits for the drawn wait.
        for &(_, wait) in &waits[1..] {
            assert!((44..=59).contains(&wait), "{waits:?}");
        }
        for &(gap, _) in &waits[2..] {
            assert!(gap >= 44, "one tile per wait: {waits:?}");
        }
    }

    /// CP13 (`ai/worker-ai.md`, "Tankers" and "When a job is chosen", round 84; traced:
    /// `orc3-orc03-tanker-order4-goals-7500-10500` tanker 1591, empty with no depot, in order 4 from n = 8616): a
    /// tanker makes the job choice at each step of its order-4 walk; with no depot it has nothing to do, gets no wait
    /// and makes no draw, and goes on in order 4. (A land unit with the tanker flag stands in for the ship here.)
    #[test]
    fn a_tanker_with_no_depot_in_order_4_goes_on_without_a_wait() {
        // The same walk by a human player's tanker, which makes no job choice, draws the same numbers.
        let walk = |c: u8| {
            let (mut w, id) = depotless_worker((10, 10), |_| Order::MoveAttack {
                goal: (30, 10),
                target: None,
            });
            let i = w.index(id).unwrap();
            w.stats.0[PEASANT as usize].flags = flags::LAND | flags::TANKER;
            w.units[i].carrying = false;
            w.players[1].controller = c;
            let mut rngs = vec![];
            while w.unit(id).unwrap().tile != (30, 10) {
                let u = w.unit(id).unwrap();
                assert!(
                    matches!(u.order, Order::MoveAttack { .. }) && u.pending.is_none(),
                    "{:?}",
                    u.order
                );
                w.step(&[]);
                rngs.push((w.rng, w.unit(id).unwrap().script.wait));
                assert!(rngs.len() < 400, "it went on walking");
            }
            rngs
        };
        assert_eq!(
            walk(controller::COMPUTER),
            walk(controller::HUMAN),
            "no draw and no wait"
        );
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

    /// Round-50 errata, harvest spec "Coming out" (`human8-10000` peasant 1508, n = 2486): a worker coming out with a
    /// repair job aims at the building's top-left tile and faces it as any come-out faces its target.
    #[test]
    fn a_worker_coming_out_with_a_repair_job_aims_at_the_buildings_top_left_tile() {
        let (mut w, hall) = town();
        let farm = w.spawn(t::FARM, 1, (30, 30), 0).unwrap();
        let fi = w.index(farm).unwrap();
        w.units[fi].hp = 200;
        let i = peasants(&mut w, 1)[0];
        let u = &mut w.units[i];
        (u.inside, u.hall_exit, u.tile, u.pos) = (true, true, (14, 14), (14 * 32, 14 * 32));
        w.come_out(i, Some(hall));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending),
            (Order::Idle, Some(Order::Repair { target: farm }))
        );
        // Aimed at (30, 30), the unit comes out on the hall's bottom-right side.
        assert_eq!(u.tile, (18, 17));
        assert_eq!(u.facing, crate::world::face_tile((18, 17), (31, 31)));
    }

    /// Round-54 errata, "The job choice gives its order as a command" (`human12-25000` tanker 1412, n = 12951): the
    /// cached route is dropped and the blocked counter cleared, so the unit plans afresh.
    /// Pin for the B-038 orders refactor: the job choice from idle is a command that takes over at once (blocked
    /// counter cleared, route dropped, a stale attacker forgotten), with no pull-back or target check of its own.
    #[test]
    fn pin_the_job_choice_takes_over_at_once() {
        let (mut w, hall) = town();
        let pi = peasants(&mut w, 1)[0];
        let p = w.units[pi].id;
        let a = w.spawn(0, 0, (20, 20), 0).unwrap();
        let ai = w.index(a).unwrap();
        w.kill(ai);
        let i = w.index(p).unwrap();
        let u = &mut w.units[i];
        (u.gather, u.carrying, u.blocked, u.attacked_by) = (Gather::Gold, true, 3, Some(a));
        u.path = vec![(9, 10), (8, 10)];
        w.idle_choice(i);
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.attacked_by),
            (Order::ReturnGoods { depot: Some(hall) }, None, None)
        );
        assert_eq!(u.script.wait, 3);
        crate::world::tests::run(&mut w, 60);
        assert_eq!(w.fingerprint(), 6992507100974240);
    }

    #[test]
    fn the_job_choice_drops_the_cached_route() {
        let (mut w, hall) = town();
        let i = peasants(&mut w, 1)[0];
        let u = &mut w.units[i];
        (u.gather, u.carrying, u.blocked) = (Gather::Gold, true, 3);
        u.path = vec![(9, 10), (8, 10)];
        w.idle_choice(i);
        let u = &w.units[i];
        assert_eq!(u.order, Order::ReturnGoods { depot: Some(hall) });
        assert!(u.path.is_empty());
        assert_eq!(u.blocked, 0);
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

    /// CP15 and the round-85 vector (`xorc10-xorc10-farm-repair-1700`, erratum 2: three enemy dragons over grass
    /// inside farm 1437's safety area, the farm repaired): an enemy flyer over an empty ground tile doesn't make a
    /// building unsafe; one over a tile whose ground layer holds a unit does, and so does a ground enemy. A dying
    /// enemy still counts.
    #[test]
    fn repair_safety_looks_at_the_air_only_over_a_ground_unit() {
        const DRAGON: u8 = 0x29;
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        w.stats.0[DRAGON as usize] = UnitStats {
            hit_points: 100,
            size: (1, 1),
            flags: flags::AIR | flags::CAN_ATTACK,
            ..Default::default()
        };
        let farm = w.spawn(t::FARM, 1, (20, 30), 0).unwrap();
        let n = w.index(farm).unwrap();
        w.units[n].hp = 200;
        let p = peasants(&mut w, 1)[0];
        let p = w.units[p].id;
        let choice = |w: &mut World| {
            let i = w.index(p).unwrap();
            w.release_job(i);
            w.worker_choice(i)
        };
        let repair = Choice::Order(Order::Repair { target: farm });
        // Dragons over open grass, inside the 9 x 9 area (17..=25, 27..=35): the farm is repaired.
        let d = w.spawn(DRAGON, 0, (22, 28), 0).unwrap();
        w.spawn(DRAGON, 0, (24, 34), 0).unwrap();
        assert_eq!(w.unit(d).unwrap().layer, crate::world::Layer::Air);
        assert_eq!(choice(&mut w), repair);
        // A dragon over the farm itself: its footprint holds the farm on the ground layer, so the air is tested.
        let over = w.spawn(DRAGON, 0, (20, 30), 0).unwrap();
        assert_ne!(choice(&mut w), repair);
        // A dying enemy still counts.
        let o = w.index(over).unwrap();
        w.units[o].dying = true;
        assert_ne!(choice(&mut w), repair, "dying counts");
        let unit = w.units[o].clone();
        w.mark(&unit, unit.tile, None);
        assert_eq!(choice(&mut w), repair);
        // An enemy on the ground inside the area.
        w.spawn(0x00, 0, (18, 33), 0).unwrap();
        assert_ne!(choice(&mut w), repair);
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

    /// Round-58 errata, "Harvest" step 2 (the `mp-gold-rush-bne-25000` peon 1505 vector at n = 6461, moved by
    /// (3, -35)): with the depot's top-left tile at (14, 14) and the worker at (14, 20), all six mines score 10, so
    /// the first in the list wins. With the worker term to the mine's nearest tile, the mine at (12, 8) would score
    /// 4 + 5 = 9 and win; from its top-left tile it is max(2, 12) = 12, so 4 + 6 = 10.
    #[test]
    fn the_mine_scores_worker_term_is_from_the_mines_top_left_tile() {
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        // The newest unit is listed first, so the mines are made in reverse: the one at (21, 20) leads the list.
        let mines: Vec<UnitId> = [(18, 8), (15, 8), (12, 8), (21, 14), (21, 17), (21, 20)]
            .into_iter()
            .map(|m| w.spawn(t::GOLD_MINE, slots::NEUTRAL, m, 100).unwrap())
            .collect();
        let first = w.owned(slots::NEUTRAL).next().unwrap().id;
        assert_eq!(first, mines[5]);
        let peon = w.spawn(PEASANT, 1, (14, 20), 0).unwrap();
        let i = w.index(peon).unwrap();
        assert!(matches!(
            w.worker_choice(i),
            Choice::Order(Order::Harvest { source: Some(m), .. }) if m == first
        ));
        assert_eq!(crate::movement::chebyshev((14, 20), (12, 8)), 12);
    }

    #[test]
    fn a_new_peons_first_step_done_chooses_a_job_that_takes_over_at_once() {
        // Orders spec, "Taking over", Human04 peon vector: created with wait 6, its first step done at n = 6 runs the
        // idle logic and the job choice, whose order takes over in the same update: order 23 on the mine, wait 3.
        let (mut w, _) = town();
        for x in 0..3 {
            w.spawn(t::FARM, 1, (2 + 3 * x, 2), 0).unwrap();
        }
        let mine = w.spawn(t::GOLD_MINE, slots::NEUTRAL, (22, 20), 100).unwrap();
        let peon = w.spawn(PEASANT, 1, (10, 10), 0).unwrap();
        let i = w.index(peon).unwrap();
        w.units[i].script.wait = 6;
        for n in 1..=6 {
            w.step(&[]);
            let u = w.unit(peon).unwrap();
            if n < 6 {
                assert_eq!(u.order, Order::Idle, "n = {n}");
            }
        }
        let u = w.unit(peon).unwrap();
        assert!(
            matches!(u.order, Order::Harvest { source: Some(m), .. } if m == mine),
            "{:?}",
            u.order
        );
        assert_eq!((u.pending, u.script.wait), (None, 3));
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

    /// "Accepting a position", the round-39 `xhuman10` vectors moved 40 rows up: with a gold mine at (40, 33), a 4 x 4
    /// hall site at (43, 32) is too close to it (distance 3) and refused; (40, 39) is not too close and accepted.
    #[test]
    fn the_site_search_refuses_a_hall_too_close_to_a_mine() {
        let (mut w, _) = town();
        w.spawn(t::GOLD_MINE, slots::NEUTRAL, (40, 33), 0).unwrap();
        let i = peasants(&mut w, 1)[0];
        assert!(!w.site_accepted(i, t::TOWN_HALL, (43, 32), false, None));
        assert!(w.site_accepted(i, t::TOWN_HALL, (40, 39), false, None));
        // A farm keeps no distance from a mine.
        assert!(w.site_accepted(i, t::FARM, (43, 32), false, None));
    }

    /// "Accepting a position" (round-42 errata): the footprint must end before the map's last row and column. On the
    /// 64 x 64 field a 2 x 2 farm at (40, 62) covers the last row and is refused; (40, 61) and (61, 40) are accepted,
    /// (62, 40) is refused.
    #[test]
    fn the_site_search_keeps_off_the_last_row_and_column() {
        let (mut w, _) = town();
        let i = peasants(&mut w, 1)[0];
        assert!(!w.site_accepted(i, t::FARM, (40, 62), false, None));
        assert!(w.site_accepted(i, t::FARM, (40, 61), false, None));
        assert!(!w.site_accepted(i, t::FARM, (62, 40), false, None));
        assert!(w.site_accepted(i, t::FARM, (61, 40), false, None));
    }

    /// The keep-clear tiles of the whole map, row by row.
    fn marked(w: &World) -> Vec<(i32, i32)> {
        let (n, m) = (w.terrain.width as i32, w.terrain.height as i32);
        (0..m)
            .flat_map(|y| (0..n).map(move |x| (x, y)))
            .filter(|&(x, y)| w.keep_clear_at(x, y))
            .collect()
    }

    fn rect(x: std::ops::RangeInclusive<i32>, y: std::ops::RangeInclusive<i32>) -> Vec<(i32, i32)> {
        y.flat_map(|y| x.clone().map(move |x| (x, y))).collect()
    }

    /// "Keep-clear rectangle" vectors (round-38 errata): the border, the clipping at the right and bottom edges and
    /// the skew of the rows after a clipped first row.
    #[test]
    fn keep_clear_rectangle_vectors() {
        let rows = |r: &[(i32, i32, i32)]| -> Vec<(i32, i32)> {
            r.iter()
                .flat_map(|&(x0, x1, y)| (x0..=x1).map(move |x| (x, y)))
                .collect()
        };
        let mut w = field(96, 96);
        w.keep_clear((93, 23), 3, true, true);
        let want = rows(&[(92, 94, 22), (90, 92, 23), (88, 90, 24), (86, 88, 25), (84, 86, 26)]);
        assert_eq!(marked(&w), want, "the orc09 stables");
        // Clearing the same rectangle clears exactly those tiles.
        w.keep_clear((93, 23), 3, true, false);
        assert!(marked(&w).is_empty());

        let mut w = field(128, 128);
        w.keep_clear((125, 12), 3, true, true);
        let want = rows(&[
            (124, 126, 11),
            (122, 124, 12),
            (120, 122, 13),
            (118, 120, 14),
            (116, 118, 15),
        ]);
        assert_eq!(marked(&w), want, "the human12 troll lumber mill");

        let mut w = field(64, 64);
        w.keep_clear((0, 0), 3, true, true);
        assert_eq!(marked(&w), rect(0..=4, 0..=4));
        let mut w = field(64, 64);
        w.keep_clear((10, 10), 2, true, true);
        assert_eq!(marked(&w), rect(9..=12, 9..=12));
        let mut w = field(64, 64);
        w.keep_clear((62, 5), 2, false, true);
        assert_eq!(
            marked(&w),
            [(62, 5), (61, 6)],
            "a farm: width 1, the second row 1 tile further left"
        );
    }

    /// "Creation marks" (round-38 errata): a computer player's map buildings are marked by type at game start; a
    /// farm and a human player's barracks are not.
    #[test]
    fn map_buildings_of_a_computer_player_get_creation_marks() {
        let mut w = field(96, 96);
        w.players[1].controller = controller::COMPUTER;
        w.players[0].controller = controller::HUMAN;
        w.stats.0[t::TOWN_HALL as usize] = building(1200, 4, flags::GOLD_STORAGE, 45);
        w.stats.0[0x42] = building(700, 3, 0, 35);
        w.stats.0[t::FARM as usize] = building(400, 2, 0, 45);
        w.stats.0[BARRACKS as usize] = building(800, 3, 0, 35);
        w.spawn(t::TOWN_HALL, 1, (85, 2), 0).unwrap();
        w.spawn(0x42, 1, (80, 0), 0).unwrap();
        w.spawn(t::FARM, 1, (40, 40), 0).unwrap();
        w.spawn(BARRACKS, 0, (20, 60), 0).unwrap();
        with_list(&mut w, &[0xff]);
        w.mark_game_start(0);
        w.mark_game_start(1);
        let mut want = rect(84..=89, 1..=6);
        want.extend(rect(79..=83, 0..=4));
        want.sort_by_key(|&(x, y)| (y, x));
        want.dedup();
        assert_eq!(
            marked(&w),
            want,
            "the hall with its border, the church keeping its size on the top edge"
        );
    }

    /// The `orc09` shipyard vector: a shore building marks every shore tile of the 13 x 13 square from (x - 6, y - 6)
    /// and nothing else; the square is clipped so column and row N - 1 are never marked.
    #[test]
    fn a_shore_building_marks_the_shore_tiles_around_it() {
        let mut w = field(96, 96);
        w.players[1].controller = controller::COMPUTER;
        w.stats.0[t::SHIPYARD as usize] = building(1100, 3, 0, 20);
        for (x, y) in [(78, 57), (90, 69), (77, 57), (91, 63), (84, 70), (80, 60)] {
            w.terrain.set_bits(x, y, bits::SHORE);
        }
        let id = w.spawn(t::SHIPYARD, 1, (84, 63), 0).unwrap();
        w.creation_mark(w.index(id).unwrap());
        assert_eq!(marked(&w), [(78, 57), (80, 60), (90, 69)]);

        let mut w = field(64, 64);
        w.players[1].controller = controller::COMPUTER;
        w.stats.0[t::SHIPYARD as usize] = building(1100, 3, 0, 20);
        for (x, y) in [(0, 55), (62, 63), (63, 63), (62, 62), (63, 62)] {
            w.terrain.set_bits(x, y, bits::SHORE);
        }
        let id = w.spawn(t::SHIPYARD, 1, (60, 60), 0).unwrap();
        w.creation_mark(w.index(id).unwrap());
        assert_eq!(marked(&w), [(62, 62)], "column and row 63 are never marked");
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
