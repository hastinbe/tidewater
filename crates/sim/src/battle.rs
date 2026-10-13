//! Attacks, missiles and death. Spec: `docs/specs/combat/damage.md`, "How an attack delivers damage", "Missile
//! flight", "Impact effects", "Applying damage" (with its Dark Portal case) and "Splash on impact". Missiles fly by
//! the line rule of `spells/spells.md` ("How flying effects move"), which `spells.rs` implements as `Line`. The attack
//! order's chase-and-face behaviour and the range measure are placeholders of our own until their specs land. The
//! cosmetic cannon smoke (kind 0x19) follows a unit's cannon missile in the next free effect slot. The kill credit,
//! mission score and lost counters follow `docs/specs/game/campaign-flow.md` ("The score screen", "Mission score").
//! Our reading: only a kill by damage from a unit is credited (a spell that kills without damage, such as
//! polymorph, credits nobody), and only deaths and rescue's owner change count as lost (a removed corpse's move to
//! the neutral player doesn't).

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags as udta_flags;

use crate::TILE_PX;
use crate::combat::{self, missile};
use crate::movement::{Goal, Walk, building_goal, chebyshev};
use crate::slots;
use crate::spells::{Fx, Line};
use crate::terrain::bits;
use crate::world::{Layer, Order, Unit, UnitId, World, face_tile, facing_tile_of};

/// The gryphon hammer and dragon breath count their steps near the aim point from this remaining distance.
const BREATH_NEAR: i32 = 50;
/// A breath missile ends on the step after its count passes this limit, or `BREATH_LONG_LIMIT` when its stored damage
/// is exactly `BREATH_LONG_DAMAGE`.
const BREATH_LIMIT: u8 = 25;
const BREATH_LONG_LIMIT: u8 = 40;
const BREATH_LONG_DAMAGE: u8 = 40;

/// The neutral player; it neither attacks nor is attacked.
const NEUTRAL: u8 = 15;

/// The Dark Portal special case (damage spec, "Applying damage"): the unit types and campaign mission table entries.
const DARK_PORTAL: u8 = 0x65;
const KHADGAR: u8 = 0x18;
const PORTAL_ENTRIES: [u8; 3] = [32, 34, 50];
/// The first building type (the farm): a victim below it is a kill, from it on a razing (campaign-flow spec).
const FIRST_BUILDING: u8 = 0x3a;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Missile {
    /// UDTA missile weapon value.
    pub kind: u8,
    pub attacker: UnitId,
    /// The unit aimed at, which a non-splash missile damages on impact wherever it is.
    pub target: Option<UnitId>,
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// Current position in map pixels.
    pub pos: (i32, i32),
    /// Number of flight steps N of a plain missile (the last one is the landing step); 0 for a breath missile, whose
    /// end is decided step by step.
    pub flight: u16,
    /// Updates made so far: the first one only starts the missile, so it has made `elapsed - 1` flight steps.
    pub elapsed: u16,
    /// Rolled damage, or the stored maximum for a splash missile.
    pub damage: u8,
    pub splash: bool,
    /// A small-cannon shot at an air unit also splashes air units.
    pub hits_air: bool,
    /// What a spell effect does (spells spec, "Spell effects in flight"). For these, `kind` is the effect kind of that
    /// spec, `attacker` the effect's owner, `damage` its stored damage and `elapsed` the cycles since its first update.
    pub fx: Fx,
}

impl Missile {
    /// Whether the effect's picture is drawn (map-drawing spec, "Effects"): from its start step on, which comes
    /// `start_cycles` cycles after its first update. A plain missile shows after its first update, a spell marker
    /// after the update one cycle later.
    pub fn showing(&self) -> bool {
        self.elapsed > crate::spells::start_cycles(self.kind)
    }
}

/// Animation-scripts spec, "Facing a tile" (round-42 errata): a warship's new facing when it strikes, by the direction
/// `d` to its target (row) and its current facing (column). It puts the target on one of the ship's sides.
const BROADSIDE: [[u8; 8]; 8] = [
    [6, 2, 2, 2, 2, 6, 6, 6],
    [7, 7, 3, 3, 3, 3, 7, 7],
    [0, 0, 4, 4, 4, 4, 0, 0],
    [1, 1, 1, 1, 5, 5, 5, 5],
    [2, 2, 2, 2, 2, 6, 6, 6],
    [7, 7, 3, 3, 3, 3, 7, 7],
    [0, 0, 4, 4, 4, 4, 0, 0],
    [1, 1, 1, 1, 5, 5, 5, 5],
];

pub(crate) fn broadside(d: u8, current: u8) -> u8 {
    BROADSIDE[(d & 7) as usize][(current & 7) as usize]
}

/// Tiles between two footprints along the larger axis: 1 for neighbours, diagonal included.
fn gap(a: &Unit, b: &Unit) -> i32 {
    let axis = |a0: i32, aw: u8, b0: i32, bw: u8| (b0 - (a0 + aw as i32 - 1)).max(a0 - (b0 + bw as i32 - 1)).max(0);
    axis(a.tile.0, a.size.0, b.tile.0, b.size.0).max(axis(a.tile.1, a.size.1, b.tile.1, b.size.1))
}

/// A target's goal zone (pathfinding spec, "Step 1"): its footprint widened by one tile on each side.
pub(crate) fn goal_zone(t: &Unit) -> (i32, i32, i32, i32) {
    (
        t.tile.0 - 1,
        t.tile.1 - 1,
        t.tile.0 + t.size.0 as i32,
        t.tile.1 + t.size.1 as i32,
    )
}

impl World {
    /// Whether `u` may be ordered to attack `target`: an enemy it can reach with its weapon.
    pub fn can_attack(&self, u: &Unit, target: UnitId) -> bool {
        self.can_strike(u, target)
            && self
                .unit(target)
                .is_some_and(|t| t.present() && t.owner != u.owner && t.owner != NEUTRAL)
    }

