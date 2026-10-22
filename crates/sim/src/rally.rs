//! Rally points (`docs/design/extras.md`, "Rally points"): an optional improvement, off unless
//! [`Extras::rally_points`](crate::Extras) is on. Our own design, not an original-engine rule.
//!
//! A player gives a training building a rally point with [`Command::Rally`](crate::Command): a ground tile, or a unit
//! (a mine to work, a friendly unit to follow, an enemy to attack). When the building finishes training a unit, the
//! unit takes the same command a right click on that spot would give it (`World::right_click_command`), so a worker
//! sent to a mine harvests and a fighter sent to the ground walks. Placing a rally point on the building itself clears
//! it. Nothing here runs with the option off.

use crate::extras::Rally;
use crate::select::kind;
use crate::tech;
use crate::world::{UnitId, World};

/// Whether building type `b` trains any unit, and so can have a rally point.
fn trains_any(b: u8) -> bool {
    (0..tech::unit::TRAINABLE).any(|k| tech::trains(b, k))
}

impl World {
    /// Whether `building` is the player's own finished training building, so a rally point can be set on it (and the
    /// option is on).
    pub fn can_rally(&self, player: u8, building: UnitId) -> bool {
        self.rules.extras.rally_points
            && self.unit(building).is_some_and(|b| {
                b.owner == player && !b.dying && b.building && b.construction.is_none() && trains_any(b.kind)
            })
    }

    /// The rally point of a building, for the interface to draw: the tile and the unit it was set on, if any.
    pub fn rally_point(&self, building: UnitId) -> Option<crate::extras::Rally> {
        self.ext.rally.get(&building).copied()
    }

    /// `Command::Rally`: sets, moves or clears the rally point of the player's own training building.
    pub(crate) fn rally_command(&mut self, player: u8, building: UnitId, target: Option<UnitId>, tile: (i32, i32)) {
        if !self.rules.extras.rally_points {
            return;
        }
        if !self.can_rally(player, building) {
            return;
        }
        if target == Some(building) {
            self.ext.rally.remove(&building);
            return;
        }
        // A target that is gone is a plain tile.
        let target = target.filter(|&t| self.unit(t).is_some_and(|t| !t.dying));
        let tile = target.and_then(|t| self.unit(t)).map_or(tile, |t| t.tile);
        self.ext.rally.insert(building, Rally { target, tile });
    }

    /// A building has finished training `unit`: sends it to the building's rally point, if there is one.
    pub(crate) fn send_to_rally(&mut self, building: UnitId, unit: UnitId) {
        if !self.rules.extras.rally_points {
            return;
        }
        let Some(r) = self.ext.rally.get(&building).copied() else {
            return;
        };
        let Some(i) = self.index(unit) else { return };
        let owner = self.units[i].owner;
        let target = r
            .target
            .filter(|&t| t != unit && self.unit(t).is_some_and(|t| !t.dying));
        if !self.is_mobile(&self.units[i]) || self.units[i].kind > kind::CRITTER {
            return;
        }
        let Some(command) = self.right_click_command(i, owner, target, r.tile) else {
            return;
        };
        if self.rules.enhanced {
            self.units[i].remembered = None;
        }
        let target = if command == 23 && target.is_none() {
            None
        } else {
            target
        };
        self.give_command(i, command, target, r.tile);
    }

