//! Spells: casting, effects, durations and summoned units. Spec: `docs/specs/spells/spells.md`.
//!
//! Covered: the spell orders 38 to 56 (command, approach, strike), the five unit timers and the timers pass (mana,
//! berserker regeneration, decay, runes), the creation rules for summoned units, and every effect in flight.
//!
//! Not covered: which spells a player may cast (research is in `tech.rs`; the button bar is a later spec) and sound.
//! Dead bodies are the remains of `corpses.rs`.

use crate::TILE_PX;
use crate::battle::Splash;
use crate::combat;
use crate::economy::Event;
use crate::movement::{Goal, Walk};
use crate::placement::{LIMIT_OUT, Source};
use crate::slots;
use crate::sound::Sound;
use crate::world::{Layer, Order, UnitId, World};
use tidewater_formats::udta::flags;

/// Rune slots, shared by all players.
pub const RUNE_SLOTS: usize = 50;
const RUNE_LIFE: u16 = 2048;
const RUNE_DAMAGE: u16 = 50;
/// A fireball's count limit (spec, "Fireball"): the count goes up to 41 on the step after the last explosion, and the
/// step after that (still drawing and moving) marks it as ending instead of counting. Round-48 errata: the limit is 40
/// for a stored damage of 40, as for every fireball, and [`FIREBALL_OTHER_LIMIT`] otherwise. Its explosions are on
/// counts 8, 16, 24, 32 and 40.
const FIREBALL_COUNT_LIMIT: u8 = 40;
/// The fireball's count limit for a stored damage other than 40 (round-48 errata, static study).
const FIREBALL_OTHER_LIMIT: u8 = 25;
/// A flame's wait at its start point (round-48 errata): its first path step comes in its update 6 cycles after its
/// first.
const FLAME_FIRST_STEP: u16 = 6;
/// Healing and exorcism (round-48 errata): their one flight step comes in their update 19 cycles after their first,
/// after the 18 cycles of the flight animation started there.
const WISP_STEP: u16 = 19;

/// Effect kinds (spec, "Spell effects in flight" and "Cosmetic effects").
mod kind {
    pub const FIREBALL: u8 = 2;
    pub const BOLT: u8 = 3;
    pub const FLAME: u8 = 4;
    pub const SHARD: u8 = 5;
    pub const CLOUD: u8 = 6;
    pub const EXORCISM: u8 = 8;
    pub const HEALING: u8 = 9;
    pub const COIL: u8 = 10;
    pub const WHIRLWIND: u8 = 12;
    pub const RUNE: u8 = 0x0b;
    pub const BALLISTA_IMPACT: u8 = 0x15;
    pub const BLINK: u8 = 0x16;
    pub const EXPLOSION: u8 = 0x17;
    pub const CANNON_SMOKE: u8 = 0x19;
    pub const CANNON_EXPLOSION: u8 = 0x1a;
}

/// The total of the waits A in an effect kind's start animation (spec, "When a new effect first runs"): the effect's
/// start step, which restarts its script and starts its picture showing, comes A cycles after its first update.
pub(crate) fn start_cycles(k: u8) -> u16 {
    match k {
        0x15 | 0x16 | 0x17 | 0x1a | 0x1c => 1,
        0x0b => 2,
        0x04 | 0x13 | 0x14 => 5,
        0x05 | 0x19 => 11,
        0x06 => 18,
        _ => 0,
    }
}

/// The total of the waits W of a cosmetic effect's restarted script (spec, "Cosmetic effects"): it is removed during
/// its update A + 1 + W cycles after its first update.
fn cosmetic_waits(k: u8) -> u16 {
    match k {
        kind::RUNE => 15,
        kind::BLINK => 18,
        kind::EXPLOSION => 48,
        kind::BALLISTA_IMPACT | kind::CANNON_SMOKE | kind::CANNON_EXPLOSION => 12,
        // Not a kind the spec lists as cosmetic; nothing creates one.
        _ => 0,
    }
}

/// The cosmetic effect kinds (spec, "Cosmetic effects"): created with only their kind and position set.
pub(crate) fn is_cosmetic(k: u8) -> bool {
    matches!(
        k,
        kind::RUNE
            | kind::BALLISTA_IMPACT
            | kind::BLINK
            | kind::EXPLOSION
            | kind::CANNON_SMOKE
            | kind::CANNON_EXPLOSION
            | 0x1c
    )
}

/// The effect a missile kind leaves at its impact (damage spec, "Impact effects").
pub(crate) fn impact_effect(missile: u8) -> Option<u8> {
    match missile {
        0x0d | 0x0e => Some(kind::BALLISTA_IMPACT),
        0x07 | 0x18 => Some(kind::CANNON_EXPLOSION),
        0x11 | 0x12 => Some(kind::EXPLOSION),
        _ => None,
    }
}

/// Unit types by PUD number that the spec names.
mod unit {
    pub const DEMOLITION: u8 = 0x0e;
    pub const SAPPER: u8 = 0x0f;
    pub const BERSERKER: u8 = 0x13;
    pub const EYE: u8 = 0x2d;
    pub const SKELETON: u8 = 0x37;
    pub const CRITTER: u8 = 0x39;
}

/// Spell orders by number.
pub mod spell {
    pub const HOLY_VISION: u8 = 38;
    pub const HEALING: u8 = 39;
    pub const EXORCISM: u8 = 41;
    pub const FLAME_SHIELD: u8 = 42;
    pub const FIREBALL: u8 = 43;
    pub const SLOW: u8 = 44;
    pub const INVISIBILITY: u8 = 45;
    pub const POLYMORPH: u8 = 46;
    pub const BLIZZARD: u8 = 47;
    pub const EYE_OF_KILROGG: u8 = 48;
    pub const BLOODLUST: u8 = 49;
    pub const RAISE_DEAD: u8 = 50;
    pub const DEATH_COIL: u8 = 51;
    pub const WHIRLWIND: u8 = 52;
    pub const HASTE: u8 = 53;
    pub const UNHOLY_ARMOUR: u8 = 54;
    pub const RUNES: u8 = 55;
    pub const DEATH_AND_DECAY: u8 = 56;
}
use spell::*;

// ----- the spell table (spec, "Orders and mana costs", "The spells") ------------------------------------------------

/// Spells aimed at a unit; the others are aimed at a point.
const UNIT_SPELLS: [u8; 8] = [
    HEALING,
    FLAME_SHIELD,
    SLOW,
    INVISIBILITY,
    POLYMORPH,
    BLOODLUST,
    HASTE,
    UNHOLY_ARMOUR,
];
/// Unit spells whose target must also be organic.
const ORGANIC_SPELLS: [u8; 3] = [HEALING, POLYMORPH, BLOODLUST];

fn is_unit_spell(spell: u8) -> bool {
    UNIT_SPELLS.contains(&spell)
}

/// The approach radius in tiles of a spell order (orders spec, "Spell orders"); `None` means "always": no walk.
fn radius(spell: u8) -> Option<i32> {
    match spell {
        HOLY_VISION | EYE_OF_KILROGG => None,
        HEALING | FLAME_SHIELD | INVISIBILITY | BLOODLUST | RAISE_DEAD | HASTE | UNHOLY_ARMOUR => Some(6),
        EXORCISM | FIREBALL | SLOW | POLYMORPH | DEATH_COIL | RUNES => Some(10),
        BLIZZARD | WHIRLWIND | DEATH_AND_DECAY => Some(12),
        _ => Some(0),
    }
}

/// How a spell's strike runs steps 2 to 4 of "The spell's strike" (the strike column of "The spells").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pays {
    /// Steps 2 to 4: the mana check, the cost paid, idle pending.
    UpFront,
    /// The two channelled spells: steps 2 and 3 with no idle, so they strike again on the next loop.
    Channelled,
    /// Idle first, and the effect pays as it goes: healing, exorcism and raise dead per hit point or skeleton, flame
    /// shield after its own checks.
    AsItGoes,
}

/// The strike's payment rule of a spell order; `None` for order 40, which has no effect.
fn pays(spell: u8) -> Option<Pays> {
    match spell {
        HEALING | EXORCISM | FLAME_SHIELD | RAISE_DEAD => Some(Pays::AsItGoes),
        BLIZZARD | DEATH_AND_DECAY => Some(Pays::Channelled),
        HOLY_VISION | FIREBALL | SLOW | INVISIBILITY | POLYMORPH | EYE_OF_KILROGG | BLOODLUST | DEATH_COIL
        | WHIRLWIND | HASTE | UNHOLY_ARMOUR | RUNES => Some(Pays::UpFront),
        _ => None,
    }
}

/// Whether a spell plays its sound itself, once its own checks have passed (sounds spec, "Spells"): healing when it
/// heals, flame shield when it pays, the eye of Kilrogg as a plain sound; the fireball has none. Every other spell's
/// sound plays as its effect starts.
fn own_sound(spell: u8) -> bool {
    matches!(spell, HEALING | FLAME_SHIELD | EYE_OF_KILROGG | FIREBALL)
}

/// A spell piece that lives in an effect slot (spec, "Spell effects in flight"), as its draws leave it.
struct Piece {
    kind: u8,
    shooter: UnitId,
    target: Option<UnitId>,
    pos: (i32, i32),
    damage: u8,
    fx: Fx,
    /// The destination it reports from creation.
    dest: (i32, i32),
}

/// A live rune on the map.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rune {
    pub tile: (i32, i32),
    pub life: u16,
}

/// A flying effect's straight-line rule (spec, "How flying effects move").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Line {
    sign: (i32, i32),
    x_major: bool,
    big: i32,
    small: i32,
    counter: i32,
    /// The major distance still to go; goes down by the speed on every step.
    pub remaining: i32,
}

impl Line {
    pub fn new(from: (i32, i32), to: (i32, i32)) -> Self {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let x_major = dx.abs() >= dy.abs();
        let (big, small) = if x_major {
            (dx.abs(), dy.abs())
        } else {
            (dy.abs(), dx.abs())
        };
        let mut sign = (dx.signum(), dy.signum());
        // Our reading: a flight to its own start drifts east (the spec's whirlwind with a new goal equal to its
        // position drifts east until it ends).
        if big == 0 {
            sign = (1, 0);
        }
        Self {
            sign,
            x_major,
            big,
            small,
            counter: (big / 2).max(1),
            remaining: big,
        }
    }

    /// One step of `speed` pixels.
    pub fn step(&mut self, pos: &mut (i32, i32), speed: i32) {
        let (sx, sy) = (self.sign.0 * speed, self.sign.1 * speed);
        if self.x_major {
            pos.0 += sx;
        } else {
            pos.1 += sy;
        }
        self.counter -= self.small;
        if self.counter <= 0 {
            if self.x_major {
                pos.1 += sy;
            } else {
                pos.0 += sx;
            }
            self.counter += self.big;
        }
        self.remaining -= speed;
    }
}

/// What a spell effect does each time it is updated.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fx {
    /// An ordinary attack missile in flight (`battle.rs`, damage spec "Missile flight"). `length` is the flight length
    /// D with the minimum applied; the line's remaining distance counts down from it.
    Shot {
        line: Line,
        length: i32,
    },
    /// A gryphon hammer or dragon breath (`battle.rs`): `count` is the number of counted steps, `ended` is set on the
    /// end step, and the final hit comes on the next update.
    Breath {
        line: Line,
        count: u8,
        ended: bool,
    },
    /// Kinds 0x0B, 0x15, 0x16, 0x17, 0x19 and 0x1A: play their script once and are removed at its step done, A + 1 + W
    /// cycles after the first update (`start_cycles`, `cosmetic_waits`).
    Cosmetic,
    /// Kinds 8 and 9: one flight step, then a touch on a wall.
    Wisp,
    /// A fireball: `ending` is set by the step after the count passed the limit; the fireball is removed at its next
    /// update (round-48 errata).
    Fireball {
        line: Line,
        count: u8,
        ending: bool,
    },
    /// A flame shield bolt: `done` is set when it makes its flames; it is removed at its next update (round-48 errata).
    Bolt {
        line: Line,
        done: bool,
    },
    Flame {
        point: u8,
        life: u16,
    },
    /// A blizzard shard. `landed` is 0 while it waits or flies; on landing it is set to 5 and counts down once per
    /// update, the shard being removed when it reaches 0.
    Shard {
        line: Line,
        repeats: u8,
        landed: u8,
    },
    Cloud,
    Whirl {
        line: Line,
        life: u16,
        goal: (i32, i32),
    },
}

/// The flame-shield flames' fixed 51-point path (spec, "Data"), as offsets from the shielded unit's centre.
const FLAME_PATH: [(i32, i32); 51] = [
    (1, -34),
    (4, -34),
    (7, -34),
    (10, -33),
    (13, -32),
    (16, -31),
    (19, -30),
    (22, -28),
    (25, -26),
    (28, -23),
    (30, -20),
    (31, -17),
    (32, -14),
    (32, -11),
    (32, -8),
    (31, -5),
    (30, -2),
    (28, 1),
    (25, 4),
    (22, 6),
    (19, 8),
    (16, 9),
    (13, 10),
    (10, 11),
    (7, 12),
    (4, 12),
    (1, 12),
    (-2, 12),
    (-5, 11),
    (-8, 11),
    (-11, 10),
    (-14, 8),
    (-17, 7),
    (-20, 5),
    (-23, 2),
    (-25, -1),
    (-27, -4),
    (-28, -7),
    (-28, -10),
    (-28, -13),
    (-27, -16),
    (-26, -19),
    (-24, -22),
    (-22, -25),
    (-19, -27),
    (-16, -29),
    (-13, -31),
    (-10, -32),
    (-7, -33),
    (-4, -34),
    (-1, -34),
];
/// The flames' start points and lifetimes.
const FLAMES: [(u8, u16); 5] = [(0, 512), (10, 524), (20, 518), (30, 521), (40, 527)];

/// Cycles a landed blizzard shard stays before it is removed (spec, "Blizzard shards", round-45 errata).
const SHARD_LINGER: u8 = 4;

/// Effect speeds in pixels per step, kinds 0 to 28 (spec, "Data").
const EFFECT_SPEED: [i32; 29] = [
    16, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 8, 8, 12, 12, 16, 16, 0, 0, 0, 0, 0, 16, 0, 0, 12, 0,
];

fn centre_of_tile(t: (i32, i32)) -> (i32, i32) {
    (t.0 * TILE_PX + TILE_PX / 2, t.1 * TILE_PX + TILE_PX / 2)
}

fn tile_of(p: (i32, i32)) -> (i32, i32) {
    (p.0.div_euclid(TILE_PX), p.1.div_euclid(TILE_PX))
}

impl World {
    // ----- 2. the command (spec, "Giving the order") -----------------------------------------------------------

    /// A spell command (spec, "Giving the order"). Order 40 has no effect and is refused.
    pub(crate) fn cast_command(
        &mut self,
        player: u8,
        unit: UnitId,
        spell: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    ) {
        if pays(spell).is_none() {
            return;
        }
        let Some(i) = self
            .index(unit)
            .filter(|&i| self.units[i].owner == player && !self.units[i].dying && !self.units[i].building)
        else {
            return;
        };
        if is_unit_spell(spell) && target.is_none() {
            return;
        }
        self.give_spell(i, spell, target, tile);
        // Orders spec, "A player's command forgets the attacker" (round-56 static correction): a spell command given
        // by the player forgets the record like any other unit command.
        self.forget_attacker_after_command(unit);
    }

    /// Gives unit `i` a spell order (spec, "Giving the order"): the all-spells cheat's top-up, then the order becomes
    /// pending, a point spell dropping its target unit. A unit spell without a target is given as it is and fails
    /// when its order logic runs (the caster AI's random search does that; caster-ai spec).
    pub(crate) fn give_spell(&mut self, i: usize, spell: u8, target: Option<UnitId>, tile: (i32, i32)) {
        if self.rules.all_spells_cheat {
            self.units[i].mana = 255;
        }
        let order = Order::Cast {
            spell,
            target: if is_unit_spell(spell) { target } else { None },
            tile,
        };
        // Our own choice, as for the other player commands: a new order forgets a remembered patrol.
        self.units[i].remembered = None;
        self.command(i, order);
    }

    /// A spell's mana cost from the spec's table ("Orders and mana costs"); for the spells paid per hit point, per
    /// strike or per skeleton, the cost of one. 0 for numbers that aren't spells.
    pub(crate) fn mana_cost(spell: u8) -> u8 {
        match spell {
            HOLY_VISION | EYE_OF_KILROGG => 70,
            HEALING => 6,
            EXORCISM => 4,
            FLAME_SHIELD => 80,
            FIREBALL | DEATH_COIL | WHIRLWIND | UNHOLY_ARMOUR => 100,
            SLOW | BLOODLUST | RAISE_DEAD | HASTE => 50,
            INVISIBILITY | POLYMORPH | RUNES => 200,
            BLIZZARD | DEATH_AND_DECAY => 25,
            _ => 0,
        }
    }

    // ----- 3. walking and the decision (spec, "Walking and the decision") --------------------------------------

    /// The order logic of orders 38 to 56 at a "step done" (spec, "Walking and the decision"): the target unit's
    /// validity, the walk into the spell's radius with its lost-target drop, the not-a-building and organic tests once
    /// in reach, then the turn to face the aim and the casting animation.
    pub(crate) fn run_spell(&mut self, i: usize) -> bool {
        let Order::Cast { spell, target, tile } = self.units[i].order else {
            return false;
        };
        let target = if is_unit_spell(spell) {
            match target.and_then(|t| self.index(t)).filter(|&t| self.units[t].present()) {
                Some(t) => Some(t),
                None => {
                    self.decision_fails(i);
                    return true;
                }
            }
        } else {
            None
        };
        // Orders spec, "Walking orders": the walk runs only for a caster not yet in reach. At each of its step
        // boundaries it drops a target that is dying, gone, inside something or no longer detected, and the decision
        // then fails for lack of a target (round 81, `xhuman10-slow-killed-goals-400`, S90). A caster already in reach
        // isn't tested, so it casts at a target its player no longer detects (`xhuman10-invisible-reach-goals-400`,
        // S92). Round-50 errata (`human14-10000` death knight 1577, n = 3740): the plain no-route rule.
        if let Some(g) = self.cast_walk(i, spell, target, tile) {
            let owner = self.units[i].owner;
            let walking = !self.at_goal(i, g);
            if walking && target.is_some_and(|t| self.target_lost(owner, self.units[t].id)) {
                self.decision_fails(i);
                return true;
            }
            if self.walk(i, g) != Walk::Arrived {
                return true;
            }
        }
        if let Some(t) = target
            && !self.cast_target_fits(spell, t)
        {
            self.decision_fails(i);
            return true;
        }
        let face = target.map_or(tile, |t| crate::world::facing_tile_of(&self.units[t]));
        self.face_and_strike(i, face);
        true
    }

    /// The decision's tests made only in reach (round 81, `xhuman10-heal-ballista-goals-500`, S91): the target unit
    /// is not a building and, for the organic-target spells, organic. A caster sent to cast such a spell on a building
    /// or a non-organic unit first walks into reach. Neither test is a detection test: that is the command's check,
    /// and the walk's.
    fn cast_target_fits(&self, spell: u8, t: usize) -> bool {
        let t = &self.units[t];
        !t.building && (!ORGANIC_SPELLS.contains(&spell) || self.stats_of(t.kind).flags & flags::ORGANIC != 0)
    }

    /// The walk toward the spell's aim: target unit `t` where it stands, or the target tile, within the spell's
    /// radius; `None` for a spell with radius "always", which casts where the caster stands (eye of Kilrogg, holy
    /// vision). Pathfinding spec, "Step 1": the walk toward a target unit uses its widened footprint as the goal zone
    /// ([`World::target_walk`]).
    fn cast_walk(&self, i: usize, spell: u8, t: Option<usize>, tile: (i32, i32)) -> Option<Goal> {
        let r = radius(spell)?;
        Some(match t {
            Some(t) => self.target_walk(&self.units[i], &self.units[t], r),
            None => Goal::tile(tile, r),
        })
    }

    /// The decision fails (spec, "Walking and the decision", round 81): idle becomes the caster's pending order, with
    /// the Enhanced resume, and takes over in the same update: order 2 (or the resumed order), the still animation,
    /// wait 3. Nothing is paid (`xhuman10-slow-killed-goals-400`, S90; `xhuman10-heal-ballista-goals-500`, S91).
    fn decision_fails(&mut self, i: usize) {
        self.idle_now(i);
    }

