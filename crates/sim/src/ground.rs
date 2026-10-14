//! Attacking the ground (commands and orders 17 and 18) and demolition (command 19, orders 20 and 21). Spec:
//! `docs/specs/combat/attack-ground-demolish.md`, with the command rules of `docs/specs/orders/orders.md`
//! ("Commands") and the blast's terrain steps of `docs/specs/movement/occupancy.md` ("Rock", steps 1 to 3, in
//! `Terrain::blast`).
//!
//! A unit whose attack-ground walk finds no path takes the general no-route rule (orders spec, "No route": the
//! immediate order 2, then the idle logic in the same update). Our readings: the blast's cosmetic explosion effects
//! are owned by the demolisher. Order 17 turns the unit to the tile on every run, even while its attack animation is
//! already playing.

use crate::movement::{Goal, Walk, chebyshev};
use crate::sound::Sound;
use crate::terrain::{bits, region};
use crate::world::{Layer, Order, UnitId, World};

/// Damage of each blast hit, applied without armour or a draw.
pub(crate) const BLAST_DAMAGE: u8 = 200;
/// The demolition blast's unit tiles, in the order they are visited ("The blast", step 2).
pub(crate) const BLAST_UNITS: [(i32, i32); 8] = [(1, 1), (-1, 1), (1, -1), (-1, -1), (0, 1), (0, -1), (1, 0), (-1, 0)];
/// Order 20's arrival radius.
const DEMOLISH_RADIUS: i32 = 1;
/// Order 18's arrival radius at its pulled-back goal, before the +1 for sea and air units ("Order 18").
const ORDER_18_RADIUS: i32 = 2;

impl World {
    /// Command 17: order 17 when the target tile shares the unit's region value or is already in range on both axes,
    /// otherwise order 18. The order carries no target unit, so any the unit had is forgotten with its old order.
    pub(crate) fn attack_ground_command(&self, i: usize, tile: (i32, i32)) -> Order {
        let u = &self.units[i];
        let same_region = self.terrain.region(tile.0, tile.1).is_some()
            && self.terrain.region(tile.0, tile.1) == self.terrain.region(u.tile.0, u.tile.1);
        if same_region || chebyshev(u.tile, tile) <= self.reach(u) {
            Order::AttackGround { tile }
        } else {
            Order::AttackGroundMove { tile, goal: tile }
        }
    }

    /// Command 19: order 20 at a target unit, or at a tile holding forest, a reserved tree, rock or a wall (by its
    /// region value or its wall bits); order 21 at any other tile.
    pub(crate) fn demolish_command(&self, i: usize, target: Option<UnitId>, tile: (i32, i32)) -> Order {
        if let Some(t) = target.filter(|&t| t != self.units[i].id && self.unit(t).is_some()) {
            return Order::Demolish { at: Some(t), tile };
        }
        let obstacle = self.terrain.region(tile.0, tile.1).is_some_and(|r| {
            matches!(
                r,
                region::FOREST | region::RESERVED_FOREST | region::ROCK | region::WALL
            )
        }) || (self.terrain.contains(tile.0, tile.1)
            && self.terrain.word(tile.0, tile.1) & (bits::HUMAN_WALL | bits::COMPUTER_WALL) != 0);
        if obstacle {
            Order::Demolish { at: None, tile }
        } else {
            Order::DemolishAt { tile }
        }
    }

    /// Order 17: in range, face the tile and strike (the attack sound and a missile at the tile's centre); out of
    /// range, walk toward it, going idle if the walk can't go on. No acquisition, no friendly-fire check, no end.
    pub(crate) fn attack_ground(&mut self, i: usize, tile: (i32, i32)) {
        let u = &self.units[i];
        let reach = self.reach(u);
        if chebyshev(u.tile, tile) <= reach {
            // It turns to the tile on every run, even when the attack animation is already playing; a warship turns
            // broadside (the table gives the same facing again at the strike decision).
            let f = self.strike_facing(u, tile);
            let u = &mut self.units[i];
            u.path.clear();
            u.facing = f;
            self.face_and_strike(i, tile);
            return;
        }
        let g = Goal {
            tile,
            radius: reach,
            range: true,
            zone: None,
        };
        // Orders spec, "No route" (round 68, code Q4): idle as an immediate order, and the idle logic runs in the same
        // update. Arriving out of reach (not expected with the range goal) keeps the plain immediate idle.
        match self.move_toward(i, g) {
            Walk::NoPath => self.idle_at_once(i),
            Walk::Arrived => self.stop(i),
            Walk::Stepping | Walk::Blocked => {}
        }
    }

