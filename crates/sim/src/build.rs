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
use crate::movement::{Goal, Walk, footprint_goal};
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
        // computer worker coming out of a building is still at the building's top-left tile here.
        let goal = self.build_goal(i, kind, site);
        Some(Order::Build {
            kind,
            site,
            goal: Some(goal),
        })
    }

    /// The build walk's goal ("Placing a building"): a tile of the planned footprint, worked out from the worker's
    /// current tile as if the building already stood at `site`. An oil platform is aimed at its top-left tile plus
    /// (1, 1); any other building by the building goal rule (pathfinding spec), with the shore rule for a shore
    /// building.
    pub(crate) fn build_goal(&self, i: usize, kind: u8, site: (i32, i32)) -> (i32, i32) {
        let s = self.stats_of(kind);
        if s.flags & flags::OIL_SOURCE != 0 {
            return (site.0 + 1, site.1 + 1);
        }
        let u = &self.units[i];
        let size = (s.size.0 as i32, s.size.1 as i32);
        let goal = footprint_goal(u.tile, site, size);
        // Pathfinding spec, "Building goal rule": a land unit heading to a shore building in a different land region
        // aims at a footprint corner in its own region, if there is one. Our reading: the goal tile's region decides,
        // and the corners are tried top-left, top-right, bottom-left, bottom-right.
        if s.flags & flags::SHORE_BUILDING != 0 && u.layer == crate::world::Layer::Land {
            let mine = self.terrain.region(u.tile.0, u.tile.1);
            if self.terrain.region(goal.0, goal.1) != mine {
                let (x1, y1) = (site.0 + size.0 - 1, site.1 + size.1 - 1);
                if let Some(c) = [site, (x1, site.1), (site.0, y1), (x1, y1)]
                    .into_iter()
                    .find(|c| self.terrain.region(c.0, c.1) == mine)
                {
                    return c;
                }
            }
        }
        goal
    }

    /// Order 28: walk to the build goal and build on standing on it (arrival radius 0; a land unit building a shore
    /// building gets the movement spec's +1). Round-41 errata (`xorc7` peasant 1421, n = 2028): the goal is worked
    /// out once and stays fixed while the worker walks; round-47 errata: it is worked out when the command is given
    /// ([`World::build_command`]), and only an order made without one works it out here. Round-42 errata: the order's own logic runs only on
    /// arrival; the walk ends without arriving only with no route (idle at once) or at the blocked counter's limit
    /// (idle as a pending order, `movement.rs` row C).
    pub(crate) fn build_order(&mut self, i: usize, kind: u8, site: (i32, i32), goal: Option<(i32, i32)>) {
        let goal = goal.unwrap_or_else(|| self.build_goal(i, kind, site));
        self.units[i].order = Order::Build {
            kind,
            site,
            goal: Some(goal),
        };
        let shore =
            self.stats_of(kind).flags & flags::SHORE_BUILDING != 0 && self.units[i].layer == crate::world::Layer::Land;
        match self.move_toward(i, Goal::tile(goal, shore as i32)) {
            Walk::Arrived => self.raise(i, kind, site),
            Walk::NoPath => self.give_back_and_stop(i, kind),
            Walk::Stepping | Walk::Blocked => {}
        }
    }

    fn give_back_and_stop(&mut self, i: usize, kind: u8) {
        let owner = self.units[i].owner;
        if self.computer_player(owner) {
            let c = cost(&self.stats_of(kind));
            self.refund(owner, c);
        }
        // Round-42 errata: idle as an immediate order, whose idle logic runs in the same update (orders spec, "No
        // route").
        self.idle_at_once(i);
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
}