    // ----- 5. the spell's strike (spec, "The spell's strike") --------------------------------------------------

    /// The spell's strike (spec, "The spell's strike"), run by the strike point after it has ended the caster's
    /// invisibility and made the strike reveal ([`World::strike`], spec, "The strike point"): step 1, the target
    /// check of a unit spell; steps 2 to 4 by the spell's payment rule ([`pays`]); step 5, the effect. Round-56
    /// errata ("At the strike"): none of the checks is a distance; a target that has left the radius during the
    /// casting animation is still affected (`orc14-25000`, healing, n = 18992).
    pub(crate) fn cast_strike(&mut self, i: usize) {
        let Order::Cast { spell, target, tile } = self.units[i].order else {
            return;
        };
        let t = if is_unit_spell(spell) {
            match target.and_then(|t| self.index(t)).filter(|&t| self.units[t].present()) {
                Some(t) => Some(t),
                None => return self.idle_pending(i),
            }
        } else {
            None
        };
        let Some(rule) = pays(spell) else {
            return self.idle_pending(i);
        };
        let paid = match rule {
            Pays::UpFront => self.pay_mana(i, Self::mana_cost(spell) as u32),
            Pays::Channelled => self.pay_mana(i, Self::mana_cost(spell) as u32),
            Pays::AsItGoes => true,
        };
        if !paid {
            return;
        }
        self.strike_done(i, rule != Pays::Channelled);
        self.spell_effect(i, spell, t, tile);
    }

    /// Order 2 becomes pending (spec, "The spell's strike", step 4).
    fn idle_pending(&mut self, i: usize) {
        self.units[i].pending = Some(Order::Idle);
    }

    /// Steps 2 and 3: the mana check (failing makes the caster idle) and payment.
    fn pay_mana(&mut self, i: usize, cost: u32) -> bool {
        if (self.units[i].mana as u32) < cost {
            self.idle_pending(i);
            return false;
        }
        self.units[i].mana -= cost as u8;
        true
    }

    /// Step 4: idle (unless channelled), and the strike counted for the caster AI. The caster's invisibility has
    /// already ended at the strike point ([`World::strike`]; spec, "The strike point", step 2).
    fn strike_done(&mut self, i: usize, idle: bool) {
        let owner = self.units[i].owner as usize;
        self.casts.strikes[owner & 15] += 1;
        if idle {
            self.idle_pending(i);
        }
    }

    /// Step 5, the spell's effect (spec, "The spells"): the spell's sound as the effect starts (unless it plays its
    /// own, [`own_sound`]; the fireball has none), then the effect.
    fn spell_effect(&mut self, i: usize, spell: u8, t: Option<usize>, tile: (i32, i32)) {
        if !own_sound(spell) {
            self.spell_sound(spell, t, tile);
        }
        let target = || t.expect("a unit spell has its target");
        let (caster, owner) = (self.units[i].id, self.units[i].owner);
        match spell {
            HOLY_VISION => self.holy_vision(owner, tile),
            HEALING => self.heal(i, target()),
            EXORCISM => self.exorcism(i, tile),
            FLAME_SHIELD => self.flame_shield(i, target()),
            FIREBALL => self.fireball(i, tile),
            SLOW | INVISIBILITY | BLOODLUST | HASTE | UNHOLY_ARMOUR => self.timer_spell(spell, target()),
            POLYMORPH => self.polymorph(target()),
            BLIZZARD => self.blizzard(caster, tile),
            EYE_OF_KILROGG => self.make_eye(i),
            RAISE_DEAD => self.raise_dead(caster, tile),
            DEATH_COIL => self.death_coil(i, tile),
            WHIRLWIND => self.whirlwind(caster, tile),
            RUNES => self.runes(i, tile),
            DEATH_AND_DECAY => self.death_and_decay(caster, tile),
            _ => {}
        }
    }

    /// A spell's sound when it takes effect (sounds spec, "Spells"): placed on the target unit `t`, else at the target
    /// tile. Open question "Spell sound placement": we take the target, as the default says.
    fn spell_sound(&mut self, spell: u8, t: Option<usize>, tile: (i32, i32)) {
        match t {
            Some(t) => self.sound_on(t, Sound::Spell(spell)),
            None => self.sound_at_tile(tile, Sound::Spell(spell)),
        }
    }

    // ----- 6. the effects (spec, "The spells") -----------------------------------------------------------------

    /// Holy vision: the seven sight-9 areas, each with its marker; the Enhanced fog refresh.
    fn holy_vision(&mut self, owner: u8, t: (i32, i32)) {
        const OFFSETS: [(i32, i32); 7] = [(0, 0), (0, -8), (0, 8), (6, 4), (6, -4), (-6, 4), (-6, -4)];
        let w = self.terrain.width as i32;
        let enhanced = self.rules.enhanced;
        let limit = if enhanced { w - 1 } else { w };
        for (dx, dy) in OFFSETS {
            let p = (t.0 + dx, t.1 + dy);
            if p.0 < 0 || p.1 < 0 || p.0 > limit || p.1 > limit {
                continue;
            }
            if p.0 < w && p.1 < w {
                self.reveal(owner, p, 9);
            }
            self.cosmetic(kind::BLINK, (p.0 * TILE_PX + 8, p.1 * TILE_PX + 8));
        }
        if enhanced {
            self.vision.countdown = 150;
        }
    }

    /// Healing: up to 40 HP, as far as the mana goes, paid per hit point healed.
    fn heal(&mut self, i: usize, t: usize) {
        let max = self.stats_of(self.units[t].kind).hit_points;
        let missing = max.saturating_sub(self.units[t].hp);
        let heal = missing.min(self.units[i].mana as u16 / 6).min(40);
        if heal == 0 {
            return;
        }
        self.units[t].hp += heal;
        self.spell_sound(HEALING, Some(t), (0, 0));
        self.units[i].mana -= (6 * heal) as u8;
        self.wisp(kind::HEALING, t);
    }

    /// Exorcism: the four rings around the target tile, paid per hit point of damage.
    fn exorcism(&mut self, i: usize, t: (i32, i32)) {
        let (caster, owner) = (self.units[i].id, self.units[i].owner);
        for k in 0..4 {
            if self.units[i].mana < 4 {
                break;
            }
            let mut tiles = Vec::new();
            for c in t.0 - k..=t.0 + k {
                tiles.push((c, t.1 + k));
                tiles.push((c, t.1 - k));
            }
            if k >= 1 {
                for r in t.1 - k + 1..=t.1 + k - 1 {
                    tiles.push((t.0 + k, r));
                    tiles.push((t.0 - k, r));
                }
            }
            for (x, y) in tiles {
                let Some(id) = self.occupant(Layer::Land, x, y) else {
                    continue;
                };
                let Some(ui) = self.index(id).filter(|&ui| self.units[ui].present()) else {
                    continue;
                };
                let u = &self.units[ui];
                let allied = self.rules.enhanced && u.owner < 8 && self.allied(owner, u.owner);
                if self.stats_of(u.kind).flags & flags::UNDEAD == 0 || allied {
                    continue;
                }
                let d = (self.units[i].mana as u16 / 4).min(self.units[ui].hp) as u8;
                self.wisp(kind::EXORCISM, ui);
                self.damage(ui, d, caster);
                self.units[i].mana -= 4 * d;
            }
        }
    }

    /// Flame shield: refused (nothing paid) on a shielded or air target; otherwise the mana check, the timer and the
    /// bolt. Round-47 errata: the bolt's destination is the target unit's centre from creation.
    fn flame_shield(&mut self, i: usize, t: usize) {
        if self.units[t].flame_shield != 0 || self.units[t].layer == Layer::Air {
            return;
        }
        if !self.pay_mana(i, Self::mana_cost(FLAME_SHIELD) as u32) {
            return;
        }
        self.spell_sound(FLAME_SHIELD, Some(t), (0, 0));
        self.units[t].flame_shield = 500;
        let (from, to) = (self.units[i].centre_px(), self.units[t].centre_px());
        let (shooter, target) = (self.units[i].id, Some(self.units[t].id));
        self.place_piece(|_| Piece {
            kind: kind::BOLT,
            shooter,
            target,
            pos: from,
            damage: 0,
            fx: Fx::Bolt {
                line: Line::new(from, to),
                done: false,
            },
            dest: to,
        });
    }

    /// Fireball: the hammer-and-breath missile with stored damage 40. Round-47 errata: its destination is the target
    /// tile's centre from creation.
    fn fireball(&mut self, i: usize, tile: (i32, i32)) {
        let (from, to) = (self.units[i].centre_px(), centre_of_tile(tile));
        let shooter = self.units[i].id;
        self.place_piece(|_| Piece {
            kind: kind::FIREBALL,
            shooter,
            target: None,
            pos: from,
            damage: 40,
            fx: Fx::Fireball {
                line: Line::new(from, to),
                count: 0,
                ending: false,
            },
            dest: to,
        });
    }

    /// The five timer spells (slow, invisibility, bloodlust, haste, unholy armour): the timer set, unholy armour's
    /// halving, then the marker on the target.
    fn timer_spell(&mut self, spell: u8, t: usize) {
        let u = &mut self.units[t];
        match spell {
            SLOW => {
                u.haste = if u.haste < 0 {
                    -1000
                } else {
                    u.haste.saturating_sub(1000)
                }
            }
            HASTE => {
                u.haste = if u.haste > 0 {
                    1000
                } else {
                    u.haste.saturating_add(1000)
                }
            }
            INVISIBILITY => u.invisibility = 2000,
            BLOODLUST => u.bloodlust = 1000,
            UNHOLY_ARMOUR => {
                u.unholy_armor = 500;
                if u.hp >= 2 {
                    u.hp /= 2;
                }
            }
            _ => unreachable!("not a timer spell"),
        }
        self.marker_on(t);
    }

    /// Polymorph: the used mark and an outright kill, then the critter (2 creation draws) and the marker at the
    /// target's tile.
    fn polymorph(&mut self, t: usize) {
        let at = self.units[t].tile;
        self.units[t].used = true;
        self.kill(t);
        let _ = self.spawn(unit::CRITTER, slots::NEUTRAL, at, 0);
        self.cosmetic(kind::BLINK, centre_of_tile(at));
    }

    /// Blizzard: five shard chains, each picking its landing point (2 draws) as its first shard starts.
    fn blizzard(&mut self, caster: UnitId, tile: (i32, i32)) {
        let c = centre_of_tile(tile);
        for _ in 0..5 {
            self.place_piece(|w| {
                let x = c.0 + ((w.rng.draw() % 5) as i32 - 2) * TILE_PX;
                let y = c.1 + ((w.rng.draw() % 5) as i32 - 2) * TILE_PX;
                w.shard(caster, (x, y), 10)
            });
        }
    }

    /// One blizzard shard of a chain landing at `land`: its start offset is two draws. Its destination is the landing
    /// point from the moment it is created, through its initial wait (round-44 errata, "Blizzard shards").
    fn shard(&mut self, shooter: UnitId, land: (i32, i32), repeats: u8) -> Piece {
        let ox = (self.rng.draw() % 3) as i32 * 11 - 11;
        let oy = (self.rng.draw() % 3) as i32 * 11 - 11;
        let start = (land.0 - 110 + ox, land.1 - 170 + oy);
        Piece {
            kind: kind::SHARD,
            shooter,
            target: None,
            pos: start,
            damage: 10,
            fx: Fx::Shard {
                line: Line::new(start, land),
                repeats,
                landed: 0,
            },
            dest: land,
        }
    }

    /// Eye of Kilrogg: an eye next to the caster, or the blocked-exit message when the free-tile search finds no
    /// tile. Round 81 (audit erratum 9, S100): its sound, a plain sound, plays only when an eye is created.
    fn make_eye(&mut self, i: usize) {
        let u = &self.units[i];
        let (src, at, owner) = (
            Source::new(u.tile.0, u.tile.1, u.size.0 as i32, u.size.1 as i32),
            u.tile,
            u.owner,
        );
        match self.find_place(unit::EYE, src, at, LIMIT_OUT) {
            Some(t) => {
                if self.spawn(unit::EYE, owner, t, 0).is_ok() {
                    self.cue_plain(Sound::Spell(EYE_OF_KILROGG));
                }
            }
            None => self.events.push(Event::NoRoom {
                player: owner,
                kind: unit::EYE,
            }),
        }
    }

    /// Raise dead: every unused body within a squared tile distance of 36, in slot order, while the mana lasts.
    fn raise_dead(&mut self, caster: UnitId, t: (i32, i32)) {
        for b in self.raisable_bodies() {
            let Some(i) = self.index(caster) else { return };
            if self.units[i].mana < 50 {
                break;
            }
            let Some(tile) = self.unit(b).map(|u| u.tile) else {
                continue;
            };
            let (dx, dy) = (tile.0 - t.0, tile.1 - t.1);
            if dx * dx + dy * dy > 36 {
                continue;
            }
            let owner = self.units[i].owner;
            if let Ok(s) = self.spawn(unit::SKELETON, owner, tile, 0) {
                let si = self.index(s).expect("just created");
                self.marker_on(si);
                let bi = self.index(b).expect("still there");
                self.units[bi].used = true;
            }
            if let Some(i) = self.index(caster) {
                self.units[i].mana -= 50;
            }
        }
    }

    /// Death coil: the victims of the 5 × 5 tiles, lowest HP first, share a budget of 50; one coil missile each; the
    /// caster gains the total.
    fn death_coil(&mut self, i: usize, t: (i32, i32)) {
        let (caster, owner) = (self.units[i].id, self.units[i].owner);
        let mut found: Vec<UnitId> = Vec::new();
        for y in t.1 - 2..=t.1 + 2 {
            for x in t.0 - 2..=t.0 + 2 {
                for l in [Layer::Land, Layer::Air] {
                    let Some(id) = self.occupant(l, x, y) else { continue };
                    let Some(u) = self.unit(id) else { continue };
                    let ally = self.rules.enhanced && u.owner < 8 && self.allied(owner, u.owner);
                    if u.present() && u.owner != owner && !ally && self.stats_of(u.kind).flags & flags::ORGANIC != 0 {
                        found.push(id);
                    }
                }
            }
        }
        let hp = |w: &World, id: UnitId| w.unit(id).map_or(0, |u| u.hp);
        for p in 0..found.len() {
            for q in p + 1..found.len() {
                if hp(self, found[p]) > hp(self, found[q]) {
                    found.swap(p, q);
                }
            }
        }
        let (mut budget, mut total) = (50u16, 0u16);
        for id in found {
            if budget == 0 {
                break;
            }
            let share = hp(self, id).min(budget);
            budget -= share;
            if share == 0 {
                continue;
            }
            total += share;
            self.launch_coil(caster, id, share as u8);
        }
        if let Some(i) = self.index(caster) {
            let max = self.stats_of(self.units[i].kind).hit_points;
            let u = &mut self.units[i];
            u.hp = (u.hp + total).min(max.max(u.hp));
        }
    }

    /// The caster's own missile weapon, launched as an attack would (spec: the damage draw and two scatter draws), with
    /// the victim's share as its stored damage. Round-45 errata, "Death coil victims": the missile is a normal missile
    /// of kind 0x0A (damage spec, "Missile flight"; `battle.rs`): its destination is its aim point from creation, it
    /// flies floor(D / 12) + 1 steps and its share is applied at its impact, 16 cycles after the landing step. As it
    /// is launched, the caster's target becomes the victim (`xorc01-death-coil` death knight 1491, n = 1191).
    fn launch_coil(&mut self, caster: UnitId, victim: UnitId, share: u8) {
        let (Some(a), Some(v)) = (self.unit(caster), self.unit(victim)) else {
            return;
        };
        let s = self.stats_of(a.kind);
        let (basic, pierce) = self.attack_values(a, &s);
        let armour = combat::armour(&self.stats_of(v.kind), &self.players[v.owner as usize].upgrades);
        let (from, mut to) = (a.centre_px(), v.centre_px());
        let _ = combat::roll(&mut self.rng, basic, pierce, armour);
        to.0 += (self.rng.draw() & 7) as i32 - 3;
        to.1 += (self.rng.draw() & 7) as i32 - 3;
        if let Some(ci) = self.index(caster)
            && let Order::Cast { target, .. } = &mut self.units[ci].order
        {
            *target = Some(victim);
        }
        let line = Line::new(from, to);
        let length = line.remaining;
        let fx = Fx::Shot { line, length };
        let n = self.new_fx(kind::COIL, caster, Some(victim), from, share, fx);
        if let Some(m) = n.and_then(|n| self.missiles[n].as_mut()) {
            m.to = to;
            m.flight = combat::flight_steps(length, combat::flight(kind::COIL).speed) as u16;
        }
    }

    /// Whirlwind: lifetime 800, damage 4, started ((r & 127) − 64) px off the target point's centre on each axis
    /// (2 draws). Round-45 errata: its destination is its current goal, the target point's centre at first.
    fn whirlwind(&mut self, caster: UnitId, tile: (i32, i32)) {
        let c = centre_of_tile(tile);
        self.place_piece(|w| {
            let x = c.0 + (w.rng.draw() & 127) as i32 - 64;
            let y = c.1 + (w.rng.draw() & 127) as i32 - 64;
            Piece {
                kind: kind::WHIRLWIND,
                shooter: caster,
                target: None,
                pos: (x, y),
                damage: 4,
                fx: Fx::Whirl {
                    line: Line::new((x, y), c),
                    life: 800,
                    goal: c,
                },
                dest: c,
            }
        });
    }

    /// Runes: up to 5 placed, and 40 mana refunded for each one not placed.
    fn runes(&mut self, i: usize, tile: (i32, i32)) {
        let refund = (5 - self.place_runes(tile)) * 40;
        let m = &mut self.units[i].mana;
        *m = (*m as u32 + refund).min(255) as u8;
    }

    /// Places up to 5 runes (spec, "Runes"); returns how many.
    fn place_runes(&mut self, t: (i32, i32)) -> u32 {
        let mut n = 0;
        for tile in [t, (t.0, t.1 - 1), (t.0 + 1, t.1), (t.0, t.1 + 1), (t.0 - 1, t.1)] {
            if !self.terrain.contains(tile.0, tile.1)
                || self.occupant(Layer::Land, tile.0, tile.1).is_some()
                || self.runes.iter().flatten().any(|r| r.tile == tile)
            {
                continue;
            }
            let Some(slot) = self.runes.iter().position(Option::is_none) else {
                continue;
            };
            self.runes[slot] = Some(Rune { tile, life: RUNE_LIFE });
            self.cosmetic(kind::RUNE, centre_of_tile(tile));
            n += 1;
        }
        n
    }

    /// Death and decay: five clouds placed off the target tile's centre (2 draws each). Round-45 errata: every
    /// cloud's destination is the target tile's centre, whatever its position.
    fn death_and_decay(&mut self, caster: UnitId, tile: (i32, i32)) {
        let c = centre_of_tile(tile);
        for _ in 0..5 {
            self.place_piece(|w| {
                let x = c.0 + ((w.rng.draw() % 5) as i32 - 2) * TILE_PX;
                let y = c.1 + ((w.rng.draw() % 5) as i32 - 2) * TILE_PX;
                Piece {
                    kind: kind::CLOUD,
                    shooter: caster,
                    target: None,
                    pos: (x, y),
                    damage: 10,
                    fx: Fx::Cloud,
                    dest: c,
                }
            });
        }
    }

    // ----- effect slots (spec, "Effect slots", "Cosmetic effects") ---------------------------------------------

    /// A spell piece in the lowest free effect slot (spec, "Effect slots"): `draw` makes the piece's draws and
    /// describes it. Round 81 (audit erratum 1, S98): the slot is found before any draw, so with no free slot the
    /// piece fails without drawing: a blizzard chain makes neither its landing-point nor its start-offset draws, a
    /// landed shard neither the next shard's start-offset draws nor the draw after them (its caller's), and a
    /// death-and-decay cloud and a whirlwind none of their position draws.
    fn place_piece(&mut self, draw: impl FnOnce(&mut World) -> Piece) -> Option<usize> {
        self.free_fx_slot()?;
        let p = draw(self);
        let n = self.new_fx(p.kind, p.shooter, p.target, p.pos, p.damage, p.fx);
        self.set_dest(n, p.dest);
        n
    }