    /// Order 18 ("Order 18: moving into range"; `orc9-orc09-ground-move-goals-1100`, V267 and V268): the unit has
    /// arrived when it is within its type's base attack range (no range upgrades) of the target tile on both axes, or
    /// when its walk toward the goal the command pulled back is within 2 tiles of it (3 for sea and air units, the +1
    /// of the arrival test). On arrival the path is dropped and order 17 on the target tile is given as a command that
    /// takes over in the same update (attack animation, wait 3, facing unchanged); its logic, which turns and strikes,
    /// first runs at the next step done (V269).
    pub(crate) fn attack_ground_move(&mut self, i: usize, tile: (i32, i32), goal: (i32, i32)) {
        let u = &self.units[i];
        let base = (self.stats_of(u.kind).attack_range as i32).max(1);
        let arrived = chebyshev(u.tile, tile) <= base
            || match self.move_toward(i, Goal::tile(goal, ORDER_18_RADIUS)) {
                Walk::Arrived => true,
                Walk::NoPath => return self.idle_at_once(i),
                Walk::Stepping | Walk::Blocked => false,
            };
        if arrived {
            self.units[i].path.clear();
            self.command_at_once(i, Order::AttackGround { tile });
        }
    }

    /// Orders 20 and 21: the reaction to an attacker, then the walk to the goal, and the blast on arrival.
    pub(crate) fn demolish(&mut self, i: usize, o: Order) {
        if self.react(i) {
            return;
        }
        let u = &self.units[i];
        let g = match o {
            Order::Demolish { at: Some(t), .. } => {
                let Some(t) = self.unit(t) else { return };
                // A shore building by its shore corner goal ([`World::target_goal`]).
                let goal = self.target_goal(u, t);
                Goal {
                    tile: goal,
                    radius: DEMOLISH_RADIUS,
                    range: false,
                    zone: Some((
                        t.tile.0 - 1,
                        t.tile.1 - 1,
                        t.tile.0 + t.size.0 as i32,
                        t.tile.1 + t.size.1 as i32,
                    )),
                }
            }
            Order::Demolish { at: None, tile } => Goal::tile(tile, DEMOLISH_RADIUS),
            Order::DemolishAt { tile } => Goal::tile(tile, 0),
            _ => return,
        };
        // Orders spec ("Getting there", `orc10-sappers-2500` sapper 1501, n = 35): the plain no-route rule.
        if self.walk(i, g) == Walk::Arrived {
            self.blast(i);
        }
    }

    /// "The blast", at the demolisher's own tile: terrain, then the ground occupants of the 8 neighbours (each hit at
    /// most once: an explosion effect, 200, and 200 more if its tile is still occupied), then the demolisher dies
    /// uncredited, then `misc\explode` at the tile.
    pub(crate) fn blast(&mut self, i: usize) {
        let me = self.units[i].id;
        let (x, y) = self.units[i].tile;
        let area = self.terrain.region(x, y).unwrap_or(region::LAND_AREA);
        self.terrain.blast(x, y, area);
        let mut hit: Vec<UnitId> = Vec::new();
        for (dx, dy) in BLAST_UNITS {
            let (tx, ty) = (x + dx, y + dy);
            let Some(id) = self.occupant(Layer::Land, tx, ty) else {
                continue;
            };
            if hit.contains(&id) {
                continue;
            }
            hit.push(id);
            let Some(ti) = self.index(id) else { continue };
            let c = self.units[ti].centre_px();
            self.explosion(me, (c.0, c.1 - 8));
            self.damage(ti, BLAST_DAMAGE, me);
            // A kill can create a unit (an oil platform's patch), so indices are looked up again.
            if self.occupant(Layer::Land, tx, ty).is_some()
                && let Some(ti) = self.index(id)
            {
                self.damage(ti, BLAST_DAMAGE, me);
            }
        }
        if let Some(i) = self.index(me) {
            self.kill(i);
        }
        self.sound_at_tile((x, y), Sound::Explode);
    }
}

#[cfg(test)]
mod tests {
    use tidewater_formats::udta::{UnitStats, flags};

