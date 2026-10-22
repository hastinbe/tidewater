//! Attacks, missiles and death, along the one chain of `docs/specs/combat/damage.md`, "The model": the attack values
//! ("Effective attack values"), the strike point of the attack cycle ([`World::strike`], "How an attack delivers
//! damage"; towers, [`World::tower_shot`], "Towers"), one of four deliveries (a melee blow, a missile, "Missile
//! flight", with its "Splash on impact", or the demolition blast of `ground.rs`), one apply-damage step
//! ([`World::damage`], "Applying damage" with its Dark Portal case) and one kill step ("A kill"). Missiles fly by the
//! line rule of `spells/spells.md` ("How flying effects move"), which `spells.rs` implements as `Line`. The attack
//! orders' walk and range test follow `orders/orders.md` ("Attack orders", "Arriving and losing the target") and
//! "Reach and range". The cosmetic cannon smoke (kind 0x19) follows a unit's cannon missile in the next free effect slot. The kill credit,
//! mission score and lost counters follow `docs/specs/game/campaign-flow.md` ("The score screen", "Mission score").
//! Our reading: only a kill by damage from a unit is credited (a spell that kills without damage, such as
//! polymorph, credits nobody), and only deaths and rescue's owner change count as lost (a removed corpse's move to
//! the neutral player doesn't).

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags as udta_flags;

use crate::TILE_PX;
use crate::combat::{self, missile};
use crate::movement::{Goal, Walk, chebyshev};
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
    /// The shooter's unit slot, recorded at creation and never cleared (spells spec, "The shooter
    /// after its slot is released"): the trace shows it after the slot is released, and a spell effect's splash
    /// credits whatever unit holds the slot by then. `None` for an effect with no shooter.
    pub shooter: Option<u16>,
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

/// Animation-scripts spec, "Facing a tile": a warship's new facing when it strikes, by the direction
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

/// How a splash treats the units it finds (damage spec, "Splash on impact" and "Missile flight"; spells spec,
/// "Splash").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Splash {
    /// A splash missile's impact: armour is subtracted, and air units are scanned only for a small-cannon shot at an
    /// air unit.
    Impact { air: bool },
    /// A gryphon hammer's or dragon breath's splash along its path, or a spell effect's: armour isn't subtracted, so
    /// every unit within 42 px is hit, and air units are scanned.
    Path,
}

/// Who makes an attack (damage spec, "The model", step 2): a unit's attack script at its strike point, or a tower
/// from its order logic ("Towers"), whose shot has no aim scatter, wall exception or smoke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shooter {
    Script,
    Tower,
}

/// What an attack acts on: the unit at index `target`, or with none the point `point` (a wall or the ground); a
/// missile aims at `point`, the target's centre when there is a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Aim {
    pub target: Option<usize>,
    pub point: (i32, i32),
}

