//! Workers placing buildings: order 28 (go to the site and build) and order 22 (inside the site), and builders coming
//! out when the building is finished, cancelled or destroyed. Specs: `docs/specs/economy/build-train.md` ("Placing a
//! building", "Completion", "Cancelling construction") and `docs/specs/movement/occupancy.md` ("Building placement
//! masks", "Entering a construction site").
//!
//! Not yet here: which worker may build which type (the tech tree and the command panel decide that), and walls,
//! whose pieces become terrain on completion.
//!
//! Our choice: a failed placement tells the player with [`Event::CantPlace`].

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::TILE_PX;
use crate::economy::{Event, cost};
use crate::movement::{Goal, Walk, chebyshev, footprint_goal};
use crate::placement::{LIMIT_OUT, Source};
use crate::site::building_mask;
use crate::terrain::bits;
use crate::world::{Gather, Order, UnitId, World, face_tile};

/// Order 22's wait at each run.
const INSIDE_SITE_WAIT: u8 = 255;
/// The wait on going inside the site, as on entering a mine (`economy/harvest.md`, "Entering"; the human01-basics
/// oracle trace: wait 150 at n = 134).
const ENTER_SITE_WAIT: u8 = 150;
/// The oil patch type (public PUD specification).
const OIL_PATCH: u8 = 0x5d;

impl World {
    /// Command to build `kind` at `site` (its top-left tile). Computer players pay now; everyone else on arrival.
    pub(crate) fn build_command(&mut self, i: usize, kind: u8, site: (i32, i32)) -> Option<Order> {
        let u = &self.units[i];
        let builder = self.stats_of(u.kind).flags & (flags::PEON | flags::TANKER) != 0;
        if !builder || self.stats_of(kind).flags & flags::BUILDING == 0 {
            return None;
        }
        let owner = u.owner;
        if self.computer_player(owner) {
            let c = cost(&self.stats_of(kind));
            if !self.afford_or_tell(owner, c, false) {
                return None;
            }
            self.pay(owner, c);
        }
        // Round-47 errata: the walk's goal is worked out now, from the worker's tile when the command is given; a
        // computer worker coming out of a building is still at the building's top-left tile here. Round-59 errata
        // (`mp-frosty-fjords-bne-30000` peasant 1560, n = 16187): the command pulls the goal back as any command
        // does (pathfinding spec, "Goal pulled back"); the site itself is unchanged.
        let goal = self.build_goal(i, kind, site);
        Some(Order::Build {
            kind,
            site,
            goal: Some(self.pull_back(i, goal)),
        })
    }

    /// The build walk's goal ("Placing a building"), before any pull-back: a tile of the planned footprint, worked
    /// out from the worker's current tile as if the building already stood at `site`. An oil platform is aimed at its
    /// top-left tile plus (1, 1); a shore building by the shore rule ([`World::shore_build_goal`]); any other
    /// building by the building goal rule (pathfinding spec).
    pub(crate) fn build_goal(&self, i: usize, kind: u8, site: (i32, i32)) -> (i32, i32) {
        let s = self.stats_of(kind);
        if s.flags & flags::OIL_SOURCE != 0 {
            return (site.0 + 1, site.1 + 1);
        }
        let size = (s.size.0 as i32, s.size.1 as i32);
        if s.flags & flags::SHORE_BUILDING != 0 {
            return self.shore_build_goal(i, site, size);
        }
        footprint_goal(self.units[i].tile, site, size)
    }

    /// Pathfinding spec, "Shore building (build goal)" (round-59 errata, `mp-frosty-fjords-bne-30000`): the planned
    /// footprint's top-left tile if it is in the worker's land region; else the first corner in that region, tried
    /// bottom-right, top-right, bottom-left, top-left; else the top-left tile. A worker type with the sea or air flag
    /// makes no corner test and gets the top-left tile.
    fn shore_build_goal(&self, i: usize, site: (i32, i32), size: (i32, i32)) -> (i32, i32) {
        let u = &self.units[i];
        if self.stats_of(u.kind).flags & (flags::SEA | flags::AIR) != 0 {
            return site;
        }
        let mine = self.terrain.region(u.tile.0, u.tile.1);
        let (x1, y1) = (site.0 + size.0 - 1, site.1 + size.1 - 1);
        [site, (x1, y1), (x1, site.1), (site.0, y1)]
            .into_iter()
            .find(|c| self.terrain.region(c.0, c.1) == mine)
            .unwrap_or(site)
    }

    /// The arrival radius of a build walk: 0, or 1 for a land worker and a shore building (movement spec, "The +1").
    fn build_radius(&self, i: usize, kind: u8) -> i32 {
        let shore = self.stats_of(kind).flags & flags::SHORE_BUILDING != 0;
        (shore && self.units[i].layer == crate::world::Layer::Land) as i32
    }