    use super::*;
    use crate::Command;
    use crate::combat::missile;
    use crate::sound::{Cue, Place};
    use crate::terrain::RUBBLE;
    use crate::world::tests::{field, run};

    const FOOTMAN: u8 = 0;
    /// An unarmed copy of the footman, which never fights back.
    const DUMMY: u8 = 1;
    const GRYPHON: u8 = 2;
    const BALLISTA: u8 = 4;
    const CATAPULT: u8 = 5;
    const SHIP: u8 = 0x1a;
    const SQUAD: u8 = 0x0e;
    const RANGER: u8 = 0x12;
    const BARRACKS: u8 = 0x40;
    /// The cosmetic explosion effect kind.
    const EXPLOSION: u8 = 0x17;

    /// A 40 x 30 open field with the unit types of these tests.
    fn map() -> World {
        let mut w = field(40, 30);
        let t = &mut w.stats.0;
        t[CATAPULT as usize].flags |= flags::GROUND_ATTACK;
        t[DUMMY as usize] = UnitStats {
            flags: flags::LAND,
            ..t[FOOTMAN as usize]
        };
        t[BALLISTA as usize] = UnitStats {
            missile: missile::BALLISTA,
            hit_points: 300,
            ..t[CATAPULT as usize]
        };
        t[SHIP as usize] = UnitStats {
            flags: flags::SEA | flags::CAN_ATTACK | flags::GROUND_ATTACK,
            attack_range: 6,
            missile: missile::BIG_CANNON,
            ..t[CATAPULT as usize]
        };
        t[RANGER as usize] = UnitStats {
            attack_range: 4,
            ..t[CATAPULT as usize]
        };
        t[SQUAD as usize] = UnitStats {
            hit_points: 40,
            size: (1, 1),
            flags: flags::LAND | flags::KILLED_BY_INVISIBILITY,
            ..Default::default()
        };
        t[BARRACKS as usize] = UnitStats {
            hit_points: 800,
            size: (3, 3),
            flags: flags::BUILDING,
            ..Default::default()
        };
        w
    }

    fn at(w: &World, id: UnitId) -> usize {
        w.index(id).unwrap()
    }

    fn set_region(w: &mut World, t: (i32, i32), r: u16, word: u16) {
        let n = w.terrain.at(t.0, t.1).unwrap();
        w.terrain.regions[n] = r;
        w.terrain.words[n] = word;
    }

    fn ground(w: &World, unit: UnitId, tile: (i32, i32)) -> Order {
        let c = Command::AttackGround {
            player: w.unit(unit).unwrap().owner,
            units: vec![unit],
            tile,
        };
        let mut w = w.clone();
        let i = at(&w, unit);
        w.command_order(i, &c).unwrap()
    }

    fn demolish(w: &World, unit: UnitId, target: Option<UnitId>, tile: (i32, i32)) -> Order {
        let c = Command::Demolish {
            player: w.unit(unit).unwrap().owner,
            units: vec![unit],
            target,
            tile,
        };
        let mut w = w.clone();
        let i = at(&w, unit);
        w.command_order(i, &c).unwrap()
    }

    fn fired(w: &World) -> bool {
        w.missiles.iter().flatten().any(|m| m.kind != EXPLOSION)
    }

    /// Steps until `done`, returning the number of steps; panics after `limit`.
    fn until(w: &mut World, limit: u32, done: impl Fn(&World) -> bool) -> u32 {
        for n in 0..limit {
            if done(w) {
                return n;
            }
            w.step(&[]);
        }
        panic!("not done after {limit} steps");
    }

    /// A wall tile broken to rubble: open land with a rubble graphic.
    fn rubble(w: &World, t: (i32, i32)) -> bool {
        w.terrain.word(t.0, t.1) & bits::OBSTACLE == 0 && w.terrain.tile(t.0, t.1).is_some_and(|i| i >= RUBBLE)
    }

    fn explosions(w: &World) -> Vec<(i32, i32)> {
        w.missiles
            .iter()
            .flatten()
            .filter(|m| m.kind == EXPLOSION)
            .map(|m| m.pos)
            .collect()
    }

    // ----- command 17 ------------------------------------------------------------------------------------------