/// The tile holding map pixel `p`.
fn tile_px(p: (i32, i32)) -> (i32, i32) {
    (p.0.div_euclid(TILE_PX), p.1.div_euclid(TILE_PX))
}

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
    /// missile. Orders spec, "Commands": an attack command on a unit of any owner gives order 9, and
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
        // Orders spec: the order loses a target its owner no longer detects (fog does not matter).
        // ("Checks on an attack command"): the order then checks only that the unit can hit the target's kind, with
        // the submarine rule; there is no owner test. The kind checks in their order, with the
        // transport-on-shore case.
        let owner = self.units[i].owner;
        let target_ok =
            !self.target_lost(owner, target) && self.unit(target).is_some_and(|t| self.can_hit_kind(&self.units[i], t));
        // Orders spec, "Attack orders" (step 5): a target that fails the kind check gives a pending idle, by the common
        // path with the resume step, taken over in the same update as order 11's steps 4.3 and 4.4 do (round 68, code
        // Q3; static study only).
        if !target_ok {
            self.units[i].path.clear();
            self.idle_now(i);
            return;
        }
        // Orders spec, "Friendly-fire check": in order 12 it runs before the range test.
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
        // Orders spec, "No route": order 12 has its own fallback, which goes idle at once only when it finds nothing;
        // order 9 has the plain rule (`orc02-assault` grunt 1573, n = 679).
        let chase = matches!(self.units[i].order, Order::Chase { .. });
        let walk = if chase { self.move_toward(i, g) } else { self.walk(i, g) };
        match walk {
            Walk::NoPath if chase => self.chase_no_route(i),
            // Kept as the code has it: arrived at the strike goal but out of strike reach, the unit goes idle with
            // the still animation (no pending idle, no resume, no idle logic).
            Walk::Arrived => {
                self.go_idle(i);
                self.switch_anim(i, anim::STILL);
            }
            Walk::NoPath | Walk::Stepping | Walk::Blocked => {}
        }
    }

    /// Orders spec, "Order 12 with no route" (`human5-10000` grunt 1531, n = 1759): a search over
    /// the attack range box with no attacker seed. A target found gets order 12 as a command that takes over at once:
    /// its attack animation starts with its first step run in this update, and its next order-logic run skips the
    /// target search. With nothing found the unit goes idle at once. The take-over makes no turn and
    /// no strike decision; order 12's logic then runs once in the same update, skips its search because of the mark,
    /// and its strike decision turns the unit and starts the cooldown.
    fn chase_no_route(&mut self, i: usize) {
        // "Forgetting a dying attacker first": this search too.
        self.forget_dying_attacker(i);
        let Some(target) = self.search_in_range(i) else {
            return self.idle_at_once(i);
        };
        // Step 2: order 12 as a command that takes over at once, as an immediate order (wait 1) whose animation's
        // first step runs now; step 3's mark.
        self.command(i, Order::Chase { target });
        if let Some(o) = self.units[i].pending.take() {
            self.immediately(i, o);
        }
        self.units[i].skip_search = true;
        self.restarts = self.restarts.wrapping_add(1);
        self.switch_anim(i, anim::ATTACK);
        // The one logic run that follows. That run doesn't move the unit, so it can't find
        // "no route" again, and the logic doesn't run a third time in the update (the guard makes that hold).
        if !self.fallback_run {
            self.fallback_run = true;
            self.run_order(i);
            self.fallback_run = false;
        }
    }

    /// The goal of a walk toward an attack target: the target's tile or, for a building, the footprint tile of the
    /// building goal rule, with the attack range as the radius.
    pub(crate) fn strike_goal(&self, u: &Unit, t: &Unit) -> Goal {
        let up = self.players[u.owner as usize].upgrades;
        let reach = combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32;
        // A shore building is aimed at by its shore corner goal ([`World::target_goal`]).
        Goal {
            range: true,
            ..self.target_walk(u, t, reach)
        }
    }

    /// Orders spec, "Attack-move: orders 4, 5 and (Enhanced) 10": a unit in one of these orders
    /// that picked a target heads for it until order 12 takes over at its next step done, stepping even when the
    /// target is already within strike reach. Our reading: the order's own walk (radius 0) carries on toward the goal
    /// the command set, the target's tile; with no route it goes idle as the order does ("No route"), and once there
    /// it waits for the take-over.
    pub(crate) fn head_for(&mut self, i: usize, target: UnitId) {
        let Some(t) = self.unit(target) else { return };
        // Traced (`human06-31357` axe thrower 1512, n = 16277): a building target is headed for as an attack
        // on it would: the footprint tile of the building goal rule, with the building's goal zone (its footprint
        // widened by one tile; pathfinding spec, "Step 1"). A unit target is headed for at its tile
        // (`orc5-40000`): with the target's goal zone too, not the 3 x 3 around its tile.
        // A shore building by its shore corner goal ([`World::target_goal`]).
        let g = self.target_walk(&self.units[i], t, 0);
        // Orders spec, "No route": a unit that goes idle with no route keeps its attacker.
        self.walk(i, g);
    }

    /// Orders spec, "Order 11: attacking a wall tile" (`orc02-defeat-4021` and `orc02-victory-7639`
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
        if self.walk(i, g) != Walk::Arrived {
            return;
        }
        if self.cooling_down(i) {
            return;
        }
        // Static study (step 4.4): the range check is the arrival test above (no +1 with an attack
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

    /// The facing a strike decision gives toward tile `b`: "Facing a tile" of the animation-scripts spec. A
    /// warship (a sea unit that isn't a submarine) turns broadside instead, by the direction to the target
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
        let range = combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32;
        // "Reach" (`orc9-orc09-shore-build-3000` archer 1476, n = 343): a unit that is neither a sea
        // nor an air unit measures its reach to a shore building's shore corner goal.
        if self.aims_at_shore_corner(u, t) {
            return chebyshev(u.tile, self.target_goal(u, t)) <= range;
        }
        gap(u, t) <= range
    }

    /// Whether the cooldown test would restart `u`'s attack animation ([`World::cooling_down`]), without doing it.
    pub(crate) fn still_cooling(&self, u: &Unit) -> bool {
        !u.building && u.layer != Layer::Air && (u.script.wait as u16) < u.cooldown
    }

    /// The strike decision's cooldown test (damage spec, "Attack timing (cooldown)"): a unit that isn't a flyer and
    /// whose wait is less than its cooldown left restarts its attack animation from its first frame and waits for the
    /// cooldown left. Returns whether it did, which ends the run.
    pub(crate) fn cooling_down(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        if !self.still_cooling(u) {
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

    /// The strike point of the attack script (damage spec, "How an attack delivers damage"; spells spec, "The strike
    /// point"): invisibility ends, the strike reveal, then the delivery at whatever the order aims at by then, with no
    /// range test, or the spell's strike for a cast (`spells.rs`).
    pub(crate) fn strike(&mut self, i: usize) {
        // "Invisibility ends" (spells spec, "The strike point", step 2, round 80): before the strike reveal, at every
        // strike point whatever the order and whatever the target's state: a cast that then fails its target or mana
        // check (`xhuman10-failcast-520`, n = 437, S84), a worker's chop and repair strikes
        // (`human13-invisible-work-hits-900`, n = 112 and 446) and an attack on a target inside included.
        self.units[i].invisibility = 0;
        self.strike_reveal(i);
        if let Order::Cast { .. } = self.units[i].order {
            return self.cast_strike(i);
        }
        if let Some(aim) = self.strike_aim(i) {
            self.attack_with(i, aim, Shooter::Script);
        }
    }

    /// What the strike of the unit at `i` acts on: the target unit of its order, or the centre of order 11's or 17's
    /// target tile. None for an order that doesn't strike (a demolisher plays the attack animation on its way but
    /// never strikes, attack-ground-demolish spec). A target that is gone (its slot released) isn't struck and nothing
    /// is drawn (V16, the stated rule held by the open question "A melee strike at a target whose slot is released").
    /// A target that started dying earlier in the cycle is still struck (V15, `human01-basics` n = 576; V17,
    /// `xhuman4-7000` n = 121), and so is a target that has gone inside something: a melee blow is rolled, a missile
    /// is launched at the target's centre with all its draws and hits it, and "Applying damage" does nothing (V93,
    /// `orc09-strike-inside-hits-400` n = 329; V94, `orc09-missile-inside-hits-600` n = 533).
    fn strike_aim(&self, i: usize) -> Option<Aim> {
        let order = self.units[i].order;
        match (order, crate::orders::target_of(order)) {
            (Order::AttackWall { tile } | Order::AttackGround { tile }, _) => Some(Aim {
                target: None,
                point: (tile.0 * TILE_PX + TILE_PX / 2, tile.1 * TILE_PX + TILE_PX / 2),
            }),
            (Order::Follow { .. } | Order::Guard { .. } | Order::Demolish { .. }, _) | (_, None) => None,
            (_, Some(target)) => {
                let ti = self.index(target)?;
                Some(Aim {
                    target: Some(ti),
                    point: self.units[ti].centre_px(),
                })
            }
        }
    }

    /// Vision spec, "When sight is applied", item 4 (`human06-31357` n = 9642): at each strike point
    /// of an attack-type order (9, 12, 15, 16, 17, 20 and 21, and the spells Slow and Haste) whose target unit is
    /// owned by another player, one of players 0 to 7, a sight-2 area centred on the attacker's tile is revealed for
    /// the target's owner and its shared-vision set, before the strike itself (damage spec, "How an attack delivers
    /// damage").
    fn strike_reveal(&mut self, i: usize) {
        let target = match self.units[i].order {
            Order::Attack { target }
            | Order::Chase { target }
            | Order::AttackInPlace { target }
            | Order::IdleReact { target } => Some(target),
            Order::Demolish { at, .. } => at,
            Order::Cast {
                spell: crate::spells::spell::SLOW | crate::spells::spell::HASTE,
                target,
                ..
            } => target,
            // Order 17 (attack the ground) has no target unit.
            _ => None,
        };
        let Some(t) = target.and_then(|t| self.unit(t)) else {
            return;
        };
        let owner = t.owner;
        self.strike_reveal_for(i, owner);
    }

    /// A tower's shot from its order-14 logic at the unit at `ti` (damage spec, "Towers"), aimed at its centre.
    pub(crate) fn tower_shot(&mut self, i: usize, ti: usize) {
        let aim = Aim {
            target: Some(ti),
            point: self.units[ti].centre_px(),
        };
        self.attack_with(i, aim, Shooter::Tower);
    }

    /// The target's armour, with its owner's upgrades (damage spec, "Effective attack values").
    pub(crate) fn armour_of(&self, ti: usize) -> u32 {
        let t = &self.units[ti];
        combat::armour(&self.stats_of(t.kind), &self.players[t.owner as usize].upgrades)
    }

    /// One attack's delivery from the unit at `i` (damage spec, "The model", steps 1 and 3): the attack values, then a
    /// melee blow, or a missile launch for a type with a missile weapon.
    pub(crate) fn attack_with(&mut self, i: usize, aim: Aim, by: Shooter) {
        if by == Shooter::Tower {
            self.tower_fire_sound(i);
        }
        let a = &self.units[i];
        let s = self.stats_of(a.kind);
        let (basic, pierce) = self.attack_values(a, &s);
        // "Damage roll", step 1: with no target unit, armour is taken as 0.
        let armour = aim.target.map_or(0, |ti| self.armour_of(ti));
        if s.missile == missile::NONE {
            // "How an attack delivers damage", melee, and (order 11) melee with no target unit: the wall on the
            // target tile takes the blow.
            let d = combat::roll(&mut self.rng, basic, pierce, armour);
            let target = aim.target.map(|ti| self.units[ti].id);
            let attacker = self.units[i].id;
            return self.hit(target, tile_px(aim.point), d, attacker);
        }
        self.launch(i, aim, by, s.missile, (basic, pierce, armour));
    }

    /// A missile launch ("How an attack delivers damage", ranged): with no effect slot free nothing is created and
    /// nothing is drawn. Otherwise the damage (a roll for a non-splash missile, the maximum damage for a splash one),
    /// then, for a unit's attack script only, the aim scatter, the computer players' wall exception and a cannon's
    /// smoke ("Towers": a tower's shot has none of these).
    fn launch(&mut self, i: usize, aim: Aim, by: Shooter, kind: u8, (basic, pierce, armour): (u32, u32, u32)) {
        let Some(slot) = self.free_fx_slot() else {
            return;
        };
        let min_flight = combat::splash(kind);
        let damage = match min_flight {
            Some(_) => (basic + pierce).min(255) as u8,
            None => combat::roll(&mut self.rng, basic, pierce, armour),
        };
        let script = by == Shooter::Script;
        let a = &self.units[i];
        let (attacker, from, owner) = (a.id, a.centre_px(), a.owner);
        // Our reading: "the attack's target tile" is the tile of the aim point before the scatter.
        let aim_tile = tile_px(aim.point);
        let mut to = aim.point;
        if script {
            to.0 += (self.rng.draw() & 7) as i32 - 3;
            to.1 += (self.rng.draw() & 7) as i32 - 3;
        }
        let mut minimum = min_flight.unwrap_or(0) as i32 * TILE_PX;
        if script
            && minimum > 2 * TILE_PX
            && self.computer_player(owner)
            && self.terrain.word(aim_tile.0, aim_tile.1) & (bits::HUMAN_WALL | bits::COMPUTER_WALL) != 0
        {
            minimum = 2 * TILE_PX;
        }
        let mut line = Line::new(from, to);
        let breath = matches!(kind, missile::GRYPHON_HAMMER | missile::DRAGON_BREATH);
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
            let steps = combat::flight_steps(length, combat::flight(kind).speed);
            (Fx::Shot { line, length }, steps as u16)
        };
        let hits_air = aim.target.is_some_and(|ti| self.units[ti].layer == Layer::Air);
        let missile = Missile {
            kind,
            attacker,
            shooter: self.last_slot(attacker),
            target: aim.target.map(|ti| self.units[ti].id),
            from,
            to,
            pos: from,
            flight,
            elapsed: 0,
            damage,
            splash: min_flight.is_some(),
            hits_air: kind == missile::SMALL_CANNON && hits_air,
            fx,
        };
        // Unit-slots spec, "Effect slots": the lowest free slot after the permanent entries.
        self.missiles[slot] = Some(missile);
        // A unit's cannon shot is followed by smoke (kind 0x19) at the missile's start point, in the next free slot;
        // with none free there is no smoke.
        if script && matches!(kind, missile::BIG_CANNON | missile::SMALL_CANNON) {
            self.cannon_smoke(attacker, from);
        }
    }

    /// A rolled damage reaching one target: the unit `target`, if it is still in the table, or with no target unit the
    /// wall on `tile` (a melee blow of order 11, a non-splash missile aimed at a tile, a hammer's final hit with no
    /// target; damage spec, "How an attack delivers damage" and "Missile flight").
    fn hit(&mut self, target: Option<UnitId>, tile: (i32, i32), d: u8, attacker: UnitId) {
        match target {
            Some(t) => {
                if let Some(ti) = self.index(t) {
                    self.damage(ti, d, attacker);
                }
            }
            None => self.hit_wall(tile, d),
        }
    }

    /// The one apply-damage step (damage spec, "The model", step 4, and "Applying damage"): every delivery's damage to a
    /// unit from `attacker` comes here. In order: a dying target or the Dark Portal case takes nothing; a target with
    /// the used mark or inside something gets no notification and no damage; the notification ("Call for help"); no
    /// damage under Unholy Armor or for 0; the HP lowered, or at or below the damage, the kill step.
    pub(crate) fn damage(&mut self, ti: usize, d: u8, attacker: UnitId) {
        if self.units[ti].dying || self.portal_ignores(ti, attacker) {
            return;
        }
        // Damage spec: a target with the used mark gets no notification and no damage.
        let t = &self.units[ti];
        if t.used || t.inside {
            return;
        }
        // The notification: the computer player's damage hook (`ai/computer-player.md`; attack-ai spec, "Call for
        // help", which keeps the movement spec's building rule).
        self.damage_hook(ti, attacker);
        let t = &mut self.units[ti];
        if t.unholy_armor > 0 || d == 0 {
            return;
        }
        if t.hp > d as u16 {
            t.hp -= d as u16;
            self.under_attack(ti);
            return;
        }
        self.kill_by(ti, attacker);
    }

    /// The one kill step (damage spec, "The model", step 5, and "A kill"): the credit to the attacker's player, if the
    /// attacker is still in the unit table; then the death; then the killer forgets. The killing blow doesn't lower
    /// the HP, and the victim keeps its target ("Applying damage").
    fn kill_by(&mut self, ti: usize, attacker: UnitId) {
        if let Some(a) = self.unit(attacker) {
            self.credit_kill(a.owner, ti);
        }
        self.kill(ti);
        self.forget_kill(attacker);
    }

    /// Damage spec, "A kill": a unit (not a building) that made a kill checks its own records at once,
    /// without changing its order: it forgets its target, and its recorded attacker, if that unit is now dying, gone
    /// or not detected by its owner. The victim is dying, so it is always forgotten when it is either. Other
    /// attackers keep their target until their own next logic run. Our orders hold their target, so the forgotten
    /// one is marked: while the order is unchanged, it has no target. The mark is what tells the killer, which reports
    /// no target (V13), from another attacker of the same victim, which still reports it until its own logic runs.
    ///
    /// Only the killer forgets the victim as its attacker at once (movement spec, "Blocked rules");
    /// every other unit keeps that record while the victim is dying. This applies to every kill,
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
        // Traced (`human13-subs-2000` dragon 1436, n = 1487): a shooter that is dying or dead, its missile
        // still in flight, checks the target it kept and reports none from then on.
        if u.dying {
            if let Some(slot) = u.dying_target
                && self.unit_in_slot(slot).is_none_or(|t| self.stale(owner, t.id))
            {
                self.units[a].dying_target = None;
            }
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
    fn credit_kill(&mut self, killer: u8, ti: usize) {
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

    /// Whether `u` is in its owner's per-type count, which the end-of-game statistics read (campaign-flow spec,
    /// "Statistics", traced in round 79): only completed units and buildings, and not circles of power, runestones,
    /// gold mines, oil patches or wall pieces, which have no count. So a completed wall piece, killed as it becomes
    /// terrain, is neither built nor lost (walls spec, "Completion: the piece becomes terrain").
    pub fn counted(u: &Unit) -> bool {
        use crate::setup::kind;
        const RUNESTONE: u8 = 0x66;
        u.construction.is_none()
            && !matches!(
                u.kind,
                kind::CIRCLE_OF_POWER | RUNESTONE | kind::GOLD_MINE | kind::OIL_PATCH | kind::WALL | kind::ORC_WALL
            )
    }

    /// Campaign-flow spec, "Statistics": the owner's count of units or of buildings went down by one, if `u` was in a
    /// count at all ([`World::counted`]).
    pub(crate) fn count_lost(&mut self, owner: u8, u: &Unit) {
        if !Self::counted(u) {
            return;
        }
        let p = &mut self.players[owner as usize];
        if u.building {
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
        // Unit-slots spec, "Death, corpses and release" (`human5` barracks 1529 at n = 1595): a
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
        // Build-train spec, "A building killed during a job" (`human5-10000` great hall 1525 at
        // n = 8086): the job is cancelled at the kill, with the cancel column's full refund, and the progress counter
        // and mana byte go to 0.
        if self.units[ti].building {
            self.cancel_job(ti);
        }
        let unit = self.units[ti].clone();
        if !self.loading {
            self.count_lost(unit.owner, &unit);
        }
        // Build-train spec, "A building killed": a site leaves the under-construction count. (The job, if any, was
        // cancelled just above, which put the training count back.)
        if unit.construction.is_some() {
            self.players[unit.owner as usize].under_construction -= 1;
        }
        self.units[ti].construction = None;
        // A building's footprint stays marked until its death animation ends ([`World::death_done`]).
        if !unit.inside && !unit.building {
            self.mark(&unit, unit.tile, None);
        }
        if unit.building {
            self.kill_inside(ti);
        }
        // A computer worker's job is released when it dies (worker-ai spec).
        self.release_job(ti);
        // A woodcutter killed while chopping frees the tree it reserved, as every other way of leaving the chopping
        // state does (harvest spec, "Lumber"): the order is cleared just below, so this is the last chance.
        self.release_tree(ti);
        // Unit-slots spec ("Death"; `xhuman12-10000` grunt 1504, n = 4851): the unit keeps the target
        // recorded at this moment, so a killer that forgot its target dies with none.
        let target = self.units[ti].recorded_target();
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
        // Right after the unit starts dying, every player's control groups are tidied (Enhanced only).
        self.tidy_all_groups();
        if self.kill_remains(ti) {
            self.kill_passengers(ti);
        }
        self.leave_oil(ti);
        // `ai/computer-player.md`, "The parts": the destroyed hook runs inside the kill, so the losses rule ("Losses
        // rule" there; `ai/worker-ai.md`, "The losses rule") counts the buildings at the kill, this dying one
        // included (traced: `xorc10-xorc10-losses-4000`, CP1 and CP2).
        if unit.building {
            self.destroyed_hook(ti, unit.construction.is_some());
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
            // One cycle after the end step the stored damage lands, as for a non-splash missile. Traced
            // (`xorc11-16000` n = 15733): the slot stays in use through the final hit and the effects it makes, such
            // as a killed unit's death explosion, and is freed last, as for every other missile ("Impact effects").
            self.land(m);
            self.missiles[n] = None;
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
        let (pos, damage, attacker) = (m.pos, m.damage, self.effect_shooter(&m));
        self.missiles[n] = Some(m);
        // Damage spec: a splash needs a free effect slot; with none, no splash, sound or effect.
        // Otherwise the explosion sound, the splash, then a cosmetic explosion (kind 0x17) in the lowest free slot.
        // The slot is taken before the splash's damage, so a killed unit's explosion comes later.
        // The explosion changes nothing the splash reads, so it is written into the slot at once.
        if splash && self.free_fx_slot().is_some() {
            self.sound_at_px(pos, crate::sound::Sound::Explode);
            self.splash_explosion(pos);
            self.splash(pos, damage, attacker, Splash::Path);
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
        // A non-splash missile (and a hammer's or breath's final hit) carries its stored damage to the unit that was
        // the target at launch, wherever it is, or to the wall on its final tile; a splash missile splashes.
        // "A missile's shooter is its slot" (V98): the attacker is whatever unit holds the shooter's slot now.
        let attacker = self.effect_shooter(&m);
        if m.splash {
            self.splash(m.pos, m.damage, attacker, Splash::Impact { air: m.hits_air });
        } else {
            self.hit(m.target, tile_px(m.pos), m.damage, attacker);
        }
        self.impact_fx(m.kind, m.attacker, m.pos);
    }

    /// The splash delivery (damage spec, "Splash on impact"): stored damage `stored` at pixel `pos` from `attacker`.
    /// The walls on the tile and its four orthogonal neighbours, then each unit considered in scan order (the
    /// ground-layer unit on the tile, then the 7 × 7 block row by row, a tile's ground-layer unit before its air-layer
    /// unit), each once, never the attacker, friends included. Hammer, breath and spell splashes take the same order. A dying unit is off the occupancy marks, so the scan never finds one.
    pub(crate) fn splash(&mut self, pos: (i32, i32), stored: u8, attacker: UnitId, how: Splash) {
        let tile = tile_px(pos);
        self.hit_wall(tile, stored);
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.hit_wall((tile.0 + dx, tile.1 + dy), stored / 4);
        }
        let air = match how {
            Splash::Impact { air } => air,
            Splash::Path => true,
        };
        let mut hit: Vec<UnitId> = Vec::new();
        let mut consider = |w: &World, l: Layer, x: i32, y: i32| {
            if let Some(id) = w.occupant(l, x, y)
                && id != attacker
                && !hit.contains(&id)
            {
                hit.push(id);
            }
        };
        // Step 2 (V95, `human13-splash-air-hits-1700` n = 949): only the impact tile's ground-layer unit comes first;
        // its air-layer unit is reached in its tile's place in the scan.
        consider(self, Layer::Land, tile.0, tile.1);
        for y in tile.1 - 3..=tile.1 + 3 {
            for x in tile.0 - 3..=tile.0 + 3 {
                consider(self, Layer::Land, x, y);
                if air {
                    consider(self, Layer::Air, x, y);
                }
            }
        }
        for id in hit {
            let Some(ti) = self.index(id) else { continue };
            let t = &self.units[ti];
            let c = t.centre_px();
            let d = (c.0 - pos.0).abs().max((c.1 - pos.1).abs());
            let dmg = match how {
                Splash::Impact { .. } => {
                    let armour = combat::armour(&self.stats_of(t.kind), &self.players[t.owner as usize].upgrades);
                    combat::splash_roll(&mut self.rng, stored, d, armour)
                }
                Splash::Path => combat::path_splash_roll(&mut self.rng, stored, d),
            };
            if let Some(dmg) = dmg {
                self.damage(ti, dmg, attacker);
            }
        }
    }

    /// A damage reaching the wall on `tile` by the wall rule of damage spec, "Splash on impact": order 11's melee blow
    /// on its (pulled-back) goal tile, rolled with armour 0 ("How an attack delivers damage", melee with no target
    /// unit; V90), or a missile or hammer hit with no target unit.
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

    /// Animation-scripts spec, the broadside table: a warship facing north (0) with its target due
    /// south (4) turns east (2); one facing east with its target due north keeps facing east; one facing south-west
    /// (5) with its target due east (2) turns south (4) rather than turning across.
    #[test]
    fn warships_turn_broadside_to_strike() {
        assert_eq!(super::broadside(4, 0), 2);
        assert_eq!(super::broadside(0, 2), 2);
        assert_eq!(super::broadside(2, 5), 4);
    }

    /// Orders spec, "Order 11: attacking a wall tile": the goal is pulled back to the wall tile
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

    /// Orders spec, "Commands" (`orc02-defeat-4021` n = 3602 and `orc02-dyingcmd-3760` n = 3723):
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

    /// Campaign-flow spec, "Statistics" (B-040 audit): only completed units and buildings of a type with a count are
    /// lost. A site killed before it is completed, a gold mine, an oil patch, a circle of power, a runestone and a wall
    /// piece are not.
    #[test]
    fn only_counted_buildings_are_lost() {
        for (k, site, lost) in [
            (0x3a, false, 1),
            (0x3a, true, 0),
            (0x5c, false, 0),
            (0x5d, false, 0),
            (0x64, false, 0),
            (0x66, false, 0),
            (0x67, false, 0),
            (0x68, false, 0),
        ] {
            let mut w = world();
            let b = w.spawn(k, 0, (5, 5), 0).unwrap();
            let i = w.index(b).unwrap();
            w.units[i].building = true;
            w.units[i].hp = 10;
            if site {
                w.units[i].construction = Some(Default::default());
                w.players[0].under_construction = 1;
            }
            w.damage(i, 255, UnitId(1));
            assert!(w.units[i].dying);
            assert_eq!(w.players[0].buildings_lost, lost, "type {k:#x}, site {site}");
        }
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

    /// Unit-slots spec, "Death, corpses and release": a destroyed building makes an explosion
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

    /// Damage spec "Applying damage" (`xhuman12-10000` grunt 1504, n = 4851): a killer that forgot
    /// its target and dies before losing it dies with no target; the other attacker keeps its target in death.
    #[test]
    fn a_killer_that_forgot_its_target_dies_with_none() {
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
        w.units[k].order = Order::Chase { target: victim };
        w.units[o].order = Order::Chase { target: victim };
        w.damage(v, 5, killer);
        assert_eq!(w.units[k].forgot, Some(Order::Chase { target: victim }));
        let victim_slot = w.units[v].slot;
        w.kill(k);
        w.kill(o);
        assert_eq!((w.units[k].dying, w.units[k].dying_target), (true, None));
        assert_eq!(w.units[o].dying_target, Some(victim_slot));
    }

    /// Damage spec "Applying damage" (`human13-subs-2000` dragon 1436, n = 1487): a shooter that is
    /// already dying when its missile kills still checks the target it kept and reports none from then on; another
    /// dying unit that had the same target keeps it.
    #[test]
    fn a_dying_shooters_kill_clears_the_target_it_kept() {
        let mut w = world();
        let shooter = UnitId(1);
        let victim = w.spawn(0, 1, (1, 0), 0).unwrap();
        let other = w.spawn(0, 0, (1, 1), 0).unwrap();
        let (k, v, o) = (
            w.index(shooter).unwrap(),
            w.index(victim).unwrap(),
            w.index(other).unwrap(),
        );
        w.units[k].order = Order::Chase { target: victim };
        w.units[o].order = Order::Chase { target: victim };
        w.kill(k);
        w.kill(o);
        let victim_slot = w.units[v].slot;
        assert_eq!(w.units[k].dying_target, Some(victim_slot));
        w.units[v].hp = 5;
        w.damage(v, 5, shooter);
        assert!(w.units[v].dying);
        assert_eq!(w.units[k].dying_target, None);
        assert_eq!(w.units[o].dying_target, Some(victim_slot));
    }

    /// Damage spec, "Applying damage" (`xhuman10-blizzard-600` vector): a kill of a unit other than
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

    /// Orders spec, "Commands forget a stale attacker" (`xhuman10-spells-700` knight 1480): a
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

    /// Damage spec, "How an attack delivers damage": a melee strike at a target that is already
    /// dying is still made and rolls its damage, with no effect; so is one at a target inside something (V93).
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
        let foe = w.spawn(0, 1, (1, 1), 0).unwrap();
        w.units[i].order = Order::Attack { target: foe };
        let f = w.index(foe).unwrap();
        w.units[f].inside = true;
        let (before, hp) = (w.rng, w.unit(foe).unwrap().hp);
        w.strike(i);
        assert_eq!(drawn(before, w.rng), 1, "V93: the damage roll at a target inside");
        assert_eq!(w.unit(foe).unwrap().hp, hp, "and no damage");
    }

    /// Damage spec, "How an attack delivers damage", "Invisibility ends"; spells spec, "Timers": every strike point
    /// ends the striker's invisibility, a strike at a target inside and a worker's chop or repair strike included.
    #[test]
    fn every_strike_point_ends_invisibility() {
        let mut w = world();
        let foe = w.spawn(0, 1, (1, 0), 0).unwrap();
        let f = w.index(foe).unwrap();
        w.units[f].inside = true;
        let i = w.index(UnitId(1)).unwrap();
        for order in [
            Order::Attack { target: foe },
            Order::Harvest {
                source: None,
                tree: (4, 0),
                chopping: true,
            },
            Order::Repair { target: foe },
        ] {
            w.units[i].order = order;
            w.units[i].invisibility = 100;
            w.strike(i);
            assert_eq!(w.units[i].invisibility, 0, "{order:?}");
        }
    }

    /// Damage spec, "How an attack delivers damage", ranged (V94): an archer's strike at a target that has gone
    /// inside launches its arrow at the target's centre, with the roll and both scatter draws; it hits and does
    /// nothing.
    #[test]
    fn a_launch_at_a_target_inside_flies_and_does_nothing() {
        let mut w = world();
        let a = w.spawn(8, 0, (2, 7), 0).unwrap();
        let t = w.spawn(0, 1, (5, 7), 0).unwrap();
        let ti = w.index(t).unwrap();
        w.units[ti].inside = true;
        let ai = w.index(a).unwrap();
        w.units[ai].order = Order::Attack { target: t };
        let before = w.rng;
        w.strike(ai);
        assert_eq!(drawn(before, w.rng), 3, "the roll and the two scatter draws");
        let m = live(&w).expect("launched");
        assert_eq!(m.target, Some(t));
        let c = w.unit(t).unwrap().centre_px();
        assert!(
            (m.to.0 - c.0).abs() <= 4 && (m.to.1 - c.1).abs() <= 4,
            "aimed at the centre"
        );
        let hp = w.unit(t).unwrap().hp;
        fly(&mut w, 20);
        assert!(live(&w).is_none(), "it landed");
        assert_eq!(w.unit(t).unwrap().hp, hp);
    }

    /// Damage spec ("A command waiting"; animation-scripts spec, "No strike while a command waits"): a
    /// footman at its strike point that was given a stop command earlier in the cycle strikes nothing: no damage, no
    /// draw, no new cooldown. Once the command has taken over, a strike works again.
    #[test]
    fn no_strike_while_a_command_waits() {
        let mut w = world();
        let foe = w.spawn(0, 1, (1, 0), 0).unwrap();
        let i = w.index(UnitId(1)).unwrap();
        w.units[i].order = Order::Attack { target: foe };
        w.units[i].script.anim = anim::ATTACK;
        w.command(i, Order::Idle);
        assert!(
            w.units[i].commanded && w.units[i].pending.is_some(),
            "the stop waits to take over"
        );
        let (before, hp, cooldown) = (w.rng, w.unit(foe).unwrap().hp, w.units[i].cooldown);
        let fx = crate::anim::Effects {
            strikes: 1,
            ..Default::default()
        };
        w.order(i, fx);
        assert_eq!(w.rng, before, "no draw");
        assert_eq!(w.unit(foe).unwrap().hp, hp, "no damage");
        assert_eq!(w.units[i].cooldown, cooldown);
        // Without the waiting command the same strike rolls its damage.
        w.units[i].pending = None;
        w.units[i].commanded = false;
        w.order(i, fx);
        assert_eq!(drawn(before, w.rng), 1, "the damage roll");
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

    /// Damage spec, "Splash on impact", step 2 (V95): the impact tile's ground-layer unit is first, and its air-layer
    /// unit comes in its tile's place in the scan, after the unit on the tile to its left. Path splashes alike.
    #[test]
    fn an_air_unit_on_the_impact_tile_is_hit_in_its_scan_place() {
        for how in [super::Splash::Impact { air: true }, super::Splash::Path] {
            let mut w = field(16, 16);
            w.stats.0[0].armor = 0;
            w.stats.0[2].armor = 0;
            let shooter = w.spawn(0, 0, (0, 0), 0).unwrap();
            let on_tile = w.spawn(0, 1, (8, 8), 0).unwrap();
            let left = w.spawn(0, 1, (7, 8), 0).unwrap();
            let air = w.spawn(2, 1, (8, 8), 0).unwrap();
            let centre = w.unit(on_tile).unwrap().centre_px();
            let mut rng = w.rng;
            let (r1, r2, r3) = (rng.draw(), rng.draw(), rng.draw());
            w.splash(centre, 40, shooter, how);
            // Armour 0: D = 40 within 22 px (h = 20), D / 4 = 10 at 32 px (h = 5).
            let hp = |id| w.unit(id).unwrap().hp as u32;
            assert_eq!(
                hp(on_tile),
                60 - (20 + r1 % 21),
                "{how:?}: first, the ground unit on the tile"
            );
            assert_eq!(hp(left), 60 - (5 + r2 % 6), "{how:?}: then the scan, row by row");
            assert_eq!(
                hp(air),
                100 - (20 + r3 % 21),
                "{how:?}: the air unit in its tile's place"
            );
        }
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
        // Damage spec, "Towers": a tower type shoots from its order logic, not from a strike point.
        if w.stats_of(w.units[i].kind).flags & flags::TOWER != 0 {
            let aim = w.strike_aim(i).unwrap();
            w.attack_with(i, aim, super::Shooter::Tower);
        } else {
            w.strike(i);
        }
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

    /// Damage spec, "Missile flight": each hammer splash with a free effect slot plays the
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

    /// Damage spec, "Missile flight" (`xorc8-10000` n = 8035): the splash takes its effect slot
    /// before its damage, so the death explosion of a unit it kills comes in the next slot.
    #[test]
    fn a_hammer_splash_takes_its_slot_before_a_kills_explosion() {
        let mut w = range();
        w.stats.0[FOOTMAN as usize].flags |= flags::EXPLODE_WHEN_KILLED;
        let gryphon = spawn_at(&mut w, GRYPHON, 0, (0, 7), (16, 240));
        let a = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        launch(&mut w, gryphon, Some(a), (0, 0));
        let i = w.index(a).unwrap();
        w.units[i].hp = 1;
        let h = crate::slots::PERMANENT_EFFECTS;
        fly(&mut w, 11);
        fly(&mut w, 1);
        let pos = w.missiles[h].as_ref().unwrap().pos;
        assert!(w.unit(a).unwrap().dying, "the splash kills A");
        let splash = w.missiles[h + 1].as_ref().expect("the splash's explosion");
        assert_eq!((splash.kind, splash.pos), (0x17, pos));
        let c = w.unit(a).unwrap().centre_px();
        let death = w.missiles[h + 2].as_ref().expect("A's explosion");
        assert_eq!((death.kind, death.pos), (0x17, (c.0, c.1 - 8)));
    }

    /// Damage spec, "Missile flight" (the `xorc11-16000` gryphon rider 1490 vector, n = 15733): the
    /// hammer keeps its effect slot through its final hit, so the death explosion of the unit that hit kills never
    /// reuses the hammer's slot; the slot is free at the end of that cycle.
    #[test]
    fn a_hammer_keeps_its_slot_through_its_final_hit() {
        let mut w = range();
        w.stats.0[FOOTMAN as usize].flags |= flags::EXPLODE_WHEN_KILLED;
        let gryphon = spawn_at(&mut w, GRYPHON, 0, (0, 7), (16, 240));
        let a = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
        launch(&mut w, gryphon, Some(a), (0, 0));
        let h = crate::slots::PERMANENT_EFFECTS;
        fly(&mut w, 31);
        assert!(w.missiles[h].is_some(), "the end step is C + 30");
        // Clear every other effect, so the hammer's is the lowest slot in use and the lowest free one is above it.
        for n in h + 1..w.missiles.len() {
            w.missiles[n] = None;
        }
        let i = w.index(a).unwrap();
        w.units[i].hp = 1;
        fly(&mut w, 1);
        assert!(w.unit(a).unwrap().dying, "the final hit kills A");
        assert!(w.missiles[h].is_none(), "the hammer's slot is freed last");
        let c = w.unit(a).unwrap().centre_px();
        let death = w.missiles[h + 1..]
            .iter()
            .flatten()
            .find(|m| m.kind == 0x17 && m.pos == (c.0, c.1 - 8));
        assert!(death.is_some(), "A's explosion, above the hammer's slot");
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
            shooter: None,
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

    /// Damage spec V98: a missile's shooter is its slot. A rock whose shooter's slot is released and then taken by
    /// another player's unit before impact spares that unit, credits its owner with the kill and leaves it as the
    /// attacker on record; the target stays the unit that was the target at launch.
    #[test]
    fn a_missiles_shooter_is_whatever_unit_holds_its_slot_at_impact() {
        let mut w = range();
        let old = spawn_at(&mut w, BALLISTA, 0, (1, 1), (48, 48));
        let slot = w.unit(old).unwrap().slot;
        let i = w.index(old).unwrap();
        w.kill(i);
        for _ in 0..5000 {
            if w.unit(old).is_none() {
                break;
            }
            w.step(&[]);
        }
        assert!(w.unit(old).is_none(), "the slot is released");
        let new = spawn_at(&mut w, FOOTMAN, 2, (6, 7), (208, 240));
        assert_eq!(w.unit(new).unwrap().slot, slot, "reused");
        let foe = spawn_at(&mut w, FOOTMAN, 1, (5, 7), (176, 240));
        let tough = spawn_at(&mut w, FOOTMAN, 1, (5, 8), (176, 256));
        let (fi, ti) = (w.index(foe).unwrap(), w.index(tough).unwrap());
        w.units[fi].hp = 1;
        w.units[ti].hp = 1000;
        let mut rock = landing(missile::CATAPULT, 20, (176, 240));
        rock.attacker = old;
        rock.shooter = Some(slot);
        w.missiles[3] = Some(rock);
        update_cues(&mut w);
        assert_eq!(w.unit(new).unwrap().hp, 60, "the slot's new unit is spared");
        assert!(w.unit(foe).unwrap().dying);
        assert_eq!(
            (w.players[2].kills, w.players[0].kills),
            (1, 0),
            "the slot's new owner is credited"
        );
        assert!(w.unit(tough).unwrap().hp < 1000);
        assert_eq!(w.unit(tough).unwrap().attacked_by, Some(new));
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

    /// Pins of the current combat behaviour, written before the B-038 refactor onto `docs/specs/combat/damage.md` so
    /// that it can't change them unseen. Each pin is the whole simulation state ([`World::fingerprint`]) with the
    /// queued events and sound cues; a trail folds the pin of every cycle of a run. Where the code differs from the
    /// spec's wording the test says so: those are questions, not rules.
    mod pin {
        use super::*;
        use crate::world::tests::world;

        fn fnv(h: u64, bytes: &[u8]) -> u64 {
            bytes
                .iter()
                .fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
        }

        /// The whole state, the queued events and the sound cues, as one number.
        fn pinned(w: &World) -> u64 {
            let s = format!("{}|{:?}|{:?}", w.fingerprint(), w.events, w.sounds.cues);
            fnv(0xcbf2_9ce4_8422_2325, s.as_bytes())
        }

        /// Steps `n` cycles, folding the pin of every cycle.
        fn trail(w: &mut World, n: u32) -> u64 {
            let mut h = 0xcbf2_9ce4_8422_2325;
            for _ in 0..n {
                w.step(&[]);
                h = fnv(h, &pinned(w).to_le_bytes());
            }
            h
        }

        /// Updates the effect table `n` times, folding the pin after each update.
        fn flight(w: &mut World, n: u32) -> u64 {
            let mut h = 0xcbf2_9ce4_8422_2325;
            for _ in 0..n {
                w.advance_missiles();
                h = fnv(h, &pinned(w).to_le_bytes());
            }
            h
        }

        /// [`ships`] with a human player 0 and a computer player 1, an unarmed footman copy (type 1), a building
        /// tower (type 0x6d), sight and reaction ranges, in either rules mode.
        fn arena(enhanced: bool) -> World {
            let mut w = ships();
            w.rules.enhanced = enhanced;
            w.players[0].controller = controller::HUMAN;
            w.players[1].controller = controller::COMPUTER;
            w.stats.0[1] = w.stats.0[FOOTMAN as usize];
            w.stats.0[1].flags &= !flags::CAN_ATTACK;
            w.stats.0[0x6d] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING | flags::TOWER | flags::CAN_ATTACK,
                basic_damage: 4,
                piercing_damage: 12,
                attack_range: 6,
                missile: missile::ARROW,
                can_target: 7,
                ..Default::default()
            };
            for s in w.stats.0.iter_mut() {
                s.sight = 8;
                s.react_range_computer = 6;
                s.react_range_human = 4;
            }
            w
        }

        fn both_modes(f: impl Fn(bool) -> u64) -> Vec<u64> {
            [true, false].into_iter().map(f).collect()
        }

        /// V13, V14: a melee kill with a second attacker on the same victim, through to both attackers' next runs.
        #[test]
        fn a_melee_kill_with_a_second_attacker() {
            let got = both_modes(|enhanced| {
                let mut w = arena(enhanced);
                let a = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
                let b = w.spawn(FOOTMAN, 0, (5, 7), 0).unwrap();
                let v = w.spawn(1, 1, (5, 6), 0).unwrap();
                let vi = w.index(v).unwrap();
                w.units[vi].hp = 12;
                attack(&mut w, 0, vec![a, b], v);
                trail(&mut w, 400)
            });
            assert_eq!(got, [17562802122257389236, 14199208771365056885]);
        }

        /// Two armed footmen fighting to the death, and an archer duel, in both rules modes.
        #[test]
        fn fights_to_the_death() {
            let got = both_modes(|enhanced| {
                let mut w = arena(enhanced);
                let a = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
                let b = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
                let c = w.spawn(ARCHER, 0, (2, 12), 0).unwrap();
                let d = w.spawn(ARCHER, 1, (7, 12), 0).unwrap();
                attack(&mut w, 0, vec![a], b);
                attack(&mut w, 0, vec![c], d);
                trail(&mut w, 900)
            });
            assert_eq!(got, [316028836331651547, 2371961271598074126]);
        }

        /// V86 to V89: a hasted and a slowed footman in steady fire.
        #[test]
        fn hasted_and_slowed_steady_fire() {
            let mut got = Vec::new();
            for haste in [400i16, -400] {
                let mut w = arena(true);
                let a = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
                let v = w.spawn(1, 1, (6, 5), 0).unwrap();
                let ai = w.index(a).unwrap();
                w.units[ai].haste = haste;
                attack(&mut w, 0, vec![a], v);
                got.push(trail(&mut w, 300));
            }
            assert_eq!(got, [15024960582255501615, 4995403519126401575]);
        }

        /// S1 and V90: order 11's melee blows on the pulled-back wall tile, with armour 0, until the wall is gone.
        #[test]
        fn melee_blows_on_a_wall() {
            let got = both_modes(|enhanced| {
                let mut w = arena(enhanced);
                for y in 3..=5 {
                    w.terrain.build_wall(8, y, false, true);
                }
                let f = w.spawn(FOOTMAN, 0, (8, 10), 0).unwrap();
                let i = w.index(f).unwrap();
                w.command(i, Order::AttackWall { tile: (8, 3) });
                trail(&mut w, 1500)
            });
            assert_eq!(got, [10362497382142069342, 6889069925561560025]);
        }

        /// V15, V16, V93, V94: strikes at a dying, a gone and an inside target, melee and ranged. A strike at an
        /// inside target rolls its melee damage (V93) or launches its missile (V94); these two pins changed with
        /// those rules. Code versus spec: a ranged strike at a target that is gone (its slot released) launches
        /// nothing, where the spec's launch "has no test of the target's state" (a question, not a rule).
        #[test]
        fn strikes_at_dying_gone_and_inside_targets() {
            let mut got = Vec::new();
            for kind in [FOOTMAN, ARCHER] {
                for case in 0..3 {
                    let mut w = arena(true);
                    let a = spawn_at(&mut w, kind, 0, (2, 7), (80, 240));
                    let t = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
                    let ai = w.index(a).unwrap();
                    w.units[ai].order = Order::Attack { target: t };
                    let ti = w.index(t).unwrap();
                    match case {
                        0 => w.kill(ti),
                        1 => {
                            w.kill(ti);
                            while w.unit(t).is_some() {
                                w.step(&[]);
                            }
                        }
                        _ => w.units[ti].inside = true,
                    }
                    let ai = w.index(a).unwrap();
                    w.units[ai].order = Order::Attack { target: t };
                    w.strike(ai);
                    got.push(pinned(&w));
                    got.push(flight(&mut w, 12));
                }
            }
            assert_eq!(
                got,
                [
                    10445388388357178190,
                    13944816926384618541,
                    6915178903912632394,
                    12481276723132371829,
                    3897332728781154625,
                    212718408572756349,
                    10758045845679428953,
                    3110147901919634608,
                    13391655594128045329,
                    15591369548482294925,
                    4000152437018133909,
                    10341829905799052527
                ]
            );
        }

        /// V53: a shooter killed while its arrow flies; the arrow's kill makes it forget its target.
        #[test]
        fn a_dead_shooters_arrow_kills() {
            let mut w = arena(true);
            let a = spawn_at(&mut w, ARCHER, 0, (0, 7), (16, 240));
            let t = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
            let ti = w.index(t).unwrap();
            w.units[ti].hp = 1;
            launch(&mut w, a, Some(t), (0, 0));
            let ai = w.index(a).unwrap();
            w.kill(ai);
            let got = [flight(&mut w, 12), pinned(&w)];
            assert_eq!(w.unit(a).unwrap().dying_target, None);
            assert_eq!(got, [2050617287154097400, 3003025494938653168]);
        }

        /// V72, V91: a ballista keeping a target that died from another unit's kill; its own bolt's splash kills a
        /// unit of its own player. No credit; the ballista forgets its dying target; then its next run.
        #[test]
        fn a_splash_kills_an_own_unit() {
            let mut w = arena(true);
            let b = spawn_at(&mut w, BALLISTA, 0, (0, 7), (16, 240));
            let t = spawn_at(&mut w, FOOTMAN, 1, (6, 7), (208, 240));
            let own = spawn_at(&mut w, FOOTMAN, 0, (6, 8), (208, 260));
            let oi = w.index(own).unwrap();
            w.units[oi].hp = 5;
            let bi = w.index(b).unwrap();
            w.units[bi].order = Order::AttackInPlace { target: t };
            w.units[bi].cooldown = 100;
            w.rng = no_scatter(0);
            w.strike(bi);
            let ti = w.index(t).unwrap();
            w.kill(ti);
            let mut got = vec![flight(&mut w, 32)];
            assert!(w.unit(own).unwrap().dying, "the splash kills its own unit");
            let bi = w.index(b).unwrap();
            assert_eq!(w.units[bi].forgot, Some(Order::AttackInPlace { target: t }));
            assert_eq!(w.players[0].kills, 0);
            w.units[bi].script.wait = 1;
            w.run_order(bi);
            got.push(pinned(&w));
            assert_eq!(got, [3256194767279609934, 4435873144727676853]);
        }

        /// V73 to V75: a killer whose target is the victim, and one whose live target isn't, each dying afterwards.
        #[test]
        fn killers_forget_and_die() {
            let mut got = Vec::new();
            for victim_is_target in [true, false] {
                let mut w = world();
                let killer = UnitId(1);
                let victim = w.spawn(0, 1, (1, 0), 0).unwrap();
                let other = w.spawn(0, 1, (1, 1), 0).unwrap();
                let (k, v) = (w.index(killer).unwrap(), w.index(victim).unwrap());
                w.units[v].hp = 5;
                let target = if victim_is_target { victim } else { other };
                w.units[k].order = Order::Chase { target };
                w.units[k].attacked_by = Some(victim);
                w.damage(v, 5, killer);
                got.push(pinned(&w));
                let k = w.index(killer).unwrap();
                w.kill(k);
                got.push(trail(&mut w, 20));
            }
            assert_eq!(
                got,
                [
                    6802127008405824141,
                    9559797479724776679,
                    5681946864872518324,
                    4580409190245841851
                ]
            );
        }

        /// V41 to V49 and V85: a gryphon hammer's whole path, its splashes and its final killing hit; and the same
        /// with no free effect slot at its splashes (V50).
        #[test]
        fn a_hammers_path_and_final_hit() {
            let mut got = Vec::new();
            for full in [false, true] {
                let mut w = arena(true);
                let g = spawn_at(&mut w, GRYPHON, 0, (0, 7), (16, 240));
                let a = spawn_at(&mut w, FOOTMAN, 1, (3, 7), (112, 240));
                spawn_at(&mut w, FOOTMAN, 1, (3, 8), (112, 260));
                spawn_at(&mut w, FOOTMAN, 0, (4, 7), (150, 240));
                let ai = w.index(a).unwrap();
                w.units[ai].hp = 14;
                w.rng = seed_with(&[(0, &|r| r % 9 == 4), (1, &|r| r & 7 == 3), (2, &|r| r & 7 == 3)]);
                launch(&mut w, g, Some(a), (0, 0));
                if full {
                    fill_except(&mut w, &[crate::slots::PERMANENT_EFFECTS]);
                }
                got.push(flight(&mut w, 40));
            }
            assert_eq!(got, [14144116741604484075, 16126167599969600505]);
        }

        /// The splash rule of path and spell effects (armour ignored, air units scanned, the owner spared), at
        /// several distances, with a kill; stored damage 40 and 255.
        #[test]
        fn a_path_splash() {
            let mut got = Vec::new();
            for stored in [40u8, 255] {
                let mut w = arena(true);
                let owner = spawn_at(&mut w, FOOTMAN, 0, (5, 5), (176, 176));
                spawn_at(&mut w, FOOTMAN, 1, (5, 6), (176, 200));
                let weak = spawn_at(&mut w, FOOTMAN, 1, (6, 5), (206, 176));
                spawn_at(&mut w, FOOTMAN, 1, (7, 5), (226, 176));
                let air = w.spawn(2, 1, (5, 5), 0).unwrap();
                let ai = w.index(air).unwrap();
                w.units[ai].pos = (160, 150);
                let wi = w.index(weak).unwrap();
                w.units[wi].hp = 3;
                for _ in 0..3 {
                    w.splash((176, 176), stored, owner, crate::battle::Splash::Path);
                    got.push(pinned(&w));
                }
            }
            assert_eq!(
                got,
                [
                    12037235348494909469,
                    14302981152822218896,
                    15036872819876671294,
                    7547006022829518396,
                    6318293117998475266,
                    6318293117998475266
                ]
            );
        }

        /// V26 to V30 and V54 to V58: splash missiles of each kind landing among units and walls, and a small cannon
        /// shot at an air unit, which splashes air units too.
        #[test]
        fn splash_missiles_land() {
            let mut got = Vec::new();
            for (kind, air) in [
                (CATAPULT, false),
                (BALLISTA, false),
                (BATTLESHIP, false),
                (DESTROYER, true),
                (DESTROYER, false),
            ] {
                let mut w = arena(true);
                w.terrain.build_wall(6, 6, false, true);
                w.terrain.build_wall(5, 7, false, true);
                let s = spawn_at(&mut w, kind, 0, (0, 7), (16, 240));
                let t = if air {
                    let t = w.spawn(2, 1, (6, 7), 0).unwrap();
                    let ti = w.index(t).unwrap();
                    w.units[ti].pos = (192, 224);
                    t
                } else {
                    spawn_at(&mut w, FOOTMAN, 1, (6, 7), (208, 240))
                };
                spawn_at(&mut w, FOOTMAN, 1, (7, 7), (240, 240));
                spawn_at(&mut w, FOOTMAN, 0, (6, 9), (208, 300));
                let air2 = w.spawn(2, 1, (7, 8), 0).unwrap();
                let a2 = w.index(air2).unwrap();
                w.units[a2].pos = (220, 250);
                launch(&mut w, s, Some(t), (0, 0));
                got.push(flight(&mut w, 40));
            }
            assert_eq!(
                got,
                [
                    12664266169040939759,
                    13938365100118872023,
                    3808366146541337087,
                    16908709551750351197,
                    342348445773404845
                ]
            );
        }

        /// Missiles at a tile: a catapult and an archer attacking a wall tile (order 11), then the ground (order
        /// 17), for a human and a computer player.
        #[test]
        fn missiles_at_a_tile() {
            let mut got = Vec::new();
            for computer in [false, true] {
                for kind in [CATAPULT, ARCHER] {
                    let mut w = arena(true);
                    if computer {
                        w.players[0].controller = controller::COMPUTER;
                    }
                    w.terrain.build_wall(5, 7, false, !computer);
                    let s = spawn_at(&mut w, kind, 0, (2, 7), (80, 240));
                    launch(&mut w, s, None, (5, 7));
                    got.push(flight(&mut w, 40));
                    let i = w.index(s).unwrap();
                    w.units[i].order = Order::AttackGround { tile: (7, 7) };
                    w.strike(i);
                    got.push(flight(&mut w, 40));
                }
            }
            assert_eq!(
                got,
                [
                    974844773171738549,
                    18301706500127088865,
                    3925475337155070820,
                    3411120722290066012,
                    10737326601502101738,
                    12181855912484551264,
                    990193458748381180,
                    11030899697452512954
                ]
            );
        }

        /// V60 to V64: cannon shots with smoke, an archer's shot, with all, one and no effect slots free.
        #[test]
        fn cannon_shots_and_smoke() {
            let mut got = Vec::new();
            for kind in [BATTLESHIP, DESTROYER, ARCHER] {
                for free in 0..3 {
                    let mut w = arena(true);
                    let base = crate::slots::PERMANENT_EFFECTS;
                    match free {
                        1 => fill_except(&mut w, &[base + 4]),
                        2 => fill_except(&mut w, &[]),
                        _ => {}
                    }
                    shoot(&mut w, kind);
                    got.push(flight(&mut w, 30));
                }
            }
            assert_eq!(
                got,
                [
                    15428887332936944248,
                    13109021599294836727,
                    10365520225296779251,
                    10283991150718007824,
                    14515035686268794010,
                    11269828920322357719,
                    15653816880364866753,
                    12657041543550177779,
                    1706731671784820474
                ]
            );
        }

        /// V25, V65: building towers (a guard tower and a cannon tower) firing from order 14 at units in range; a
        /// tower that kills keeps its records. The synthetic towers have no step done of their own, so each logic run
        /// is given one every 60 cycles.
        #[test]
        fn towers_fire_and_kill() {
            let mut got = Vec::new();
            for m in [missile::ARROW, missile::SMALL_CANNON] {
                let mut w = arena(true);
                w.stats.0[0x6d].missile = m;
                let t = w.spawn(0x6d, 0, (5, 5), 0).unwrap();
                let f = w.spawn(1, 1, (9, 5), 0).unwrap();
                w.spawn(1, 1, (9, 6), 0).unwrap();
                let fi = w.index(f).unwrap();
                w.units[fi].hp = 10;
                run(&mut w, 2);
                let ti = w.index(t).unwrap();
                w.units[ti].attacked_by = Some(f);
                let mut h = 0;
                for n in 0..240 {
                    if n % 60 == 0 {
                        let ti = w.index(t).unwrap();
                        w.tower(
                            ti,
                            crate::anim::Effects {
                                step_done: true,
                                ..Default::default()
                            },
                        );
                    }
                    w.step(&[]);
                    h = fnv(h, &pinned(&w).to_le_bytes());
                }
                got.push(h);
            }
            assert_eq!(got, [16755860080350148721, 7655459991827678900]);
        }

        /// The demolition blast on its 8 neighbours: own and enemy units, a building, a unit with the used mark.
        #[test]
        fn a_demolition_blast() {
            let mut w = arena(true);
            let d = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            w.spawn(FOOTMAN, 1, (4, 4), 0).unwrap();
            w.spawn(FOOTMAN, 0, (6, 5), 0).unwrap();
            let strong = w.spawn(1, 1, (5, 6), 0).unwrap();
            let si = w.index(strong).unwrap();
            w.units[si].hp = 500;
            w.spawn(0x3a, 1, (6, 3), 0).unwrap();
            let used = w.spawn(1, 1, (4, 6), 0).unwrap();
            let ui = w.index(used).unwrap();
            w.units[ui].used = true;
            let di = w.index(d).unwrap();
            w.blast(di);
            let got = [pinned(&w), trail(&mut w, 40)];
            assert_eq!(got, [4277876101808047327, 6936592701643301166]);
        }

        /// The apply-damage checks: dying, used, inside, unholy armour, zero damage, and the Dark Portal case.
        #[test]
        fn applying_damage_checks() {
            let mut got = Vec::new();
            for case in 0..5 {
                let mut w = arena(true);
                let a = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
                let t = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
                let ti = w.index(t).unwrap();
                let d = match case {
                    0 => {
                        w.kill(ti);
                        9
                    }
                    1 => {
                        w.units[ti].used = true;
                        9
                    }
                    2 => {
                        w.units[ti].inside = true;
                        9
                    }
                    3 => {
                        w.units[ti].unholy_armor = 10;
                        9
                    }
                    _ => 0,
                };
                w.damage(ti, d, a);
                got.push(pinned(&w));
            }
            for (campaign, multiplayer) in [(Some(50), false), (Some(50), true), (Some(51), false)] {
                let (mut w, portal, foot, khadgar) = portal_world(campaign, multiplayer);
                hit(&mut w, portal, foot);
                hit(&mut w, portal, khadgar);
                got.push(pinned(&w));
            }
            assert_eq!(
                got,
                [
                    70294831625206533,
                    8970288226595977013,
                    7192386966663730699,
                    1163958946427715318,
                    6375788400760873233,
                    5311499050263474487,
                    936959735797052187,
                    15686914118834853151
                ]
            );
        }

        /// Attack values in a strike: bloodlust, and the invulnerability cheat for a human and a computer player.
        #[test]
        fn attack_values_in_a_strike() {
            let mut got = Vec::new();
            for (bloodlust, cheat, owner) in [(true, false, 0u8), (false, true, 0), (true, true, 0), (false, true, 1)] {
                for kind in [FOOTMAN, ARCHER] {
                    let mut w = arena(true);
                    w.rules.invulnerable_cheat = cheat;
                    let a = spawn_at(&mut w, kind, owner, (2, 7), (80, 240));
                    let t = spawn_at(&mut w, 1, 1 - owner, (3, 7), (112, 240));
                    let ai = w.index(a).unwrap();
                    w.units[ai].bloodlust = if bloodlust { 100 } else { 0 };
                    launch(&mut w, a, Some(t), (0, 0));
                    got.push(flight(&mut w, 12));
                }
            }
            assert_eq!(
                got,
                [
                    5957984226047859813,
                    15142748491032081865,
                    17341045545419207173,
                    15236796780127254473,
                    10296788754695823405,
                    6828252659927914776,
                    17832372739548570973,
                    15147867855994454397
                ]
            );
        }

        /// V76, V77: no strike while a command waits to take over.
        #[test]
        fn no_strike_while_a_command_waits() {
            let mut w = arena(true);
            let a = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let t = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
            let i = w.index(a).unwrap();
            w.units[i].order = Order::Attack { target: t };
            w.units[i].script.anim = anim::ATTACK;
            w.command(i, Order::Idle);
            let fx = crate::anim::Effects {
                strikes: 1,
                ..Default::default()
            };
            w.order(i, fx);
            assert_eq!(pinned(&w), 13342833165557620207);
        }
    }
}
