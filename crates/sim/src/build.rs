//! Workers placing buildings: order 28 (go to the site and build) and order 22 (inside the site), and builders coming
//! out when the building is finished, cancelled or destroyed. Specs: `docs/specs/economy/build-train.md` ("Placing a
//! building", "Completion", "Cancelling construction") and `docs/specs/movement/occupancy.md` ("Building placement
//! masks", "Entering a construction site").
//!
//! Not yet here: which worker may build which type (the tech tree and the command panel decide that), and walls,
//! whose pieces become terrain on completion.
//!
//! Our choices: a platform placed on an oil patch removes the patch unit through the death path; a failed placement
//! tells the player with [`Event::CantPlace`].

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::TILE_PX;
use crate::economy::{Event, cost};
use crate::movement::{Goal, Walk};
use crate::placement::{LIMIT_OUT, Source};
use crate::terrain::{bits, mask};
use crate::world::{Gather, Order, UnitId, World};

/// Order 22's wait.
const INSIDE_SITE_WAIT: u8 = 255;
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
        Some(Order::Build { kind, site })
    }

    /// Order 28.
    pub(crate) fn build_order(&mut self, i: usize, kind: u8, site: (i32, i32)) {
        match self.move_toward(i, Goal::tile(site, 0)) {
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
        self.stop(i);
    }

    /// Arrival at the site: pay (players other than computer players), check the site, then place the building and
    /// go inside it.
    fn raise(&mut self, i: usize, kind: u8, site: (i32, i32)) {
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
            let size = self.stats_of(kind).size;
            let on_site =
                (0..size.0 as i32).contains(&(u.tile.0 - site.0)) && (0..size.1 as i32).contains(&(u.tile.1 - site.1));
            if !on_site {
                self.mark(&u, u.tile, Some(u.id));
            }
            self.events.push(Event::CantPlace { player: u.owner, kind });
            self.give_back_and_stop(i, kind);
            return;
        };
        if let Some(Some(p)) = patch
            && let Some(pi) = self.index(p)
        {
            self.kill(pi);
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
        self.units[i].script.wait = INSIDE_SITE_WAIT;
    }

    /// The placement check of the occupancy spec for a building of type `kind` with its top-left tile at `site`.
    /// `Some(patch)` if it passes, with the oil patch an oil platform is placed on.
    pub(crate) fn site_ok(&self, kind: u8, site: (i32, i32)) -> Option<Option<UnitId>> {
        let s = self.stats_of(kind);
        let platform = s.flags & flags::OIL_SOURCE != 0;
        let shore = s.flags & flags::SHORE_BUILDING != 0;
        let m = if platform {
            mask::OIL_PLATFORM
        } else if shore {
            mask::SHORE_BUILDING
        } else {
            mask::LAND_BUILDING
        };
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
                if tanker && bf & flags::OIL_SOURCE != 0 {
                    self.units[k].order = Order::Enter {
                        building: b.id,
                        stepped: true,
                    };
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
                    let unit = self.units[k].clone();
                    self.mark(&unit, to, Some(unit.id));
                    self.start_anim(k, anim::STILL);
                    self.units[k].script.wait = 1;
                    self.apply_unit_sight(k);
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
        farm.gold = 50;
        farm.lumber = 25;
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
        // The builder stands on the site's top-left tile, a footprint tile, so its mark isn't put back
        // (occupancy spec, "Entering a construction site").
        let u = w.unit(p).unwrap();
        assert_eq!(u.tile, (8, 8));
        assert_eq!(w.terrain.word(8, 8) & bits::UNIT, 0);
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
    }
}