    /// A cosmetic effect (spec, "Cosmetic effects"): only its kind and position are set.
    fn cosmetic(&mut self, k: u8, pos: (i32, i32)) {
        self.new_fx(k, UnitId(0), None, pos, 0, Fx::Cosmetic);
    }

    /// The cosmetic explosion of a unit with the explode flag (corpses spec, "Units with the explode flag").
    pub(crate) fn explosion(&mut self, _unit: UnitId, pos: (i32, i32)) {
        self.cosmetic(kind::EXPLOSION, pos);
    }

    /// The cosmetic effect a missile of kind `missile` leaves at its impact at `pos`, if any (damage spec, "Impact
    /// effects"); it takes the lowest free slot, or isn't made when there is none.
    pub(crate) fn impact_fx(&mut self, missile: u8, _owner: UnitId, pos: (i32, i32)) {
        if let Some(k) = impact_effect(missile) {
            self.cosmetic(k, pos);
        }
    }

    /// The cosmetic smoke of a unit's cannon shot (damage spec), at the missile's start point; none if no slot is free.
    pub(crate) fn cannon_smoke(&mut self, _owner: UnitId, pos: (i32, i32)) {
        self.cosmetic(kind::CANNON_SMOKE, pos);
    }

    /// The cosmetic explosion (kind 0x17) of a hammer or breath splash (damage spec, "Missile flight", gryphon hammer
    /// and dragon breath), at `pos`.
    pub(crate) fn splash_explosion(&mut self, pos: (i32, i32)) {
        self.cosmetic(kind::EXPLOSION, pos);
    }

    /// Puts an effect in the lowest free effect slot after the permanent ones; `None` when the table is full. A
    /// cosmetic kind is created with only its kind and position set (spec, "Cosmetic effects", round-39 errata):
    /// destination (0, 0), no target, no shooter, damage 0, whatever its creator passes.
    pub(crate) fn new_fx(
        &mut self,
        kind: u8,
        owner: UnitId,
        target: Option<UnitId>,
        pos: (i32, i32),
        damage: u8,
        fx: Fx,
    ) -> Option<usize> {
        let (owner, target, damage, from, to) = if is_cosmetic(kind) {
            (UnitId(0), None, 0, (0, 0), (0, 0))
        } else {
            (owner, target, damage, pos, pos)
        };
        let shooter = self.last_slot(owner).filter(|_| owner != UnitId(0));
        let m = crate::battle::Missile {
            kind,
            attacker: owner,
            shooter,
            target,
            from,
            to,
            pos,
            flight: 0,
            elapsed: 0,
            damage,
            splash: false,
            hits_air: false,
            fx,
        };
        let n = self.free_fx_slot()?;
        self.missiles[n] = Some(m);
        Some(n)
    }

    /// The unit an effect credits as its attacker (round-56 errata, "The shooter after its slot is released"): its
    /// shooter while that unit still has the recorded slot (released or not), else whatever unit holds the slot now.
    pub(crate) fn effect_shooter(&self, m: &crate::battle::Missile) -> UnitId {
        let Some(slot) = m.shooter else {
            return m.attacker;
        };
        if self.last_slot(m.attacker) == Some(slot) {
            return m.attacker;
        }
        let lo = self.units.partition_point(|u| u.slot < slot);
        self.units[lo..]
            .iter()
            .take_while(|u| u.slot == slot)
            .map(|u| u.id)
            .last()
            .unwrap_or(m.attacker)
    }

    /// The lowest free effect slot after the permanent ones.
    pub(crate) fn free_fx_slot(&self) -> Option<usize> {
        self.missiles
            .iter()
            .enumerate()
            .skip(slots::PERMANENT_EFFECTS)
            .find(|(_, e)| e.is_none())
            .map(|(n, _)| n)
    }

    /// The pixel position of an effect placed on unit `t` from its type's box (spec, "Cosmetic effects"): its pixel
    /// position plus (floor(w / 2), floor(h / 2) − 4).
    fn on_unit(&self, t: usize) -> (i32, i32) {
        let u = &self.units[t];
        let b = self.stats_of(u.kind).box_size;
        (u.pos.0 + b.0 as i32 / 2, u.pos.1 + b.1 as i32 / 2 - 4)
    }

    /// A spell marker (kind 0x16) on the target of slow, invisibility, bloodlust, haste or unholy armour, and raise
    /// dead's marker on each new skeleton (spec, "Cosmetic effects"; round-45 errata).
    fn marker_on(&mut self, t: usize) {
        let pos = self.on_unit(t);
        self.cosmetic(kind::BLINK, pos);
    }

    /// Sets the destination of the effect just made in slot `n`, if it was made.
    fn set_dest(&mut self, n: Option<usize>, to: (i32, i32)) {
        if let Some(m) = n.and_then(|n| self.missiles[n].as_mut()) {
            m.to = to;
        }
    }

    /// The exorcism or healing effect on a unit (spec, "Cosmetic effects"). Round-48 errata: no shooter, no target
    /// and destination (0, 0).
    fn wisp(&mut self, k: u8, t: usize) {
        let pos = self.on_unit(t);
        let n = self.new_fx(k, UnitId(0), None, pos, 0, Fx::Wisp);
        self.set_dest(n, (0, 0));
    }

    // ----- 7. the timers pass (spec, "Timers", "The timers pass", "Runes on the map") --------------------------