    /// Whether `u`'s weapon can reach `target` at all, whoever owns it: another unit, and an air unit only with a
    /// missile. Orders spec, "Commands" (round-48 errata): an attack command on a unit of any owner gives order 9, and
    /// an attack order keeps a target of any owner.
    pub(crate) fn can_strike(&self, u: &Unit, target: UnitId) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let s = self.stats_of(u.kind);
        t.id != u.id
            && s.flags & tidewater_formats::udta::flags::CAN_ATTACK != 0
            && (t.layer != Layer::Air || s.missile != missile::NONE)
    }

    /// Attack order at a "step done": attack when the target is within range, otherwise move toward it (movement
    /// spec, "Arriving": the radius is the attack range, measured to the target's tile or, for a building, to the
    /// footprint tile picked by the building goal rule, re-read at every check).
    pub(crate) fn attack_order(&mut self, i: usize, target: UnitId) {
        // Orders spec: the order loses a target its owner no longer detects (fog does not matter). Round-49 errata
        // ("Checks on an attack command"): the order then checks only that the unit can hit the target's kind, with
        // the submarine rule; there is no owner test.
        let owner = self.units[i].owner;
        let target_ok = self
            .unit(target)
            .is_some_and(|t| t.present() && Self::detected(t, owner) && self.can_target_kind(&self.units[i], t));
        if !target_ok {
            self.go_idle(i);
            self.switch_anim(i, anim::STILL);
            return;
        }
        // Orders spec, "Friendly-fire check" (round-41 errata): in order 12 it runs before the range test.
        let tile = self.unit(target).expect("checked above").tile;
        if self.holds_fire(i, tile) {
            return;
        }
        let u = &self.units[i];
        let t = self.unit(target).expect("checked above");
        if self.within_strike_reach(u, t) {
            let b = facing_tile_of(t);
            self.units[i].path.clear();
            self.face_and_strike(i, b);
            return;
        }
        let g = self.strike_goal(u, t);
        match self.move_toward(i, g) {
            // Orders spec, "No route" (round-40 errata, `xhuman4` axe thrower 1490 at n = 5): order 12 goes idle at
            // once, its idle logic running in the same update, when its own fallback (round-47 errata) finds nothing.
            // Round-42 errata (`orc02-assault` grunt 1573, n = 679): order 9, with no such fallback, goes idle at once.
            Walk::NoPath if matches!(self.units[i].order, Order::Chase { .. }) => self.chase_no_route(i),
            Walk::NoPath if matches!(self.units[i].order, Order::Attack { .. }) => self.idle_at_once(i),
            Walk::NoPath | Walk::Arrived => {
                self.go_idle(i);
                self.switch_anim(i, anim::STILL);
            }
            Walk::Stepping | Walk::Blocked => {}
        }
    }

    /// Orders spec, "Order 12 with no route" (round-47 errata; `human5-10000` grunt 1531, n = 1759): a search over
    /// the attack range box with no attacker seed. A target found gets order 12 as a command that takes over at once:
    /// the unit turns as for a strike, its attack animation starts with its first step run in this update, and its
    /// next order-logic run skips the target search. With nothing found the unit goes idle at once. Round-48 errata:
    /// the take-over is a strike decision, so the cooldown starts in this update (and the attack sound is pending).
    fn chase_no_route(&mut self, i: usize) {
        let Some(target) = self.search_in_range(i) else {
            return self.idle_at_once(i);
        };
        let o = Order::Chase { target };
        self.release_tree(i);
        let u = &mut self.units[i];
        u.order = o;
        u.pending = None;
        u.commanded = false;
        u.path.clear();
        u.blocked = 0;
        u.skip_search = true;
        self.forget_stale_attacker(i);
        let b = facing_tile_of(self.unit(target).expect("found by the search"));
        self.decide_strike(i, b, false);
        self.switch_anim(i, anim::ATTACK);
    }

    /// The goal of a walk toward an attack target: the target's tile or, for a building, the footprint tile of the
    /// building goal rule, with the attack range as the radius.
    fn strike_goal(&self, u: &Unit, t: &Unit) -> Goal {
        let up = self.players[u.owner as usize].upgrades;
        let reach = combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32;
        let goal = if t.building { building_goal(u.tile, t) } else { t.tile };
        Goal {
            tile: goal,
            radius: reach,
            range: true,
            zone: Some(goal_zone(t)),
        }
    }

    /// Orders spec, "Attack-move: orders 4, 5 and (Enhanced) 10" (round-41 errata): a unit in one of these orders
    /// that picked a target heads for it until order 12 takes over at its next step done, stepping even when the
    /// target is already within strike reach. Our reading: the order's own walk (radius 0) carries on toward the goal
    /// the command set, the target's tile; with no route it goes idle as the order does ("No route"), and once there
    /// it waits for the take-over.
    pub(crate) fn head_for(&mut self, i: usize, target: UnitId) {
        let Some(t) = self.unit(target) else { return };
        // Round-44 errata (`human06-31357` axe thrower 1512, n = 16277): a building target is headed for as an attack
        // on it would: the footprint tile of the building goal rule, with the building's goal zone (its footprint
        // widened by one tile; pathfinding spec, "Step 1"). A unit target is headed for at its tile.
        let g = if t.building {
            Goal {
                zone: Some(goal_zone(t)),
                ..Goal::tile(building_goal(self.units[i].tile, t), 0)
            }
        } else {
            Goal::tile(t.tile, 0)
        };
        // Orders spec, "No route" (round-45 errata): a unit that goes idle with no route keeps its attacker.
        if self.move_toward(i, g) == Walk::NoPath {
            self.idle_at_once(i);
        }
    }

    /// Orders spec, "Order 11: attacking a wall tile" (round-48 errata; `orc02-defeat-4021` and `orc02-victory-7639`
    /// grunts 1568, 1575 and 1576). The unit walks with the normal movement rules until it is within its attack range
    /// of the goal tile (no extra tile); with no route it goes idle at once, its idle logic running in the same update
    /// (order 12's own fallback doesn't apply). On arrival, in this order: a cooldown left is waited out; a goal tile
    /// that is no longer a wall, or out of attack range, makes idle the pending order, taken over in the same update;
    /// otherwise the unit faces the goal tile, strikes and starts its cooldown again. The dying-attacker step comes
    /// first, in [`World::order`].
    pub(crate) fn attack_wall_order(&mut self, i: usize, tile: (i32, i32)) {
        let u = &self.units[i];
        let up = self.players[u.owner as usize].upgrades;
        let reach = combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32;
        let g = Goal {
            tile,
            radius: reach,
            range: true,
            zone: None,
        };
        match self.move_toward(i, g) {
            Walk::Arrived => {}
            Walk::NoPath => return self.idle_at_once(i),
            Walk::Stepping | Walk::Blocked => return,
        }
        if self.cooling_down(i) {
            return;
        }
        // Round-49 errata (step 4.4, static study): the range check is the arrival test above (no +1 with an attack
        // range), so it fails only if the unit or its goal has moved since.
        if !self.wall_tile(tile) || chebyshev(self.units[i].tile, tile) > reach {
            return self.idle_now(i);
        }
        self.decide_strike(i, tile, false);
        if self.units[i].script.anim != anim::ATTACK {
            self.switch_anim(i, anim::ATTACK);
        }
    }

    /// Turns toward the target and plays the attack animation, starting it in the same cycle if needed. The facing
    /// comes from "Facing a tile" (animation-scripts spec) toward tile `b`; warships turn broadside instead.
    pub(crate) fn face_and_strike(&mut self, i: usize, b: (i32, i32)) {
        let cast = matches!(self.units[i].order, Order::Cast { .. });
        if !cast && self.cooling_down(i) {
            return;
        }
        self.decide_strike(i, b, cast);
        if self.units[i].script.anim != anim::ATTACK {
            self.switch_anim(i, anim::ATTACK);
        }
    }

    /// A strike decision that passed the cooldown test: the unit turns to face its target and its cooldown starts
    /// again (damage spec, "Attack timing (cooldown)"). A cast turns the same way but has no cooldown.
    pub(crate) fn decide_strike(&mut self, i: usize, b: (i32, i32), cast: bool) {
        // Sounds spec, "Attacks": each turn to strike makes the type's attack sound pending. Our reading: casting
        // (which also turns to the target) doesn't.
        if !cast {
            self.pend_attack(i);
            self.start_cooldown(i);
        }
        self.units[i].facing = self.strike_facing(&self.units[i], b);
    }

    /// The facing a strike decision gives toward tile `b`: "Facing a tile" of the animation-scripts spec. Round-42
    /// errata: a warship (a sea unit that isn't a submarine) turns broadside instead, by the direction to the target
    /// and its current facing.
    pub(crate) fn strike_facing(&self, u: &Unit, b: (i32, i32)) -> u8 {
        let towards = face_tile(u.tile, b);
        let f = self.stats_of(u.kind).flags;
        if f & udta_flags::SEA != 0 && f & udta_flags::SUBMARINE == 0 {
            broadside(towards, u.facing)
        } else {
            towards
        }
    }

    /// Whether `t` is within reach of an attack order's strike (orders 9 and 12): the footprints' gap against the
    /// attack range.
    pub(crate) fn within_strike_reach(&self, u: &Unit, t: &Unit) -> bool {
        let up = self.players[u.owner as usize].upgrades;
        gap(u, t) <= combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32
    }

    /// The strike decision's cooldown test (damage spec, "Attack timing (cooldown)"): a unit that isn't a flyer and
    /// whose wait is less than its cooldown left restarts its attack animation from its first frame and waits for the
    /// cooldown left. Returns whether it did, which ends the run.
    pub(crate) fn cooling_down(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        if u.building || u.layer == Layer::Air || u.script.wait as u16 >= u.cooldown {
            return false;
        }
        let left = u.cooldown.min(255) as u8;
        self.start_anim(i, anim::ATTACK);
        let u = &mut self.units[i];
        u.script.frame = 0;
        u.script.wait = left;
        true
    }

    /// A strike decision starts the cooldown again: the type's attack loop + 1. Buildings have none.
    pub(crate) fn start_cooldown(&mut self, i: usize) {
        let u = &self.units[i];
        if u.building {
            return;
        }
        let lp = self.attack_loop.get(u.kind as usize).copied().unwrap_or(0);
        self.units[i].cooldown = lp + 1;
    }

    /// The strike point of the attack script: a melee hit or a missile launch.
    pub(crate) fn strike(&mut self, i: usize) {
        let order = self.units[i].order;
        if let Order::Cast { .. } = order {
            return self.cast_strike(i);
        }
        let (ti, to) = match (order, crate::orders::target_of(order)) {
            (Order::AttackWall { tile } | Order::AttackGround { tile }, _) => {
                (None, (tile.0 * TILE_PX + TILE_PX / 2, tile.1 * TILE_PX + TILE_PX / 2))
            }
            // A demolisher plays the attack animation on its way but never strikes (attack-ground-demolish spec).
            (Order::Follow { .. } | Order::Guard { .. } | Order::Demolish { .. }, _) | (_, None) => return,
            (_, Some(target)) => {
                // Damage spec, "How an attack delivers damage" (round-41 errata, `human01-basics` n = 576): a melee
                // strike is made at a target that started dying earlier in the cycle, and rolls its damage to no
                // effect; a target that is gone or inside something isn't struck. Ranged attackers are that spec's
                // open question, where a missile is expected to behave the same; we follow that expectation.
                let Some(ti) = self.index(target).filter(|&ti| !self.units[ti].inside) else {
                    return;
                };
                (Some(ti), self.units[ti].centre_px())
            }
        };
        self.hit_or_launch(i, ti, to);
    }

    /// A strike's hit or launch from the unit at `i` at the unit at `ti`, or at the point `to` (a wall or the ground).
    pub(crate) fn hit_or_launch(&mut self, i: usize, ti: Option<usize>, to: (i32, i32)) {
        // Spells spec, "Timers": a unit's attack strike ends its own invisibility.
        self.units[i].invisibility = 0;
        if self.units[i].building {
            self.tower_fire_sound(i);
        }
        let a = &self.units[i];
        let s = self.stats_of(a.kind);
        let (basic, pierce) = self.attack_values(a, &s);
        let armour = ti.map_or(0, |ti| {
            let t = &self.units[ti];
            combat::armour(&self.stats_of(t.kind), &self.players[t.owner as usize].upgrades)
        });
        let hits_air = ti.is_some_and(|ti| self.units[ti].layer == Layer::Air);
        let target = ti.map(|ti| self.units[ti].id);
        let (attacker, from, owner) = (a.id, a.centre_px(), a.owner);
        if s.missile == missile::NONE {
            let d = combat::roll(&mut self.rng, basic, pierce, armour);
            match ti {
                Some(ti) => self.damage(ti, d, attacker),
                // A melee hit on a wall: the spec doesn't cover it yet, so the rolled damage goes to the wall.
                None => self.hit_wall((to.0.div_euclid(TILE_PX), to.1.div_euclid(TILE_PX)), d),
            }
            return;
        }
        // Damage spec: with no effect slot free for the missile, nothing is created and nothing is drawn.
        let Some(slot) = self
            .missiles
            .iter()
            .skip(slots::PERMANENT_EFFECTS)
            .position(|e| e.is_none())
            .map(|n| n + slots::PERMANENT_EFFECTS)
        else {
            return;
        };
        let min_flight = combat::splash(s.missile);
        let damage = match min_flight {
            Some(_) => (basic + pierce).min(255) as u8,
            None => combat::roll(&mut self.rng, basic, pierce, armour),
        };
        // Towers (guard and cannon) fire without the aim scatter and without the computer players' wall exception.
        let tower = s.flags & tidewater_formats::udta::flags::TOWER != 0;
        let aim_tile = (to.0.div_euclid(TILE_PX), to.1.div_euclid(TILE_PX));
        let mut to = to;
        if !tower {
            to.0 += (self.rng.draw() & 7) as i32 - 3;
            to.1 += (self.rng.draw() & 7) as i32 - 3;
        }
        // Our reading: "the attack's target tile" is the tile of the aim point before the scatter.
        let mut minimum = min_flight.unwrap_or(0) as i32 * TILE_PX;
        if !tower
            && minimum > 2 * TILE_PX
            && self.computer_player(owner)
            && self.terrain.word(aim_tile.0, aim_tile.1) & (bits::HUMAN_WALL | bits::COMPUTER_WALL) != 0
        {
            minimum = 2 * TILE_PX;
        }
        let mut line = Line::new(from, to);
        let breath = matches!(s.missile, missile::GRYPHON_HAMMER | missile::DRAGON_BREATH);
        let (fx, flight) = if breath {
            // The gryphon hammer and dragon breath have no minimum flight distance.
            (
                Fx::Breath {
                    line,
                    count: 0,
                    ended: false,
                },
                0,
            )
        } else {
            let length = line.remaining.max(minimum);
            line.remaining = length;
            let steps = combat::flight_steps(length, combat::flight(s.missile).speed);
            (Fx::Shot { line, length }, steps as u16)
        };
        let missile = Missile {
            kind: s.missile,
            attacker,
            target,
            from,
            to,
            pos: from,
            flight,
            elapsed: 0,
            damage,
            splash: min_flight.is_some(),
            hits_air: s.missile == missile::SMALL_CANNON && hits_air,
            fx,
        };
        // Unit-slots spec, "Effect slots": the lowest free slot after the permanent entries.
        let cannon = matches!(s.missile, missile::BIG_CANNON | missile::SMALL_CANNON);
        self.missiles[slot] = Some(missile);
        // Damage spec, "How an attack delivers damage": a unit's cannon shot is followed by smoke (kind 0x19) at the
        // missile's start point, in the next free slot; with none free there is no smoke.
        if cannon && !tower {
            self.cannon_smoke(attacker, from);
        }
    }

    /// Applies damage from `attacker` to the unit at index `ti`.
    pub(crate) fn damage(&mut self, ti: usize, d: u8, attacker: UnitId) {
        if self.units[ti].dying || self.portal_ignores(ti, attacker) {
            return;
        }
        // Damage spec: a target with the used mark gets no notification and no damage.
        let t = &self.units[ti];
        if t.used || t.inside {
            return;
        }
        // The notification: attack-ai spec, "Call for help" (which keeps the movement spec's building rule).
        self.call_for_help(ti, attacker);
        let t = &mut self.units[ti];
        if t.unholy_armor > 0 || d == 0 {
            return;
        }
        if t.hp > d as u16 {
            t.hp -= d as u16;
            self.under_attack(ti);
            return;
        }
        if let Some(a) = self.unit(attacker) {
            self.credit_kill(a.owner, ti);
        }
        // Damage spec, "Applying damage": the killing blow doesn't lower the HP, and the target keeps its target.
        self.kill(ti);
        self.forget_kill(attacker);
    }

    /// Damage spec, "Applying damage": a unit (not a building) that made a kill checks its own records at once,
    /// without changing its order: it forgets its target, and its recorded attacker, if that unit is now dying, gone
    /// or not detected by its owner. The victim is dying, so it is always forgotten when it is either. Other
    /// attackers keep their target until their own next logic run. Our orders hold their target, so the forgotten
    /// one is marked: while the order is unchanged, it has no target.
    ///
    /// Round-42 errata: only the killer forgets the victim as its attacker at once (movement spec, "Blocked rules");
    /// every other unit keeps that record while the victim is dying. Round-47 errata: this applies to every kill,
    /// a splash kill of a unit other than the target included (`xhuman10-blizzard` ballista 1483, n = 309).
    fn forget_kill(&mut self, attacker: UnitId) {
        let Some(a) = self.index(attacker) else { return };
        let u = &self.units[a];
        if u.building {
            return;
        }
        let owner = u.owner;
        if let Some(k) = u.attacked_by
            && self.stale(owner, k)
        {
            self.units[a].attacked_by = None;
        }
        let u = &self.units[a];
        if u.dying {
            return;
        }
        if let Some(t) = crate::orders::target_of(u.order)
            && self.stale(owner, t)
        {
            self.units[a].forgot = Some(self.units[a].order);
        }
    }

    /// Campaign-flow spec, "Mission score": a unit of player `killer` killed the unit at `ti`. Unless the victim is
    /// the killer's own, the killer's player gets the victim type's point value and a kill (a type below the first
    /// building type) or a razing.
    pub(crate) fn credit_kill(&mut self, killer: u8, ti: usize) {
        let victim = &self.units[ti];
        if victim.owner == killer {
            return;
        }
        let points = self.stats.0.get(victim.kind as usize).map_or(0, |s| s.points as u32);
        let kind = victim.kind;
        let p = &mut self.players[killer as usize];
        if kind < FIRST_BUILDING {
            p.kills += 1;
        } else {
            p.razings += 1;
        }
        p.score += points;
    }

    /// Campaign-flow spec, "Statistics": the owner's count of units or of buildings went down by one.
    pub(crate) fn count_lost(&mut self, owner: u8, building: bool) {
        let p = &mut self.players[owner as usize];
        if building {
            p.buildings_lost += 1;
        } else {
            p.units_lost += 1;
        }
    }

    /// The damage spec's Dark Portal case: in a single-player campaign game on campaign entries 32, 34 and 50, a hit
    /// on the Dark Portal does nothing, not even the notification, unless the attacker is Khadgar. Our reading: an
    /// attacker that is no longer in the unit table (a missile whose shooter has gone) is not Khadgar.
    fn portal_ignores(&self, ti: usize, attacker: UnitId) -> bool {
        !self.rules.multiplayer
            && self.victory.campaign.is_some_and(|e| PORTAL_ENTRIES.contains(&e))
            && self.units[ti].kind == DARK_PORTAL
            && self.unit(attacker).is_none_or(|a| a.kind != KHADGAR)
    }

    pub(crate) fn kill(&mut self, ti: usize) {
        self.kill_as(ti, true);
    }

    /// [`World::kill`]; `explodes: false` leaves out a building's explosion (our oil patch removal under a new
    /// platform, which isn't a destruction).
    pub(crate) fn kill_as(&mut self, ti: usize, explodes: bool) {
        self.death_sound(ti);
        // Unit-slots spec, "Death, corpses and release" (round-40 errata, `human5` barracks 1529 at n = 1595): a
        // building other than a wall piece or one with the used mark makes a cosmetic explosion (kind 0x17) at its
        // centre, 8 px up, in the lowest free effect slot, right after its death sound.
        let u = &self.units[ti];
        if explodes
            && u.building
            && !u.used
            && !matches!(
                u.kind,
                crate::economy::kind::HUMAN_WALL | crate::economy::kind::ORC_WALL
            )
        {
            let pos = (
                u.tile.0 * TILE_PX + u.size.0 as i32 * TILE_PX / 2,
                u.tile.1 * TILE_PX + u.size.1 as i32 * TILE_PX / 2 - 8,
            );
            self.splash_explosion(pos);
        }
        // Build-train spec, "A building killed during a job" (round-48 errata, `human5-10000` great hall 1525 at
        // n = 8086): the job is cancelled at the kill, with the cancel column's full refund, and the progress counter
        // and mana byte go to 0.
        if self.units[ti].building {
            self.cancel_job(ti);
        }
        let unit = self.units[ti].clone();
        if !self.loading {
            self.count_lost(unit.owner, unit.building);
        }
        let p = &mut self.players[unit.owner as usize];
        if unit.construction.is_some() {
            p.under_construction -= 1;
        }
        if let Some(crate::economy::Job {
            kind: crate::economy::JobKind::Train(_),
            ..
        }) = unit.job
        {
            p.in_training -= 1;
        }
        self.units[ti].construction = None;
        self.units[ti].job = None;
        if !unit.inside {
            self.mark(&unit, unit.tile, None);
        }
        if unit.building {
            self.release_builders(ti, false);
            self.mark_build_list(unit.owner, unit.kind, false);
        }
        // A computer worker's job is released when it dies (worker-ai spec).
        self.release_job(ti);
        let u = &self.units[ti];
        let target = u.pending.and_then(Order::target).or_else(|| u.order.target());
        let slot = target.and_then(|t| self.unit(t)).map(|t| t.slot);
        let u = &mut self.units[ti];
        u.dying_target = slot;
        // Unit-slots spec, "Death, corpses and release": no kill, by damage or outright, changes the HP.
        u.dying = true;
        u.order = Order::Idle;
        u.pending = None;
        u.remembered = None;
        u.path.clear();
        self.start_anim(ti, anim::DEATH);
        if self.kill_remains(ti) {
            self.kill_passengers(ti);
        }
        self.leave_oil(ti);
        // Worker-ai spec, "Losses": too few buildings left starts the all-out attack (attack-ai spec).
        if unit.building {
            self.building_lost(unit.owner);
        }
    }

    /// Updates every missile once, in effect slot order. Plain missiles and breath missiles are updated here, spell
    /// effects in `step_fx`.
    pub(crate) fn advance_missiles(&mut self) {
        for n in 0..self.missiles.len() {
            let Some(m) = &self.missiles[n] else { continue };
            match m.fx {
                Fx::Shot { .. } => self.step_shot(n),
                Fx::Breath { .. } => self.step_breath(n),
                _ => self.step_fx(n),
            }
        }
    }

    /// One update of a plain missile (damage spec, "Missile flight"). The first update only starts it. Updates 1 to N
    /// are flight steps, each ending with a discarded draw (two for the catapult rock and small cannon while the travel
    /// is short of the aim point, rounded up to 5 px). The impact comes `delay` cycles after update N.
    fn step_shot(&mut self, n: usize) {
        let Some(mut m) = self.missiles[n].clone() else { return };
        let age = m.elapsed;
        m.elapsed = m.elapsed.saturating_add(1);
        let Fx::Shot { mut line, length } = m.fx else { return };
        let f = combat::flight(m.kind);
        if age == 0 {
            self.missiles[n] = Some(m);
        } else if age <= m.flight {
            line.step(&mut m.pos, f.speed);
            m.fx = Fx::Shot { line, length };
            debug_assert!(
                age < m.flight || line.remaining < 0,
                "the landing step is the one below 0"
            );
            self.rng.draw();
            if f.extra_draw && (age as i32) * f.speed < (length + 4) / 5 * 5 {
                self.rng.draw();
            }
            self.missiles[n] = Some(m);
        } else if age == m.flight + f.delay {
            // Damage spec, "Impact effects": the missile's slot is freed only after the impact and its effect.
            self.land(m);
            self.missiles[n] = None;
        } else {
            self.missiles[n] = Some(m);
        }
    }

    /// One update of a gryphon hammer or dragon breath (damage spec, "Gryphon hammer and dragon breath"): a draw at
    /// the start of each step, a move of 12 px (6 once the remaining distance is below 50), and a count of the steps
    /// that end below 50, with a splash on every eighth.
    fn step_breath(&mut self, n: usize) {
        let Some(mut m) = self.missiles[n].clone() else { return };
        let age = m.elapsed;
        m.elapsed = m.elapsed.saturating_add(1);
        let Fx::Breath {
            mut line,
            mut count,
            ended,
        } = m.fx
        else {
            return;
        };
        if age == 0 {
            self.missiles[n] = Some(m);
            return;
        }
        if ended {
            // One cycle after the end step the stored damage lands, as for a non-splash missile.
            self.missiles[n] = None;
            self.land(m);
            return;
        }
        self.rng.draw();
        line.step(&mut m.pos, if line.remaining < BREATH_NEAR { 6 } else { 12 });
        let mut ended = false;
        let mut splash = false;
        if line.remaining < BREATH_NEAR {
            let limit = if m.damage == BREATH_LONG_DAMAGE {
                BREATH_LONG_LIMIT
            } else {
                BREATH_LIMIT
            };
            if count > limit {
                ended = true;
            } else {
                count += 1;
                splash = count % 8 == 0;
            }
        }
        m.fx = Fx::Breath { line, count, ended };
        let (pos, damage, attacker) = (m.pos, m.damage, m.attacker);
        self.missiles[n] = Some(m);
        // Damage spec (round-39 errata): a splash needs a free effect slot; with none, no splash, sound or effect.
        // Otherwise the explosion sound, the splash, then a cosmetic explosion (kind 0x17) in the lowest free slot.
        if splash && self.free_fx_slot().is_some() {
            self.sound_at_px(pos, crate::sound::Sound::Explode);
            self.fx_splash(pos, damage, attacker);
            self.splash_explosion(pos);
        }
    }

    /// The impact, at the missile's final position: the impact sound, the hit or splash, then the impact effect
    /// (damage spec, "Impact effects").
    fn land(&mut self, m: Missile) {
        let sound = crate::sound::Sound::Impact {
            kind: m.kind,
            damaging: m.damage != 0,
        };
        self.sound_at_px(m.pos, sound);
        self.hit_or_splash(&m);
        self.impact_fx(m.kind, m.attacker, m.pos);
    }

    fn hit_or_splash(&mut self, m: &Missile) {
        if !m.splash {
            match m.target {
                Some(t) => {
                    if let Some(ti) = self.index(t) {
                        self.damage(ti, m.damage, m.attacker);
                    }
                }
                // Aimed at a tile with no unit: the wall there takes it (damage spec).
                None => self.hit_wall((m.pos.0.div_euclid(TILE_PX), m.pos.1.div_euclid(TILE_PX)), m.damage),
            }
            return;
        }
        let tile = (m.pos.0.div_euclid(TILE_PX), m.pos.1.div_euclid(TILE_PX));
        self.hit_wall(tile, m.damage);
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.hit_wall((tile.0 + dx, tile.1 + dy), m.damage / 4);
        }
        let mut layers = vec![Layer::Land, Layer::Sea];
        if m.hits_air {
            layers.push(Layer::Air);
        }
        let mut hit: Vec<UnitId> = Vec::new();
        let consider = |w: &World, x: i32, y: i32, hit: &mut Vec<UnitId>| {
            for &l in &layers {
                if let Some(id) = w.occupant(l, x, y)
                    && id != m.attacker
                    && !hit.contains(&id)
                {
                    hit.push(id);
                }
            }
        };
        consider(self, tile.0, tile.1, &mut hit);
        for y in tile.1 - 3..=tile.1 + 3 {
            for x in tile.0 - 3..=tile.0 + 3 {
                consider(self, x, y, &mut hit);
            }
        }
        for id in hit {
            let Some(ti) = self.index(id) else { continue };
            let t = &self.units[ti];
            if t.dying {
                continue;
            }
            let c = t.centre_px();
            let d = (c.0 - m.pos.0).abs().max((c.1 - m.pos.1).abs());
            let armour = combat::armour(&self.stats_of(t.kind), &self.players[t.owner as usize].upgrades);
            if let Some(dmg) = combat::splash_roll(&mut self.rng, m.damage, d, armour) {
                self.damage(ti, dmg, m.attacker);
            }
        }
    }

    pub(crate) fn hit_wall(&mut self, tile: (i32, i32), damage: u8) {
        self.terrain.hit_wall(tile.0, tile.1, damage as u32);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::tests::{run, world};
    use crate::{Command, Layer, Order, UnitId};
    use tidewater_formats::script::anim;

    fn attack(w: &mut crate::World, player: u8, units: Vec<UnitId>, target: UnitId) {
        w.step(&[Command::Attack { player, units, target }]);
    }

    /// Animation-scripts spec, the broadside table (round-42 errata): a warship facing north (0) with its target due
    /// south (4) turns east (2); one facing east with its target due north keeps facing east; one facing south-west
    /// (5) with its target due east (2) turns south (4) rather than turning across.
    #[test]
    fn warships_turn_broadside_to_strike() {
        assert_eq!(super::broadside(4, 0), 2);
        assert_eq!(super::broadside(0, 2), 2);
        assert_eq!(super::broadside(2, 5), 4);
    }

    /// Orders spec, "Order 11: attacking a wall tile" (round-48 errata): the goal is pulled back to the wall tile
    /// nearest the unit, which walks to within its attack range, faces the tile, strikes and starts its cooldown.
    /// When the wall is gone, idle becomes the pending order and takes over in the same update (still, wait 3).
    #[test]
    fn order_11_strikes_the_pulled_back_wall_tile_until_it_is_gone() {
        let mut w = crate::world::tests::field(16, 16);
        for y in 3..=5 {
            w.terrain.build_wall(8, y, false, true);
        }
        let f = w.spawn(0, 0, (8, 10), 0).unwrap();
        let i = w.index(f).unwrap();
        assert!(w.wall_tile((8, 3)));
        w.command(i, Order::AttackWall { tile: (8, 3) });
        let goal = Order::AttackWall { tile: (8, 5) };
        assert_eq!(w.units[i].pending, Some(goal));
        let mut n = 0;
        while w.unit(f).unwrap().cooldown == 0 {
            w.step(&[]);
            n += 1;
            assert!(n < 300, "no strike");
        }
        let u = w.unit(f).unwrap();
        assert_eq!(crate::movement::chebyshev(u.tile, (8, 5)), 1, "{:?}", u.tile);
        let facing = crate::world::face_tile(u.tile, (8, 5));
        assert_eq!((u.facing, u.order, u.script.anim), (facing, goal, anim::ATTACK));
        while w.wall_tile((8, 5)) {
            w.hit_wall((8, 5), 255);
        }
        let mut n = 0;
        while w.unit(f).unwrap().order == goal {
            w.step(&[]);
            n += 1;
            assert!(n < 300, "still attacking");
        }
        let u = w.unit(f).unwrap();
        assert_eq!((u.order, u.script.anim, u.script.wait), (Order::Idle, anim::STILL, 3));
    }

    /// Orders spec, "Commands" (round-48 errata, `orc02-defeat-4021` n = 3602 and `orc02-dyingcmd-3760` n = 3723):
    /// an attack command on the player's own unit gives order 9; on a dying unit it leaves a pending idle.
    #[test]
    fn an_attack_command_takes_any_owner_and_a_dying_target_leaves_a_pending_idle() {
        let mut w = crate::world::tests::field(16, 16);
        let a = w.spawn(0, 0, (3, 3), 0).unwrap();
        let own = w.spawn(0, 0, (6, 3), 0).unwrap();
        let c = Command::Attack {
            player: 0,
            units: vec![a],
            target: own,
        };
        let i = w.index(a).unwrap();
        let o = w.command_order(i, &c);
        assert_eq!(o, Some(Order::Attack { target: own }));
        w.command(i, o.unwrap());
        assert_eq!(w.units[i].pending, Some(Order::Attack { target: own }));
        let oi = w.index(own).unwrap();
        w.kill(oi);
        let o = w.command_order(i, &c).unwrap();
        w.command(i, o);
        assert_eq!(w.units[i].pending, Some(Order::Idle));
    }

    /// Campaign-flow spec, test vector: a footman killed by an enemy archer.
    #[test]
    fn a_kill_credits_the_killers_player_with_the_victims_points() {
        let mut w = world();
        w.stats.0[0].points = 50;
        let archer = w.spawn(8, 1, (3, 3), 0).unwrap();
        let footman = UnitId(1);
        let i = w.index(footman).unwrap();
        w.damage(i, 255, archer);
        let p = &w.players[1];
        assert_eq!((p.kills, p.razings, p.score), (1, 0, 50));
        assert_eq!((w.players[0].units_lost, w.players[0].buildings_lost), (1, 0));
        assert_eq!(w.players[0].score, 0);
    }

    /// Campaign-flow spec, test vector: an own farm destroyed by an own catapult.
    #[test]
    fn destroying_an_own_building_gives_no_credit_but_counts_as_lost() {
        let mut w = world();
        w.stats.0[0x3a].points = 100;
        let farm = w.spawn(0x3a, 0, (5, 5), 0).unwrap();
        let catapult = w.spawn(5, 0, (2, 6), 0).unwrap();
        let i = w.index(farm).unwrap();
        // The test table has no building flags; the farm stands as one.
        w.units[i].building = true;
        w.units[i].hp = 10;
        w.damage(i, 255, catapult);
        let p = &w.players[0];
        assert_eq!((p.kills, p.razings, p.score), (0, 0, 0));
        assert_eq!((p.units_lost, p.buildings_lost), (0, 1));
    }

    #[test]
    fn an_enemy_building_is_a_razing() {
        let mut w = world();
        w.stats.0[0x3b].points = 100;
        let farm = w.spawn(0x3b, 1, (5, 5), 0).unwrap();
        let i = w.index(farm).unwrap();
        w.units[i].building = true;
        w.units[i].hp = 10;
        w.damage(i, 255, UnitId(1));
        let p = &w.players[0];
        assert_eq!((p.kills, p.razings, p.score), (0, 1, 100));
        assert_eq!(w.players[1].buildings_lost, 1);
    }

    /// Unit-slots spec, "Death, corpses and release" (round-40 errata): a destroyed building makes an explosion
    /// (kind 0x17) at its centre, 8 px up; a wall piece or a building with the used mark makes none.
    #[test]
    fn a_destroyed_building_explodes_at_its_centre() {
        for (kind, used, explodes) in [
            (0x3b, false, true),
            (0x3b, true, false),
            (crate::economy::kind::HUMAN_WALL, false, false),
        ] {
            let mut w = world();
            let b = w.spawn(kind, 1, (5, 5), 0).unwrap();
            let i = w.index(b).unwrap();
            w.units[i].building = true;
            w.units[i].used = used;
            w.units[i].hp = 10;
            let size = w.units[i].size;
            w.damage(i, 255, UnitId(1));
            let fx: Vec<_> = w
                .missiles
                .iter()
                .flatten()
                .filter(|m| m.kind == 0x17)
                .map(|m| m.pos)
                .collect();
            let centre = (5 * 32 + size.0 as i32 * 16, 5 * 32 + size.1 as i32 * 16 - 8);
            assert_eq!(
                fx,
                if explodes { vec![centre] } else { vec![] },
                "kind {kind:#x}, used {used}"
            );
        }
    }

    /// Damage spec, "Applying damage", the 5-HP vector and `human01-basics` n = 288: a target with 5 HP taking 5
    /// dies but keeps HP 5 and its target; the killer forgets its target at once and keeps its order 9; another
    /// attacker isn't touched until its own next logic run (n = 291: order 2, wait 3).
    #[test]
    fn a_killing_blow_keeps_the_victims_hp_and_the_killer_forgets_at_once() {
        let mut w = world();
        let killer = UnitId(1);
        let victim = w.spawn(0, 1, (1, 0), 0).unwrap();
        let other = w.spawn(0, 0, (1, 1), 0).unwrap();
        let (k, v, o) = (
            w.index(killer).unwrap(),
            w.index(victim).unwrap(),
            w.index(other).unwrap(),
        );
        w.units[v].hp = 5;
        w.units[v].order = Order::Attack { target: other };
        w.units[k].order = Order::Attack { target: victim };
        w.units[o].order = Order::Attack { target: victim };
        w.damage(v, 5, killer);
        let u = &w.units[v];
        let other_slot = w.unit(other).unwrap().slot;
        assert_eq!((u.dying, u.hp, u.dying_target), (true, 5, Some(other_slot)));
        let attack = Order::Attack { target: victim };
        assert_eq!((w.units[k].order, w.units[k].forgot), (attack, Some(attack)));
        assert_eq!((w.units[o].order, w.units[o].forgot), (attack, None));
        // Each attacker's next logic run loses the target: order 2, taking over with wait 3.
        for i in [o, k] {
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!((u.order, u.script.wait, u.forgot), (Order::Idle, 3, None));
        }
    }

    /// Damage spec, "Applying damage" (round-47 errata, `xhuman10-blizzard-600` vector): a kill of a unit other than
    /// the killer's target makes the killer forget a target that is dying from someone else's kill, and its recorded
    /// attacker if dying; a live target that isn't the victim is kept.
    #[test]
    fn any_kill_makes_the_killer_forget_a_dying_target_and_attacker() {
        for target_dying in [true, false] {
            let mut w = world();
            let killer = UnitId(1);
            let target = w.spawn(0, 1, (3, 0), 0).unwrap();
            let bystander = w.spawn(0, 0, (1, 1), 0).unwrap();
            let attacker = w.spawn(0, 1, (0, 3), 0).unwrap();
            let k = w.index(killer).unwrap();
            let order = Order::IdleReact { target };
            w.units[k].order = order;
            w.units[k].attacked_by = Some(attacker);
            let a = w.index(attacker).unwrap();
            w.kill(a);
            if target_dying {
                let t = w.index(target).unwrap();
                w.kill(t);
            }
            let b = w.index(bystander).unwrap();
            w.units[b].hp = 5;
            w.damage(b, 10, killer);
            let u = &w.units[k];
            assert_eq!(u.order, order, "the order isn't changed");
            assert_eq!(u.attacked_by, None, "the dying attacker is forgotten");
            assert_eq!(u.forgot, target_dying.then_some(order), "target dying: {target_dying}");
        }
    }

    /// Orders spec, "Commands forget a stale attacker" (round-47 errata, `xhuman10-spells-700` knight 1480): a
    /// command whose target unit is dying makes idle the pending order instead; it still counts as a command.
    #[test]
    fn a_command_on_a_dying_target_leaves_a_pending_idle() {
        let mut w = world();
        let foe = w.spawn(0, 1, (3, 0), 0).unwrap();
        let i = w.index(UnitId(1)).unwrap();
        let f = w.index(foe).unwrap();
        w.kill(f);
        w.command(i, Order::Chase { target: foe });
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.commanded),
            (Order::Idle, Some(Order::Idle), true)
        );
    }

    /// Damage spec, "How an attack delivers damage" (round-41 errata): a melee strike at a target that is already
    /// dying is still made and rolls its damage, with no effect; one at a target inside something isn't made and
    /// draws nothing.
    #[test]
    fn a_melee_strike_at_a_dying_target_still_rolls_its_damage() {
        let mut w = world();
        let foe = w.spawn(0, 1, (1, 0), 0).unwrap();
        let i = w.index(UnitId(1)).unwrap();
        w.units[i].order = Order::Attack { target: foe };
        let f = w.index(foe).unwrap();
        w.kill(f);
        let (before, hp) = (w.rng, w.unit(foe).unwrap().hp);
        w.strike(i);
        assert_eq!(drawn(before, w.rng), 1, "the damage roll");
        assert_eq!(w.unit(foe).unwrap().hp, hp);
        let f = w.index(foe).unwrap();
        w.units[f].inside = true;
        let before = w.rng;
        w.strike(i);
        assert_eq!(w.rng, before);
    }

    /// Damage spec, "Attack timing (cooldown)": the synthetic attack loop strikes every 25 cycles, so a strike
    /// decision starts a cooldown of 26 (a footman's). `orc02-assault` footman 1582 then waits 23 three cycles after
    /// the take-over (n = 379), and archer 1584 (cooldown 66) waits 63 (n = 346), restarted at frame 0 and unturned.
    #[test]
    fn a_strike_decision_waits_out_the_cooldown() {
        let mut w = world();
        assert_eq!(w.attack_loop[0], 25);
        let foe = w.spawn(0, 1, (1, 0), 0).unwrap();
        let i = w.index(UnitId(1)).unwrap();
        w.units[i].order = Order::Attack { target: foe };
        w.face_and_strike(i, (1, 0));
        assert_eq!((w.units[i].cooldown, w.units[i].facing), (26, 2));
        for (kind, left) in [(0usize, 23), (8, 63)] {
            w.attack_loop[kind] = if kind == 8 { 65 } else { 25 };
            w.units[i].kind = kind as u8;
            w.units[i].cooldown = left;
            w.units[i].script.wait = 1;
            w.units[i].script.frame = 7;
            w.units[i].facing = 5;
            w.face_and_strike(i, (1, 0));
            let u = &w.units[i];
            assert_eq!(
                (u.script.anim, u.script.frame, u.script.wait, u.facing, u.cooldown),
                (tidewater_formats::script::anim::ATTACK, 0, left as u8, 5, left),
                "type {kind}"
            );
        }
        // Steady fire: one loop later the cooldown has 1 cycle left and the wait is 1, not less, so the strike goes
        // on and the cooldown starts again (archer: 66, one strike every 65 cycles).
        w.units[i].cooldown = 1;
        w.face_and_strike(i, (1, 0));
        assert_eq!((w.units[i].cooldown, w.units[i].facing), (66, 2));
    }

    /// The cooldown counts down every cycle, except for transports; flyers and buildings skip the test.
    #[test]
    fn the_cooldown_counts_down_except_on_transports() {
        let mut w = world();
        let i = w.index(UnitId(1)).unwrap();
        w.units[i].cooldown = 26;
        run(&mut w, 3);
        assert_eq!(w.units[i].cooldown, 23);
        w.stats.0[0].flags |= tidewater_formats::udta::flags::TRANSPORT;
        run(&mut w, 3);
        assert_eq!(w.units[i].cooldown, 23);
        w.units[i].layer = Layer::Air;
        w.units[i].script.wait = 1;
        assert!(!w.cooling_down(i));
        w.units[i].layer = Layer::Land;
        assert!(w.cooling_down(i));
        w.units[i].building = true;
        w.units[i].cooldown = 0;
        w.start_cooldown(i);
        assert_eq!(w.units[i].cooldown, 0);
    }

    #[test]
    fn the_lost_counters_change_the_state_hash() {
        let w = world();
        let mut v = world();
        v.players[0].units_lost = 1;
        assert_ne!(w.state_hash(), v.state_hash());
    }

    #[test]
    fn melee_fight_to_the_death() {
        let mut w = world();
        // An unarmed copy of the footman, so it doesn't fight back.
        w.stats.0[1] = w.stats.0[0];
        w.stats.0[1].flags &= !tidewater_formats::udta::flags::CAN_ATTACK;
        let foe = w.spawn(1, 1, (1, 0), 0).unwrap();
        attack(&mut w, 0, vec![UnitId(1)], foe);
        run(&mut w, 40);
        let hp = w.unit(foe).unwrap().hp;
        assert!((60 - 2 * 8..60).contains(&hp), "one or two hits of 4 to 8, got {hp}");
        run(&mut w, 800);
        assert!(w.unit(foe).is_none(), "removed after its body decays");
        assert_eq!(w.occupant(Layer::Land, 1, 0), None);
        assert_eq!(w.players[0].kills, 1);
        assert_eq!(w.unit(UnitId(1)).unwrap().order, Order::Idle);
        // Unit-slots spec: the released slot is the first one reused, under a new id.
        let slot = 1600 - 2;
        let next = w.spawn(0, 1, (5, 7), 0).unwrap();
        assert_eq!((w.unit(next).unwrap().slot, next), (slot, UnitId(3)));
        assert_eq!(w.slots.len(crate::slots::NEUTRAL), 0);
    }

    #[test]
    fn chases_out_of_range_targets() {
        let mut w = world();
        let foe = w.spawn(0, 1, (0, 5), 0).unwrap();
        attack(&mut w, 0, vec![UnitId(1)], foe);
        run(&mut w, 100);
        let me = w.unit(UnitId(1)).unwrap();
        assert!(me.tile.1 >= 4, "walked next to the target, at {:?}", me.tile);
        assert!(w.unit(foe).unwrap().hp < 60);
    }

    #[test]
    fn arrows_fly_and_hit() {
        let mut w = world();
        let archer = w.spawn(8, 0, (0, 7), 0).unwrap();
        let foe = w.spawn(0, 1, (3, 7), 0).unwrap();
        attack(&mut w, 0, vec![archer], foe);
        let mut saw_missile = false;
        for _ in 0..40 {
            w.step(&[]);
            saw_missile |= w.missiles.iter().any(Option::is_some);
        }
        assert!(saw_missile);
        assert_eq!(w.unit(archer).unwrap().tile, (0, 7), "in range, so it stays put");
        assert!(w.unit(foe).unwrap().hp < 60);
    }

    #[test]
    fn splash_hits_friends_but_not_the_thrower() {
        let mut w = world();
        let cat = w.spawn(5, 0, (0, 7), 0).unwrap();
        let foe = w.spawn(0, 1, (5, 7), 0).unwrap();
        let friend = w.spawn(0, 0, (6, 7), 0).unwrap();
        w.unit_visibility();
        attack(&mut w, 0, vec![cat], foe);
        // The order takes over at the next still boundary, then the strike and 160 px of flight at 5 px a cycle. Stop
        // at the first landing: a surviving foe then fights back.
        for _ in 0..100 {
            if w.unit(foe).is_none_or(|u| u.hp < 60) {
                break;
            }
            run(&mut w, 1);
        }
        assert!(w.unit(foe).is_none_or(|u| u.hp < 60));
        assert!(w.unit(friend).is_none_or(|u| u.hp < 60), "friendly fire");
        assert_eq!(w.unit(cat).unwrap().hp, 110);
    }

    #[test]
    fn same_battle_same_hash() {
        let go = || {
            let mut w = world();
            let foe = w.spawn(8, 1, (2, 2), 0).unwrap();
            attack(&mut w, 0, vec![UnitId(1)], foe);
            run(&mut w, 300);
            w.state_hash()
        };
        assert_eq!(go(), go());
    }

    // ----- missile flight (damage spec, "Missile flight" test vectors) ---------------------------------------

    use crate::ai::controller;
    use crate::combat::missile;
    use crate::world::tests::field;
    use crate::{Rng, World};
    use tidewater_formats::udta::{UnitStats, flags};

    const FOOTMAN: u8 = 0;
    const CATAPULT: u8 = 5;
    const BALLISTA: u8 = 6;
    const MAGE: u8 = 7;
    const ARCHER: u8 = 8;
    const GRYPHON: u8 = 9;
    const TOWER: u8 = 10;

    /// An open map with the shooters of these tests added to the synthetic unit table.
    fn range() -> World {
        let mut w = field(16, 16);
        let shooter = |m: u8, basic: u8, pierce: u8| UnitStats {
            hit_points: 100,
            size: (1, 1),
            flags: flags::LAND | flags::CAN_ATTACK,
            basic_damage: basic,
            piercing_damage: pierce,
            attack_range: 8,
            missile: m,
            can_target: 3,
            ..Default::default()
        };
        w.stats.0[BALLISTA as usize] = shooter(missile::BALLISTA, 80, 0);
        w.stats.0[MAGE as usize] = shooter(missile::LIGHTNING, 0, 10);
        w.stats.0[GRYPHON as usize] = shooter(missile::GRYPHON_HAMMER, 0, 16);
        w.stats.0[TOWER as usize] = UnitStats {
            flags: flags::LAND | flags::CAN_ATTACK | flags::TOWER,
            ..shooter(missile::ARROW, 4, 12)
        };
        w
    }

    /// A generator whose next draws satisfy `want` (draw index, test).
    fn seed_with(want: &[(usize, &dyn Fn(u32) -> bool)]) -> Rng {
        let n = want.iter().map(|w| w.0 + 1).max().unwrap();
        (0u32..)
            .map(Rng::new)
            .find(|start| {
                let mut r = *start;
                let d: Vec<u32> = (0..n).map(|_| r.draw()).collect();
                want.iter().all(|(i, f)| f(d[*i]))
            })
            .unwrap()
    }

    /// Both scatter draws give an offset of 0, the first after `before` other draws.
    fn no_scatter(before: usize) -> Rng {
        seed_with(&[(before, &|r| r & 7 == 3), (before + 1, &|r| r & 7 == 3)])
    }

    /// The number of draws from state `from` to state `to`.
    fn drawn(from: Rng, to: Rng) -> u32 {
        let mut r = from;
        let mut n = 0;
        while r != to {
            r.draw();
            n += 1;
            assert!(n < 10_000);
        }
        n
    }

    /// Spawns a unit and puts its centre at the given pixel position (its tile stays the spawn tile).
    fn spawn_at(w: &mut World, kind: u8, owner: u8, tile: (i32, i32), centre: (i32, i32)) -> UnitId {
        let id = w.spawn(kind, owner, tile, 0).unwrap();
        let i = w.index(id).unwrap();
        w.units[i].pos = (centre.0 - 16, centre.1 - 16);
        id
    }

    /// The strike of `shooter` on a unit or, with `None`, on the wall tile `wall`: cycle C of the flight vectors.
    fn launch(w: &mut World, shooter: UnitId, target: Option<UnitId>, wall: (i32, i32)) {
        let i = w.index(shooter).unwrap();
        w.units[i].order = match target {
            Some(target) => Order::Attack { target },
            None => Order::AttackWall { tile: wall },
        };
        w.strike(i);
    }

    /// Updates the missiles `n` times (the cycles C, C + 1, ...), returning the draws each update made.
    fn fly(w: &mut World, n: usize) -> Vec<u32> {
        (0..n)
            .map(|_| {
                let before = w.rng;
                w.advance_missiles();
                drawn(before, w.rng)
            })
            .collect()
    }

    fn live(w: &World) -> Option<&crate::Missile> {
        w.missiles.iter().flatten().next()
    }

    #[test]
    fn arrow_at_100_px() {
        let mut w = range();
        let archer = spawn_at(&mut w, ARCHER, 0, (0, 7), (16, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        // Scatter of +4 on x and 0 on y makes the 96 px distance 100.
        w.rng = seed_with(&[(1, &|r| r & 7 == 7), (2, &|r| r & 7 == 3)]);
        let start = w.rng;
        launch(&mut w, archer, Some(foe), (0, 0));
        assert_eq!(drawn(start, w.rng), 3, "damage, x scatter, y scatter");
        let m = live(&w).unwrap();
        assert_eq!((m.flight, m.to), (9, (116, 240)));
        // C only starts the missile; C + 1 to C + 9 make one draw each, and the landing step ends 108 px out.
        assert_eq!(fly(&mut w, 10), [0, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(live(&w).unwrap().pos, (16 + 108, 240));
        assert_eq!(w.unit(foe).unwrap().hp, 60, "the impact comes a cycle later");
        // C + 10: the stored damage lands, with no draw of its own.
        assert_eq!(fly(&mut w, 1), [0]);
        assert!(live(&w).is_none());
        assert!(w.unit(foe).unwrap().hp < 60);
        assert_eq!(drawn(start, w.rng), 12, "12 draws in all");
    }

    #[test]
    fn catapult_rock_at_160_px() {
        let mut w = range();
        let cat = spawn_at(&mut w, CATAPULT, 0, (0, 7), (16, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (5, 7), (176, 240));
        w.rng = no_scatter(0);
        let start = w.rng;
        launch(&mut w, cat, Some(foe), (0, 0));
        assert_eq!(
            drawn(start, w.rng),
            2,
            "no damage draw for a splash missile, then the scatter"
        );
        assert_eq!(live(&w).unwrap().flight, 21);
        // Steps 1 to 19 make 2 draws (travel 8 to 152 is below 160), steps 20 and 21 make 1.
        let per_cycle = fly(&mut w, 22);
        let mut want = vec![0];
        want.extend([2; 19]);
        want.extend([1, 1]);
        assert_eq!(per_cycle, want);
        assert_eq!(per_cycle.iter().sum::<u32>(), 40);
        assert_eq!(live(&w).unwrap().pos, (16 + 168, 240), "landing at C + 21, 168 px out");
        assert_eq!(w.unit(foe).unwrap().hp, 60);
        // The splash is at C + 26: nothing until then, then one draw for the one unit hit.
        assert_eq!(fly(&mut w, 4), [0, 0, 0, 0]);
        assert!(live(&w).is_some());
        assert_eq!(fly(&mut w, 1), [1]);
        // Damage spec, "Impact effects": only the rock's ballista-impact effect is left.
        assert_eq!(live(&w).map(|m| m.kind), Some(0x15));
        // Damage spec, "Applying damage": a killing blow leaves the hit points as they were.
        let f = w.unit(foe).unwrap();
        assert!(f.dying || f.hp < 60);
    }

    #[test]
    fn catapult_rock_at_103_px_draws_twice_on_every_step() {
        let mut w = range();
        let cat = spawn_at(&mut w, CATAPULT, 0, (2, 7), (73, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (5, 7), (176, 240));
        w.rng = no_scatter(0);
        launch(&mut w, cat, Some(foe), (0, 0));
        assert_eq!(live(&w).unwrap().flight, 13, "N = 13, 104 px");
        // 5 * ceil(103 / 5) = 105: even the landing step (travel 104) makes 2 draws.
        let per_cycle = fly(&mut w, 14);
        assert_eq!(per_cycle[0], 0);
        assert!(per_cycle[1..].iter().all(|&n| n == 2));
        assert_eq!(per_cycle.iter().sum::<u32>(), 26);
    }

    #[test]
    fn ballista_bolt_at_100_px() {
        let mut w = range();
        let bal = spawn_at(&mut w, BALLISTA, 0, (2, 7), (76, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (5, 7), (176, 240));
        w.rng = no_scatter(0);
        launch(&mut w, bal, Some(foe), (0, 0));
        assert_eq!(live(&w).unwrap().flight, 13);
        let per_cycle = fly(&mut w, 14);
        assert_eq!(per_cycle.iter().sum::<u32>(), 13, "13 flight draws, none extra");
        assert_eq!(live(&w).unwrap().pos, (76 + 104, 240), "landing at C + 13, 104 px out");
        assert_eq!(fly(&mut w, 1), [1], "the splash at C + 14 hits the footman");
        assert_eq!(live(&w).map(|m| m.kind), Some(0x15), "only the impact effect is left");
    }

    #[test]
    fn lightning_at_100_px() {
        let mut w = range();
        let mage = spawn_at(&mut w, MAGE, 0, (0, 7), (16, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        w.rng = seed_with(&[(1, &|r| r & 7 == 7), (2, &|r| r & 7 == 3)]);
        launch(&mut w, mage, Some(foe), (0, 0));
        assert_eq!(live(&w).unwrap().flight, 7);
        let start = w.rng;
        let per_cycle = fly(&mut w, 8);
        assert_eq!(per_cycle.iter().sum::<u32>(), 7);
        assert_eq!(live(&w).unwrap().pos, (16 + 112, 240), "landing at C + 7, 112 px out");
        // The hit is 11 cycles after the landing step, at C + 18.
        for _ in 0..10 {
            assert_eq!(fly(&mut w, 1), [0]);
            assert_eq!(w.unit(foe).unwrap().hp, 60);
        }
        assert_eq!(fly(&mut w, 1), [0]);
        assert!(live(&w).is_none());
        assert!(w.unit(foe).unwrap().hp < 60);
        assert_eq!(drawn(start, w.rng), 7);
    }

    #[test]
    fn catapult_attacking_a_wall_tile_human_and_computer() {
        for (computer, steps, out) in [(false, 13, 104), (true, 9, 72)] {
            let mut w = range();
            w.players[0].controller = if computer {
                controller::COMPUTER
            } else {
                controller::HUMAN
            };
            w.terrain.build_wall(5, 7, false, !computer);
            // 70 px from the wall tile's centre, so the human minimum of 96 px applies.
            let cat = spawn_at(&mut w, CATAPULT, 0, (2, 7), (106, 240));
            w.rng = no_scatter(0);
            launch(&mut w, cat, None, (5, 7));
            assert_eq!(live(&w).unwrap().flight, steps, "computer: {computer}");
            fly(&mut w, steps as usize + 1);
            assert_eq!(live(&w).unwrap().pos, (106 + out, 240), "computer: {computer}");
        }
    }

    #[test]
    fn the_wall_exception_needs_a_wall_and_no_tower() {
        let build = |kind: u8, wall: bool| {
            let mut w = range();
            w.players[0].controller = controller::COMPUTER;
            if wall {
                w.terrain.build_wall(5, 7, false, false);
            }
            let shooter = spawn_at(&mut w, kind, 0, (2, 7), (106, 240));
            w.rng = no_scatter(0);
            launch(&mut w, shooter, None, (5, 7));
            live(&w).unwrap().flight
        };
        assert_eq!(build(CATAPULT, false), 13, "no wall on the tile");
        assert_eq!(build(BALLISTA, true), 9, "a ballista bolt has a 3-tile minimum too");
        // A tower's shot gets no exception and no scatter.
        let mut w = range();
        w.stats.0[TOWER as usize].missile = missile::BALLISTA;
        w.players[0].controller = controller::COMPUTER;
        w.terrain.build_wall(5, 7, false, false);
        let tower = spawn_at(&mut w, TOWER, 0, (2, 7), (106, 240));
        let start = w.rng;
        launch(&mut w, tower, None, (5, 7));
        assert_eq!(
            drawn(start, w.rng),
            0,
            "no damage draw for a splash missile and no scatter"
        );
        let m = live(&w).unwrap();
        assert_eq!((m.to, m.flight), ((176, 240), 13));
    }

    #[test]
    fn guard_tower_arrows_are_not_scattered() {
        let mut w = range();
        let tower = spawn_at(&mut w, TOWER, 0, (0, 7), (16, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        let start = w.rng;
        launch(&mut w, tower, Some(foe), (0, 0));
        assert_eq!(drawn(start, w.rng), 1, "only the damage draw");
        assert_eq!(live(&w).unwrap().to, (112, 240));
    }

    #[test]
    fn gryphon_hammer_splashes_along_its_path() {
        let mut w = range();
        let gryphon = spawn_at(&mut w, GRYPHON, 0, (0, 7), (16, 240));
        let a = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        let b = spawn_at(&mut w, FOOTMAN, 1, (3, 8), (112, 260));
        w.rng = seed_with(&[(0, &|r| r % 9 == 4), (1, &|r| r & 7 == 3), (2, &|r| r & 7 == 3)]);
        let start = w.rng;
        launch(&mut w, gryphon, Some(a), (0, 0));
        assert_eq!(drawn(start, w.rng), 3);
        let m = live(&w).unwrap();
        assert_eq!((m.damage, m.to), (12, (112, 240)), "stored damage 8 + 4");
        let h = crate::slots::PERMANENT_EFFECTS;
        let hammer = |w: &World| w.missiles[h].clone();
        // Update k is cycle C + k, and step k is update k.
        let mut per_cycle = vec![];
        let mut out = vec![];
        for _ in 0..31 {
            per_cycle.push(fly(&mut w, 1)[0]);
            out.push(hammer(&w).unwrap().pos.0 - 16);
        }
        // Steps 1 to 4 move 12 px (48 px out, remaining 48: count 1), then 6 px a step.
        assert_eq!(out[..5], [0, 12, 24, 36, 48]);
        assert_eq!(out[5], 54);
        assert_eq!(out[11], 90, "step 11 is count 8");
        assert_eq!(out[19], 138, "step 19 is count 16");
        assert_eq!(out[27], 186, "step 27 is count 24");
        assert_eq!(out[30], 204, "step 30 moves, then ends");
        // One draw per step, plus splash draws on steps 11 (A and B) and 19 (A and B); step 27 hits nobody.
        for (k, n) in per_cycle.iter().enumerate() {
            let want = match k {
                0 => 0,
                11 | 19 => 3,
                _ => 1,
            };
            assert_eq!(*n, want, "step {k}");
        }
        let (hp_a, hp_b) = (w.unit(a).unwrap().hp, w.unit(b).unwrap().hp);
        assert!((60 - hp_a) >= 8 && (60 - hp_b) >= 8, "both took the first splash");
        assert!(hammer(&w).is_some(), "the end step is C + 30; the hit comes at C + 31");
        // C + 31: A takes the stored 12 with no draw.
        assert_eq!(fly(&mut w, 1), [0]);
        assert!(hammer(&w).is_none());
        assert_eq!(w.unit(a).unwrap().hp, hp_a - 12);
        assert_eq!(w.unit(b).unwrap().hp, hp_b);
        // 3 launch draws + 30 step draws + 4 splash draws; A lost 20 to 28 in all, B 8 to 16.
        assert_eq!(drawn(start, w.rng), 37);
        assert!((20..=28).contains(&(60 - w.unit(a).unwrap().hp)));
        assert!((8..=16).contains(&(60 - w.unit(b).unwrap().hp)));
    }

    /// Damage spec, "Missile flight" (round-39 errata): each hammer splash with a free effect slot plays the
    /// explosion sound, applies the splash and creates a kind 0x17 explosion at the hammer's position, with zero
    /// fields, in the lowest free slot. With every slot in use there is no splash, no sound and no explosion.
    #[test]
    fn a_hammer_splash_makes_an_explosion_and_needs_a_free_slot() {
        for full in [false, true] {
            let mut w = range();
            let gryphon = spawn_at(&mut w, GRYPHON, 0, (0, 7), (16, 240));
            let a = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
            launch(&mut w, gryphon, Some(a), (0, 0));
            let h = crate::slots::PERMANENT_EFFECTS;
            if full {
                for m in w.missiles.iter_mut().skip(h + 1) {
                    *m = Some(filler());
                }
            }
            // Steps 1 to 10, then step 11 (count 8) splashes.
            fly(&mut w, 11);
            w.sounds.cues.clear();
            let before = w.rng;
            fly(&mut w, 1);
            let pos = w.missiles[h].as_ref().unwrap().pos;
            let cues: Vec<Sound> = w.sounds.cues.iter().map(|c| c.sound).collect();
            if full {
                assert_eq!(drawn(before, w.rng), 1, "the step's draw only");
                assert!(!cues.contains(&Sound::Explode));
                assert_eq!(w.unit(a).unwrap().hp, 60);
                assert!(w.missiles.iter().skip(h + 1).all(|m| m.as_ref().unwrap().kind == 0x16));
                continue;
            }
            assert_eq!(drawn(before, w.rng), 2, "the step's draw and one for A");
            assert!(cues.contains(&Sound::Explode));
            assert!(w.unit(a).unwrap().hp < 60);
            let e = w.missiles[h + 1].as_ref().expect("the explosion");
            assert_eq!(
                (e.kind, e.pos, e.to, e.target, e.attacker, e.damage),
                (0x17, pos, (0, 0), None, UnitId(0), 0)
            );
        }
    }

    // ----- impact effects (damage spec, "Impact effects"; sounds spec, "Missile impacts") -----------------------

    use crate::sound::Sound;

    /// A missile of `kind` whose next update is its impact, at `pos`.
    #[test]
    fn an_effect_shows_from_its_first_update() {
        let mut m = landing(missile::BALLISTA, 0, (100, 100));
        m.elapsed = 0;
        assert!(!m.showing());
        m.elapsed = 1;
        assert!(m.showing());
    }

    fn landing(kind: u8, damage: u8, pos: (i32, i32)) -> crate::Missile {
        crate::Missile {
            kind,
            attacker: UnitId(0),
            target: None,
            from: pos,
            to: pos,
            pos,
            flight: 1,
            elapsed: 1 + crate::combat::flight(kind).delay,
            damage,
            splash: crate::combat::splash(kind).is_some(),
            hits_air: false,
            fx: crate::spells::Fx::Shot {
                line: crate::spells::Line::new(pos, pos),
                length: 1,
            },
        }
    }

    /// A cosmetic spell marker (kind 0x16) that only holds its slot and makes no sound.
    fn filler() -> crate::Missile {
        crate::Missile {
            kind: 0x16,
            fx: crate::spells::Fx::Cosmetic,
            ..landing(missile::ARROW, 0, (0, 0))
        }
    }

    /// The cues of the next update of the effect table.
    fn update_cues(w: &mut World) -> Vec<Sound> {
        w.sounds.cues.clear();
        w.advance_missiles();
        w.sounds.cues.iter().map(|c| c.sound).collect()
    }

    fn impact(kind: u8, damaging: bool) -> Sound {
        Sound::Impact { kind, damaging }
    }

    #[test]
    fn each_missile_kind_leaves_its_impact_effect() {
        use crate::spells::impact_effect;
        assert_eq!(impact_effect(missile::CATAPULT), Some(0x15));
        assert_eq!(impact_effect(missile::BALLISTA), Some(0x15));
        assert_eq!(impact_effect(missile::BIG_CANNON), Some(0x1a));
        assert_eq!(impact_effect(missile::SMALL_CANNON), Some(0x1a));
        assert_eq!(impact_effect(missile::SUBMARINE), Some(0x17));
        assert_eq!(impact_effect(missile::TURTLE), Some(0x17));
        for k in [
            missile::LIGHTNING,
            missile::GRYPHON_HAMMER,
            missile::DRAGON_BREATH,
            missile::TOUCH_OF_DEATH,
            missile::ARROW,
            missile::AXE,
            missile::DEMON_FIRE,
        ] {
            assert_eq!(impact_effect(k), None, "kind {k:#x}");
        }
    }

    #[test]
    fn catapult_rock_impact_effect_takes_a_later_slot_and_sounds_a_cycle_after_its_first_update() {
        let mut w = range();
        let foe = spawn_at(&mut w, FOOTMAN, 1, (5, 7), (176, 240));
        // Slots 0 to 2 are the permanent effects; the rock is in slot 3, slot 4 is in use and slot 5 is free.
        w.missiles[3] = Some(landing(missile::CATAPULT, 20, (176, 240)));
        w.missiles[4] = Some(filler());
        let cues = update_cues(&mut w);
        assert!(w.unit(foe).unwrap().hp < 60, "the splash");
        assert!(w.missiles[3].is_none(), "the rock's slot is freed");
        let fx = w.missiles[5].as_ref().unwrap();
        assert_eq!((fx.kind, fx.pos), (0x15, (176, 240)));
        assert_eq!(
            fx.elapsed, 1,
            "slot 5 comes after slot 3: its first update is in the same cycle"
        );
        assert!(!fx.showing(), "not showing before its start step");
        // The rock's impact cue has no file (sounds spec); the effect's explosion follows at its tile, at the start
        // step one cycle after the first update.
        assert_eq!(cues, [impact(missile::CATAPULT, true)]);
        assert_eq!(update_cues(&mut w), [Sound::Explode]);
        assert!(w.missiles[5].as_ref().unwrap().showing(), "showing from the start step");
        let place = w.sounds.cues[0].place;
        assert!(
            matches!(place, Some(crate::sound::Place::Tile { tile: (5, 7), .. })),
            "at its tile"
        );
        // Removed during its update in cycle U + 14: 12 cycles of waits after the restart in U + 1.
        for _ in 0..12 {
            assert!(update_cues(&mut w).is_empty(), "no further sound");
            assert!(w.missiles[5].is_some());
        }
        w.advance_missiles();
        assert!(w.missiles[5].is_none());
    }

    #[test]
    fn ballista_bolt_with_no_damage_still_leaves_its_effect() {
        let mut w = range();
        w.missiles[3] = Some(landing(missile::BALLISTA, 0, (100, 100)));
        let cues = update_cues(&mut w);
        assert!(w.missiles[3].is_none());
        assert_eq!(w.missiles[4].as_ref().map(|m| m.kind), Some(0x15));
        assert_eq!(cues, [impact(missile::BALLISTA, false)]);
        assert_eq!(
            update_cues(&mut w),
            [Sound::Explode],
            "at its start step, a cycle later"
        );
    }

    #[test]
    fn big_cannon_impact_effect_in_an_earlier_slot_runs_next_cycle() {
        let mut w = range();
        // The spec's "slot 1 free, the missile in slot 6": here the first free slot (3) is before the missile's (6).
        for n in 4..6 {
            w.missiles[n] = Some(filler());
        }
        w.missiles[6] = Some(landing(missile::BIG_CANNON, 30, (100, 100)));
        let cues = update_cues(&mut w);
        assert!(w.missiles[6].is_none());
        let fx = w.missiles[3].as_ref().unwrap();
        assert_eq!((fx.kind, fx.elapsed), (0x1a, 0), "not updated yet");
        assert_eq!(cues, [impact(missile::BIG_CANNON, true)]);
        assert!(
            update_cues(&mut w).is_empty(),
            "its first update, in the next cycle: no sound yet"
        );
        assert_eq!(
            update_cues(&mut w),
            [Sound::Explode],
            "its explosion at its start step, a cycle after the first update"
        );
        for _ in 0..12 {
            w.advance_missiles();
        }
        assert!(w.missiles[3].is_some());
        w.advance_missiles();
        assert!(w.missiles[3].is_none(), "removed in cycle U + 14");
    }

    #[test]
    fn small_cannon_with_no_free_slot_makes_no_effect() {
        let mut w = range();
        let n = w.missiles.len();
        for k in crate::slots::PERMANENT_EFFECTS..n {
            w.missiles[k] = Some(filler());
        }
        w.missiles[7] = Some(landing(missile::SMALL_CANNON, 30, (100, 100)));
        let cues = update_cues(&mut w);
        assert!(w.missiles[7].is_none(), "freed only after the impact");
        assert!(w.missiles.iter().flatten().all(|m| m.kind == 0x16), "no impact effect");
        assert_eq!(cues, [impact(missile::SMALL_CANNON, true)]);
    }

    #[test]
    fn submarine_missile_leaves_a_silent_explosion() {
        let mut w = range();
        w.missiles[3] = Some(landing(missile::SUBMARINE, 20, (100, 100)));
        let cues = update_cues(&mut w);
        assert_eq!(cues, [impact(missile::SUBMARINE, true)]);
        assert_eq!(w.missiles[4].as_ref().map(|m| m.kind), Some(0x17));
        for _ in 0..49 {
            assert!(update_cues(&mut w).is_empty());
            assert!(w.missiles[4].is_some());
        }
        w.advance_missiles();
        assert!(w.missiles[4].is_none(), "removed in cycle U + 50");
    }

    #[test]
    fn explosions_per_impact_in_all() {
        // Sounds spec: the impact sound is `misc\explode` for these kinds (the ballista bolt only if damaging).
        let explodes = |s: &Sound| match *s {
            Sound::Impact { kind, damaging } => {
                matches!(kind, 0x07 | 0x11 | 0x12) || (kind == missile::BALLISTA && damaging)
            }
            Sound::Explode => true,
            _ => false,
        };
        for (kind, damage, want) in [
            (missile::CATAPULT, 50, 1),
            (missile::BALLISTA, 50, 2),
            (missile::BALLISTA, 0, 1),
            (missile::BIG_CANNON, 50, 2),
            (missile::SMALL_CANNON, 50, 1),
            (missile::SUBMARINE, 50, 1),
            (missile::TURTLE, 50, 1),
        ] {
            let mut w = range();
            w.missiles[3] = Some(landing(kind, damage, (100, 100)));
            let mut n = 0;
            for _ in 0..60 {
                n += update_cues(&mut w).iter().filter(|s| explodes(s)).count();
            }
            assert_eq!(n, want, "kind {kind:#x}, damage {damage}");
        }
    }

    // ----- the Dark Portal case (damage spec, "Applying damage") ----------------------------------------------

    /// A field with a Dark Portal (type 0x65) of player 1, a footman and Khadgar (type 0x18) of player 0, and the
    /// campaign entry and game kind given. Returns the world, the portal, the footman and Khadgar.
    fn portal_world(campaign: Option<u8>, multiplayer: bool) -> (World, UnitId, UnitId, UnitId) {
        let mut w = field(16, 16);
        w.stats.0[0x65] = UnitStats {
            hit_points: 1000,
            size: (2, 2),
            flags: flags::BUILDING,
            ..Default::default()
        };
        w.stats.0[0x18] = w.stats.0[FOOTMAN as usize];
        w.victory.campaign = campaign;
        w.rules.multiplayer = multiplayer;
        let portal = w.spawn(0x65, 1, (8, 8), 0).unwrap();
        let foot = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
        let khadgar = w.spawn(0x18, 0, (3, 2), 0).unwrap();
        (w, portal, foot, khadgar)
    }

    fn hit(w: &mut World, target: UnitId, attacker: UnitId) -> (u16, Option<UnitId>) {
        let ti = w.index(target).unwrap();
        w.damage(ti, 10, attacker);
        let t = w.unit(target).unwrap();
        (t.hp, t.attacked_by)
    }

    #[test]
    fn the_dark_portal_takes_hits_only_from_khadgar_on_three_campaign_entries() {
        for entry in [32, 34, 50] {
            let (mut w, portal, foot, khadgar) = portal_world(Some(entry), false);
            assert_eq!(
                hit(&mut w, portal, foot),
                (1000, None),
                "entry {entry}: nothing, not even the notification"
            );
            assert_eq!(
                hit(&mut w, portal, khadgar).0,
                990,
                "entry {entry}: Khadgar's hit counts"
            );
        }
        // Other entries, custom scenarios and network games: an ordinary hit.
        for (campaign, multiplayer) in [(Some(33), false), (Some(4), false), (None, false), (Some(32), true)] {
            let (mut w, portal, foot, _) = portal_world(campaign, multiplayer);
            assert_eq!(hit(&mut w, portal, foot).0, 990, "{campaign:?}, network {multiplayer}");
        }
        // Only type 0x65 is protected.
        let (mut w, _, foot, khadgar) = portal_world(Some(32), false);
        assert_eq!(hit(&mut w, khadgar, foot).0, 50);
    }

    // ----- cannon smoke and launch slots (damage spec, "Cannon smoke and launch slots") --------------------------

    const BATTLESHIP: u8 = 11;
    const DESTROYER: u8 = 12;
    const CANNON_TOWER: u8 = 13;

    /// `range()` plus a big-cannon ship, a small-cannon ship and a small-cannon tower.
    fn ships() -> World {
        let mut w = range();
        let shooter = |m: u8, f: u32| UnitStats {
            hit_points: 100,
            size: (1, 1),
            flags: flags::LAND | flags::CAN_ATTACK | f,
            basic_damage: 10,
            piercing_damage: 5,
            attack_range: 8,
            missile: m,
            can_target: 3,
            ..Default::default()
        };
        w.stats.0[BATTLESHIP as usize] = shooter(missile::BIG_CANNON, 0);
        w.stats.0[DESTROYER as usize] = shooter(missile::SMALL_CANNON, 0);
        w.stats.0[CANNON_TOWER as usize] = shooter(missile::SMALL_CANNON, flags::TOWER);
        w
    }

    /// Fills every effect slot after the permanent ones except those in `free`.
    fn fill_except(w: &mut World, free: &[usize]) {
        for n in crate::slots::PERMANENT_EFFECTS..w.missiles.len() {
            if !free.contains(&n) {
                w.missiles[n] = Some(filler());
            }
        }
    }

    fn kinds(w: &World) -> Vec<(usize, u8)> {
        w.missiles
            .iter()
            .enumerate()
            .filter_map(|(n, m)| m.as_ref().map(|m| (n, m.kind)))
            .collect()
    }

    /// Strikes with `kind` at a footman three tiles away.
    fn shoot(w: &mut World, kind: u8) -> u32 {
        let shooter = spawn_at(w, kind, 0, (0, 7), (16, 240));
        let foe = spawn_at(w, FOOTMAN, 1, (3, 7), (112, 240));
        let start = w.rng;
        launch(w, shooter, Some(foe), (0, 0));
        drawn(start, w.rng)
    }

    #[test]
    fn a_ships_cannon_is_followed_by_smoke_that_lasts_to_c_plus_24() {
        let mut w = ships();
        let ship = spawn_at(&mut w, BATTLESHIP, 0, (0, 7), (16, 240));
        let foe = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        let base = crate::slots::PERMANENT_EFFECTS;
        // Three effects in use, the rest free.
        for n in base..base + 3 {
            w.missiles[n] = Some(filler());
        }
        launch(&mut w, ship, Some(foe), (0, 0));
        let ks = kinds(&w);
        assert_eq!(
            ks[base + 3 - base..],
            [(base + 3, missile::BIG_CANNON), (base + 4, 0x19)][..]
        );
        let smoke = w.missiles[base + 4].as_ref().unwrap();
        assert_eq!(
            (smoke.pos, smoke.elapsed),
            ((16, 240), 0),
            "the shooter's centre, no first update yet"
        );
        // Updates at C to C + 23 keep it (its start animation waits 11 cycles); the one at C + 24 removes it.
        for cycle in 0..24 {
            w.advance_missiles();
            let smoke = w.missiles[base + 4].as_ref().expect("alive");
            assert_eq!(smoke.showing(), cycle >= 11, "showing from C + 11, at C + {cycle}");
        }
        w.advance_missiles();
        assert!(w.missiles[base + 4].is_none(), "removed in C + 24");
    }

    #[test]
    fn smoke_takes_the_next_free_slot_after_the_missile() {
        let mut w = ships();
        let base = crate::slots::PERMANENT_EFFECTS;
        fill_except(&mut w, &[base + 1, base + 3]);
        assert_eq!(shoot(&mut w, DESTROYER), 2, "a splash missile: scatter only");
        assert_eq!(w.missiles[base + 1].as_ref().unwrap().kind, missile::SMALL_CANNON);
        assert_eq!(w.missiles[base + 3].as_ref().unwrap().kind, 0x19);
    }

    #[test]
    fn with_one_free_slot_the_missile_flies_without_smoke() {
        let mut w = ships();
        let base = crate::slots::PERMANENT_EFFECTS;
        fill_except(&mut w, &[base + 7]);
        assert_eq!(shoot(&mut w, DESTROYER), 2);
        assert_eq!(w.missiles[base + 7].as_ref().unwrap().kind, missile::SMALL_CANNON);
        assert!(!kinds(&w).iter().any(|k| k.1 == 0x19));
    }

    #[test]
    fn with_no_free_slot_a_shot_creates_nothing_and_draws_nothing() {
        for kind in [BATTLESHIP, DESTROYER, ARCHER] {
            let mut w = ships();
            fill_except(&mut w, &[]);
            let before = kinds(&w);
            assert_eq!(shoot(&mut w, kind), 0, "kind {kind}");
            assert_eq!(kinds(&w), before, "kind {kind}");
        }
    }

    #[test]
    fn a_cannon_tower_fires_without_scatter_or_smoke() {
        let mut w = ships();
        assert_eq!(
            shoot(&mut w, CANNON_TOWER),
            0,
            "splash missile: no damage draw, no scatter"
        );
        let ks = kinds(&w);
        assert_eq!(ks, [(crate::slots::PERMANENT_EFFECTS, missile::SMALL_CANNON)]);
        assert_eq!(live(&w).unwrap().to, (112, 240));
    }

    #[test]
    fn catapults_and_ballistas_scatter_but_make_no_smoke() {
        for kind in [CATAPULT, BALLISTA] {
            let mut w = ships();
            assert_eq!(shoot(&mut w, kind), 2, "kind {kind}");
            assert_eq!(kinds(&w).len(), 1, "kind {kind}");
        }
    }

    #[test]
    fn a_higher_slot_effect_may_take_the_smokes_old_slot() {
        let mut w = ships();
        let base = crate::slots::PERMANENT_EFFECTS;
        let smoke = crate::Missile {
            kind: 0x19,
            fx: crate::spells::Fx::Cosmetic,
            elapsed: 24,
            ..landing(missile::ARROW, 0, (0, 0))
        };
        w.missiles[base] = Some(smoke);
        w.missiles[base + 1] = Some(landing(missile::BIG_CANNON, 0, (100, 100)));
        w.advance_missiles();
        assert_eq!(kinds(&w), [(base, 0x1a)], "the explosion took the smoke's slot");
    }
}