    /// Order 28: walk to the build goal and build on arriving within the arrival radius (0; a land unit building a
    /// shore building gets the movement spec's +1). Round-41 errata (`xorc7` peasant 1421, n = 2028): the goal is
    /// worked out once and stays fixed while the worker walks; round-47 errata: it is worked out when the command is
    /// given ([`World::build_command`]), and only an order made without one works it out here. Round-42 errata: the
    /// order's own logic runs only on arrival, where round-59 errata has it work the goal out again
    /// ([`World::build_arrived`]); the walk ends without arriving only with no route (idle at once) or at the blocked
    /// counter's limit (idle as a pending order, `movement.rs` row C).
    pub(crate) fn build_order(&mut self, i: usize, kind: u8, site: (i32, i32), goal: Option<(i32, i32)>) {
        let goal = goal.unwrap_or_else(|| self.build_goal(i, kind, site));
        self.units[i].order = Order::Build {
            kind,
            site,
            goal: Some(goal),
        };
        let radius = self.build_radius(i, kind);
        match self.move_toward(i, Goal::tile(goal, radius)) {
            Walk::Arrived => self.build_arrived(i, kind, site),
            // Round-42 errata: idle as an immediate order, whose idle logic runs in the same update (orders spec, "No
            // route"). Round-49 errata (`xhuman12-10000` n = 116, `human14-10000` n = 1040): a computer player's
            // payment isn't given back; only a failed site check refunds it.
            Walk::NoPath => self.idle_at_once(i),
            Walk::Stepping | Walk::Blocked => {}
        }
    }

    /// Round-59 errata, "Placing a building" (`mp-frosty-fjords-bne-30000` peasant 1560, n = 16734): on arrival at
    /// the stored goal, order 28 works the goal out again from the worker's tile, not pulled back, and applies the
    /// arrival test to it. If the worker is there, placement goes ahead; if not, the build is given again as a command
    /// with the new goal pulled back, and that pending order takes over in the same update (still animation, wait 3).
    ///
    /// Our reading: giving the build again makes no new payment (a computer player paid when the AI chose the build);
    /// the spec says nothing about paying here.
    fn build_arrived(&mut self, i: usize, kind: u8, site: (i32, i32)) {
        let goal = self.build_goal(i, kind, site);
        if chebyshev(self.units[i].tile, goal) <= self.build_radius(i, kind) {
            return self.raise(i, kind, site);
        }
        let again = Order::Build {
            kind,
            site,
            goal: Some(self.pull_back(i, goal)),
        };
        // A command: the blocked counter is cleared and a stale attacker forgotten (as `World::command` does).
        let u = &mut self.units[i];
        u.blocked = 0;
        u.path.clear();
        u.pending = None;
        self.forget_stale_attacker(i);
        self.take_over(i, again);
    }

    /// Arrival at the site: pay (players other than computer players), check the site, then place the building and
    /// go inside it.
    pub(crate) fn raise(&mut self, i: usize, kind: u8, site: (i32, i32)) {
        let u = self.units[i].clone();
        let c = cost(&self.stats_of(kind));
        let pays_now = !self.computer_player(u.owner);
        if pays_now && !self.afford_or_tell(u.owner, c, false) {
            self.stop(i);
            return;
        }
        // The builder leaves its tile so it doesn't block its site, and comes back if placing fails. (The spec leaves
        // the ground-unit pointer alone; our spawn refuses a named tile, so it is cleared too, and the building's
        // footprint names the building anyway.)
        self.mark(&u, u.tile, None);
        let patch = self.site_ok(kind, site);
        let placed = patch.and_then(|_| self.place_building(kind, u.owner, site).ok());
        // Spawning keeps the units sorted by slot, so the builder's index may have moved.
        let i = self.index(u.id).expect("the builder is alive");
        let Some(b) = placed else {
            // Round-40 errata (`xhuman2`, peon 1562 at n = 85): the worker's tile stays marked as occupied by it, and
            // idle takes over in the same update (still animation, wait 3), not as an immediate order.
            self.mark(&u, u.tile, Some(u.id));
            self.events.push(Event::CantPlace { player: u.owner, kind });
            if self.computer_player(u.owner) {
                self.refund(u.owner, c);
            }
            self.take_over(i, Order::Idle);
            return;
        };
        if let Some(Some(p)) = patch
            && let Some(pi) = self.index(p)
        {
            // Harvest spec, "Mine and platform amounts": the platform takes the patch's trips.
            let trips = self.units[pi].resources;
            if let Some(bi) = self.index(b) {
                self.units[bi].resources = trips;
            }
            // Round-41 errata (`orc09-naval` patch 1502, n = 268): the patch is marked used and killed, and leaves no
            // ruin: its "remove" order starts in the same cycle, with its type kept (core/corpses spec), and the
            // cycle's record shows it with wait 1. Our reading: the removal starts as of the patch's own update this
            // cycle, so a patch still to be updated (a higher slot than the builder's) has its first wait run there.
            let pi = self.index(p).expect("the patch is alive");
            self.units[pi].used = true;
            self.kill_as(pi, false);
            let pi = self.index(p).expect("the patch is still in its slot");
            self.death_done(pi);
            let later = self.units[pi].slot > u.slot;
            self.units[pi].script.wait = 1 + later as u8;
            let patch_slot = self.units[pi].slot;
            self.fresh_platforms.push((b, patch_slot));
        }
        if pays_now {
            self.pay(u.owner, c);
        }
        let u = &mut self.units[i];
        u.inside = true;
        u.tile = site;
        u.pos = (site.0 * TILE_PX, site.1 * TILE_PX);
        u.path.clear();
        u.order = Order::InsideSite { building: b };
        self.start_anim(i, anim::STILL);
        self.units[i].script.wait = ENTER_SITE_WAIT;
    }