    /// The timers pass at the end of phase 4: the runes first, then each unit's steps in ascending slot order. Units
    /// that are dying, remains or released are skipped.
    pub(crate) fn timers_pass(&mut self) {
        self.advance_runes();
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if u.dying || u.removing || u.remains || u.script.anim == crate::world::RELEASED {
                continue;
            }
            // Caster-ai spec, "Marks": the seven target marks last until the unit's next visit here.
            self.units[i].marks = 0;
            self.regenerate(i);
            // Round 81 (audit erratum 7; `xhuman10-invisible-dwarves-150`, S93): a unit killed in step 2 still runs
            // steps 3 to 5 in this pass; from the next pass on it is dying and skipped.
            self.kill_marked_demolition(i);
            self.count_timers(i);
            self.gain_mana_or_decay(i);
        }
    }

    /// Step 1, berserker regeneration: the count in the mana byte, 1 HP every 240 passes.
    fn regenerate(&mut self, i: usize) {
        let u = &self.units[i];
        // Our reading: the regeneration upgrade is the level line it shares with marksmanship.
        if u.kind != unit::BERSERKER || !self.players[u.owner as usize].upgrades.marksmanship {
            return;
        }
        let max = self.stats_of(u.kind).hit_points;
        let u = &mut self.units[i];
        u.mana += 1;
        if u.mana >= 240 {
            u.mana = 0;
            if u.hp < max {
                u.hp += 1;
            }
        }
    }

    /// Step 2: a demolition squad or sapper with its invisibility or unholy-armour timer running is killed.
    fn kill_marked_demolition(&mut self, i: usize) {
        let u = &self.units[i];
        if matches!(u.kind, unit::DEMOLITION | unit::SAPPER) && (u.invisibility > 0 || u.unholy_armor > 0) {
            self.kill(i);
        }
    }

    /// Step 3, the five timers: each moves 1 toward 0.
    fn count_timers(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.invisibility = u.invisibility.saturating_sub(1);
        u.unholy_armor = u.unholy_armor.saturating_sub(1);
        u.bloodlust = u.bloodlust.saturating_sub(1);
        u.flame_shield = u.flame_shield.saturating_sub(1);
        u.haste -= u.haste.signum();
    }

    /// Steps 4 and 5: mana for a caster (+1 every 40 passes up to 255), else decay for a unit created during play
    /// with a decay rate. The decay periods left are kept in the mana byte (unit-slots spec, "Creating a unit").
    fn gain_mana_or_decay(&mut self, i: usize) {
        let s = self.stats_of(self.units[i].kind);
        let u = &mut self.units[i];
        if s.flags & flags::CASTER != 0 {
            if u.mana < 255 {
                u.mana_counter = u.mana_counter.wrapping_sub(1);
                if u.mana_counter == 0 {
                    u.mana_counter = 40;
                    u.mana += 1;
                }
            }
        } else if u.decays {
            u.decay_count = u.decay_count.saturating_sub(1);
            if u.decay_count == 0 {
                u.decay_count = s.decay_rate;
                if u.mana > 0 {
                    u.mana -= 1;
                } else if u.hp >= 5 {
                    u.hp -= 5;
                } else {
                    self.kill(i);
                }
            }
        }
    }

    /// The runes, before the units: each counts its lifetime down, shows its marker again every 256 cycles, and goes
    /// off under a ground-layer unit (a direct HP change, no credit), or is gone when its lifetime ends.
    fn advance_runes(&mut self) {
        for r in 0..self.runes.len() {
            let Some(mut rune) = self.runes[r] else { continue };
            rune.life -= 1;
            if rune.life & 0xff == 1 {
                self.cosmetic(kind::RUNE, centre_of_tile(rune.tile));
            }
            let hit = self.occupant(Layer::Land, rune.tile.0, rune.tile.1);
            if let Some(ui) = hit.and_then(|id| self.index(id)) {
                self.runes[r] = None;
                self.sound_at_tile(rune.tile, Sound::Explode);
                self.cosmetic(kind::EXPLOSION, centre_of_tile(rune.tile));
                let u = &mut self.units[ui];
                if u.unholy_armor == 0 && !u.dying {
                    if u.hp <= RUNE_DAMAGE {
                        self.kill(ui);
                    } else {
                        u.hp -= RUNE_DAMAGE;
                    }
                }
            } else if rune.life == 0 {
                self.runes[r] = None;
            } else {
                self.runes[r] = Some(rune);
            }
        }
    }

    // ----- effects in flight (spec, "Spell effects in flight") -------------------------------------------------

    /// Frees effect slot `n`; the value is `false` for "the effect is gone".
    fn gone(&mut self, n: usize) -> bool {
        self.missiles[n] = None;
        false
    }

    /// Updates the spell effect in slot `n` once (called in effect slot order).
    pub(crate) fn step_fx(&mut self, n: usize) {
        let Some(mut m) = self.missiles[n].clone() else { return };
        let age = m.elapsed;
        m.elapsed = m.elapsed.saturating_add(1);
        if self.fx_update(n, &mut m, age) {
            self.missiles[n] = Some(m);
        }
    }

    /// Returns whether the effect stays; when it doesn't, the slot has been freed. Every splash is the one splash of
    /// `battle.rs` ([`World::splash`], the hammer and breath's path splash), credited to the effect's shooter slot.
    fn fx_update(&mut self, n: usize, m: &mut crate::battle::Missile, age: u16) -> bool {
        let owner = self.effect_shooter(m);
        match &mut m.fx {
            // Attack missiles are updated by `battle.rs`.
            Fx::Shot { .. } | Fx::Breath { .. } => true,
            Fx::Cosmetic => {
                // Sounds spec, "Missile impacts": the impact effects' explosion plays at their start step.
                if age == start_cycles(m.kind) && matches!(m.kind, kind::BALLISTA_IMPACT | kind::CANNON_EXPLOSION) {
                    self.sound_at_px(m.pos, Sound::Explode);
                }
                age < start_cycles(m.kind) + 1 + cosmetic_waits(m.kind) || self.gone(n)
            }
            // Round-48 errata (`xhuman10-spells-700` healing, n = 619): the first update starts the 18-cycle flight
            // animation; the step comes 19 cycles after it, and the end in the next update.
            Fx::Wisp => match age {
                0..WISP_STEP => true,
                WISP_STEP => {
                    self.rng.draw();
                    m.pos.0 -= 12;
                    m.pos.1 -= 12;
                    true
                }
                _ => {
                    let t = tile_of(m.pos);
                    self.hit_wall(t, 0);
                    self.gone(n)
                }
            },
            Fx::Fireball { line, count, ending } => {
                if age == 0 {
                    return true;
                }
                // Round-48 errata (`xhuman10-spells-700` slot 3, n = 393): a fireball marked as ending is removed at
                // its next update, with no draw and no move, touching a wall on its final tile.
                if *ending {
                    let t = tile_of(m.pos);
                    self.hit_wall(t, m.damage);
                    return self.gone(n);
                }
                self.rng.draw();
                let speed = if line.remaining < 50 { 6 } else { 12 };
                line.step(&mut m.pos, speed);
                let limit = if m.damage == 40 {
                    FIREBALL_COUNT_LIMIT
                } else {
                    FIREBALL_OTHER_LIMIT
                };
                if *count > limit {
                    *ending = true;
                } else if line.remaining < 50 {
                    *count += 1;
                    // Round-40 errata: each splash follows the hammer and breath rule. It needs a free effect slot (with
                    // none, no splash or explosion on that step), and the explosion has only kind and position set.
                    if matches!(*count, 8 | 16 | 24 | 32 | 40) && self.free_fx_slot().is_some() {
                        let pos = m.pos;
                        self.splash(pos, m.damage, owner, Splash::Path);
                        self.splash_explosion(pos);
                    }
                }
                true
            }
            Fx::Bolt { line, done } => {
                if age == 0 {
                    return true;
                }
                // Round-48 errata (`xhuman10-blizzard-600` slot 9, n = 426): after making its flames the bolt stays,
                // unmoved, and is removed at its next update with no draw.
                if *done {
                    return self.gone(n);
                }
                self.rng.draw();
                // Round 81 (audit erratum 4): on the step after it arrives the bolt makes its 5 flames whatever the
                // target's state; flames made for a unit that is dying end at their first step. Sounds spec, "Other
                // explosions": the bolt's explosion plays as it makes them.
                if line.remaining <= 0 {
                    if let Some(t) = m.target {
                        self.sound_at_px(m.pos, Sound::Explode);
                        let c = self.unit(t).map_or(m.to, |u| u.centre_px());
                        self.make_flames(t, c);
                    }
                    *done = true;
                    return true;
                }
                line.step(&mut m.pos, EFFECT_SPEED[kind::BOLT as usize]);
                true
            }
            Fx::Flame { point, life } => {
                // Round-48 errata (`xhuman10-blizzard-600` slot 11, n = 426 to 432): a flame waits at its start point
                // and makes its first path step 6 cycles after its first update; its lifetime count starts there.
                // Round-49 errata (static study): it doesn't look at the shielded unit during that wait. A flame that
                // finds the unit dying or gone at its first step ends there, its slot freed at its next update (its
                // lifetime count is 0 only then); from then on it ends as soon as the unit is dying or gone.
                // Round-50 errata (static study): every step makes that check, and an ending flame (the unit dying or
                // gone, or its count reaching 0) has its slot freed at its next update. Round 81: a unit inside
                // something is neither ([`World::flame_centre`]).
                if age < FLAME_FIRST_STEP {
                    return true;
                }
                if *life == 0 {
                    return self.gone(n);
                }
                // Round-54 errata (`xhuman10-slow-3000` slot 4, n = 740): each step counts down first, and a step that
                // reaches 0 ends the flame without moving or splashing.
                *life -= 1;
                if *life == 0 {
                    return true;
                }
                let Some(c) = self.flame_centre(m.attacker) else {
                    *life = 0;
                    return true;
                };
                *point = (*point + 1) % 51;
                let p = FLAME_PATH[*point as usize];
                m.pos = (c.0 + p.0, c.1 + p.1);
                if *life & 15 == 1 {
                    let pos = m.pos;
                    self.splash(pos, m.damage, owner, Splash::Path);
                }
                true
            }
            // Round-45 errata, "Blizzard shards": a shard lands in the same update as the flight step that takes it to or
            // past its landing point (unclamped). Its own slot isn't free yet, so the chain's next shard takes another.
            // It then stays, unmoving, for 4 more cycles and is removed in its 5th update after landing
            // (`xhuman10-blizzard` slots 7 and 8, n = 170 to 175). A remaining distance of exactly 0 never happens
            // (round 81, open question closed).
            Fx::Shard { line, repeats, landed } => {
                if *landed > 0 {
                    *landed -= 1;
                    return *landed > 0 || self.gone(n);
                }
                if age < 12 {
                    return true;
                }
                line.step(&mut m.pos, EFFECT_SPEED[kind::SHARD as usize]);
                if line.remaining > 0 {
                    self.rng.draw();
                    return true;
                }
                *landed = SHARD_LINGER + 1;
                let (pos, repeats, land) = (m.pos, *repeats, m.to);
                self.splash(pos, m.damage, owner, Splash::Path);
                if repeats > 0 && self.place_piece(|w| w.shard(owner, land, repeats - 1)).is_some() {
                    self.rng.draw();
                }
                true
            }
            Fx::Cloud => {
                match age {
                    34..=250 if (age - 34).is_multiple_of(24) => {
                        let pos = m.pos;
                        self.splash(pos, m.damage, owner, Splash::Path);
                    }
                    275 => {
                        let t = tile_of(m.pos);
                        self.hit_wall(t, m.damage);
                        return self.gone(n);
                    }
                    _ => {}
                }
                true
            }
            Fx::Whirl { line, life, goal } => {
                if age == 0 || !age.is_multiple_of(3) {
                    return true;
                }
                if *life & 1 == 1 {
                    let pos = m.pos;
                    self.splash(pos, m.damage, owner, Splash::Path);
                }
                *life -= 1;
                if *life == 0 {
                    let t = tile_of(m.pos);
                    self.hit_wall(t, m.damage);
                    return self.gone(n);
                }
                line.step(&mut m.pos, 1);
                if m.pos == *goal {
                    let x = m.pos.0 + (self.rng.draw() & 127) as i32 - 64;
                    let y = m.pos.1 + (self.rng.draw() & 127) as i32 - 64;
                    *goal = (x, y);
                    *line = Line::new(m.pos, *goal);
                    m.to = (x, y);
                }
                true
            }
        }
    }

    /// The bolt's five flames on shielded unit `t`, around centre `c`, at path points 0, 10, 20, 30 and 40. Round-48
    /// errata: the shielded unit is each flame's shooter; no target, destination (0, 0).
    fn make_flames(&mut self, t: UnitId, c: (i32, i32)) {
        for (point, life) in FLAMES {
            let p = FLAME_PATH[point as usize];
            self.place_piece(|_| Piece {
                kind: kind::FLAME,
                shooter: t,
                target: None,
                pos: (c.0 + p.0, c.1 + p.1),
                damage: 4,
                fx: Fx::Flame { point, life },
                dest: (0, 0),
            });
        }
    }

    /// The centre a flame circles: its shielded unit's, or `None` once the flame ends for the unit (it is dying,
    /// remains or gone). Round 81 (audit erratum 4; `xhuman10-shield-mine-hits-800`, S94 to S96): a unit inside a gold
    /// mine, a building or a transport is neither, so its flames go on circling its stored position (inside a mine or a
    /// building, the building's top-left tile) with the usual offset. Their splash hits that building, credited to the
    /// unit inside; the unit itself, off the map's unit marks and the splash's attacker, is never hit.
    fn flame_centre(&self, shielded: UnitId) -> Option<(i32, i32)> {
        self.unit(shielded).filter(|u| !u.dying).map(|u| u.centre_px())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use crate::rng::Rng;
    use crate::world::tests::{field, run};
    use tidewater_formats::udta::UnitStats;

    const MAGE: u8 = 0x0a;
    const DEATH_KNIGHT: u8 = 0x0b;
    const PALADIN: u8 = 0x0c;
    const KNIGHT: u8 = 6;
    const GRYPHON: u8 = 0x16;

    /// A world of open ground with the spell-relevant unit types filled in (footman 0 is organic).
    fn lab(size: u16) -> World {
        let mut w = field(size, size);
        let base = UnitStats {
            size: (1, 1),
            box_size: (32, 32),
            ..Default::default()
        };
        let land = flags::LAND | flags::ORGANIC;
        let st = &mut w.stats.0;
        st[0].flags |= flags::ORGANIC;
        st[KNIGHT as usize] = UnitStats {
            hit_points: 100,
            flags: land,
            ..base
        };
        st[MAGE as usize] = UnitStats {
            hit_points: 60,
            flags: land | flags::CASTER,
            ..base
        };
        st[PALADIN as usize] = st[MAGE as usize];
        st[DEATH_KNIGHT as usize] = UnitStats {
            hit_points: 90,
            flags: land | flags::CASTER | flags::UNDEAD | flags::CAN_ATTACK,
            basic_damage: 6,
            piercing_damage: 3,
            attack_range: 6,
            missile: 0x0f,
            can_target: 7,
            ..base
        };
        st[unit::BERSERKER as usize] = UnitStats {
            hit_points: 55,
            flags: land,
            ..base
        };
        st[unit::SAPPER as usize] = UnitStats {
            hit_points: 40,
            flags: flags::LAND,
            ..base
        };
        st[unit::SKELETON as usize] = UnitStats {
            hit_points: 40,
            flags: land | flags::UNDEAD,
            decay_rate: 100,
            ..base
        };
        st[unit::EYE as usize] = UnitStats {
            hit_points: 100,
            flags: flags::AIR,
            decay_rate: 3,
            ..base
        };
        st[unit::CRITTER as usize] = UnitStats {
            hit_points: 10,
            flags: flags::LAND | flags::CRITTER,
            ..base
        };
        st[GRYPHON as usize] = UnitStats {
            hit_points: 100,
            flags: flags::AIR | flags::ORGANIC,
            ..base
        };
        w.stride[unit::EYE as usize] = 2;
        w.stride[GRYPHON as usize] = 2;
        w
    }

    /// A caster with `mana`, its regeneration counter parked so a test of a few dozen cycles sees no gain.
    fn caster(w: &mut World, kind: u8, tile: (i32, i32), mana: u8) -> UnitId {
        let id = w.spawn(kind, 0, tile, 0).unwrap();
        let i = w.index(id).unwrap();
        (w.units[i].mana, w.units[i].mana_counter) = (mana, 255);
        id
    }

    fn mana(w: &World, id: UnitId) -> u8 {
        w.unit(id).unwrap().mana
    }

    fn hp(w: &World, id: UnitId) -> u16 {
        w.unit(id).unwrap().hp
    }

    fn set_hp(w: &mut World, id: UnitId, hp: u16) {
        let i = w.index(id).unwrap();
        w.units[i].hp = hp;
    }

    fn cast(w: &mut World, id: UnitId, spell: u8, target: Option<UnitId>, tile: (i32, i32)) {
        w.step(&[Command::Cast {
            player: 0,
            unit: id,
            spell,
            target,
            tile,
        }]);
    }

    /// Strikes at once, as the casting animation's strike point would, with `order` set directly.
    fn strike(w: &mut World, id: UnitId, spell: u8, target: Option<UnitId>, tile: (i32, i32)) {
        let i = w.index(id).unwrap();
        w.units[i].order = Order::Cast { spell, target, tile };
        w.strike(i);
    }

    fn fx_of_kind(w: &World, k: u8) -> usize {
        w.missiles.iter().flatten().filter(|m| m.kind == k).count()
    }

    fn idle_pending(w: &World, id: UnitId) -> bool {
        w.unit(id).unwrap().pending == Some(Order::Idle)
    }

    /// The generator state from which the next draws, masked with `mask`, equal `wanted`.
    fn state_for(mask: u32, wanted: &[u32]) -> u32 {
        (0u32..)
            .find(|&s| {
                let mut r = Rng::new(s);
                wanted.iter().all(|&v| r.draw() & mask == v)
            })
            .unwrap()
    }

    fn advanced(state: u32, n: usize) -> Rng {
        let mut r = Rng::new(state);
        for _ in 0..n {
            r.draw();
        }
        r
    }

    // ----- timers, mana and decay ----------------------------------------------------------------------------

    #[test]
    fn new_casters_start_with_85_mana_and_gain_one_per_40_passes() {
        let mut w = lab(16);
        let m = w.spawn(MAGE, 0, (1, 1), 0).unwrap();
        assert_eq!((mana(&w, m), w.unit(m).unwrap().mana_counter), (85, 40));
        run(&mut w, 39);
        assert_eq!(mana(&w, m), 85);
        run(&mut w, 1);
        assert_eq!(mana(&w, m), 86);
        run(&mut w, 40);
        assert_eq!(mana(&w, m), 87);
    }

    #[test]
    fn a_building_with_the_caster_flag_gains_mana_from_its_256th_pass() {
        // Spec vector (round-35 errata): custom data gives a farm the caster flag and magic 0. It starts at its
        // "magic" value with its counter at 0, which wraps to 255 on the first pass: mana 0 until the 256th pass,
        // then +1 every 40 passes.
        let mut w = lab(16);
        const FARM: u8 = 0x3a;
        w.stats.0[FARM as usize] = UnitStats {
            size: (2, 2),
            box_size: (64, 64),
            hit_points: 400,
            flags: flags::BUILDING | flags::CASTER,
            magic: false,
            ..Default::default()
        };
        let f = w.spawn(FARM, 0, (4, 4), 0).unwrap();
        assert_eq!((mana(&w, f), w.unit(f).unwrap().mana_counter), (0, 0));
        run(&mut w, 255);
        assert_eq!(mana(&w, f), 0);
        run(&mut w, 1);
        assert_eq!(mana(&w, f), 1);
        run(&mut w, 40);
        assert_eq!(mana(&w, f), 2);
    }

    #[test]
    fn mana_stops_at_255() {
        let mut w = lab(16);
        let m = w.spawn(MAGE, 0, (1, 1), 0).unwrap();
        let i = w.index(m).unwrap();
        w.units[i].mana = 254;
        run(&mut w, 40);
        assert_eq!(mana(&w, m), 255);
        run(&mut w, 120);
        assert_eq!(mana(&w, m), 255);
    }

    #[test]
    fn non_casters_have_no_mana() {
        let mut w = lab(16);
        let f = w.spawn(0, 0, (1, 1), 0).unwrap();
        run(&mut w, 100);
        assert_eq!(mana(&w, f), 0);
    }

    #[test]
    fn summoned_units_decay_on_the_spec_timetable() {
        let mut w = lab(16);
        let eye = w.spawn(unit::EYE, 0, (2, 2), 0).unwrap();
        assert_eq!(w.unit(eye).unwrap().mana, 255);
        run(&mut w, 827);
        assert!(w.unit(eye).unwrap().alive());
        // 255 periods, then 20 losses of 5 hit points: the unit is still alive at 0 hit points.
        assert_eq!(hp(&w, eye), 0);
        run(&mut w, 1);
        assert!(!w.unit(eye).unwrap().alive(), "killed on pass 828");
    }

    #[test]
    fn a_skeleton_lasts_26400_passes() {
        let mut w = lab(16);
        let s = w.spawn(unit::SKELETON, 0, (2, 2), 0).unwrap();
        run(&mut w, 26399);
        assert!(w.unit(s).unwrap().alive());
        run(&mut w, 1);
        assert!(!w.unit(s).unwrap().alive());
    }

    #[test]
    fn units_created_while_the_map_loads_do_not_decay() {
        let mut w = lab(16);
        w.loading = true;
        let s = w.spawn(unit::SKELETON, 0, (2, 2), 0).unwrap();
        w.loading = false;
        assert!(!w.unit(s).unwrap().decays);
        run(&mut w, 30_000);
        assert!(w.unit(s).unwrap().alive());
    }

    #[test]
    fn creation_draws_skip_the_facing_for_decaying_units() {
        let mut w = lab(16);
        let before = w.rng;
        w.spawn(unit::EYE, 0, (2, 2), 0).unwrap();
        let mut r = before;
        r.draw();
        assert_eq!(w.rng, r, "only the first-wait draw");
        let before = w.rng;
        w.spawn(0, 0, (4, 4), 0).unwrap();
        let mut r = before;
        r.draw();
        r.draw();
        assert_eq!(w.rng, r, "facing, then first wait");
    }

    #[test]
    fn berserkers_regenerate_with_the_upgrade() {
        let mut w = lab(16);
        let b = w.spawn(unit::BERSERKER, 0, (2, 2), 0).unwrap();
        set_hp(&mut w, b, 50);
        run(&mut w, 480);
        assert_eq!(hp(&w, b), 50, "no upgrade");
        w.players[0].upgrades.marksmanship = true;
        run(&mut w, 239);
        assert_eq!(hp(&w, b), 50);
        run(&mut w, 1);
        assert_eq!(hp(&w, b), 51);
    }

    #[test]
    fn timers_run_down_and_the_speed_timer_moves_toward_zero() {
        let mut w = lab(16);
        let f = w.spawn(0, 0, (2, 2), 0).unwrap();
        let i = w.index(f).unwrap();
        let u = &mut w.units[i];
        (u.invisibility, u.unholy_armor, u.bloodlust, u.flame_shield) = (3, 2, 1, 5);
        u.haste = -2;
        run(&mut w, 3);
        let u = w.unit(f).unwrap();
        assert_eq!(
            (u.invisibility, u.unholy_armor, u.bloodlust, u.flame_shield),
            (0, 0, 0, 2)
        );
        assert_eq!(u.haste, 0);
    }

    #[test]
    fn invisible_or_armoured_sappers_are_killed() {
        for armoured in [false, true] {
            let mut w = lab(16);
            let s = w.spawn(unit::SAPPER, 0, (2, 2), 0).unwrap();
            let i = w.index(s).unwrap();
            if armoured {
                w.units[i].unholy_armor = 9;
            } else {
                w.units[i].invisibility = 9;
            }
            assert!(w.unit(s).unwrap().alive());
            run(&mut w, 1);
            assert!(!w.unit(s).unwrap().alive());
        }
    }

    #[test]
    fn the_fog_refresh_counter_starts_at_1_and_counts_100_or_101() {
        let mut w = lab(8);
        assert_eq!(w.vision.countdown, 1);
        run(&mut w, 1);
        assert_eq!(w.vision.countdown, 100);
        let mut w = lab(8);
        w.rules.enhanced = false;
        run(&mut w, 1);
        assert_eq!(w.vision.countdown, 101);
    }

    // ----- commands, orders and unit spells ------------------------------------------------------------------

    fn foe(w: &mut World, tile: (i32, i32)) -> UnitId {
        w.spawn(0, 1, tile, 0).unwrap()
    }

    #[test]
    fn healing_through_the_command_and_the_casting_animation() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 100);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        set_hp(&mut w, f, 30);
        cast(&mut w, p, HEALING, Some(f), (0, 0));
        run(&mut w, 80);
        assert_eq!((hp(&w, f), mana(&w, p)), (46, 4));
        assert_eq!(w.unit(p).unwrap().order, Order::Idle);
    }

    #[test]
    fn a_caster_walks_into_range_before_casting() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (2, 5), 100);
        let f = w.spawn(0, 0, (20, 5), 0).unwrap();
        set_hp(&mut w, f, 30);
        cast(&mut w, p, HEALING, Some(f), (0, 0));
        run(&mut w, 40);
        assert_eq!(hp(&w, f), 30, "still walking");
        run(&mut w, 600);
        assert_eq!(hp(&w, f), 46);
        assert!(w.unit(p).unwrap().tile.0 >= 14, "stopped within 6 tiles");
    }

    #[test]
    fn healing_vectors() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 255);
        let full = w.spawn(0, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, HEALING, Some(full), (0, 0));
        assert_eq!(mana(&w, p), 255, "nothing missing, nothing spent");
        assert!(idle_pending(&w, p));
        let knight = w.spawn(KNIGHT, 0, (7, 5), 0).unwrap();
        set_hp(&mut w, knight, 40);
        strike(&mut w, p, HEALING, Some(knight), (0, 0));
        assert_eq!(
            (hp(&w, knight), mana(&w, p)),
            (80, 15),
            "capped at 40 hit points per cast"
        );
        // Healing needs an organic target; an unorganic one is refused when the order runs.
        let g = w.spawn(unit::SAPPER, 0, (8, 5), 0).unwrap();
        cast(&mut w, p, HEALING, Some(g), (0, 0));
        run(&mut w, 60);
        assert_eq!(mana(&w, p), 15);
    }

    #[test]
    fn healing_is_limited_by_mana() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 20);
        let k = w.spawn(KNIGHT, 0, (6, 5), 0).unwrap();
        set_hp(&mut w, k, 10);
        strike(&mut w, p, HEALING, Some(k), (0, 0));
        assert_eq!((hp(&w, k), mana(&w, p)), (13, 2));
    }

    #[test]
    fn a_dead_or_building_target_costs_nothing() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let b = w.spawn(0x3a, 0, (8, 8), 0).unwrap();
        cast(&mut w, p, HASTE, Some(b), (0, 0));
        run(&mut w, 60);
        assert_eq!((mana(&w, p), w.unit(b).unwrap().haste), (255, 0));
        let f = foe(&mut w, (6, 5));
        let i = w.index(f).unwrap();
        w.kill(i);
        strike(&mut w, p, HASTE, Some(f), (0, 0));
        assert_eq!(mana(&w, p), 255);
        assert!(idle_pending(&w, p));
    }

    #[test]
    fn the_command_is_checked() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 100);
        let f = foe(&mut w, (6, 5));
        let order = |w: &World| w.unit(p).unwrap().pending;
        w.step(&[Command::Cast {
            player: 1,
            unit: p,
            spell: HASTE,
            target: Some(f),
            tile: (0, 0),
        }]);
        assert_eq!(order(&w), None, "not the player's unit");
        cast(&mut w, p, 40, None, (3, 3));
        assert_eq!(order(&w), None, "order 40 is not used");
        cast(&mut w, p, HASTE, None, (3, 3));
        assert_eq!(order(&w), None, "a unit spell needs a target");
        cast(&mut w, p, FIREBALL, Some(f), (3, 3));
        assert!(matches!(
            order(&w),
            Some(Order::Cast {
                target: None,
                tile: (3, 3),
                ..
            })
        ));
    }

    #[test]
    fn the_all_spells_cheat_fills_mana() {
        let mut w = lab(32);
        w.rules.all_spells_cheat = true;
        let p = caster(&mut w, MAGE, (5, 5), 3);
        cast(&mut w, p, FIREBALL, None, (9, 5));
        assert_eq!(mana(&w, p), 255);
    }

    #[test]
    fn a_cast_ends_the_casters_invisibility_and_so_does_an_attack() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 100);
        let i = w.index(p).unwrap();
        w.units[i].invisibility = 900;
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, HASTE, Some(f), (0, 0));
        assert_eq!(w.unit(p).unwrap().invisibility, 0);
        // An attack strike does the same.
        let mut w = lab(32);
        let a = w.spawn(0, 0, (5, 5), 0).unwrap();
        let i = w.index(a).unwrap();
        w.units[i].invisibility = 900;
        let t = foe(&mut w, (6, 5));
        w.step(&[Command::Attack {
            player: 0,
            units: vec![a],
            target: t,
        }]);
        run(&mut w, 60);
        assert!(hp(&w, t) < 60);
        assert_eq!(w.unit(a).unwrap().invisibility, 0);
    }

    /// Spec, "The strike point", step 2 (round 80, S86): the strike point ends the caster's invisibility before the
    /// spell's strike, so a cast whose target has died during the casting animation ends it too, with nothing paid.
    #[test]
    fn a_cast_that_fails_its_target_check_ends_invisibility() {
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (5, 5), 100);
        let i = w.index(m).unwrap();
        w.units[i].invisibility = 900;
        let g = foe(&mut w, (8, 5));
        let gi = w.index(g).unwrap();
        w.kill(gi);
        strike(&mut w, m, SLOW, Some(g), (8, 5));
        assert_eq!((w.unit(m).unwrap().invisibility, mana(&w, m)), (0, 100));
        assert!(idle_pending(&w, m));
    }

    /// Spec, "The strike point", step 2 (round 80, S89): an invisible ogre-mage's eye of Kilrogg with 60 mana fails
    /// its mana check and still ends the invisibility.
    #[test]
    fn a_cast_that_fails_its_mana_check_ends_invisibility() {
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (5, 5), 60);
        let i = w.index(m).unwrap();
        w.units[i].invisibility = 900;
        strike(&mut w, m, EYE_OF_KILROGG, None, (20, 20));
        assert_eq!((w.unit(m).unwrap().invisibility, mana(&w, m)), (0, 60));
        assert!(w.units.iter().all(|u| u.kind != unit::EYE));
        assert!(idle_pending(&w, m));
    }

    /// The strike point is the only place a cast ends invisibility: the spell's strike alone leaves the timer.
    #[test]
    fn the_spells_strike_itself_leaves_invisibility_to_the_strike_point() {
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        let i = w.index(m).unwrap();
        w.units[i].invisibility = 900;
        w.units[i].order = Order::Cast {
            spell: HASTE,
            target: Some(f),
            tile: (6, 5),
        };
        w.cast_strike(i);
        assert_eq!((w.unit(m).unwrap().invisibility, mana(&w, m)), (900, 205));
    }

    /// S84 (`xhuman10-failcast-520`, n = 437): an invisible mage channelling blizzard with 15 mana reaches a strike
    /// point through its casting animation. The timer drops to 0 there, nothing is paid and no shard is made; the
    /// spell order and the casting animation go on with idle pending.
    #[test]
    fn a_channelled_blizzard_short_of_mana_ends_invisibility_at_its_strike() {
        use tidewater_formats::script::anim;
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (5, 5), 15);
        cast(&mut w, m, BLIZZARD, None, (9, 5));
        let i = w.index(m).unwrap();
        w.units[i].invisibility = 1000;
        let mut n = 0;
        while w.unit(m).unwrap().invisibility != 0 {
            w.step(&[]);
            n += 1;
            assert!(n < 80, "no strike");
            assert!(w.unit(m).unwrap().invisibility > 900 || w.unit(m).unwrap().invisibility == 0);
        }
        let u = w.unit(m).unwrap();
        assert_eq!((u.mana, fx_of_kind(&w, kind::SHARD)), (15, 0));
        assert!(matches!(u.order, Order::Cast { spell: BLIZZARD, .. }));
        assert_eq!((u.script.anim, u.pending), (anim::ATTACK, Some(Order::Idle)));
    }

    #[test]
    fn timer_spells() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let t = w.spawn(0, 0, (6, 5), 0).unwrap();
        let set = |w: &mut World, v: i16| {
            let i = w.index(t).unwrap();
            w.units[i].haste = v;
        };
        let haste = |w: &World| w.unit(t).unwrap().haste;
        for (spell, before, after) in [
            (SLOW, 300, -700),
            (SLOW, -200, -1000),
            (HASTE, -300, 700),
            (HASTE, 400, 1000),
        ] {
            set(&mut w, before);
            strike(&mut w, p, spell, Some(t), (0, 0));
            assert_eq!(haste(&w), after, "spell {spell} on {before}");
        }
        assert_eq!(mana(&w, p), 255 - 4 * 50);
        strike(&mut w, p, BLOODLUST, Some(t), (0, 0));
        assert_eq!(w.unit(t).unwrap().bloodlust, 1000);
        assert_eq!(mana(&w, p), 5);
        let i = w.index(p).unwrap();
        w.units[i].mana = 255;
        strike(&mut w, p, INVISIBILITY, Some(t), (0, 0));
        assert_eq!(w.unit(t).unwrap().invisibility, 2000);
        assert_eq!(mana(&w, p), 55);
    }

    /// Spells spec, kind 0x16 markers, the `xorc01-death-knight` (haste, n = 37) and `xorc12-ogre-mage` (bloodlust,
    /// n = 36) vectors: an ogre with a 42 x 42 box gets its marker at its pixel plus (21, 17), with destination (0, 0),
    /// no target, no shooter and damage 0.
    #[test]
    fn a_marker_on_a_unit_is_placed_from_its_box() {
        for (spell, px, want) in [
            (HASTE, (2848, 2944), (2869, 2961)),
            (BLOODLUST, (3712, 288), (3733, 305)),
        ] {
            let mut w = lab(128);
            w.stats.0[0].box_size = (42, 42);
            let tile = (px.0 / 32, px.1 / 32);
            let p = caster(&mut w, DEATH_KNIGHT, (tile.0 + 1, tile.1), 255);
            let t = w.spawn(0, 0, tile, 0).unwrap();
            let ti = w.index(t).unwrap();
            w.units[ti].pos = px;
            strike(&mut w, p, spell, Some(t), (0, 0));
            let m = w.missiles.iter().flatten().find(|m| m.kind == 0x16).unwrap();
            assert_eq!((m.pos, m.from, m.to), (want, (0, 0), (0, 0)), "spell {spell}");
            assert_eq!((m.target, m.attacker, m.damage), (None, UnitId(0), 0));
        }
    }

    #[test]
    fn a_spell_without_enough_mana_does_nothing() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 49);
        let t = w.spawn(0, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, BLOODLUST, Some(t), (0, 0));
        assert_eq!((w.unit(t).unwrap().bloodlust, mana(&w, p)), (0, 49));
        assert!(idle_pending(&w, p));
    }

    #[test]
    fn unholy_armour_halves_hit_points() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let a = w.spawn(KNIGHT, 0, (6, 5), 0).unwrap();
        let b = w.spawn(KNIGHT, 0, (7, 5), 0).unwrap();
        set_hp(&mut w, a, 61);
        set_hp(&mut w, b, 1);
        strike(&mut w, p, UNHOLY_ARMOUR, Some(a), (0, 0));
        strike(&mut w, p, UNHOLY_ARMOUR, Some(b), (0, 0));
        assert_eq!((hp(&w, a), hp(&w, b)), (30, 1));
        assert_eq!(
            (w.unit(a).unwrap().unholy_armor, w.unit(b).unwrap().unholy_armor),
            (500, 500)
        );
        // Armoured units take no damage from hits.
        let i = w.index(a).unwrap();
        w.damage(i, 20, p);
        assert_eq!(hp(&w, a), 30);
    }

    #[test]
    fn invisibility_on_a_sapper_kills_it_in_the_next_timers_pass() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let s = w.spawn(unit::SAPPER, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, INVISIBILITY, Some(s), (0, 0));
        assert!(w.unit(s).unwrap().alive());
        run(&mut w, 1);
        assert!(!w.unit(s).unwrap().alive());
    }

    #[test]
    fn flame_shield_refusals_cost_nothing() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        let i = w.index(f).unwrap();
        w.units[i].flame_shield = 120;
        strike(&mut w, p, FLAME_SHIELD, Some(f), (0, 0));
        assert_eq!((mana(&w, p), fx_of_kind(&w, 3)), (255, 0));
        assert!(idle_pending(&w, p));
        let g = w.spawn(GRYPHON, 0, (8, 8), 0).unwrap();
        strike(&mut w, p, FLAME_SHIELD, Some(g), (0, 0));
        assert_eq!((mana(&w, p), fx_of_kind(&w, 3)), (255, 0));
    }

    /// Runs until the flame shield's five flames exist; returns their offsets from the target's centre.
    fn flame_offsets(w: &World, centre: (i32, i32)) -> Vec<(i32, i32)> {
        w.missiles
            .iter()
            .flatten()
            .filter(|m| m.kind == 4)
            .map(|m| (m.pos.0 - centre.0, m.pos.1 - centre.1))
            .collect()
    }

    #[test]
    fn flame_shield_flames_follow_the_path() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        let c = w.unit(f).unwrap().centre_px();
        strike(&mut w, p, FLAME_SHIELD, Some(f), (0, 0));
        assert_eq!((mana(&w, p), w.unit(f).unwrap().flame_shield), (175, 500));
        let mut n = 0;
        while fx_of_kind(&w, 4) < 5 {
            w.step(&[]);
            n += 1;
            assert!(n < 40, "the flames never came");
        }
        assert_eq!(
            flame_offsets(&w, c),
            [(1, -34), (30, -20), (19, 8), (-11, 10), (-27, -16)]
        );
        // Round-48 errata: each flame waits at its start point and steps first 6 cycles after its first update.
        let first = flame_offsets(&w, c);
        run(&mut w, 5);
        assert_eq!(flame_offsets(&w, c), first, "still waiting");
        let m = w.missiles.iter().flatten().find(|m| m.kind == 4).unwrap();
        assert_eq!((m.target, m.to, m.attacker), (None, (0, 0), f));
        run(&mut w, 5);
        assert!(flame_offsets(&w, c).contains(&(31, -5)), "start 10, step 5: point 15");
        run(&mut w, 6);
        assert!(
            flame_offsets(&w, c).contains(&(1, -34)),
            "start 40, step 11: wraps to point 0"
        );
        // The shielded unit is never hurt by its own flames, and they all end by 527 steps.
        run(&mut w, 540);
        assert_eq!(fx_of_kind(&w, 4), 0);
        assert_eq!(hp(&w, f), 60);
    }

    /// Round-49 errata (static study): a flame doesn't look at the shielded unit during its wait; at its first step
    /// it finds the unit dying and ends there, its slot freed at its next update.
    #[test]
    fn flames_check_the_shielded_unit_from_their_first_step() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, FLAME_SHIELD, Some(f), (0, 0));
        let mut n = 0;
        while fx_of_kind(&w, 4) < 5 {
            w.step(&[]);
            n += 1;
            assert!(n < 40, "the flames never came");
        }
        let fi = w.index(f).unwrap();
        w.kill(fi);
        run(&mut w, 5);
        assert_eq!(fx_of_kind(&w, 4), 5, "still waiting");
        run(&mut w, 1);
        assert_eq!(fx_of_kind(&w, 4), 5, "ended at the first step, slots still held");
        run(&mut w, 1);
        assert_eq!(fx_of_kind(&w, 4), 0);
    }

    /// Round-50 errata (static study): every flame step checks the shielded unit, and a flame that ends on a later
    /// step also has its slot freed at its next update.
    #[test]
    fn a_flame_ending_on_a_later_step_frees_its_slot_at_its_next_update() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, FLAME_SHIELD, Some(f), (0, 0));
        let mut n = 0;
        while fx_of_kind(&w, 4) < 5 {
            w.step(&[]);
            n += 1;
            assert!(n < 40, "the flames never came");
        }
        run(&mut w, 20);
        let fi = w.index(f).unwrap();
        w.kill(fi);
        run(&mut w, 1);
        assert_eq!(fx_of_kind(&w, 4), 5, "ended at this step, slots still held");
        run(&mut w, 1);
        assert_eq!(fx_of_kind(&w, 4), 0);
    }

    /// Round-54 errata (`xhuman10-slow-3000` slot 4, n = 740): each flame step counts down first, and the step that
    /// reaches 0 ends the flame where it stands, without moving or splashing; its slot is freed at its next update.
    #[test]
    fn a_flame_ends_without_moving_on_its_last_step() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        strike(&mut w, p, FLAME_SHIELD, Some(f), (0, 0));
        let mut n = 0;
        while fx_of_kind(&w, 4) < 5 {
            w.step(&[]);
            n += 1;
            assert!(n < 40, "the flames never came");
        }
        run(&mut w, 20);
        let slot = w
            .missiles
            .iter()
            .position(|m| m.as_ref().is_some_and(|m| m.kind == 4))
            .unwrap();
        let flame = |w: &World| w.missiles[slot].as_ref().map(|m| (m.pos, m.fx));
        if let Some(Fx::Flame { life, .. }) = w.missiles[slot].as_mut().map(|m| &mut m.fx) {
            *life = 2;
        }
        let (start, _) = flame(&w).unwrap();
        run(&mut w, 1);
        let (moved, fx) = flame(&w).unwrap();
        assert_ne!(moved, start, "a count of 1 left: the flame steps");
        assert!(matches!(fx, Fx::Flame { life: 1, .. }));
        run(&mut w, 1);
        let (last, fx) = flame(&w).unwrap();
        assert_eq!(last, moved, "the step that reaches 0 doesn't move");
        assert!(matches!(fx, Fx::Flame { life: 0, .. }));
        run(&mut w, 1);
        assert!(
            w.missiles[slot].as_ref().is_none_or(|m| m.kind != 4),
            "freed at its next update"
        );
    }

    /// Round-54 errata, "Giving the order" (`xhuman10-slow-3000` mage 1481, n = 302): a spell command's target unit
    /// gets the command check. A dying or undetected target is dropped and idle becomes the pending order, which counts
    /// as a command; a live, detected one gives the spell order as usual.
    #[test]
    fn a_spell_command_on_a_dead_or_undetected_target_leaves_a_pending_idle() {
        for case in ["live", "dying", "undetected"] {
            let mut w = lab(32);
            let m = caster(&mut w, MAGE, (5, 5), 255);
            let g = w.spawn(0, 1, (8, 5), 0).unwrap();
            let gi = w.index(g).unwrap();
            match case {
                "dying" => w.kill(gi),
                "undetected" => w.units[gi].detected_by &= !crate::vision::bit(0),
                _ => {}
            }
            w.cast_command(0, m, SLOW, Some(g), (8, 5));
            let u = w.unit(m).unwrap();
            let expected = if case == "live" {
                Order::Cast {
                    spell: SLOW,
                    target: Some(g),
                    tile: (8, 5),
                }
            } else {
                Order::Idle
            };
            assert_eq!(
                (u.order, u.pending, u.commanded),
                (Order::Idle, Some(expected), true),
                "{case}"
            );
        }
    }

    /// Spec, "Walking and the decision" (round 80): the decision doesn't test detection, so a target that turns
    /// invisible after the command is still cast on.
    #[test]
    fn the_decision_does_not_test_detection() {
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (5, 5), 255);
        let g = foe(&mut w, (8, 5));
        cast(&mut w, m, SLOW, Some(g), (8, 5));
        let gi = w.index(g).unwrap();
        w.units[gi].invisibility = 1000;
        w.step(&[]);
        assert!(!w.detected_by(0, g), "the target is undetected");
        run(&mut w, 80);
        assert!(w.unit(g).unwrap().haste < 0, "slowed");
        assert_eq!(mana(&w, m), 205);
    }

    /// Orders spec, "A player's command forgets the attacker" (round-56 static correction): a direct spell command
    /// forgets the caster's recorded attacker once it is given.
    #[test]
    fn a_direct_cast_forgets_the_attacker() {
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (5, 5), 255);
        let g = w.spawn(0, 1, (8, 5), 0).unwrap();
        let mi = w.index(m).unwrap();
        w.units[mi].attacked_by = Some(g);
        w.cast_command(0, m, SLOW, Some(g), (8, 5));
        assert_eq!(w.unit(m).unwrap().attacked_by, None);
        assert!(matches!(
            w.unit(m).unwrap().pending,
            Some(Order::Cast { spell: SLOW, .. })
        ));
    }

    /// Round-50 errata (`human14-10000` death knight 1577, n = 3740): a spell walk with no route makes the caster idle
    /// at once, and its idle logic runs in the same update (still animation, wait 1).
    #[test]
    fn a_spell_walk_with_no_route_goes_idle_at_once() {
        let mut w = lab(32);
        let d = caster(&mut w, PALADIN, (5, 5), 255);
        let f = w.spawn(0, 0, (20, 5), 0).unwrap();
        // The caster is walled in.
        for y in 4..=6 {
            for x in 4..=6 {
                if (x, y) != (5, 5) {
                    w.terrain.set_bits(x, y, crate::terrain::bits::OBSTACLE);
                }
            }
        }
        let i = w.index(d).unwrap();
        w.units[i].order = Order::Cast {
            spell: HEALING,
            target: Some(f),
            tile: (0, 0),
        };
        assert!(w.run_spell(i));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.script.anim, u.script.wait),
            (Order::Idle, None, tidewater_formats::script::anim::STILL, 1)
        );
    }

    #[test]
    fn flames_hurt_the_neighbours() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        let n = w.spawn(KNIGHT, 1, (6, 6), 0).unwrap();
        strike(&mut w, p, FLAME_SHIELD, Some(f), (0, 0));
        run(&mut w, 600);
        assert!(w.unit(n).is_none_or(|u| u.hp < 100));
        assert_eq!(hp(&w, f), 60);
    }

    #[test]
    fn polymorph_kills_marks_and_creates_a_critter() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(0, 0, (6, 5), 0).unwrap();
        let before = w.rng.state;
        strike(&mut w, p, POLYMORPH, Some(f), (0, 0));
        assert_eq!(w.rng, advanced(before, 2), "the critter's facing, then its first wait");
        assert_eq!(mana(&w, p), 55);
        let u = w.unit(f).unwrap();
        assert!(u.dying && u.used);
        let critters: Vec<_> = w.units.iter().filter(|u| u.kind == unit::CRITTER).collect();
        assert_eq!(
            (critters.len(), critters[0].owner, critters[0].tile),
            (1, slots::NEUTRAL, (6, 5))
        );
        // The body keeps the used mark, and raise dead skips it.
        run(&mut w, 8);
        let u = w.unit(f).unwrap();
        assert!(u.remains && u.used && u.kind == crate::corpses::DEAD_BODY);
        strike(&mut w, p, RAISE_DEAD, None, (6, 5));
        assert!(w.units.iter().all(|u| u.kind != unit::SKELETON));
    }

    #[test]
    fn eye_of_kilrogg_appears_next_to_the_caster() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let before = w.rng;
        strike(&mut w, p, EYE_OF_KILROGG, None, (20, 20));
        let eye = w.units.iter().find(|u| u.kind == unit::EYE).expect("an eye");
        assert!(
            eye.tile != (5, 5) && crate::movement::chebyshev(eye.tile, (5, 5)) <= 3,
            "{:?}",
            eye.tile
        );
        assert_eq!((eye.owner, mana(&w, p)), (0, 185));
        assert_eq!(w.rng, advanced(before.state, 1), "the eye's first wait");
    }

    #[test]
    fn eye_of_kilrogg_with_no_room_still_costs_mana() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        for i in 0..w.terrain.words.len() {
            w.terrain.words[i] |= 0xffff;
        }
        strike(&mut w, p, EYE_OF_KILROGG, None, (20, 20));
        assert!(w.units.iter().all(|u| u.kind != unit::EYE));
        assert_eq!(mana(&w, p), 185);
        assert!(w.events.iter().any(|e| matches!(e, Event::NoRoom { kind: 0x2d, .. })));
    }

    // ----- point spells and effects --------------------------------------------------------------------------

    fn undead_skeleton(w: &mut World, tile: (i32, i32)) -> UnitId {
        w.loading = true; // no decay, so the test can look at it as long as it likes
        let s = w.spawn(unit::SKELETON, 1, tile, 0).unwrap();
        w.loading = false;
        s
    }

    #[test]
    fn exorcism_vectors() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 100);
        let s = undead_skeleton(&mut w, (8, 8));
        strike(&mut w, p, EXORCISM, None, (8, 8));
        assert_eq!((hp(&w, s), mana(&w, p)), (15, 0));
        assert!(
            w.unit(s).unwrap().attacked_by.is_some(),
            "told it was attacked, even by the 0 of the second visit"
        );

        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 255);
        let s = undead_skeleton(&mut w, (8, 8));
        strike(&mut w, p, EXORCISM, None, (8, 8));
        assert!(!w.unit(s).unwrap().alive());
        assert_eq!(mana(&w, p), 95);
        assert_eq!(fx_of_kind(&w, 8), 1, "one exorcism effect");
    }

    #[test]
    fn exorcism_ignores_the_living_and_own_undead_in_enhanced_mode() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 255);
        let f = foe(&mut w, (8, 8));
        let own = w.spawn(unit::SKELETON, 0, (9, 8), 0).unwrap();
        strike(&mut w, p, EXORCISM, None, (8, 8));
        assert_eq!((hp(&w, f), hp(&w, own), mana(&w, p)), (60, 40, 255));
        w.rules.enhanced = false;
        strike(&mut w, p, EXORCISM, None, (9, 8));
        let own = w.unit(own).unwrap();
        assert!(own.dying || own.hp < 40, "Classic mode hits its own undead");
    }

    #[test]
    fn exorcism_scans_rings_up_to_3_tiles_out() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 255);
        let near = undead_skeleton(&mut w, (11, 8));
        let far = undead_skeleton(&mut w, (12, 8));
        strike(&mut w, p, EXORCISM, None, (8, 8));
        assert_eq!((w.unit(near).unwrap().alive(), hp(&w, far)), (false, 40));
    }

    #[test]
    fn runes_are_placed_on_free_tiles_and_refund_the_rest() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 200);
        w.spawn(0, 1, (10, 9), 0).unwrap();
        w.spawn(0, 1, (11, 10), 0).unwrap();
        strike(&mut w, p, RUNES, None, (10, 10));
        let tiles: Vec<_> = w.runes.iter().flatten().map(|r| r.tile).collect();
        assert_eq!(tiles, [(10, 10), (10, 11), (9, 10)]);
        assert_eq!(mana(&w, p), 80);
        assert!(w.runes.iter().flatten().all(|r| r.life == 2048));
        assert_eq!(fx_of_kind(&w, 0x0b), 3);
    }

    #[test]
    fn runes_stop_at_50_slots_and_skip_existing_runes() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 200);
        for r in w.runes.iter_mut().take(48) {
            *r = Some(Rune {
                tile: (30, 30),
                life: 5,
            });
        }
        strike(&mut w, p, RUNES, None, (10, 10));
        let placed: Vec<_> = w
            .runes
            .iter()
            .flatten()
            .filter(|r| r.tile != (30, 30))
            .map(|r| r.tile)
            .collect();
        assert_eq!(placed, [(10, 10), (10, 9)]);
        assert_eq!(mana(&w, p), 120);
        // A tile that holds a live rune is skipped.
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 200);
        w.runes[0] = Some(Rune {
            tile: (10, 10),
            life: 100,
        });
        strike(&mut w, p, RUNES, None, (10, 10));
        assert_eq!(w.runes.iter().flatten().count(), 5);
        assert_eq!(mana(&w, p), 40);
    }

    #[test]
    fn runes_hurt_whoever_walks_onto_them() {
        for (hp0, armoured, want) in [(60, false, Some(10)), (50, false, None), (100, true, Some(100))] {
            let mut w = lab(32);
            let k = w.spawn(KNIGHT, 0, (4, 4), 0).unwrap();
            set_hp(&mut w, k, hp0);
            let i = w.index(k).unwrap();
            w.units[i].unholy_armor = if armoured { 500 } else { 0 };
            w.runes[3] = Some(Rune {
                tile: (4, 4),
                life: 2048,
            });
            run(&mut w, 1);
            assert_eq!(w.runes[3], None, "used up");
            assert_eq!(w.unit(k).filter(|u| u.alive()).map(|u| u.hp), want);
            assert_eq!(fx_of_kind(&w, 0x17), 1, "explosion");
        }
    }

    #[test]
    fn a_rune_expires_after_2048_passes() {
        let mut w = lab(32);
        w.runes[0] = Some(Rune {
            tile: (4, 4),
            life: 2048,
        });
        run(&mut w, 2047);
        assert!(w.runes[0].is_some());
        run(&mut w, 1);
        assert_eq!(w.runes[0], None);
    }

    /// A footman of player 1 killed at `tile` and left as a fresh body.
    fn body(w: &mut World, tile: (i32, i32)) -> UnitId {
        let f = w.spawn(0, 1, tile, 0).unwrap();
        let i = w.index(f).unwrap();
        w.kill(i);
        while !w.unit(f).unwrap().remains {
            w.step(&[]);
        }
        f
    }

    #[test]
    fn raise_dead_vectors() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 120);
        let bodies = [(12, 10), (15, 10), (17, 10)].map(|t| body(&mut w, t));
        let before = w.rng.state;
        strike(&mut w, p, RAISE_DEAD, None, (10, 10));
        let skeletons: Vec<_> = w
            .units
            .iter()
            .filter(|u| u.kind == unit::SKELETON)
            .map(|u| (u.tile, u.owner))
            .collect();
        assert_eq!(skeletons.len(), 2);
        assert!(skeletons.contains(&((12, 10), 0)) && skeletons.contains(&((15, 10), 0)));
        assert_eq!(mana(&w, p), 20);
        assert_eq!(bodies.map(|b| w.unit(b).unwrap().used), [true, true, false]);
        assert_eq!(w.rng, advanced(before, 2), "one creation draw per skeleton");
        assert_eq!(fx_of_kind(&w, 0x16), 2);
        // Round-45 errata: each marker is placed from the new skeleton's box, like the other spell markers.
        for s in w.units.iter().filter(|u| u.kind == unit::SKELETON) {
            let b = w.stats_of(s.kind).box_size;
            let pos = (s.pos.0 + b.0 as i32 / 2, s.pos.1 + b.1 as i32 / 2 - 4);
            assert!(
                w.missiles
                    .iter()
                    .flatten()
                    .any(|m| m.kind == 0x16 && m.pos == pos && m.to == (0, 0)),
                "{pos:?}"
            );
        }
        // Used bodies are not raised again.
        strike(&mut w, p, RAISE_DEAD, None, (10, 10));
        assert_eq!(w.units.iter().filter(|u| u.kind == unit::SKELETON).count(), 2);
    }

    #[test]
    fn raise_dead_at_the_unit_cap_pays_and_leaves_the_body() {
        let mut w = lab(32);
        w.slots = slots::Slots::new(3);
        let p = caster(&mut w, MAGE, (5, 5), 60);
        w.spawn(0, 0, (1, 1), 0).unwrap();
        let b = body(&mut w, (10, 10));
        strike(&mut w, p, RAISE_DEAD, None, (10, 10));
        assert!(w.units.iter().all(|u| u.kind != unit::SKELETON));
        assert_eq!((mana(&w, p), w.unit(b).unwrap().used), (10, false));
    }

    #[test]
    fn a_killed_unit_leaves_a_body_where_it_died() {
        let mut w = lab(32);
        let f = w.spawn(0, 1, (7, 7), 0).unwrap();
        let i = w.index(f).unwrap();
        w.kill(i);
        assert!(w.raisable_bodies().is_empty(), "not until the death animation ends");
        run(&mut w, 8);
        assert_eq!(w.raisable_bodies(), [f]);
        assert_eq!(w.unit(f).unwrap().tile, (7, 7));
    }

    /// Death coil victims: footmen of player 1 with the given hit points at tiles on row 9, left to right.
    fn coil_setup(hps: &[u16], mana: u8) -> (World, UnitId, Vec<UnitId>) {
        let mut w = lab(32);
        let p = caster(&mut w, DEATH_KNIGHT, (3, 3), mana);
        set_hp(&mut w, p, 10);
        let vs = hps
            .iter()
            .enumerate()
            .map(|(n, &h)| {
                let v = foe(&mut w, (9 + n as i32, 9));
                set_hp(&mut w, v, h);
                v
            })
            .collect();
        (w, p, vs)
    }

    fn coils(w: &World) -> Vec<(UnitId, u8)> {
        w.missiles
            .iter()
            .flatten()
            .filter(|m| m.kind == 10)
            .map(|m| (m.target.unwrap(), m.damage))
            .collect()
    }

    #[test]
    fn death_coil_shares_a_budget_of_50_lowest_hit_points_first() {
        let (mut w, p, v) = coil_setup(&[10, 30, 25], 100);
        let before = w.rng.state;
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        assert_eq!(coils(&w), [(v[0], 10), (v[2], 25), (v[1], 15)]);
        assert_eq!(hp(&w, p), 60);
        assert_eq!(mana(&w, p), 0);
        assert_eq!(w.rng, advanced(before, 9), "three draws per kept victim");
        run(&mut w, 60);
        assert!(w.unit(v[0]).is_none_or(|u| !u.alive()));
        assert!(w.unit(v[2]).is_none_or(|u| !u.alive()));
        assert_eq!(hp(&w, v[1]), 15);
    }

    /// Round-45 errata: a coil missile's destination is its aim point (the victim's centre ± 3) from creation, and it
    /// flies floor(D / 12) + 1 steps.
    #[test]
    fn a_death_coil_missile_aims_and_flies_like_a_normal_missile() {
        let (mut w, p, v) = coil_setup(&[10, 30], 100);
        let from = w.unit(p).unwrap().centre_px();
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        for m in w.missiles.iter().flatten().filter(|m| m.kind == 10) {
            let c = w.unit(m.target.unwrap()).unwrap().centre_px();
            assert!((-3..=4).contains(&(m.to.0 - c.0)) && (-3..=4).contains(&(m.to.1 - c.1)));
            let d = (m.to.0 - from.0).abs().max((m.to.1 - from.1).abs());
            assert_eq!(i32::from(m.flight), d / 12 + 1);
        }
        assert_eq!(coils(&w).len(), v.len());
    }

    #[test]
    fn death_coil_other_vectors() {
        let (mut w, p, v) = coil_setup(&[60, 5], 100);
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        assert_eq!(coils(&w), [(v[1], 5), (v[0], 45)]);
        assert_eq!(hp(&w, p), 60);

        // Equal hit points are not kept in scan order: A (30), B (30), C (5) become C, B, A.
        let (mut w, p, v) = coil_setup(&[30, 30, 5], 100);
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        assert_eq!(coils(&w), [(v[2], 5), (v[1], 30), (v[0], 15)]);

        // The gain is capped at the caster's maximum.
        let (mut w, p, _) = coil_setup(&[60, 5], 100);
        set_hp(&mut w, p, 80);
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        assert_eq!(hp(&w, p), 90);

        let (mut w, p, _) = coil_setup(&[], 100);
        let before = w.rng;
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        assert_eq!((mana(&w, p), hp(&w, p), coils(&w).len(), w.rng), (0, 10, 0, before));
    }

    #[test]
    fn death_coil_skips_own_inorganic_and_dead_units() {
        let (mut w, p, _) = coil_setup(&[], 100);
        w.spawn(0, 0, (9, 9), 0).unwrap();
        w.spawn(0x3a, 1, (10, 9), 0).unwrap();
        let d = foe(&mut w, (9, 10));
        let i = w.index(d).unwrap();
        w.kill(i);
        strike(&mut w, p, DEATH_COIL, None, (10, 9));
        assert!(coils(&w).is_empty());
    }

    #[test]
    fn holy_vision_enhanced_and_classic() {
        let mut w = lab(64);
        let p = caster(&mut w, MAGE, (5, 5), 100);
        strike(&mut w, p, HOLY_VISION, None, (10, 60));
        assert_eq!((fx_of_kind(&w, 0x16), mana(&w, p), w.vision.countdown), (4, 30, 150));
        for (x, y) in [(10, 60), (10, 52), (16, 56), (4, 56)] {
            assert!(!w.vision.unseen_by(0, x, y), "({x}, {y}) revealed");
        }
        assert!(w.vision.unseen_by(0, 10, 40));

        let mut w = lab(64);
        w.rules.enhanced = false;
        let p = caster(&mut w, MAGE, (5, 5), 100);
        strike(&mut w, p, HOLY_VISION, None, (10, 60));
        assert_eq!((fx_of_kind(&w, 0x16), w.vision.countdown), (6, 1));
        assert!(!w.vision.unseen_by(0, 10, 60));

        let mut w = lab(64);
        let p = caster(&mut w, MAGE, (5, 5), 100);
        strike(&mut w, p, HOLY_VISION, None, (3, 30));
        assert_eq!(fx_of_kind(&w, 0x16), 5, "(-3, 34) and (-3, 26) are off the map");
    }

    #[test]
    fn fireball_flight_and_explosions() {
        let mut w = lab(32);
        let line = Line::new((16, 16), (216, 16));
        let fx = Fx::Fireball {
            line,
            count: 0,
            ending: false,
        };
        let owner = UnitId(0);
        let slot = w.new_fx(2, owner, None, (16, 16), 40, fx).unwrap();
        let start = w.rng.state;
        let mut booms = Vec::new();
        let mut ended = 0;
        for s in 1..=60 {
            w.step(&[]);
            booms.extend(
                w.missiles
                    .iter()
                    .flatten()
                    .filter(|m| m.kind == 0x17 && m.elapsed == 1)
                    .map(|m| (s, m.pos.0)),
            );
            if ended == 0 && w.missiles[slot].is_none() {
                ended = s;
            }
        }
        // The first update is in step 1, so fireball step n is simulation step n + 1.
        let steps: Vec<_> = booms.iter().map(|b| b.0 - 1).collect();
        assert_eq!(steps, [20, 28, 36, 44, 52]);
        let xs: Vec<_> = booms.iter().map(|b| b.1).collect();
        assert_eq!(xs, [214, 262, 310, 358, 406]);
        assert_eq!(
            ended - 1,
            55,
            "step 54 marks it as ending; it is removed at the next update"
        );
        assert_eq!(
            w.rng,
            advanced(start, 54),
            "one draw at the start of each of the 54 steps"
        );
    }

    #[test]
    fn fireball_cast_creates_the_effect_with_its_first_step_one_cycle_later() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        strike(&mut w, p, FIREBALL, None, (15, 5));
        assert_eq!(mana(&w, p), 155);
        let m = w.missiles.iter().flatten().next().unwrap().clone();
        assert_eq!(
            (m.kind, m.damage, m.pos, m.elapsed),
            (2, 40, (5 * 32 + 16, 5 * 32 + 16), 0)
        );
        let before = w.rng;
        w.step(&[]);
        assert_eq!(w.rng, before, "the first update only starts the effect");
        w.step(&[]);
        assert_ne!(w.rng, before, "the first step draws");
    }

    #[test]
    fn fireball_splash_hurts_units_around_the_impact_but_not_the_caster() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(KNIGHT, 1, (14, 5), 0).unwrap();
        let friend = w.spawn(KNIGHT, 0, (14, 6), 0).unwrap();
        strike(&mut w, p, FIREBALL, None, (14, 5));
        run(&mut w, 80);
        assert!(hp(&w, f) < 100 && hp(&w, friend) < 100);
        assert_eq!(hp(&w, p), 60);
        assert_eq!(fx_of_kind(&w, 2), 0);
    }

    #[test]
    fn whirlwind_vectors() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (9, 18), 255);
        // Flag bit 2 keeps the idle caster from making look-around draws, so only the whirlwind draws.
        w.stats.0[MAGE as usize].flags |= flags::EXPLODE_WHEN_KILLED;
        w.rng = Rng::new(state_for(127, &[100, 10]));
        let start = w.rng.state;
        strike(&mut w, p, WHIRLWIND, None, (10, 20));
        assert_eq!(w.rng, advanced(start, 2));
        let m = w.missiles.iter().flatten().next().unwrap().clone();
        assert_eq!(m.pos, (372, 602));
        let Fx::Whirl { goal, life, .. } = m.fx else {
            panic!("a whirlwind")
        };
        assert_eq!((goal, life, m.damage), ((336, 656), 800, 4));
        // Round-45 errata: its destination is its current goal.
        assert_eq!(m.to, goal);
        // Nothing is drawn until it reaches the goal on step 54 (one step every 3 cycles).
        let at = w.rng;
        run(&mut w, 1 + 3 * 53);
        assert_eq!(w.rng, at);
        w.rng = Rng::new(state_for(127, &[64, 0]));
        let at = w.rng.state;
        run(&mut w, 3);
        let m = w.missiles.iter().flatten().next().unwrap().clone();
        let Fx::Whirl { goal, life, .. } = m.fx else {
            panic!("a whirlwind")
        };
        assert_eq!(life, 746);
        assert_eq!(goal, (336, 592));
        assert_eq!(m.to, goal, "the destination follows each new goal");
        assert_eq!(w.rng, advanced(at, 2));
    }

    #[test]
    fn a_flight_to_its_own_start_drifts_east() {
        let mut l = Line::new((5, 5), (5, 5));
        let mut p = (5, 5);
        l.step(&mut p, 1);
        l.step(&mut p, 1);
        assert_eq!(p, (7, 5));
    }

    #[test]
    fn lines_reach_their_goal_exactly() {
        for (a, b) in [
            ((0, 0), (54, 36)),
            ((100, 80), (3, 7)),
            ((0, 0), (0, 40)),
            ((9, 9), (60, 61)),
        ] {
            let mut l = Line::new(a, b);
            let mut p = a;
            let n = (b.0 - a.0).abs().max((b.1 - a.1).abs());
            for _ in 0..n {
                l.step(&mut p, 1);
            }
            assert_eq!(p, b);
        }
    }

    #[test]
    fn blizzard_strikes_while_the_mana_lasts_and_lands_55_shards_per_strike() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 60);
        let before = w.rng;
        strike(&mut w, p, BLIZZARD, None, (10, 10));
        assert_eq!((mana(&w, p), w.rng), (35, advanced(before.state, 20)));
        assert!(!idle_pending(&w, p), "channelled: the order stays");
        // The order repeats while 25 mana remain: 60 mana give two strikes. Each of the 5 chains lands 11 shards.
        let mut started = 0;
        for _ in 0..600 {
            w.step(&[]);
            started += w
                .missiles
                .iter()
                .flatten()
                .filter(|m| m.kind == 5 && m.elapsed == 1)
                .count();
        }
        assert_eq!(started, 110);
        assert_eq!(fx_of_kind(&w, 5), 0);
        assert!(
            mana(&w, p) < 25,
            "below 25 (plus a little regeneration): no third strike"
        );
    }

    #[test]
    fn a_blizzard_shard_has_its_landing_point_as_destination_from_creation() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 25);
        strike(&mut w, p, BLIZZARD, None, (10, 10));
        // Every landing point is the target tile's centre plus a multiple of 32 in −64..=64 on each axis; each shard
        // starts 110 ± 11 px left of and 170 ± 11 px above it, and its destination stays put through every repeat.
        let centre = 10 * 32 + 16;
        let lands = |w: &World| -> Vec<(i32, i32)> {
            w.missiles
                .iter()
                .flatten()
                .filter(|m| m.kind == 5)
                .map(|m| m.to)
                .collect()
        };
        let first = lands(&w);
        assert_eq!(first.len(), 5);
        for m in w.missiles.iter().flatten().filter(|m| m.kind == 5) {
            let (dx, dy) = (m.to.0 - centre, m.to.1 - centre);
            assert!(
                dx % 32 == 0 && dy % 32 == 0 && dx.abs() <= 64 && dy.abs() <= 64,
                "{:?}",
                m.to
            );
            assert!((99..=121).contains(&(m.to.0 - m.pos.0)), "{:?} {:?}", m.pos, m.to);
            assert!((159..=181).contains(&(m.to.1 - m.pos.1)), "{:?} {:?}", m.pos, m.to);
        }
        for _ in 0..200 {
            w.step(&[]);
            assert!(lands(&w).iter().all(|t| first.contains(t)), "{:?}", lands(&w));
        }
    }

    #[test]
    fn blizzard_through_the_order_repeats_until_the_mana_runs_out() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 60);
        cast(&mut w, p, BLIZZARD, None, (10, 10));
        run(&mut w, 200);
        assert_eq!(mana(&w, p), 10);
        assert_eq!(w.unit(p).unwrap().order, Order::Idle);
    }

    #[test]
    fn blizzard_shards_hurt_with_armour_ignored() {
        let mut w = lab(32);
        let k = w.spawn(KNIGHT, 1, (10, 10), 0).unwrap();
        let pos = (10 * 32 + 16 + 10, 10 * 32 + 16);
        w.splash(pos, 10, UnitId(0), crate::battle::Splash::Path);
        let lost = 100 - hp(&w, k);
        assert!((5..=10).contains(&lost), "{lost}");
    }

    #[test]
    fn death_and_decay_makes_five_clouds_that_pulse_ten_times_and_end() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 25);
        let before = w.rng;
        strike(&mut w, p, DEATH_AND_DECAY, None, (10, 10));
        assert_eq!(
            (mana(&w, p), fx_of_kind(&w, 6), w.rng),
            (0, 5, advanced(before.state, 10))
        );
        // Round-45 errata: every cloud's destination is the target tile's centre.
        let c = (10 * 32 + 16, 10 * 32 + 16);
        assert!(w.missiles.iter().flatten().filter(|m| m.kind == 6).all(|m| m.to == c));
        // One update to start, then it lasts until age 275: gone after simulation step 276.
        run(&mut w, 275);
        assert_eq!(fx_of_kind(&w, 6), 5);
        run(&mut w, 1);
        assert_eq!(fx_of_kind(&w, 6), 0);
    }

    #[test]
    fn a_cloud_pulses_10_times_at_5_to_10_damage() {
        let mut w = lab(32);
        let k = w.spawn(KNIGHT, 1, (10, 10), 0).unwrap();
        set_hp(&mut w, k, 1000);
        let c = (10 * 32 + 16, 10 * 32 + 16);
        w.new_fx(6, UnitId(0), None, c, 10, Fx::Cloud).unwrap();
        run(&mut w, 300);
        let lost = 1000 - hp(&w, k);
        assert!((50..=100).contains(&lost), "{lost}");
    }

    /// Round-56 errata ("At the strike"; `orc14-25000` paladin 1433, n = 18992): the strike makes no distance test, so
    /// a target that left the radius during the casting animation is still healed, or slowed.
    #[test]
    fn the_strike_makes_no_distance_test() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 255);
        let k = w.spawn(KNIGHT, 0, (12, 5), 0).unwrap();
        set_hp(&mut w, k, 85);
        strike(&mut w, p, HEALING, Some(k), (0, 0));
        assert_eq!(hp(&w, k), 100, "7 tiles away, past healing's radius of 6");
        assert_eq!(fx_of_kind(&w, 9), 1);
        let m = caster(&mut w, MAGE, (5, 10), 255);
        let f = foe(&mut w, (17, 10));
        strike(&mut w, m, SLOW, Some(f), (0, 0));
        assert!(w.unit(f).unwrap().haste < 0, "12 tiles away");
    }

    /// Round-56 errata ("The shooter after its slot is released"; `xorc12-spells-1500` effect 3, n = 1166): an effect
    /// keeps its shooter's slot after the slot is released, and its splash then credits whatever unit holds the slot.
    #[test]
    fn an_effect_keeps_its_shooters_released_slot() {
        let mut w = lab(32);
        let s = w.spawn(KNIGHT, 1, (25, 25), 0).unwrap();
        let slot = w.unit(s).unwrap().slot;
        let c = (5 * 32 + 16, 5 * 32 + 16);
        let fx = Fx::Whirl {
            line: Line::new(c, (c.0 + 32, c.1)),
            life: 30000,
            goal: (c.0 + 32, c.1),
        };
        let n = w.new_fx(12, s, None, c, 4, fx).unwrap();
        let i = w.index(s).unwrap();
        w.kill(i);
        for _ in 0..5000 {
            if w.unit(s).is_none() {
                break;
            }
            w.step(&[]);
        }
        assert!(w.unit(s).is_none(), "the slot is released");
        let m = w.missiles[n].clone().expect("the whirlwind goes on");
        assert_eq!(m.shooter, Some(slot));
        assert_eq!(w.effect_shooter(&m), s, "released, not reused: still the old shooter");
        let u = w.spawn(KNIGHT, 1, (28, 28), 0).unwrap();
        assert_eq!(w.unit(u).unwrap().slot, slot, "reused");
        assert_eq!(w.effect_shooter(&m), u);
        // A footman under the whirlwind records U as its attacker.
        let at = (m.pos.0.div_euclid(32), m.pos.1.div_euclid(32));
        let f = w.spawn(0, 0, at, 0).unwrap();
        set_hp(&mut w, f, 60);
        for _ in 0..200 {
            if hp(&w, f) < 60 {
                break;
            }
            w.step(&[]);
        }
        assert!(hp(&w, f) < 60, "splashed");
        assert_eq!(w.unit(f).unwrap().attacked_by, Some(u));
    }

    #[test]
    fn a_whirlwind_splashes_every_other_step() {
        let mut w = lab(32);
        let k = w.spawn(KNIGHT, 1, (10, 10), 0).unwrap();
        set_hp(&mut w, k, 1000);
        let c = (10 * 32 + 16, 10 * 32 + 16);
        let fx = Fx::Whirl {
            line: Line::new(c, (c.0 + 400, c.1)),
            life: 800,
            goal: (c.0 + 400, c.1),
        };
        w.new_fx(12, UnitId(0), None, c, 4, fx).unwrap();
        // 3 cycles per step: after 12 steps (+1 for the start) it has splashed 6 times.
        run(&mut w, 1 + 3 * 12);
        let lost = 1000 - hp(&w, k);
        assert!((12..=24).contains(&lost), "{lost}");
    }

    #[test]
    fn the_wisp_effects_touch_a_wall_with_zero_damage() {
        let mut w = lab(32);
        let p = caster(&mut w, PALADIN, (5, 5), 255);
        let k = w.spawn(KNIGHT, 0, (6, 5), 0).unwrap();
        set_hp(&mut w, k, 90);
        let before = w.rng.state;
        strike(&mut w, p, HEALING, Some(k), (0, 0));
        assert_eq!(fx_of_kind(&w, 9), 1);
        assert_eq!(w.rng.state, before, "no draw at the strike");
        // Round-48 errata (`xhuman10-spells-700`, n = 619 to 639): no shooter, target or destination; unmoved for
        // the 18-cycle flight animation, one step 19 cycles after the first update, gone in the next.
        let m = w.missiles.iter().flatten().find(|m| m.kind == 9).unwrap().clone();
        assert_eq!((m.attacker, m.target, m.to), (UnitId(0), None, (0, 0)));
        let at = |w: &World| w.missiles.iter().flatten().find(|m| m.kind == 9).map(|m| m.pos);
        // `strike` runs the spell alone: the first update comes in the first cycle run here.
        run(&mut w, 19);
        assert_eq!(at(&w), Some(m.pos), "no step yet");
        run(&mut w, 1);
        assert_eq!(at(&w), Some((m.pos.0 - 12, m.pos.1 - 12)), "one flight step");
        run(&mut w, 1);
        assert_eq!(fx_of_kind(&w, 9), 0);
    }

    #[test]
    fn cosmetic_effects_are_removed_in_cycle_u_plus_a_plus_1_plus_w() {
        // (kind, A, W) from the spec's table: removal in U + A + 1 + W.
        for (k, a, waits) in [
            (0x0b, 2, 15),
            (0x15, 1, 12),
            (0x16, 1, 18),
            (0x17, 1, 48),
            (0x19, 11, 12),
            (0x1a, 1, 12),
        ] {
            let mut w = lab(32);
            let n = w.new_fx(k, UnitId(0), None, (40, 40), 0, Fx::Cosmetic).unwrap();
            assert_eq!(n, slots::PERMANENT_EFFECTS, "the lowest free slot");
            let rng = w.rng;
            assert!(!w.missiles[n].as_ref().unwrap().showing(), "not showing when made");
            // Cycle U, the first update, then U + 1 to U + A + W: still there; showing from the start step U + A.
            for i in 0..=a + waits {
                w.advance_missiles();
                let m = w.missiles[n].as_ref().expect("still there");
                assert_eq!(m.showing(), i >= a, "kind {k:#x}, update {i}");
            }
            // Removed during its update in cycle U + A + 1 + W; the slot is free for the next effect.
            w.advance_missiles();
            assert!(w.missiles[n].is_none(), "kind {k:#x}");
            assert_eq!(w.rng, rng, "no draws");
            assert_eq!(w.new_fx(0x16, UnitId(0), None, (0, 0), 0, Fx::Cosmetic), Some(n));
        }
    }

    /// Spells spec, "Cosmetic effects" (round-39 errata), with the `orc09-repair-600` n = 65 impact (kind 0x15 at
    /// (1072, 2768)) and the `xorc12-ogre-mage-180` n = 77 rune markers: every cosmetic kind is created with only its
    /// kind and position set, whoever creates it and whatever it passes.
    #[test]
    fn cosmetic_effects_keep_only_kind_and_position() {
        let mut w = lab(32);
        let shooter = w.spawn(0, 0, (1, 1), 0).unwrap();
        let target = Some(shooter);
        for k in [0x0b, 0x15, 0x16, 0x17, 0x19, 0x1a, 0x1c] {
            let n = w.new_fx(k, shooter, target, (1072, 2768), 9, Fx::Cosmetic).unwrap();
            let m = w.missiles[n].take().unwrap();
            assert_eq!(
                (m.kind, m.pos, m.to, m.target, m.attacker, m.damage),
                (k, (1072, 2768), (0, 0), None, UnitId(0), 0),
                "kind {k:#x}"
            );
        }
        // The creators that name a unit: a missile's impact effect, a cannon's smoke and an explosion.
        w.impact_fx(0x0d, shooter, (1072, 2768));
        w.cannon_smoke(shooter, (10, 10));
        w.explosion(shooter, (20, 20));
        let made: Vec<_> = w
            .missiles
            .iter()
            .flatten()
            .map(|m| (m.kind, m.to, m.attacker))
            .collect();
        assert_eq!(
            made,
            [
                (0x15, (0, 0), UnitId(0)),
                (0x19, (0, 0), UnitId(0)),
                (0x17, (0, 0), UnitId(0))
            ]
        );
        // A spell effect in flight keeps its fields.
        let n = w.new_fx(2, shooter, None, (16, 16), 40, Fx::Cosmetic).unwrap();
        let m = w.missiles[n].as_ref().unwrap();
        assert_eq!((m.attacker, m.damage, m.to), (shooter, 40, (16, 16)));
    }

    #[test]
    fn start_animation_waits_follow_the_spec_table() {
        for (k, a) in [(0x00, 0), (0x02, 0), (0x0c, 0), (0x12, 0), (0x18, 0), (0x1b, 0)] {
            assert_eq!(start_cycles(k), a, "kind {k:#x}");
        }
        for k in [0x15, 0x16, 0x17, 0x1a, 0x1c] {
            assert_eq!(start_cycles(k), 1, "kind {k:#x}");
        }
        assert_eq!(start_cycles(0x0b), 2);
        for k in [0x04, 0x13, 0x14] {
            assert_eq!(start_cycles(k), 5, "kind {k:#x}");
        }
        assert_eq!((start_cycles(0x05), start_cycles(0x19)), (11, 11));
        assert_eq!(start_cycles(0x06), 18);
    }

    #[test]
    fn the_ground_marker_shows_after_the_updates_of_u_plus_1_to_u_plus_13() {
        let mut w = lab(32);
        // The marker is a permanent effect that the client re-makes; here only its showing rule is checked, so it is
        // updated by hand and kept in its slot.
        let n = w.new_fx(0x1c, UnitId(0), None, (40, 40), 0, Fx::Cosmetic).unwrap();
        let mut m = w.missiles[n].clone().unwrap();
        assert!(!m.showing(), "not drawn when made");
        m.elapsed = 1;
        assert!(!m.showing(), "not drawn after U");
        for i in 1..=13 {
            m.elapsed = 1 + i;
            assert!(m.showing(), "drawn after U + {i}");
        }
    }

    #[test]
    fn a_spell_marker_made_in_cycle_c_shows_from_c_plus_1_and_goes_in_c_plus_20() {
        let mut w = lab(32);
        let id = w.spawn(0, 0, (5, 5), 0).unwrap();
        let i = w.index(id).unwrap();
        w.marker_on(i);
        let n = slots::PERMANENT_EFFECTS;
        for c in 0..=20 {
            w.advance_missiles();
            match w.missiles[n].as_ref() {
                Some(m) => assert_eq!(m.showing(), c >= 1, "update in C + {c}"),
                None => assert_eq!(c, 20, "removed in C + 20"),
            }
        }
    }

    #[test]
    fn a_rune_marker_made_in_the_timers_pass_of_cycle_c_goes_in_c_plus_19() {
        let mut w = lab(32);
        // A rune one update from making its marker; nothing stands on it.
        w.runes[0] = Some(Rune {
            tile: (10, 10),
            life: 2,
        });
        let mut made = None;
        for c in 0..40 {
            w.step(&[]);
            let marker = w.missiles.iter().flatten().find(|m| m.kind == 0x0b);
            match (made, marker) {
                (None, Some(m)) => {
                    assert_eq!(m.elapsed, 0, "first update in the next cycle");
                    made = Some(c);
                }
                (Some(m0), Some(m)) => {
                    assert_eq!(m.elapsed as i32, c - m0, "updates since");
                    // Start step in C + 3: shown after the update of C + 3.
                    assert_eq!(m.showing(), c - m0 >= 3, "cycle C + {}", c - m0);
                }
                (Some(m0), None) => {
                    assert_eq!(c - m0, 19, "removed in C + 19");
                    return;
                }
                (None, None) => {}
            }
        }
        panic!("no rune marker seen ended");
    }

    #[test]
    fn rune_explosion_runs_from_the_next_cycle() {
        // Spec, "When a new effect first runs": an effect made in the timers pass first runs in the next cycle, so
        // the rune's explosion (kind 0x17) is gone 51 cycles after the one it was made in.
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        strike(&mut w, p, RUNES, None, (10, 10));
        w.spawn(0, 1, (10, 10), 0).unwrap();
        let mut made = None;
        for c in 0..200 {
            w.step(&[]);
            let live = fx_of_kind(&w, 0x17) > 0;
            match made {
                None if live => made = Some(c),
                Some(m) if !live => {
                    assert_eq!(c - m, 51);
                    return;
                }
                _ => {}
            }
        }
        panic!("no rune explosion seen ended");
    }

    #[test]
    fn effects_fail_quietly_when_the_table_is_full() {
        let mut w = lab(32);
        let p = caster(&mut w, MAGE, (5, 5), 255);
        for m in w.missiles.iter_mut().skip(slots::PERMANENT_EFFECTS) {
            *m = Some(crate::battle::Missile {
                kind: 0x16,
                attacker: UnitId(0),
                shooter: None,
                target: None,
                from: (0, 0),
                to: (0, 0),
                pos: (0, 0),
                flight: 0,
                elapsed: 0,
                damage: 0,
                splash: false,
                hits_air: false,
                fx: Fx::Cosmetic,
            });
        }
        strike(&mut w, p, FIREBALL, None, (9, 5));
        assert_eq!(mana(&w, p), 155, "the mana is spent anyway");
        assert_eq!(fx_of_kind(&w, 2), 0);
    }

    #[test]
    fn spell_state_is_part_of_the_hash_and_deterministic() {
        let run_it = || {
            let mut w = lab(32);
            let p = caster(&mut w, MAGE, (5, 5), 255);
            strike(&mut w, p, RUNES, None, (10, 10));
            strike(&mut w, p, DEATH_AND_DECAY, None, (12, 5));
            run(&mut w, 40);
            w
        };
        let (a, b) = (run_it(), run_it());
        assert_eq!(a.state_hash(), b.state_hash());
        assert!(a.runes[0].is_some());
        let mut c = run_it();
        c.runes[0] = None;
        assert_ne!(a.state_hash(), c.state_hash());
    }

    /// Pins of the spell behaviour, written before the B-038 spells refactor onto `docs/specs/spells/spells.md` so that
    /// it can't change them unseen. Each pin is the whole simulation state ([`World::fingerprint`]) with the queued
    /// events and sound cues, folded over every cycle of a run.
    // ----- round 81 -----------------------------------------------------------------------------------------

    /// [`lab`] with sight for every type, so that detection works as in a game.
    fn sighted(size: u16) -> World {
        let mut w = lab(size);
        for s in w.stats.0.iter_mut() {
            s.sight = 6;
        }
        w
    }

    /// Steps until unit `id` has a spell order and then until it no longer has one; panics after `limit` cycles.
    fn until_not_casting(w: &mut World, id: UnitId, limit: u32) {
        let casting = |w: &World| matches!(w.unit(id).unwrap().order, Order::Cast { .. });
        let mut started = casting(w);
        for _ in 0..limit {
            w.step(&[]);
            started |= casting(w);
            if started && !casting(w) {
                return;
            }
        }
        panic!("no spell order that ended");
    }

    /// S90 (`xhuman10-slow-killed-goals-400`, n = 98): the target of a slow dies while the mage walks to it. At the
    /// next step boundary idle takes over in the same update: order 2, the still animation, wait 3, nothing pending
    /// and nothing paid; the idle logic first runs 3 cycles later.
    #[test]
    fn a_target_that_dies_on_the_way_makes_the_caster_idle_in_the_same_update() {
        use tidewater_formats::script::anim;
        let mut w = sighted(48);
        let m = caster(&mut w, MAGE, (30, 10), 255);
        let g = foe(&mut w, (8, 10));
        cast(&mut w, m, SLOW, Some(g), (8, 10));
        run(&mut w, 40);
        let gi = w.index(g).unwrap();
        w.kill(gi);
        until_not_casting(&mut w, m, 40);
        let u = w.unit(m).unwrap();
        assert_eq!(
            (u.order, u.script.anim, u.script.wait, u.pending, u.mana),
            (Order::Idle, anim::STILL, 3, None, 255)
        );
    }

    /// S90 with the Enhanced resume (orders spec, "Resuming patrol and attack-move"; untraced for a cast): a caster
    /// with a remembered patrol resumes it through the same take-over.
    #[test]
    fn a_failed_decision_resumes_a_remembered_patrol() {
        let mut w = sighted(48);
        let m = caster(&mut w, MAGE, (30, 10), 255);
        let g = foe(&mut w, (8, 10));
        cast(&mut w, m, SLOW, Some(g), (8, 10));
        let mi = w.index(m).unwrap();
        w.units[mi].remembered = Some(crate::world::Remembered::Patrol {
            to: (30, 20),
            back: (30, 10),
        });
        run(&mut w, 40);
        let gi = w.index(g).unwrap();
        w.kill(gi);
        until_not_casting(&mut w, m, 40);
        let u = w.unit(m).unwrap();
        assert!(matches!(u.order, Order::Patrol { .. }), "{:?}", u.order);
        assert_eq!((u.script.wait, u.pending, u.mana), (3, None, 255));
    }

    /// S91 (`xhuman10-heal-ballista-goals-500`, n = 160): a paladin sent to heal a non-organic unit 17 tiles away walks
    /// until it is within radius 6, and only then becomes idle: order 2, the still animation, wait 3, nothing paid.
    #[test]
    fn healing_a_non_organic_unit_walks_into_reach_first() {
        use tidewater_formats::script::anim;
        let mut w = sighted(48);
        let p = caster(&mut w, PALADIN, (30, 10), 255);
        let t = w.spawn(unit::SAPPER, 0, (13, 10), 0).unwrap();
        cast(&mut w, p, HEALING, Some(t), (13, 10));
        until_not_casting(&mut w, p, 600);
        let u = w.unit(p).unwrap();
        assert!(
            u.tile.0 - 13 <= 6 && u.tile.0 < 25,
            "stopped at {:?} in {:?}",
            u.tile,
            u.order
        );
        assert_eq!(
            (u.order, u.script.anim, u.script.wait, u.mana),
            (Order::Idle, anim::STILL, 3, 255)
        );
    }

    /// Spec, "Walking and the decision" (round 81, static study for the building test): a unit spell aimed at a
    /// building walks into reach before the not-a-building test makes the caster idle.
    #[test]
    fn a_unit_spell_on_a_building_walks_into_reach_first() {
        let mut w = sighted(48);
        w.stats.0[0x3a] = UnitStats {
            size: (2, 2),
            box_size: (64, 64),
            hit_points: 400,
            flags: flags::BUILDING,
            ..Default::default()
        };
        let m = caster(&mut w, MAGE, (30, 10), 255);
        let b = w.spawn(0x3a, 0, (12, 10), 0).unwrap();
        assert!(w.unit(b).unwrap().building);
        cast(&mut w, m, HASTE, Some(b), (12, 10));
        until_not_casting(&mut w, m, 600);
        let u = w.unit(m).unwrap();
        assert!(u.tile.0 <= 12 + 1 + 6, "stopped at {:?}", u.tile);
        assert_eq!((u.order, u.mana), (Order::Idle, 255));
    }

    /// S90 for the other lost targets (orders spec, "Walking orders", step 2): a target that turns invisible or goes
    /// inside something while the mage walks is dropped at the next step boundary, with the same take-over.
    #[test]
    fn a_target_lost_on_the_way_is_dropped_at_a_step_boundary() {
        for invisible in [true, false] {
            let mut w = sighted(48);
            let m = caster(&mut w, MAGE, (30, 10), 255);
            let g = foe(&mut w, (8, 10));
            cast(&mut w, m, SLOW, Some(g), (8, 10));
            run(&mut w, 40);
            let gi = w.index(g).unwrap();
            if invisible {
                w.units[gi].invisibility = 1000;
            } else {
                let unit = w.units[gi].clone();
                w.mark(&unit, unit.tile, None);
                w.units[gi].inside = true;
            }
            until_not_casting(&mut w, m, 40);
            let u = w.unit(m).unwrap();
            assert_eq!(
                (u.order, u.script.wait, u.mana),
                (Order::Idle, 3, 255),
                "invisible {invisible}"
            );
            assert!(u.tile.0 > 20);
        }
    }

    /// S92 (`xhuman10-invisible-reach-goals-400`, n = 299 and 310): a caster already within the radius doesn't walk,
    /// so nothing tests detection: it casts its slow at a target its player no longer detects.
    #[test]
    fn a_caster_in_reach_casts_at_a_target_it_no_longer_detects() {
        let mut w = sighted(48);
        let m = caster(&mut w, MAGE, (11, 10), 255);
        let g = foe(&mut w, (8, 10));
        cast(&mut w, m, SLOW, Some(g), (8, 10));
        let gi = w.index(g).unwrap();
        w.units[gi].invisibility = 1000;
        run(&mut w, 60);
        assert!(!w.detected_by(0, g));
        assert_eq!(mana(&w, m), 205);
        assert!((-1000..-900).contains(&w.unit(g).unwrap().haste));
    }

    /// A 2 x 2 building type (0x3a) with 400 HP.
    fn building_type(w: &mut World) {
        w.stats.0[0x3a] = UnitStats {
            size: (2, 2),
            box_size: (64, 64),
            hit_points: 400,
            flags: flags::BUILDING,
            ..Default::default()
        };
    }

    /// Puts unit `id` inside building `b`, as entering a gold mine does: off the map's unit marks, its tile and pixel
    /// position the building's top-left tile.
    fn go_inside(w: &mut World, id: UnitId, b: UnitId) {
        let i = w.index(id).unwrap();
        let unit = w.units[i].clone();
        w.mark(&unit, unit.tile, None);
        let bt = w.unit(b).unwrap().tile;
        let u = &mut w.units[i];
        u.inside = true;
        u.tile = bt;
        u.pos = (bt.0 * TILE_PX, bt.1 * TILE_PX);
    }

    /// S94 to S96 (`xhuman10-shield-mine-hits-800`): a shielded unit that goes inside a building keeps its 5 flames.
    /// They circle its stored position (the building's top-left tile) with the usual offset, their splash hits the
    /// building and never the unit inside, and they end only at the end of their lifetimes.
    #[test]
    fn flames_go_on_around_a_unit_inside_a_building() {
        let mut w = sighted(32);
        building_type(&mut w);
        let m = caster(&mut w, MAGE, (5, 5), 255);
        let f = w.spawn(KNIGHT, 0, (9, 8), 0).unwrap();
        let b = w.spawn(0x3a, 0, (12, 12), 0).unwrap();
        strike(&mut w, m, FLAME_SHIELD, Some(f), (0, 0));
        let mut n = 0;
        while fx_of_kind(&w, kind::FLAME) < 5 {
            w.step(&[]);
            n += 1;
            assert!(n < 40, "no flames");
        }
        go_inside(&mut w, f, b);
        let c = (12 * TILE_PX + 16, 12 * TILE_PX + 16);
        for _ in 0..450 {
            w.step(&[]);
            assert_eq!(fx_of_kind(&w, kind::FLAME), 5);
            for fl in w
                .missiles
                .iter()
                .flatten()
                .filter(|e| e.kind == kind::FLAME && e.elapsed > 8)
            {
                let d = (fl.pos.0 - c.0, fl.pos.1 - c.1);
                assert!((-28..=32).contains(&d.0) && (-34..=12).contains(&d.1), "{d:?}");
            }
        }
        assert!(hp(&w, b) < 400);
        assert_eq!(hp(&w, f), 100);
        run(&mut w, 120);
        assert_eq!(fx_of_kind(&w, kind::FLAME), 0);
    }

    /// Spec, "Flame shield" (round 81, audit erratum 4): the bolt makes its 5 flames whatever its target's state. On
    /// a dying unit they end at their first step; on a unit inside a building they go on.
    #[test]
    fn the_bolt_makes_its_flames_whatever_the_targets_state() {
        for dying in [true, false] {
            let mut w = sighted(32);
            building_type(&mut w);
            let m = caster(&mut w, MAGE, (5, 5), 255);
            let f = w.spawn(KNIGHT, 0, (9, 8), 0).unwrap();
            let b = w.spawn(0x3a, 0, (12, 12), 0).unwrap();
            strike(&mut w, m, FLAME_SHIELD, Some(f), (0, 0));
            run(&mut w, 2);
            if dying {
                let fi = w.index(f).unwrap();
                w.kill(fi);
            } else {
                go_inside(&mut w, f, b);
            }
            let mut n = 0;
            while fx_of_kind(&w, kind::BOLT) > 0 {
                w.step(&[]);
                n += 1;
                assert!(n < 40, "the bolt stays");
            }
            assert_eq!(fx_of_kind(&w, kind::FLAME), 5, "dying {dying}");
            run(&mut w, 10);
            assert_eq!(fx_of_kind(&w, kind::FLAME), if dying { 0 } else { 5 }, "dying {dying}");
        }
    }

    /// S93 (`xhuman10-invisible-dwarves-150`, n = 37): a demolition squad made invisible is killed in the timers
    /// pass's step 2 and still runs steps 3 to 5 in that pass, so its invisibility reads 1999; from the next pass on it
    /// is dying and skipped, so the timer keeps that value. The same for a sapper under unholy armour (static study).
    #[test]
    fn a_demolition_squad_killed_in_the_timers_pass_still_counts_its_timers() {
        let mut w = lab(16);
        w.stats.0[unit::DEMOLITION as usize] = w.stats.0[unit::SAPPER as usize];
        let d = w.spawn(unit::DEMOLITION, 0, (3, 3), 0).unwrap();
        let s = w.spawn(unit::SAPPER, 0, (6, 3), 0).unwrap();
        let di = w.index(d).unwrap();
        w.units[di].invisibility = 2000;
        let si = w.index(s).unwrap();
        (w.units[si].unholy_armor, w.units[si].bloodlust) = (500, 300);
        w.timers_pass();
        let (d0, s0) = (w.unit(d).unwrap(), w.unit(s).unwrap());
        assert!(d0.dying && s0.dying);
        assert_eq!((d0.invisibility, s0.unholy_armor, s0.bloodlust), (1999, 499, 299));
        w.timers_pass();
        let (d1, s1) = (w.unit(d).unwrap(), w.unit(s).unwrap());
        assert_eq!((d1.invisibility, s1.unholy_armor, s1.bloodlust), (1999, 499, 299));
    }

    /// How many draws took the generator from `before` to its state now.
    fn draws_since(before: u32, w: &World) -> usize {
        (0..10_000)
            .find(|&n| advanced(before, n).state == w.rng.state)
            .expect("a draw count")
    }

    /// S98 (static study): a blizzard with exactly 2 effect slots free makes 8 draws at its strike (chains 1 and 2,
    /// 4 each); chains 3 to 5 find no slot before their draws, so they start nothing and draw nothing. Death and decay
    /// and the whirlwind find their slot first too.
    #[test]
    fn a_spell_piece_with_no_free_slot_makes_no_draws() {
        for (spell, free, shards, draws) in [
            (BLIZZARD, 2, 2, 8),
            (BLIZZARD, 0, 0, 0),
            (DEATH_AND_DECAY, 2, 2, 4),
            (DEATH_AND_DECAY, 0, 0, 0),
            (WHIRLWIND, 0, 0, 0),
            (WHIRLWIND, 1, 1, 2),
        ] {
            let mut w = lab(32);
            let m = caster(&mut w, MAGE, (12, 10), 255);
            pin::fill_effects(&mut w, free);
            let before = w.rng.state;
            strike(&mut w, m, spell, None, (10, 10));
            let made = w.missiles.iter().flatten().filter(|e| e.kind != 0x13).count();
            assert_eq!(
                (made, draws_since(before, &w)),
                (shards, draws),
                "spell {spell}, {free} free"
            );
        }
    }

    /// Spec, "Effect slots" (round 81, audit erratum 1): a landed shard whose chain finds no free slot for its next
    /// shard makes neither that shard's start-offset draws nor the draw after them, and the chain ends. With one slot
    /// free, the only draws after the strike are the shard's one per step that doesn't land.
    #[test]
    fn a_landed_shard_with_no_free_slot_draws_nothing_for_the_next() {
        let mut w = lab(32);
        let m = caster(&mut w, MAGE, (12, 10), 255);
        pin::fill_effects(&mut w, 1);
        strike(&mut w, m, BLIZZARD, None, (20, 20));
        let n = w
            .missiles
            .iter()
            .position(|e| e.as_ref().is_some_and(|e| e.kind == kind::SHARD))
            .unwrap();
        let Some(Fx::Shard { line, .. }) = w.missiles[n].as_ref().map(|e| e.fx) else {
            panic!("no shard");
        };
        let steps = (line.remaining + 11) / 12;
        // Only the shard's own updates, so that nothing else draws.
        let before = w.rng.state;
        let mut updates = 0;
        while w.missiles[n].is_some() {
            w.step_fx(n);
            updates += 1;
            assert!(updates < 60, "the shard stays");
        }
        assert_eq!(fx_of_kind(&w, kind::SHARD), 0);
        assert_eq!(draws_since(before, &w), steps as usize - 1);
    }

    /// The spell sounds `spell` has queued.
    fn spell_cues(w: &World, spell: u8) -> Vec<Option<crate::sound::Place>> {
        w.sounds
            .cues
            .iter()
            .filter(|c| c.sound == Sound::Spell(spell))
            .map(|c| c.place)
            .collect()
    }

    /// S100 (static study): an eye of Kilrogg cast with 100 mana where the free-tile search finds no tile pays 70,
    /// makes no eye, sets the blocked-exit message and plays no sound; with room, the eye comes with its plain sound.
    #[test]
    fn the_eye_of_kilrogg_sounds_only_when_an_eye_is_made() {
        for room in [false, true] {
            let mut w = lab(16);
            let m = caster(&mut w, MAGE, (5, 5), 100);
            if !room {
                for i in 0..w.terrain.words.len() {
                    w.terrain.words[i] |= 0xffff;
                }
            }
            w.sounds.cues.clear();
            strike(&mut w, m, EYE_OF_KILROGG, None, (5, 5));
            let eyes = w.units.iter().filter(|u| u.kind == unit::EYE).count();
            let no_room = w
                .events
                .iter()
                .any(|e| matches!(e, Event::NoRoom { kind: unit::EYE, .. }));
            assert_eq!(mana(&w, m), 30);
            assert_eq!((eyes, no_room), (room as usize, !room));
            assert_eq!(spell_cues(&w, EYE_OF_KILROGG), if room { vec![None] } else { vec![] });
        }
    }

    mod pin {
        use super::*;

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

        /// [`lab`] in either rules mode, with sight for every type so that detection works as in a game.
        fn arena(enhanced: bool) -> World {
            let mut w = lab(48);
            w.rules.enhanced = enhanced;
            for s in w.stats.0.iter_mut() {
                s.sight = 6;
            }
            w
        }

        fn both_modes(f: impl Fn(bool) -> u64) -> Vec<u64> {
            [true, false].into_iter().map(f).collect()
        }

        /// Fills the effect table but for `free` slots after the permanent ones.
        pub(super) fn fill_effects(w: &mut World, free: usize) {
            let n = w.missiles.len();
            for m in w
                .missiles
                .iter_mut()
                .skip(slots::PERMANENT_EFFECTS)
                .take(n - slots::PERMANENT_EFFECTS - free)
            {
                *m = Some(crate::battle::Missile {
                    kind: 0x13,
                    attacker: UnitId(0),
                    shooter: None,
                    target: None,
                    from: (0, 0),
                    to: (0, 0),
                    pos: (0, 0),
                    flight: 0,
                    elapsed: 0,
                    damage: 0,
                    splash: false,
                    hits_air: false,
                    fx: Fx::Cosmetic,
                });
            }
        }

        /// Every unit spell through the command, from in reach and from out of reach, on a live organic target.
        #[test]
        fn unit_spells_through_the_command() {
            let mut got = Vec::new();
            for spell in UNIT_SPELLS {
                for from in [(10, 10), (30, 10)] {
                    let mut w = arena(true);
                    let kind = if spell == HEALING { PALADIN } else { MAGE };
                    let m = caster(&mut w, kind, from, 255);
                    let t = w.spawn(KNIGHT, 0, (8, 10), 0).unwrap();
                    set_hp(&mut w, t, 50);
                    w.spawn(KNIGHT, 1, (8, 11), 0).unwrap();
                    cast(&mut w, m, spell, Some(t), (8, 10));
                    got.push(trail(&mut w, 700));
                }
            }
            assert_eq!(
                got,
                [
                    6570963657842673534,
                    8097664081137240887,
                    11408135500012695118,
                    15328756865882170178,
                    15731648898169908686,
                    10270464277794095079,
                    3594636114233252516,
                    3921954593429825157,
                    1954568858974130358,
                    15193670579354657652,
                    16007800629143889252,
                    10337943895675894106,
                    16017233247351834687,
                    14103656345668071246,
                    13468066094147184355,
                    7460459612866995019
                ]
            );
        }

        /// Unit spells whose decision fails: a building, a non-organic unit, an air unit and a shielded unit, from in
        /// reach and from out of reach.
        #[test]
        fn unit_spells_refused_at_the_decision_or_the_strike() {
            let mut got = Vec::new();
            for (spell, target) in [
                (HASTE, 0x3a),
                (FLAME_SHIELD, 0x3a),
                (HEALING, unit::SAPPER),
                (POLYMORPH, unit::SAPPER),
                (BLOODLUST, unit::SAPPER),
                (FLAME_SHIELD, GRYPHON),
            ] {
                for from in [(10, 10), (30, 10)] {
                    let mut w = arena(true);
                    let m = caster(&mut w, PALADIN, from, 255);
                    let t = w.spawn(target, 0, (8, 10), 0).unwrap();
                    cast(&mut w, m, spell, Some(t), (8, 10));
                    got.push(trail(&mut w, 400));
                }
            }
            let mut w = arena(true);
            let m = caster(&mut w, MAGE, (10, 10), 255);
            let t = w.spawn(KNIGHT, 0, (8, 10), 0).unwrap();
            let ti = w.index(t).unwrap();
            w.units[ti].flame_shield = 100;
            cast(&mut w, m, FLAME_SHIELD, Some(t), (8, 10));
            got.push(trail(&mut w, 200));
            assert_eq!(
                got,
                [
                    2572084909219178404,
                    7462593974808655426,
                    18155415536659990849,
                    11262496229158613140,
                    7394615447669147193,
                    9596783022638148097,
                    12491648875814917563,
                    12744599090150666451,
                    4206339763271490563,
                    10718632011590191401,
                    1701851420691300144,
                    17700092751089797101,
                    6610920448925080138
                ]
            );
        }

        /// A target that dies, turns invisible or goes inside while the caster walks to it or plays its casting
        /// animation, with a remembered patrol for the resume, in both rules modes.
        #[test]
        fn the_target_is_lost_on_the_way() {
            let mut got = Vec::new();
            for case in 0..4 {
                got.extend(both_modes(|enhanced| {
                    let mut w = arena(enhanced);
                    let m = caster(&mut w, MAGE, (30, 10), 255);
                    let g = w.spawn(KNIGHT, 1, (8, 10), 0).unwrap();
                    cast(&mut w, m, SLOW, Some(g), (8, 10));
                    let mi = w.index(m).unwrap();
                    w.units[mi].remembered = Some(crate::world::Remembered::Patrol {
                        to: (30, 20),
                        back: (30, 10),
                    });
                    let mut h = trail(&mut w, if case == 3 { 1 } else { 40 });
                    let gi = w.index(g).unwrap();
                    match case {
                        0 => w.kill(gi),
                        1 => w.units[gi].invisibility = 1000,
                        _ => {
                            let unit = w.units[gi].clone();
                            w.mark(&unit, unit.tile, None);
                            w.units[gi].inside = true;
                        }
                    }
                    h ^= trail(&mut w, 300);
                    h
                }));
            }
            assert_eq!(
                got,
                [
                    17913793911137785833,
                    10891004090879039118,
                    15840398852364308474,
                    8895920582660002836,
                    8118140382518496718,
                    15592006276913019551,
                    7089697309869978124,
                    13303408218503098646
                ]
            );
        }

        /// Every point spell through the command, with units around the target point, in both rules modes.
        #[test]
        fn point_spells_through_the_command() {
            let mut got = Vec::new();
            for spell in [
                HOLY_VISION,
                EXORCISM,
                FIREBALL,
                BLIZZARD,
                EYE_OF_KILROGG,
                RAISE_DEAD,
                DEATH_COIL,
                WHIRLWIND,
                RUNES,
                DEATH_AND_DECAY,
            ] {
                got.extend(both_modes(|enhanced| {
                    let mut w = arena(enhanced);
                    let kind = if spell == EXORCISM { PALADIN } else { DEATH_KNIGHT };
                    let m = caster(&mut w, kind, (30, 12), 255);
                    for (n, t) in [(9, 10), (11, 10), (10, 12), (12, 9)].into_iter().enumerate() {
                        let f = w.spawn(KNIGHT, 1, t, 0).unwrap();
                        set_hp(&mut w, f, 20 + 15 * n as u16);
                    }
                    w.spawn(KNIGHT, 0, (10, 9), 0).unwrap();
                    undead_skeleton(&mut w, (10, 11));
                    body(&mut w, (8, 11));
                    body(&mut w, (13, 13));
                    cast(&mut w, m, spell, None, (10, 10));
                    trail(&mut w, 900)
                }));
            }
            assert_eq!(
                got,
                [
                    8367387278495162366,
                    6257947175778283097,
                    9837872630853965157,
                    9406309246085243925,
                    15996869705984266614,
                    838986092751857970,
                    15329728077870442821,
                    15678352070897889620,
                    14722665048184563267,
                    8080222541946584983,
                    17542933558180363899,
                    9299367441620635930,
                    12616406815721033994,
                    18286447325076922779,
                    18217374512074841316,
                    3907660467990746567,
                    2850462967033419272,
                    750727095162681827,
                    17766905698848526938,
                    2171650096389122988
                ]
            );
        }

        /// Holy vision near each edge of the map, in both rules modes.
        #[test]
        fn holy_vision_at_the_edges() {
            let mut got = Vec::new();
            for tile in [(0, 0), (47, 47), (3, 44), (24, 24)] {
                got.extend(both_modes(|enhanced| {
                    let mut w = arena(enhanced);
                    let m = caster(&mut w, MAGE, (20, 20), 255);
                    strike(&mut w, m, HOLY_VISION, None, tile);
                    trail(&mut w, 60)
                }));
            }
            assert_eq!(
                got,
                [
                    12956272492664224182,
                    13502872167226954976,
                    9634383685240036618,
                    13150989468658042820,
                    9692961426084186022,
                    10758491430547138358,
                    14749539739432364931,
                    15461669505548495064
                ]
            );
        }

        /// The point spells that need effect slots, with 0 to 3 slots free.
        #[test]
        fn effect_pieces_with_few_slots() {
            let mut got = Vec::new();
            for spell in [BLIZZARD, DEATH_AND_DECAY, WHIRLWIND, FIREBALL, HOLY_VISION, RUNES] {
                for free in 0..4 {
                    let mut w = arena(true);
                    let m = caster(&mut w, MAGE, (12, 10), 255);
                    w.spawn(KNIGHT, 1, (10, 10), 0).unwrap();
                    fill_effects(&mut w, free);
                    strike(&mut w, m, spell, None, (10, 10));
                    got.push(trail(&mut w, 300));
                }
            }
            assert_eq!(
                got,
                [
                    11885426215582548983,
                    88524946932623572,
                    993188367256940341,
                    7698509570116490980,
                    13870454793258606143,
                    1673314346665158255,
                    1650534446358021315,
                    2864280565326091246,
                    1535142317792771723,
                    15992806477622592395,
                    14131176227219162947,
                    5770906763512571262,
                    12335873988336403766,
                    18388956045413560216,
                    13744938671817938163,
                    10824971562359585752,
                    15235576187569990751,
                    12598839188188960853,
                    9615052051909174644,
                    8332751505918460810,
                    7574288775041092963,
                    8883422678463670431,
                    12395061211281444655,
                    5834566190544122958
                ]
            );
        }

        /// The eye of Kilrogg with room and without, and with too little mana.
        #[test]
        fn eye_of_kilrogg() {
            let mut got = Vec::new();
            for (room, mana) in [(true, 255), (false, 255), (true, 60)] {
                let mut w = arena(true);
                let m = caster(&mut w, MAGE, (5, 5), mana);
                if !room {
                    for i in 0..w.terrain.words.len() {
                        w.terrain.words[i] |= 0xffff;
                    }
                }
                strike(&mut w, m, EYE_OF_KILROGG, None, (20, 20));
                got.push(trail(&mut w, 100));
            }
            assert_eq!(got, [8385898450897783958, 2628578634225963494, 3390235678664876339]);
        }

        /// A flame shield whose unit dies, goes inside a building, or is fine, before and after the flames are made;
        /// and a bolt whose target is gone when it arrives.
        #[test]
        fn flames_and_the_shielded_unit() {
            let mut got = Vec::new();
            for case in 0..5 {
                for delay in [3, 30] {
                    let mut w = arena(true);
                    let m = caster(&mut w, MAGE, (5, 5), 255);
                    let f = w.spawn(KNIGHT, 0, (9, 8), 0).unwrap();
                    let b = w.spawn(0x3a, 0, (12, 12), 0).unwrap();
                    w.spawn(KNIGHT, 1, (12, 11), 0).unwrap();
                    w.spawn(KNIGHT, 1, (10, 8), 0).unwrap();
                    strike(&mut w, m, FLAME_SHIELD, Some(f), (0, 0));
                    let mut h = trail(&mut w, delay);
                    let fi = w.index(f).unwrap();
                    match case {
                        0 => w.kill(fi),
                        1 | 2 => {
                            let unit = w.units[fi].clone();
                            w.mark(&unit, unit.tile, None);
                            let bt = w.unit(b).unwrap().tile;
                            let u = &mut w.units[fi];
                            u.inside = true;
                            u.tile = bt;
                            u.pos = (bt.0 * TILE_PX, bt.1 * TILE_PX);
                            if case == 2 {
                                w.kill(fi);
                            }
                        }
                        3 => {
                            w.kill(fi);
                            h ^= trail(&mut w, 600);
                        }
                        _ => {}
                    }
                    h ^= trail(&mut w, 600);
                    got.push(h);
                }
            }
            assert_eq!(
                got,
                [
                    10092197340396237477,
                    16154662666603851558,
                    14368531150592430028,
                    3831019930421031670,
                    1178450463650365013,
                    3343725924962192024,
                    4615546886751034159,
                    1369650586003858535,
                    12169452165749080812,
                    14436589221238129288
                ]
            );
        }

        /// The timers pass: invisible and armoured sappers and demolition squads, a berserker with the upgrade, a
        /// decaying eye and skeleton, casters' mana.
        #[test]
        fn the_timers_pass() {
            let mut w = arena(true);
            w.stats.0[unit::DEMOLITION as usize] = w.stats.0[unit::SAPPER as usize];
            for (n, k) in [unit::SAPPER, unit::DEMOLITION].into_iter().enumerate() {
                let a = w.spawn(k, 0, (3 + n as i32, 3), 0).unwrap();
                let b = w.spawn(k, 0, (3 + n as i32, 5), 0).unwrap();
                let ai = w.index(a).unwrap();
                w.units[ai].invisibility = 2000;
                let bi = w.index(b).unwrap();
                w.units[bi].unholy_armor = 1;
            }
            w.players[0].upgrades.marksmanship = true;
            let z = w.spawn(unit::BERSERKER, 0, (8, 8), 0).unwrap();
            set_hp(&mut w, z, 20);
            w.spawn(unit::EYE, 0, (10, 10), 0).unwrap();
            w.spawn(unit::SKELETON, 0, (12, 10), 0).unwrap();
            let m = w.spawn(MAGE, 0, (14, 10), 0).unwrap();
            let mi = w.index(m).unwrap();
            (w.units[mi].haste, w.units[mi].bloodlust, w.units[mi].flame_shield) = (-30, 20, 40);
            let got = trail(&mut w, 1000);
            assert_eq!(got, 5368486384455272299);
        }

        /// A walk with no route, and a channelled blizzard running out of mana.
        #[test]
        fn no_route_and_channelling() {
            let mut w = arena(true);
            let d = caster(&mut w, PALADIN, (5, 5), 255);
            let f = w.spawn(0, 0, (20, 5), 0).unwrap();
            for y in 4..=6 {
                for x in 4..=6 {
                    if (x, y) != (5, 5) {
                        w.terrain.set_bits(x, y, crate::terrain::bits::OBSTACLE);
                    }
                }
            }
            cast(&mut w, d, HEALING, Some(f), (0, 0));
            let a = trail(&mut w, 100);
            let mut w = arena(true);
            let m = caster(&mut w, MAGE, (12, 12), 70);
            cast(&mut w, m, BLIZZARD, None, (10, 10));
            let b = trail(&mut w, 400);
            let got = vec![a, b];
            assert_eq!(got, [13173154376033451605, 4000273995908231716]);
        }
    }
}