    #[test]
    fn command_17_by_region_then_range() {
        let mut w = map();
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        // Orders spec vectors: 6 tiles away, and 12 tiles away in the catapult's own land area: order 17 at once.
        assert_eq!(ground(&w, c, (16, 10)), Order::AttackGround { tile: (16, 10) });
        assert_eq!(ground(&w, c, (22, 10)), Order::AttackGround { tile: (22, 10) });
        assert_eq!(ground(&w, c, (30, 10)), Order::AttackGround { tile: (30, 10) });
        // A different region in range: order 17; out of range: order 18.
        set_region(&mut w, (14, 10), 0, bits::WATER);
        assert_eq!(ground(&w, c, (14, 10)), Order::AttackGround { tile: (14, 10) });
        set_region(&mut w, (25, 10), region::FOREST, bits::OBSTACLE | bits::LAND);
        assert_eq!(
            ground(&w, c, (25, 10)),
            Order::AttackGroundMove {
                tile: (25, 10),
                goal: (25, 10)
            }
        );
        // In range means both axes.
        set_region(&mut w, (18, 19), region::FOREST, bits::OBSTACLE | bits::LAND);
        assert_eq!(
            ground(&w, c, (18, 19)),
            Order::AttackGroundMove {
                tile: (18, 19),
                goal: (18, 19)
            }
        );
        set_region(&mut w, (18, 18), region::FOREST, bits::OBSTACLE | bits::LAND);
        assert_eq!(ground(&w, c, (18, 18)), Order::AttackGround { tile: (18, 18) });
    }