    /// The placement check of the occupancy spec for a building of type `kind` with its top-left tile at `site`, made
    /// when the worker arrives. `Some(patch)` if it passes, with the oil patch an oil platform is placed on. The click
    /// in placement mode makes the fuller check of [`World::check_site`] first.
    pub(crate) fn site_ok(&self, kind: u8, site: (i32, i32)) -> Option<Option<UnitId>> {
        let s = self.stats_of(kind);
        let platform = s.flags & flags::OIL_SOURCE != 0;
        let shore = s.flags & flags::SHORE_BUILDING != 0;
        let m = building_mask(s.flags);
        let tiles = (0..s.size.1 as i32).flat_map(|dy| (0..s.size.0 as i32).map(move |dx| (site.0 + dx, site.1 + dy)));
        let mut touches_shore = false;
        for (x, y) in tiles {
            if !self.terrain.contains(x, y) {
                return None;
            }
            let w = self.terrain.word(x, y);
            if w & m != 0 {
                return None;
            }
            touches_shore |= w & bits::SHORE != 0;
        }
        if shore && !touches_shore {
            return None;
        }
        if platform {
            let patch = self
                .units
                .iter()
                .find(|p| p.kind == OIL_PATCH && p.tile == site && p.present())?;
            return Some(Some(patch.id));
        }
        Some(None)
    }

    /// Order 22: forget the attacker and wait.
    pub(crate) fn inside_site(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.attacked_by = None;
        u.script.wait = INSIDE_SITE_WAIT;
    }