    /// Forgets the rally points of buildings that are gone.
    pub(crate) fn prune_rally(&mut self) {
        if self.ext.rally.is_empty() {
            return;
        }
        let gone: Vec<UnitId> = self
            .ext
            .rally
            .keys()
            .copied()
            .filter(|&b| !self.unit(b).is_some_and(|u| !u.dying && u.building))
            .collect();
        for b in gone {
            self.ext.rally.remove(&b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::kind as k;
    use crate::select::action;
    use crate::world::Command;
    use crate::world::tests::{field, run};
    use tidewater_formats::udta::{UnitStats, flags};

    /// A world with a town hall for player 0 at (5, 6), rich, with food, and the option as given.
    fn town(on: bool) -> (World, UnitId) {
        let mut w = field(24, 24);
        for b in 58..100 {
            w.stats.0[b] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING,
                ..Default::default()
            };
        }
        w.stats.0[2] = UnitStats {
            hit_points: 30,
            size: (1, 1),
            flags: flags::LAND | flags::PEON,
            right_click: action::MOVE,
            ..Default::default()
        };
        w.rules.extras.rally_points = on;
        w.rules.enforce_requirements = false;
        let hall = w.spawn(k::TOWN_HALL, 0, (5, 6), 0).unwrap();
        w.spawn(k::FARM, 0, (0, 2), 0).unwrap();
        let p = &mut w.players[0];
        (p.gold, p.lumber, p.oil) = (10_000, 10_000, 10_000);
        w.stats.0[2].build_time = 2;
        w.stats.0[2].gold = 400;
        (w, hall)
    }

    fn train(w: &mut World, hall: UnitId) {
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
    }

    fn rally(hall: UnitId, target: Option<UnitId>, tile: (i32, i32)) -> Command {
        Command::Rally {
            player: 0,
            building: hall,
            target,
            tile,
        }
    }

    #[test]
    fn a_trained_unit_walks_to_the_rally_tile() {
        let (mut w, hall) = town(true);
        w.step(&[rally(hall, None, (14, 14))]);
        assert_eq!(w.ext.rally[&hall].tile, (14, 14));
        train(&mut w, hall);
        run(&mut w, 400);
        let u = w.units.iter().rfind(|u| u.kind == 2).expect("a peasant");
        let near = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() <= 2 && (a.1 - b.1).abs() <= 2;
        assert!(near(u.tile, (14, 14)), "{:?}", u.tile);
    }

    #[test]
    fn without_a_rally_point_the_unit_stays_by_the_building() {
        let (mut w, hall) = town(true);
        train(&mut w, hall);
        run(&mut w, 400);
        let u = w.units.iter().rfind(|u| u.kind == 2).expect("a peasant");
        assert!(u.tile.0 < 10, "{:?}", u.tile);
    }

    #[test]
    fn with_the_option_off_the_command_does_nothing() {
        let (mut w, hall) = town(false);
        let before = w.state_hash();
        w.step(&[rally(hall, None, (14, 14))]);
        assert!(w.ext.is_empty());
        assert_eq!(w.state_hash(), {
            let (mut v, _) = town(false);
            v.step(&[]);
            v.state_hash()
        });
        let _ = before;
    }

    #[test]
    fn a_rally_on_the_building_clears_it_and_only_the_owner_sets_one() {
        let (mut w, hall) = town(true);
        w.step(&[Command::Rally {
            player: 1,
            building: hall,
            target: None,
            tile: (3, 3),
        }]);
        assert!(w.ext.rally.is_empty(), "not the owner's");
        w.step(&[rally(hall, None, (3, 3))]);
        assert_eq!(w.ext.rally.len(), 1);
        w.step(&[rally(hall, Some(hall), (3, 3))]);
        assert!(w.ext.rally.is_empty());
        // A farm trains nothing.
        let farm = w.units.iter().find(|u| u.kind == k::FARM).unwrap().id;
        w.step(&[rally(farm, None, (3, 3))]);
        assert!(w.ext.rally.is_empty());
    }

    #[test]
    fn a_dead_building_loses_its_rally_point() {
        let (mut w, hall) = town(true);
        w.step(&[rally(hall, None, (14, 14))]);
        let i = w.index(hall).unwrap();
        w.kill(i);
        run(&mut w, 2);
        assert!(w.ext.rally.is_empty());
    }

    #[test]
    fn two_machines_agree_with_rally_points_on() {
        let one = || {
            let (mut w, hall) = town(true);
            let mut hashes = Vec::new();
            w.step(&[
                rally(hall, None, (12, 3)),
                Command::Train {
                    player: 0,
                    building: hall,
                    kind: 2,
                },
            ]);
            for _ in 0..300 {
                w.step(&[]);
                hashes.push(w.state_hash());
            }
            hashes
        };
        assert_eq!(one(), one());
    }

    #[test]
    fn the_rally_point_survives_a_save_and_the_loaded_game_runs_the_same_twice() {
        let (mut a, hall) = town(true);
        a.step(&[rally(hall, None, (12, 3))]);
        train(&mut a, hall);
        run(&mut a, 20);
        let bytes = a.save_state();
        let load = || {
            let (mut b, _) = town(false);
            b.restore(&bytes, 1).unwrap();
            b
        };
        let (mut b, mut c) = (load(), load());
        assert_eq!(b.ext.rally, a.ext.rally);
        assert_eq!(b.rules.extras, a.rules.extras);
        run(&mut b, 300);
        run(&mut c, 300);
        assert_eq!(b.state_hash(), c.state_hash());
        let u = b
            .units
            .iter()
            .rfind(|u| u.kind == 2)
            .expect("the unit trained after the load");
        assert!(u.tile.0 >= 8, "it went to the rally point: {:?}", u.tile);
    }
}