    #[test]
    fn command_17_ignores_the_unit_under_the_cursor() {
        let mut w = map();
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        let foe = w.spawn(DUMMY, 1, (12, 10), 0).unwrap();
        let i = at(&w, c);
        w.units[i].order = Order::Attack { target: foe };
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (12, 10),
        }]);
        assert_eq!(w.units[at(&w, c)].pending, Some(Order::AttackGround { tile: (12, 10) }));
    }

    #[test]
    fn battleship_targets_land_in_range_at_once() {
        let mut w = map();
        for y in 0..30 {
            for x in 0..8 {
                set_region(&mut w, (x, y), 0, bits::WATER);
            }
        }
        let s = w.spawn(SHIP, 0, (4, 10), 0).unwrap();
        assert_eq!(ground(&w, s, (7, 10)), Order::AttackGround { tile: (7, 10) });
        assert_eq!(ground(&w, s, (10, 10)), Order::AttackGround { tile: (10, 10) });
        assert_eq!(
            ground(&w, s, (11, 10)),
            Order::AttackGroundMove {
                tile: (11, 10),
                goal: (11, 10)
            }
        );
    }

    // ----- order 17 --------------------------------------------------------------------------------------------

    #[test]
    fn catapult_walks_into_range_in_its_own_area_and_fires() {
        let mut w = map();
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (30, 10),
        }]);
        assert_eq!(w.unit(c).unwrap().pending, Some(Order::AttackGround { tile: (30, 10) }));
        until(&mut w, 400, fired);
        let u = w.unit(c).unwrap();
        assert_eq!((u.tile, u.order), ((22, 10), Order::AttackGround { tile: (30, 10) }));
        // The missile is aimed at the tile's centre (give or take the scatter).
        let m = w.missiles.iter().flatten().next().unwrap();
        assert!((m.to.0 - (30 * 32 + 16)).abs() <= 4 && (m.to.1 - (10 * 32 + 16)).abs() <= 4);
        assert_eq!((m.target, m.splash), (None, true));
    }

    #[test]
    fn catapult_fires_at_water_in_range_without_moving() {
        let mut w = map();
        set_region(&mut w, (14, 10), 0, bits::WATER);
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (14, 10),
        }]);
        until(&mut w, 100, fired);
        assert_eq!(w.unit(c).unwrap().tile, (10, 10));
        // It faced the tile (east) to strike.
        assert_eq!(w.unit(c).unwrap().facing, 2);
    }

    #[test]
    fn order_18_becomes_17_at_base_range() {
        let mut w = map();
        set_region(&mut w, (25, 10), region::FOREST, bits::OBSTACLE | bits::LAND);
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (25, 10),
        }]);
        assert_eq!(
            w.unit(c).unwrap().pending,
            Some(Order::AttackGroundMove {
                tile: (25, 10),
                goal: (25, 10)
            })
        );
        until(&mut w, 400, |w| {
            w.unit(c).unwrap().order == Order::AttackGround { tile: (25, 10) }
        });
        // The path may bend around the forest goal; the column is what the range decides.
        let u = w.unit(c).unwrap();
        let tile = u.tile;
        assert_eq!(tile.0, 17);
        assert!(u.path.is_empty());
        until(&mut w, 100, fired);
        assert_eq!(w.unit(c).unwrap().tile, tile);
    }

    /// "Order 18" (V268): a target far inside another region is reached by the second test, 2 tiles from the goal the
    /// command pulled back, however far the target tile still is.
    #[test]
    fn order_18_arrives_within_2_of_its_pulled_back_goal() {
        let mut w = map();
        for y in 0..30 {
            for x in 20..36 {
                set_region(&mut w, (x, y), region::FOREST, bits::OBSTACLE | bits::LAND);
            }
        }
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (34, 10),
        }]);
        // The tile before the first usable one on the line back toward the catapult.
        assert_eq!(
            w.unit(c).unwrap().pending,
            Some(Order::AttackGroundMove {
                tile: (34, 10),
                goal: (20, 10)
            })
        );
        // The tile it stands on when order 18 gives way to order 17.
        let mut last = (0, 0);
        for _ in 0..400 {
            let u = w.unit(c).unwrap();
            if u.order == (Order::AttackGround { tile: (34, 10) }) {
                break;
            }
            last = u.tile;
            w.step(&[]);
        }
        assert_eq!(last, (18, 9));
    }

    #[test]
    fn order_18_ignores_range_upgrades() {
        let mut w = map();
        w.players[0].upgrades.longbow = true;
        set_region(&mut w, (20, 10), region::FOREST, bits::OBSTACLE | bits::LAND);
        let r = w.spawn(RANGER, 0, (10, 10), 0).unwrap();
        // Range 5 with the upgrade, which command 17 uses; order 18's arrival uses the base 4.
        assert_eq!(w.reach(&w.units[at(&w, r)]), 5);
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![r],
            tile: (20, 10),
        }]);
        until(&mut w, 400, |w| {
            w.unit(r).unwrap().order == Order::AttackGround { tile: (20, 10) }
        });
        assert_eq!(w.unit(r).unwrap().tile.0, 16);
    }

    #[test]
    fn splash_hurts_own_units_and_firing_never_stops() {
        let mut w = map();
        let b = w.spawn(BALLISTA, 0, (5, 10), 0).unwrap();
        let own = w.spawn(DUMMY, 0, (10, 10), 0).unwrap();
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![b],
            tile: (10, 10),
        }]);
        run(&mut w, 600);
        assert!(
            w.unit(own).is_none_or(|u| u.dying || u.hp < 60),
            "no friendly-fire check"
        );
        // With nothing left at the tile it keeps firing.
        assert_eq!(w.unit(b).unwrap().order, Order::AttackGround { tile: (10, 10) });
        until(&mut w, 100, fired);
    }

    #[test]
    fn ignores_an_enemy_that_attacks_it() {
        let mut w = map();
        let b = w.spawn(BALLISTA, 0, (5, 10), 0).unwrap();
        let grunt = w.spawn(FOOTMAN, 1, (5, 12), 0).unwrap();
        w.step(&[
            Command::AttackGround {
                player: 0,
                units: vec![b],
                tile: (12, 10),
            },
            Command::Attack {
                player: 1,
                units: vec![grunt],
                target: b,
            },
        ]);
        run(&mut w, 300);
        let u = w.unit(b).unwrap();
        assert!(u.hp < 300, "the grunt hit it");
        assert_eq!(u.order, Order::AttackGround { tile: (12, 10) });
        assert_eq!(u.pending, None);
        assert_eq!(w.unit(grunt).unwrap().hp, 60);
    }

    #[test]
    fn stop_ends_the_attack() {
        let mut w = map();
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (16, 10),
        }]);
        until(&mut w, 100, fired);
        w.step(&[Command::Stop {
            player: 0,
            units: vec![c],
        }]);
        run(&mut w, 60);
        assert_eq!(w.unit(c).unwrap().order, Order::Idle);
    }

    #[test]
    fn out_of_range_and_stuck_goes_idle() {
        let mut w = map();
        let c = w.spawn(CATAPULT, 0, (10, 10), 0).unwrap();
        for (dx, dy) in BLAST_UNITS {
            w.terrain.set_bits(10 + dx, 10 + dy, bits::OBSTACLE);
        }
        w.step(&[Command::AttackGround {
            player: 0,
            units: vec![c],
            tile: (30, 10),
        }]);
        assert_eq!(w.unit(c).unwrap().pending, Some(Order::AttackGround { tile: (30, 10) }));
        run(&mut w, 30);
        assert_eq!(w.unit(c).unwrap().order, Order::Idle);
        assert!(!fired(&w));
    }

    // ----- command 19 and the walk -----------------------------------------------------------------------------

    #[test]
    fn command_19_targets() {
        let mut w = map();
        let s = w.spawn(SQUAD, 0, (10, 10), 0).unwrap();
        let g = w.spawn(DUMMY, 1, (14, 10), 0).unwrap();
        assert_eq!(
            demolish(&w, s, Some(g), (14, 10)),
            Order::Demolish {
                at: Some(g),
                tile: (14, 10)
            }
        );
        // Orders spec vectors: a tree tile gives order 20, open grass order 21.
        set_region(&mut w, (20, 20), region::FOREST, bits::OBSTACLE | bits::LAND);
        assert_eq!(
            demolish(&w, s, None, (20, 20)),
            Order::Demolish {
                at: None,
                tile: (20, 20)
            }
        );
        assert_eq!(demolish(&w, s, None, (15, 15)), Order::DemolishAt { tile: (15, 15) });
        // A reserved tree, rock, a map wall, and a built wall (by its wall bit) count as obstacles.
        set_region(&mut w, (21, 20), region::RESERVED_FOREST, bits::OBSTACLE | bits::LAND);
        set_region(&mut w, (22, 20), region::ROCK, bits::OBSTACLE | bits::LAND);
        set_region(&mut w, (23, 20), region::WALL, bits::OBSTACLE | bits::LAND);
        w.terrain.build_wall(24, 20, false, false);
        for x in 21..=24 {
            assert_eq!(
                demolish(&w, s, None, (x, 20)),
                Order::Demolish {
                    at: None,
                    tile: (x, 20)
                },
                "x = {x}"
            );
        }
    }

    #[test]
    fn squad_walks_up_to_a_grunt_and_blows_up() {
        let mut w = map();
        let s = w.spawn(SQUAD, 0, (5, 10), 0).unwrap();
        let g = w.spawn(DUMMY, 1, (12, 10), 0).unwrap();
        w.step(&[Command::Demolish {
            player: 0,
            units: vec![s],
            target: Some(g),
            tile: (12, 10),
        }]);
        assert_eq!(
            w.unit(s).unwrap().pending,
            Some(Order::Demolish {
                at: Some(g),
                tile: (12, 10)
            })
        );
        until(&mut w, 400, |w| w.unit(s).is_none_or(|u| u.dying));
        let tile = w.unit(s).unwrap().tile;
        assert_eq!((tile.0, chebyshev(tile, (12, 10))), (11, 1));
        assert!(w.unit(g).is_none_or(|u| u.dying));
        assert_eq!(w.players[0].kills, 1);
    }

    #[test]
    fn the_blast_is_where_the_squad_stands_not_on_the_tree() {
        let mut w = map();
        set_region(&mut w, (20, 20), region::FOREST, bits::OBSTACLE | bits::LAND);
        // A corridor along row 20, so the squad comes from the west.
        for x in 11..=19 {
            w.terrain.set_bits(x, 19, bits::OBSTACLE);
            w.terrain.set_bits(x, 21, bits::OBSTACLE);
        }
        w.terrain.build_wall(18, 21, false, true);
        let near = w.spawn(DUMMY, 1, (20, 21), 0).unwrap();
        let far = w.spawn(DUMMY, 1, (21, 20), 0).unwrap();
        let s = w.spawn(SQUAD, 0, (10, 20), 0).unwrap();
        w.step(&[Command::Demolish {
            player: 0,
            units: vec![s],
            target: None,
            tile: (20, 20),
        }]);
        until(&mut w, 400, |w| w.unit(s).is_none_or(|u| u.dying));
        assert_eq!(w.unit(s).unwrap().tile, (19, 20));
        assert!(rubble(&w, (18, 21)));
        assert!(w.unit(near).unwrap().dying, "(20, 21) is next to (19, 20)");
        assert_eq!(w.unit(far).unwrap().hp, 60, "(21, 20) is not");
        let blast = w.sounds.cues.iter().rev().find(|c| c.sound == Sound::Explode).unwrap();
        assert_eq!(blast.place.unwrap().tile(), (19, 20));
    }

    #[test]
    fn squad_blows_up_on_an_open_tile() {
        let mut w = map();
        let s = w.spawn(SQUAD, 0, (10, 10), 0).unwrap();
        w.step(&[Command::Demolish {
            player: 0,
            units: vec![s],
            target: None,
            tile: (15, 15),
        }]);
        assert_eq!(w.unit(s).unwrap().pending, Some(Order::DemolishAt { tile: (15, 15) }));
        until(&mut w, 400, |w| w.unit(s).is_none_or(|u| u.dying));
        assert_eq!(w.unit(s).unwrap().tile, (15, 15));
    }

    #[test]
    fn a_lost_target_ends_the_walk() {
        let mut w = map();
        let s = w.spawn(SQUAD, 0, (5, 10), 0).unwrap();
        let g = w.spawn(DUMMY, 1, (20, 10), 0).unwrap();
        w.step(&[Command::Demolish {
            player: 0,
            units: vec![s],
            target: Some(g),
            tile: (20, 10),
        }]);
        run(&mut w, 30);
        let gi = at(&w, g);
        w.kill(gi);
        run(&mut w, 30);
        let u = w.unit(s).unwrap();
        assert_eq!((u.order, u.dying), (Order::Idle, false));
    }

    /// "Getting there" (`orc10-sappers-2500` sapper 1501, n = 35): with no route a demolisher goes
    /// idle as an immediate order (still animation, wait 1, no target) and its idle logic runs in the same update,
    /// with its look-around draw. Nothing explodes.
    #[test]
    fn a_demolisher_with_no_route_goes_idle_at_once_and_runs_the_idle_logic() {
        for o in [
            Order::DemolishAt { tile: (20, 20) },
            Order::Demolish {
                at: None,
                tile: (20, 20),
            },
        ] {
            let mut w = map();
            let s = w.spawn(SQUAD, 0, (10, 10), 0).unwrap();
            for y in 9..=11 {
                for x in 9..=11 {
                    if (x, y) != (10, 10) {
                        w.terrain.set_bits(x, y, crate::terrain::bits::OBSTACLE);
                    }
                }
            }
            let i = at(&w, s);
            w.units[i].order = o;
            let before = w.rng.state;
            w.demolish(i, o);
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait, u.dying),
                (Order::Idle, None, tidewater_formats::script::anim::STILL, 1, false),
                "{o:?}"
            );
            assert_ne!(w.rng.state, before, "the look-around drew in the same update");
        }
    }

    // ----- the blast -------------------------------------------------------------------------------------------

    /// A squad of player 0 at `tile`, blown up at once.
    fn blow(w: &mut World, tile: (i32, i32)) -> UnitId {
        let s = w.spawn(SQUAD, 0, tile, 0).unwrap();
        let i = at(w, s);
        w.blast(i);
        s
    }

    #[test]
    fn a_grunt_next_to_the_blast_dies_and_the_kill_is_credited() {
        let mut w = map();
        let g = w.spawn(DUMMY, 1, (11, 10), 0).unwrap();
        blow(&mut w, (10, 10));
        assert!(w.unit(g).unwrap().dying);
        assert_eq!(explosions(&w), [(11 * 32 + 16, 10 * 32 + 16 - 8)]);
        assert_eq!((w.players[0].kills, w.players[1].kills), (1, 0));
    }

    #[test]
    fn a_building_on_several_neighbours_is_hit_once() {
        let mut w = map();
        let b = w.spawn(BARRACKS, 1, (11, 9), 0).unwrap();
        blow(&mut w, (10, 10));
        assert_eq!(w.unit(b).unwrap().hp, 400);
        assert_eq!(explosions(&w), [(12 * 32 + 16, 10 * 32 + 16 - 8)]);
    }

    #[test]
    fn own_units_are_hit_and_air_units_are_not() {
        let mut w = map();
        let own = w.spawn(DUMMY, 0, (9, 9), 0).unwrap();
        let air = w.spawn(GRYPHON, 1, (10, 11), 0).unwrap();
        blow(&mut w, (10, 10));
        assert!(w.unit(own).unwrap().dying);
        assert_eq!(w.unit(air).unwrap().hp, 100);
        assert_eq!(explosions(&w).len(), 1);
    }

    #[test]
    fn unholy_armour_blocks_the_damage_but_not_the_notice() {
        let mut w = map();
        let g = w.spawn(DUMMY, 1, (10, 11), 0).unwrap();
        let gi = at(&w, g);
        w.units[gi].unholy_armor = 100;
        blow(&mut w, (10, 10));
        let u = w.unit(g).unwrap();
        assert_eq!((u.hp, u.dying), (60, false));
        assert_eq!(explosions(&w).len(), 1);
        // The notice itself: the squad is recorded as the attacker (the squad's death then makes it forgotten).
        let mut w = map();
        let g = w.spawn(DUMMY, 1, (10, 11), 0).unwrap();
        let gi = at(&w, g);
        w.units[gi].unholy_armor = 100;
        let s = w.spawn(SQUAD, 0, (10, 10), 0).unwrap();
        let gi = at(&w, g);
        w.damage(gi, BLAST_DAMAGE, s);
        assert_eq!(w.unit(g).unwrap().attacked_by, Some(s));
    }

    #[test]
    fn a_blast_in_the_corner_skips_off_map_tiles() {
        let mut w = map();
        let ids: Vec<UnitId> = [(1, 1), (0, 1), (1, 0)]
            .iter()
            .map(|&t| w.spawn(DUMMY, 1, t, 0).unwrap())
            .collect();
        blow(&mut w, (0, 0));
        assert!(ids.iter().all(|&id| w.unit(id).unwrap().dying));
        assert_eq!(explosions(&w), [(48, 48 - 8), (16, 48 - 8), (48, 16 - 8)]);
    }

    #[test]
    fn neighbours_are_visited_in_order_and_hit_twice() {
        let mut w = map();
        w.stats.0[DUMMY as usize].hit_points = 500;
        let ids: Vec<UnitId> = BLAST_UNITS
            .iter()
            .rev()
            .map(|&(dx, dy)| w.spawn(DUMMY, 1, (10 + dx, 10 + dy), 0).unwrap())
            .collect();
        blow(&mut w, (10, 10));
        let want: Vec<(i32, i32)> = BLAST_UNITS
            .iter()
            .map(|&(dx, dy)| ((10 + dx) * 32 + 16, (10 + dy) * 32 + 16 - 8))
            .collect();
        assert_eq!(explosions(&w), want);
        assert!(ids.iter().all(|&id| w.unit(id).unwrap().hp == 100));
        // 250 HP survives the first hit and dies to the second.
        let mut w = map();
        w.stats.0[DUMMY as usize].hit_points = 250;
        let g = w.spawn(DUMMY, 1, (11, 10), 0).unwrap();
        blow(&mut w, (10, 10));
        assert!(w.unit(g).unwrap().dying);
        assert_eq!(w.players[0].kills, 1);
    }

    #[test]
    fn the_squad_dies_uncredited_and_leaves_nothing() {
        let mut w = map();
        let s = blow(&mut w, (10, 10));
        assert!(w.unit(s).unwrap().dying);
        assert_eq!(w.occupant(Layer::Land, 10, 10), None);
        assert!(w.players.iter().all(|p| p.kills == 0));
        run(&mut w, 60);
        assert!(w.unit(s).is_none_or(|u| !u.remains));
    }

    #[test]
    fn order_of_events() {
        let mut w = map();
        w.terrain.build_wall(11, 11, false, true);
        let g = w.spawn(DUMMY, 1, (9, 10), 0).unwrap();
        w.sounds.cues.clear();
        let s = blow(&mut w, (10, 10));
        assert!(rubble(&w, (11, 11)));
        let cues: Vec<Cue> = w.sounds.cues.clone();
        let sounds: Vec<Sound> = cues.iter().map(|c| c.sound).collect();
        let death = |kind: u8| Sound::Death {
            kind,
            flags: w.stats.0[kind as usize].flags,
        };
        // The grunt's death (a unit hit), then the squad's own death sound, then the blast at the tile.
        assert_eq!(sounds, [death(DUMMY), death(SQUAD), Sound::Explode]);
        assert!(matches!(cues[1].place, Some(Place::Unit { tile: (10, 10), .. })));
        assert!(matches!(cues[2].place, Some(Place::Tile { tile: (10, 10), .. })));
        assert!(w.unit(g).unwrap().dying && w.unit(s).unwrap().dying);
    }
}