    /// Lets out every builder inside building `bi` when it is finished (`finished`), cancelled or destroyed
    /// ("Completion"): carried goods are delivered to a finished depot, a tanker starts on its new platform, and
    /// anyone else comes out by the ring search or dies.
    pub(crate) fn release_builders(&mut self, bi: usize, finished: bool) {
        let b = self.units[bi].clone();
        let inside: Vec<usize> = (0..self.units.len())
            .filter(|&k| self.units[k].order == Order::InsideSite { building: b.id })
            .collect();
        let bf = self.stats_of(b.kind).flags;
        for k in inside {
            let tanker = self.stats_of(self.units[k].kind).flags & flags::TANKER != 0;
            if finished {
                let u = &self.units[k];
                let accepts = if tanker {
                    bf & flags::OIL_STORAGE != 0
                } else {
                    match u.gather {
                        Gather::Gold => bf & flags::GOLD_STORAGE != 0,
                        Gather::Lumber => bf & (flags::GOLD_STORAGE | flags::WOOD_STORAGE) != 0,
                        Gather::None => false,
                    }
                };
                if u.carrying && accepts {
                    self.deliver(k, b.owner, tanker);
                }
                // Round-47 errata: a hall's builder gets the hall-sized exit mark, carrying or not; being let out
                // here neither uses nor clears it.
                if !tanker && bf & flags::GOLD_STORAGE != 0 {
                    self.units[k].hall_exit = true;
                }
                if tanker && bf & flags::OIL_SOURCE != 0 {
                    // Round-43 errata: order 25 is an immediate order (move animation, wait 1); its logic runs in the
                    // next cycle and takes the tanker into the platform (order 26). The facing is kept.
                    let enter = Order::Enter {
                        building: b.id,
                        stepped: true,
                    };
                    self.immediately(k, enter);
                    continue;
                }
            }
            let kind = self.units[k].kind;
            let src = Source::new(b.tile.0, b.tile.1, b.size.0 as i32, b.size.1 as i32);
            match self.find_place(kind, src, b.tile, LIMIT_OUT) {
                Some(to) => {
                    let u = &mut self.units[k];
                    u.tile = to;
                    u.pos = (to.0 * TILE_PX, to.1 * TILE_PX);
                    u.inside = false;
                    u.order = Order::Idle;
                    u.pending = None;
                    // "Completion" (round-39 errata): the builder faces the building, B its top-left tile + (1, 1).
                    // Only completion is traced; the open question's default turns it on cancel and destroy too.
                    u.facing = face_tile(to, (b.tile.0 + 1, b.tile.1 + 1));
                    let unit = self.units[k].clone();
                    self.mark(&unit, to, Some(unit.id));
                    self.start_anim(k, anim::STILL);
                    self.units[k].script.wait = 1;
                    self.apply_unit_sight(k);
                    if finished {
                        self.work_complete(k);
                    }
                }
                None => self.kill(k),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::economy::kind;
    use crate::world::Command;
    use crate::world::tests::{field, run};

    const PEASANT: u8 = 3;

    fn town() -> (World, UnitId) {
        let mut w = field(24, 24);
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        w.stats.0[PEASANT as usize] = w.stats.0[0];
        w.stats.0[PEASANT as usize].flags = flags::LAND | flags::PEON;
        let farm = &mut w.stats.0[kind::FARM as usize];
        farm.gold = 500;
        farm.lumber = 250;
        farm.build_time = 2;
        farm.hit_points = 400;
        for p in &mut w.players[..2] {
            p.gold = 1000;
            p.lumber = 1000;
        }
        let p = w.spawn(PEASANT, 0, (2, 2), 0).unwrap();
        (w, p)
    }

    fn build(w: &mut World, player: u8, unit: UnitId, site: (i32, i32)) {
        w.step(&[Command::Build {
            player,
            unit,
            kind: kind::FARM,
            tile: site,
        }]);
    }

    fn farm_at(w: &World, site: (i32, i32)) -> Option<UnitId> {
        w.units
            .iter()
            .find(|u| u.kind == kind::FARM && u.tile == site && u.present())
            .map(|u| u.id)
    }

    /// Orders spec, "A player's command forgets the attacker" (round-56 static correction): a build command keeps the
    /// worker's recorded attacker and leaves its remembered order as it is.
    #[test]
    fn a_build_command_keeps_the_attacker_and_the_remembered_order() {
        let (mut w, p) = town();
        w.rules.enhanced = true;
        let g = w.spawn(PEASANT, 1, (12, 12), 0).unwrap();
        let old = Some(crate::world::Remembered::AttackMove { goal: (3, 3) });
        let i = w.index(p).unwrap();
        w.units[i].attacked_by = Some(g);
        w.units[i].remembered = old;
        build(&mut w, 0, p, (8, 8));
        let u = w.unit(p).unwrap();
        assert!(
            matches!(u.pending.unwrap_or(u.order), Order::Build { .. }),
            "{:?}",
            u.order
        );
        assert_eq!((u.attacked_by, u.remembered), (Some(g), old));
    }

    #[test]
    fn a_peasant_builds_a_farm_and_comes_out() {
        let (mut w, p) = town();
        build(&mut w, 0, p, (8, 8));
        // Nothing is paid until the peasant arrives.
        assert_eq!(w.players[0].gold, 1000);
        for _ in 0..400 {
            w.step(&[]);
            if w.unit(p).unwrap().inside {
                break;
            }
        }
        let farm = farm_at(&w, (8, 8)).expect("placed");
        assert_eq!(w.unit(p).unwrap().order, Order::InsideSite { building: farm });
        assert_eq!((w.players[0].gold, w.players[0].lumber), (500, 750));
        assert_eq!(w.occupant(crate::Layer::Land, 8, 8), Some(farm));
        run(&mut w, 12 * 3 + 2);
        assert!(w.unit(farm).unwrap().construction.is_none(), "complete");
        let u = w.unit(p).unwrap();
        assert!(!u.inside && u.order == Order::Idle);
        // The ring search starts west of the top-left tile.
        assert_eq!(u.tile, (7, 8));
        assert_eq!(w.occupant(crate::Layer::Land, 7, 8), Some(p));
        // Round-39 errata (`human06-tech` n = 178 and the like): it faces SE (3), toward (9, 9), as an immediate idle.
        assert_eq!((u.facing, u.script.anim, u.pending), (3, anim::STILL, None));
    }

    /// Build-train spec, the round-39 exit-facing vectors: a builder let out faces the building's top-left + (1, 1).
    #[test]
    fn a_builder_let_out_faces_the_building() {
        // (exit tile, building's top-left, facing)
        for (exit, tl, want) in [
            ((75, 5), (76, 5), 3),
            ((62, 2), (63, 2), 3),
            ((26, 113), (27, 113), 3),
            ((16, 82), (17, 80), 1),
            ((80, 7), (81, 6), 2),
            ((21, 4), (20, 2), 0),
        ] {
            assert_eq!(face_tile(exit, (tl.0 + 1, tl.1 + 1)), want, "{exit:?} from {tl:?}");
        }
    }

    /// Build-train spec, "Placing a building", test vectors: the build walk's goal is the building goal rule on the
    /// planned footprint.
    #[test]
    fn the_build_walk_aims_at_a_footprint_tile() {
        let (mut w, p) = town();
        w.stats.0[kind::FARM as usize].size = (2, 2);
        let barracks = 60;
        w.stats.0[barracks as usize] = w.stats.0[kind::FARM as usize];
        w.stats.0[barracks as usize].size = (3, 3);
        let i = w.index(p).unwrap();
        assert_eq!(w.build_goal(i, kind::FARM, (5, 5)), (5, 5));
        w.units[i].tile = (6, 9);
        assert_eq!(w.build_goal(i, barracks, (5, 5)), (6, 7));
        // human01-basics: peasant 1596 on (14, 9), a barracks at (5, 8).
        w.units[i].tile = (14, 9);
        assert_eq!(w.build_goal(i, barracks, (5, 8)), (7, 9));
        // An oil platform: top-left plus (1, 1), wherever the builder is.
        let platform = 0x56;
        w.stats.0[platform as usize] = w.stats.0[barracks as usize];
        w.stats.0[platform as usize].flags |= flags::OIL_SOURCE;
        assert_eq!(w.build_goal(i, platform, (5, 8)), (6, 9));
    }

    /// The same, walked: the peasant stands on the goal tile when the building is placed (human01-basics: (7, 9),
    /// the barracks at (5, 8), the peasant inside with wait 150).
    #[test]
    fn the_builder_arrives_on_the_goal_tile_and_goes_inside_with_wait_150() {
        let (mut w, p) = town();
        w.stats.0[kind::FARM as usize].size = (3, 3);
        let i = w.index(p).unwrap();
        w.units[i].tile = (14, 9);
        w.units[i].pos = (14 * 32, 9 * 32);
        build(&mut w, 0, p, (5, 8));
        let mut before = None;
        for _ in 0..400 {
            let u = w.unit(p).unwrap();
            if u.inside {
                break;
            }
            before = Some(u.tile);
            w.step(&[]);
        }
        assert_eq!(before, Some((7, 9)));
        let u = w.unit(p).unwrap();
        assert!(matches!(u.order, Order::InsideSite { .. }));
        assert_eq!(u.script.wait, 150);
    }

    /// Round-41 errata (`xorc7` peasant 1421): the build walk's goal is worked out once and kept in the order.
    #[test]
    fn the_build_goal_stays_fixed_while_walking() {
        let (mut w, p) = town();
        w.stats.0[kind::FARM as usize].size = (3, 3);
        let i = w.index(p).unwrap();
        w.units[i].tile = (14, 9);
        w.units[i].pos = (14 * 32, 9 * 32);
        build(&mut w, 0, p, (5, 8));
        for _ in 0..20 {
            if matches!(w.unit(p).unwrap().order, Order::Build { goal: Some(_), .. }) {
                break;
            }
            w.step(&[]);
        }
        let want = Order::Build {
            kind: kind::FARM,
            site: (5, 8),
            goal: Some((7, 9)),
        };
        assert_eq!(w.unit(p).unwrap().order, want);
        // Moved somewhere whose own goal would differ: the order keeps the first one.
        let i = w.index(p).unwrap();
        w.mark(&w.units[i].clone(), w.units[i].tile, None);
        w.units[i].tile = (6, 16);
        w.units[i].pos = (6 * 32, 16 * 32);
        w.units[i].path.clear();
        assert_ne!(w.build_goal(i, kind::FARM, (5, 8)), (7, 9));
        w.step(&[]);
        assert_eq!(w.unit(p).unwrap().order, want);
    }

    /// Round-47 errata (`xorc4-10000` peasant 1580): the goal is worked out when the command is given, from the
    /// worker's tile then, so a worker that moves before its order first runs keeps it.
    #[test]
    fn the_build_goal_comes_from_the_tile_when_the_command_is_given() {
        let (mut w, p) = town();
        w.stats.0[kind::FARM as usize].size = (3, 3);
        let i = w.index(p).unwrap();
        w.units[i].tile = (14, 9);
        let o = w.build_command(i, kind::FARM, (5, 8)).expect("a builder");
        w.units[i].tile = (6, 16);
        assert_eq!(
            o,
            Order::Build {
                kind: kind::FARM,
                site: (5, 8),
                goal: Some((7, 9)),
            }
        );
    }

    /// Round-41 errata (`orc09-naval` patch 1502): an oil platform's patch is marked used and removed with no ruin,
    /// its type kept, with wait 1 in the cycle's record (the patch here updates before the builder, so nothing more
    /// runs on it this cycle).
    #[test]
    fn a_platform_removes_its_oil_patch() {
        let (mut w, _) = town();
        const TANKER: u8 = 0x1a;
        const PLATFORM: u8 = 0x56;
        let base = w.stats.0[0];
        for (k, size, f) in [
            (TANKER, (1, 1), flags::SEA | flags::TANKER),
            (PLATFORM, (2, 2), flags::BUILDING | flags::OIL_SOURCE),
            (OIL_PATCH, (2, 2), flags::BUILDING | flags::OIL_PATCH),
        ] {
            let s = &mut w.stats.0[k as usize];
            *s = base;
            s.size = size;
            s.flags = f;
        }
        for y in 0..24 {
            for x in 10..24 {
                w.terrain.clear_bits(x, y, crate::terrain::bits::LAND);
                w.terrain.set_bits(x, y, crate::terrain::bits::WATER);
            }
        }
        let t = w.spawn(TANKER, 0, (12, 12), 0).unwrap();
        let patch = w.spawn(OIL_PATCH, crate::slots::NEUTRAL, (16, 16), 0).unwrap();
        let i = w.index(t).unwrap();
        w.raise(i, PLATFORM, (16, 16));
        let pu = w.unit(patch).unwrap();
        assert_eq!(pu.kind, OIL_PATCH);
        assert!(pu.used && pu.removing && pu.dying, "{pu:?}");
        assert!(pu.slot < w.unit(t).unwrap().slot);
        assert_eq!(pu.script.wait, 1);
        assert!(w.unit(t).unwrap().inside);
        // Round-43 errata (`orc09-naval` tanker 1473, n = 468): when the platform is finished, its tanker gets order
        // 25 as an immediate order (move animation, wait 1, facing kept), and goes in at the next cycle (order 26).
        let platform = w.units.iter().find(|u| u.kind == PLATFORM).unwrap().id;
        let i = w.index(t).unwrap();
        w.units[i].facing = 2;
        let bi = w.index(platform).unwrap();
        w.units[bi].construction = None;
        w.release_builders(bi, true);
        let u = w.unit(t).unwrap();
        let enter = Order::Enter {
            building: platform,
            stepped: true,
        };
        assert_eq!(
            (u.order, u.script.anim, u.script.wait, u.facing),
            (enter, anim::MOVE, 1, 2)
        );
        w.step(&[]);
        assert!(matches!(w.unit(t).unwrap().order, Order::Inside { .. }));
    }

    /// Round-49 errata (`orc12-10000` tanker 1409, `xhuman8-10000` tanker 1468): a tanker's harvest command on a
    /// platform placed in this cycle fails while the oil patch under it is still to have its own update.
    #[test]
    fn a_platform_placed_this_cycle_is_no_harvest_target_until_its_patch_updates() {
        let (mut w, _) = town();
        const TANKER: u8 = 0x1a;
        const PLATFORM: u8 = 0x56;
        let base = w.stats.0[0];
        for (k, size, f) in [
            (TANKER, (1, 1), flags::SEA | flags::TANKER),
            (PLATFORM, (2, 2), flags::BUILDING | flags::OIL_SOURCE),
            (OIL_PATCH, (2, 2), flags::BUILDING | flags::OIL_PATCH),
        ] {
            let s = &mut w.stats.0[k as usize];
            *s = base;
            s.size = size;
            s.flags = f;
        }
        for y in 0..24 {
            for x in 10..24 {
                w.terrain.clear_bits(x, y, crate::terrain::bits::LAND);
                w.terrain.set_bits(x, y, crate::terrain::bits::WATER);
            }
        }
        let builder = w.spawn(TANKER, 0, (12, 12), 0).unwrap();
        let patch = w.spawn(OIL_PATCH, crate::slots::NEUTRAL, (16, 16), 0).unwrap();
        let other = w.spawn(TANKER, 0, (12, 14), 0).unwrap();
        let patch_slot = w.unit(patch).unwrap().slot;
        // As if the builder's update ran first in this cycle's walk, from below the patch's slot.
        w.updating = Some(patch_slot - 1);
        let i = w.index(builder).unwrap();
        w.raise(i, PLATFORM, (16, 16));
        let platform = w.units.iter().find(|u| u.kind == PLATFORM).unwrap().id;
        let j = w.index(other).unwrap();
        assert_eq!(w.harvest_command(j, Some(platform), (16, 16)), Some(Order::Idle));
        // Once the patch has had its update, the same command works.
        w.updating = Some(patch_slot);
        assert!(matches!(
            w.harvest_command(j, Some(platform), (16, 16)),
            Some(Order::Harvest { source: Some(s), .. }) if s == platform
        ));
    }

    #[test]
    fn a_finished_wall_piece_becomes_wall_terrain() {
        let (mut w, p) = town();
        let mut wall = w.stats.0[kind::FARM as usize];
        wall.size = (1, 1);
        w.stats.0[kind::HUMAN_WALL as usize] = wall;
        w.terrain.build_wall(9, 8, true, false);
        w.step(&[Command::Build {
            player: 0,
            unit: p,
            kind: kind::HUMAN_WALL,
            tile: (8, 8),
        }]);
        for _ in 0..400 {
            w.step(&[]);
            if w.terrain.tile(8, 8) != Some(0) {
                break;
            }
        }
        // Joined east to the orc wall, which joins back west; a human-controlled owner's bit.
        assert_eq!(
            (w.terrain.tile(8, 8), w.terrain.tile(9, 8)),
            (Some(16 + 2), Some(34 + 9))
        );
        assert_eq!(w.terrain.word(8, 8) & 0xff, 0x95);
        let piece = w.units.iter().find(|u| u.kind == kind::HUMAN_WALL).unwrap();
        assert!(piece.dying && piece.used && piece.construction.is_none());
        assert_eq!(w.players[0].under_construction, 0);
        let u = w.unit(p).unwrap();
        assert!(!u.inside && u.order == Order::Idle);
    }

    #[test]
    fn computer_players_pay_when_the_build_is_decided() {
        let (mut w, _) = town();
        let p = w.spawn(PEASANT, 1, (12, 2), 0).unwrap();
        build(&mut w, 1, p, (14, 14));
        assert_eq!((w.players[1].gold, w.players[1].lumber), (500, 750));
    }

    /// Round-49 errata (`xhuman12-10000` n = 116, `human14-10000` n = 1040): a computer player's build walk that ends
    /// with no route doesn't give the payment back.
    #[test]
    fn a_computer_build_walk_with_no_route_keeps_the_payment() {
        let (mut w, _) = town();
        for y in 0..24 {
            for x in 18..24 {
                w.terrain.clear_bits(x, y, crate::terrain::bits::LAND);
                w.terrain.set_bits(x, y, crate::terrain::bits::WATER);
                w.terrain.regions[(y * 24 + x) as usize] = 2;
            }
        }
        let p = w.spawn(PEASANT, 1, (12, 2), 0).unwrap();
        build(&mut w, 1, p, (20, 20));
        for _ in 0..300 {
            w.step(&[]);
        }
        let u = w.unit(p).unwrap();
        assert!(!matches!(u.order, Order::Build { .. }), "{:?} at {:?}", u.order, u.tile);
        assert_eq!(farm_at(&w, (20, 20)), None);
        assert_eq!((w.players[1].gold, w.players[1].lumber), (500, 750));
    }

    #[test]
    fn an_occupied_site_is_refused() {
        let (mut w, p) = town();
        w.spawn(0, 0, (9, 9), 0).unwrap();
        build(&mut w, 0, p, (8, 8));
        for _ in 0..400 {
            w.step(&[]);
            if w.events.iter().any(|e| matches!(e, Event::CantPlace { player: 0, .. })) {
                break;
            }
        }
        assert_eq!(w.unit(p).unwrap().order, Order::Idle);
        assert_eq!(farm_at(&w, (8, 8)), None);
        assert_eq!(w.players[0].gold, 1000);
        // The failed worker goes idle in the same update with wait 3, and its tile stays marked even on the site's
        // footprint (round-40 errata).
        let u = w.unit(p).unwrap();
        assert_eq!(u.tile, (8, 8));
        assert_ne!(w.terrain.word(8, 8) & bits::UNIT, 0);
        assert!(w.events.iter().any(|e| matches!(e, Event::CantPlace { player: 0, .. })));
    }

    #[test]
    fn cancelling_lets_the_builder_out() {
        let (mut w, p) = town();
        build(&mut w, 0, p, (8, 8));
        for _ in 0..400 {
            w.step(&[]);
            if w.unit(p).unwrap().inside {
                break;
            }
        }
        let farm = farm_at(&w, (8, 8)).unwrap();
        w.step(&[Command::Cancel { player: 0, unit: farm }]);
        let u = w.unit(p).unwrap();
        assert!(!u.inside && u.order == Order::Idle);
        // The open question's default: it faces the building as at completion.
        assert_eq!((u.tile, u.facing), ((7, 8), 3));
    }

    /// A 3 x 3 shore building type (a shipyard-like synthetic type).
    const SHORE: u8 = 60;

    fn shore_town() -> (World, UnitId) {
        let (mut w, p) = town();
        w.stats.0[SHORE as usize] = w.stats.0[kind::FARM as usize];
        w.stats.0[SHORE as usize].size = (3, 3);
        w.stats.0[SHORE as usize].flags = flags::BUILDING | flags::SHORE_BUILDING;
        (w, p)
    }

    fn set_region(w: &mut World, t: (i32, i32), r: u16) {
        let n = w.terrain.at(t.0, t.1).unwrap();
        w.terrain.regions[n] = r;
    }

    /// Pathfinding spec, "Shore building (build goal)" and its round-59 vectors: the top-left tile if it is in the
    /// worker's region (not the bottom-left corner), else the first corner in it (bottom-right, top-right,
    /// bottom-left, top-left), else the top-left tile; a sea or air worker type makes no corner test.
    #[test]
    fn the_shore_build_goal_prefers_the_top_left_then_the_corners() {
        let (mut w, p) = shore_town();
        let i = w.index(p).unwrap();
        w.units[i].tile = (5, 9);
        let mine = w.terrain.region(5, 9).unwrap();
        let other = mine + 1;
        let site = (10, 5);
        let (tl, tr, bl, br) = ((10, 5), (12, 5), (10, 7), (12, 7));
        // The whole footprint in the worker's region: the top-left tile, though the building goal rule picks (10, 7).
        assert_eq!(footprint_goal((5, 9), site, (3, 3)), bl);
        assert_eq!(w.build_goal(i, SHORE, site), tl);
        // The top-left tile in another region: the first corner in the worker's region.
        set_region(&mut w, tl, other);
        assert_eq!(w.build_goal(i, SHORE, site), br);
        set_region(&mut w, br, other);
        assert_eq!(w.build_goal(i, SHORE, site), tr);
        set_region(&mut w, tr, other);
        assert_eq!(w.build_goal(i, SHORE, site), bl);
        // No corner in the worker's region: the top-left tile.
        set_region(&mut w, bl, other);
        assert_eq!(w.build_goal(i, SHORE, site), tl);
        // A sea or air worker type: the top-left tile with no corner test.
        set_region(&mut w, br, mine);
        assert_eq!(w.build_goal(i, SHORE, site), br);
        for f in [flags::SEA, flags::AIR] {
            w.stats.0[PEASANT as usize].flags = flags::PEON | f;
            assert_eq!(w.build_goal(i, SHORE, site), tl, "{f:#x}");
        }
    }

    /// Pathfinding spec, "Goal pulled back" (round-59 errata, the `mp-frosty-fjords-bne-30000` peasant 1560 vector):
    /// the build command stores its goal pulled back; the site is unchanged.
    #[test]
    fn the_build_command_pulls_the_goal_back() {
        let (mut w, p) = shore_town();
        let i = w.index(p).unwrap();
        w.units[i].tile = (2, 10);
        let site = (10, 9);
        assert_eq!(w.build_goal(i, SHORE, site), (10, 9));
        // The raw goal and the first tile tested unusable (a terrain bit of the worker's mask), the second usable:
        // the goal becomes the first tile tested.
        for t in [(10, 9), (9, 9), (9, 10)] {
            w.terrain.set_bits(t.0, t.1, bits::OBSTACLE);
        }
        let pulled = w.pull_back(i, (10, 9));
        assert!(matches!(pulled, (9, 9) | (9, 10)), "{pulled:?}");
        assert_eq!(
            w.build_command(i, SHORE, site),
            Some(Order::Build {
                kind: SHORE,
                site,
                goal: Some(pulled),
            })
        );
    }

    /// Build-train spec, "Placing a building" (round-59 errata, the `mp-frosty-fjords-bne-30000` peasant 1560 vectors
    /// at n = 16718 to 16753): on arrival at the stored goal, order 28 works the goal out again from the worker's tile.
    /// Within the arrival radius (1 for a land worker and a shore building) the building is placed; 2 tiles away, the
    /// build is given again as a command with the new goal pulled back, taking over in the same update (still
    /// animation, wait 3).
    #[test]
    fn order_28_works_the_goal_out_again_on_arrival() {
        let (mut w, p) = shore_town();
        let site = (7, 6);
        for y in 6..9 {
            for x in 7..10 {
                w.terrain.clear_bits(x, y, bits::LAND);
                w.terrain.set_bits(x, y, bits::SHORE);
            }
        }
        let move_to = |w: &mut World, t: (i32, i32)| {
            let i = w.index(p).unwrap();
            let u = w.units[i].clone();
            w.mark(&u, u.tile, None);
            w.units[i].tile = t;
            w.units[i].pos = (t.0 * TILE_PX, t.1 * TILE_PX);
            let u = w.units[i].clone();
            w.mark(&u, t, Some(u.id));
            i
        };
        let i = move_to(&mut w, (5, 7));
        // Arrived at the stored goal (its own tile), but the goal worked out again, (7, 6), is 2 tiles away.
        assert_eq!(w.build_goal(i, SHORE, site), (7, 6));
        w.build_order(i, SHORE, site, Some((5, 7)));
        let u = &w.units[i];
        let again = Order::Build {
            kind: SHORE,
            site,
            goal: Some(w.pull_back(i, (7, 6))),
        };
        assert_eq!(
            (u.order, u.pending, u.inside, u.script.anim, u.script.wait),
            (again, None, false, anim::STILL, 3)
        );
        // One tile away the arrival test passes and the building goes up.
        let i = move_to(&mut w, (6, 7));
        w.build_order(i, SHORE, site, Some((6, 7)));
        let u = w.unit(p).unwrap();
        assert!(u.inside && matches!(u.order, Order::InsideSite { .. }), "{:?}", u.order);
    }
}
