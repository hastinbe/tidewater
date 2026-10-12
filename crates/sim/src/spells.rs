//! Spells: casting, effects, durations and summoned units. Spec: `docs/specs/spells/spells.md`.
//!
//! Covered: the spell orders 38 to 56 (command, approach, strike), the five unit timers and the timers pass (mana,
//! berserker regeneration, decay, runes), the creation rules for summoned units, and every effect in flight.
//!
//! Not covered: which spells a player may cast (research is in `tech.rs`; the button bar is a later spec) and sound.
//! Dead bodies are the remains of `corpses.rs`.

use crate::TILE_PX;
use crate::combat;
use crate::economy::Event;
use crate::movement::{Goal, Walk, chebyshev};
use crate::placement::{LIMIT_OUT, Source};
use crate::slots;
use crate::sound::Sound;
use crate::world::{Layer, Order, UnitId, World};
use tidewater_formats::udta::flags;

/// Rune slots, shared by all players.
pub const RUNE_SLOTS: usize = 50;
const RUNE_LIFE: u16 = 2048;
const RUNE_DAMAGE: u16 = 50;
/// A fireball ends on the first step that starts with its count above this limit (spec, "Fireball"): the count goes
/// up to 41 on the step after the last explosion, and the flight ends on the step after that. Its explosions are on
/// counts 8, 16, 24, 32 and 40.
const FIREBALL_COUNT_LIMIT: u8 = 40;

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

/// The approach radius in tiles of a spell order (orders spec, "Spell orders"); `None` means "always".
fn radius(spell: u8) -> Option<i32> {
    match spell {
        HOLY_VISION | EYE_OF_KILROGG => None,
        HEALING | FLAME_SHIELD | INVISIBILITY | BLOODLUST | RAISE_DEAD | HASTE | UNHOLY_ARMOUR => Some(6),
        EXORCISM | FIREBALL | SLOW | POLYMORPH | DEATH_COIL | RUNES => Some(10),
        BLIZZARD | WHIRLWIND | DEATH_AND_DECAY => Some(12),
        _ => Some(0),
    }
}

/// A live rune on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rune {
    pub tile: (i32, i32),
    pub life: u16,
}

/// A flying effect's straight-line rule (spec, "How flying effects move").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    Fireball {
        line: Line,
        count: u8,
    },
    Bolt {
        line: Line,
    },
    Flame {
        point: u8,
        life: u16,
    },
    Shard {
        line: Line,
        repeats: u8,
    },
    Cloud,
    Whirl {
        line: Line,
        life: u16,
        goal: (i32, i32),
    },
    Coil {
        line: Line,
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
    // ----- commands and orders -------------------------------------------------------------------------------

    /// A spell command (spec, "Giving the order"). Order 40 has no effect and is refused.
    pub(crate) fn cast_command(
        &mut self,
        player: u8,
        unit: UnitId,
        spell: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    ) {
        if !(HOLY_VISION..=DEATH_AND_DECAY).contains(&spell) || spell == 40 {
            return;
        }
        let Some(i) = self
            .index(unit)
            .filter(|&i| self.units[i].owner == player && !self.units[i].dying && !self.units[i].building)
        else {
            return;
        };
        let unit_spell = UNIT_SPELLS.contains(&spell);
        if unit_spell && target.is_none() {
            return;
        }
        self.give_spell(i, spell, target, tile);
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
            target: if UNIT_SPELLS.contains(&spell) { target } else { None },
            tile,
        };
        // Our own choice, as for the other player commands: a new order forgets a remembered patrol.
        self.units[i].remembered = None;
        self.give(i, order);
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

    /// The order logic of orders 38 to 56 at a "step done" (spec, "Moving and casting").
    pub(crate) fn run_spell(&mut self, i: usize) -> bool {
        let Order::Cast { spell, target, tile } = self.units[i].order else {
            return false;
        };
        let owner = self.units[i].owner;
        let (goal, aim) = if UNIT_SPELLS.contains(&spell) {
            let ok = target.and_then(|t| self.unit(t)).filter(|t| {
                t.present()
                    && Self::detected(t, owner)
                    && !t.building
                    && (!ORGANIC_SPELLS.contains(&spell) || self.stats_of(t.kind).flags & flags::ORGANIC != 0)
            });
            let Some(t) = ok else {
                self.cast_idle(i);
                return true;
            };
            (t.tile, t.centre_px())
        } else {
            (tile, centre_of_tile(tile))
        };
        if let Some(r) = radius(spell) {
            match self.move_toward(i, Goal::tile(goal, r)) {
                Walk::Arrived => {}
                Walk::NoPath => {
                    self.cast_idle(i);
                    return true;
                }
                Walk::Stepping | Walk::Blocked => return true,
            }
        }
        let c = self.units[i].centre_px();
        self.face_and_strike(i, (aim.0 - c.0, aim.1 - c.1));
        true
    }

    /// The caster stops without casting.
    fn cast_idle(&mut self, i: usize) {
        use tidewater_formats::script::anim;
        self.go_idle(i);
        self.switch_anim(i, anim::STILL);
    }

    /// Order 2 becomes pending (spec, "At the strike", step 4).
    fn idle_pending(&mut self, i: usize) {
        self.units[i].pending = Some(Order::Idle);
    }

    /// Step 2 and 3 of the strike: the mana check (failing makes the caster idle) and payment.
    fn pay_mana(&mut self, i: usize, cost: u32) -> bool {
        if (self.units[i].mana as u32) < cost {
            self.idle_pending(i);
            return false;
        }
        self.units[i].mana -= cost as u8;
        true
    }

    /// Steps 4 and 5: idle (unless channelled) and the caster's invisibility ends.
    fn finish_cast(&mut self, i: usize, idle: bool) {
        let owner = self.units[i].owner as usize;
        self.casts.strikes[owner & 15] += 1;
        if idle {
            self.idle_pending(i);
        }
        self.units[i].invisibility = 0;
    }

    /// Steps 2 to 5 together.
    fn begin(&mut self, i: usize, cost: u32, idle: bool) -> bool {
        if !self.pay_mana(i, cost) {
            return false;
        }
        self.finish_cast(i, idle);
        true
    }

    /// The strike of the casting animation (spec, "At the strike").
    pub(crate) fn cast_strike(&mut self, i: usize) {
        let Order::Cast { spell, target, tile } = self.units[i].order else {
            return;
        };
        let (caster, owner) = (self.units[i].id, self.units[i].owner);
        let unit_spell = UNIT_SPELLS.contains(&spell);
        let mut ti = None;
        let goal = if unit_spell {
            match target.and_then(|t| self.index(t)).filter(|&t| self.units[t].present()) {
                Some(t) => {
                    ti = Some(t);
                    self.units[t].tile
                }
                None => {
                    self.idle_pending(i);
                    return;
                }
            }
        } else {
            tile
        };
        // A strike before the caster is within the spell's radius (the animation starts with the order) is ignored.
        if let Some(r) = radius(spell) {
            let u = &self.units[i];
            let bonus = i32::from(matches!(u.layer, Layer::Sea | Layer::Air));
            if chebyshev(u.tile, goal) > r + bonus {
                return;
            }
        }
        let tid = ti.map(|t| self.units[t].id);
        match spell {
            HOLY_VISION => {
                if !self.begin(i, 70, true) {
                    return;
                }
                self.spell_sound(spell, None, tile);
                self.holy_vision(owner, tile);
            }
            HEALING => {
                let t = ti.expect("unit spell");
                self.finish_cast(i, true);
                let max = self.stats_of(self.units[t].kind).hit_points;
                let missing = max.saturating_sub(self.units[t].hp);
                if missing == 0 {
                    return;
                }
                let heal = missing.min(self.units[i].mana as u16 / 6).min(40);
                if heal == 0 {
                    return;
                }
                self.units[t].hp += heal;
                self.spell_sound(spell, Some(t), tile);
                self.units[i].mana -= (6 * heal) as u8;
                self.wisp(kind::HEALING, t);
            }
            EXORCISM => {
                self.finish_cast(i, true);
                self.spell_sound(spell, None, tile);
                self.exorcism(i, tile);
            }
            FLAME_SHIELD => {
                let t = ti.expect("unit spell");
                self.finish_cast(i, true);
                if self.units[t].flame_shield != 0 || self.units[t].layer == Layer::Air {
                    return;
                }
                if !self.pay_mana(i, 80) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                self.units[t].flame_shield = 500;
                let (from, to) = (self.units[i].centre_px(), self.units[t].centre_px());
                self.new_fx(
                    kind::BOLT,
                    caster,
                    tid,
                    from,
                    0,
                    Fx::Bolt {
                        line: Line::new(from, to),
                    },
                );
            }
            FIREBALL => {
                if !self.begin(i, 100, true) {
                    return;
                }
                let (from, to) = (self.units[i].centre_px(), centre_of_tile(tile));
                let fx = Fx::Fireball {
                    line: Line::new(from, to),
                    count: 0,
                };
                self.new_fx(kind::FIREBALL, caster, None, from, 40, fx);
            }
            SLOW => {
                let t = ti.expect("unit spell");
                if !self.begin(i, 50, true) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                let h = &mut self.units[t].haste;
                *h = if *h < 0 { -1000 } else { h.saturating_sub(1000) };
                self.blink(t);
            }
            INVISIBILITY => {
                let t = ti.expect("unit spell");
                if !self.begin(i, 200, true) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                self.units[t].invisibility = 2000;
                self.blink(t);
            }
            POLYMORPH => {
                let t = ti.expect("unit spell");
                if !self.begin(i, 200, true) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                let at = self.units[t].tile;
                self.units[t].used = true;
                self.kill(t);
                if let Ok(c) = self.spawn(unit::CRITTER, slots::NEUTRAL, at, 0) {
                    let _ = c;
                }
                self.new_fx(kind::BLINK, caster, None, centre_of_tile(at), 0, Fx::Cosmetic);
            }
            BLIZZARD => {
                if !self.pay_mana(i, 25) {
                    return;
                }
                self.finish_cast(i, false);
                self.spell_sound(spell, None, tile);
                let c = centre_of_tile(tile);
                for _ in 0..5 {
                    let x = c.0 + ((self.rng.draw() % 5) as i32 - 2) * TILE_PX;
                    let y = c.1 + ((self.rng.draw() % 5) as i32 - 2) * TILE_PX;
                    self.start_shard(caster, (x, y), 10);
                }
            }
            EYE_OF_KILROGG => {
                if !self.begin(i, 70, true) {
                    return;
                }
                self.cue_plain(Sound::Spell(spell));
                let u = &self.units[i];
                let src = Source::new(u.tile.0, u.tile.1, u.size.0 as i32, u.size.1 as i32);
                let at = u.tile;
                match self.find_place(unit::EYE, src, at, LIMIT_OUT) {
                    Some(t) => {
                        let _ = self.spawn(unit::EYE, owner, t, 0);
                    }
                    None => self.events.push(Event::NoRoom {
                        player: owner,
                        kind: unit::EYE,
                    }),
                }
            }
            BLOODLUST => {
                let t = ti.expect("unit spell");
                if !self.begin(i, 50, true) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                self.units[t].bloodlust = 1000;
                self.blink(t);
            }
            RAISE_DEAD => {
                self.finish_cast(i, true);
                self.spell_sound(spell, None, tile);
                self.raise_dead(caster, tile);
            }
            DEATH_COIL => {
                if !self.begin(i, 100, true) {
                    return;
                }
                self.spell_sound(spell, None, tile);
                self.death_coil(i, tile);
            }
            WHIRLWIND => {
                if !self.begin(i, 100, true) {
                    return;
                }
                self.spell_sound(spell, None, tile);
                let c = centre_of_tile(tile);
                let x = c.0 + (self.rng.draw() & 127) as i32 - 64;
                let y = c.1 + (self.rng.draw() & 127) as i32 - 64;
                let fx = Fx::Whirl {
                    line: Line::new((x, y), c),
                    life: 800,
                    goal: c,
                };
                self.new_fx(kind::WHIRLWIND, caster, None, (x, y), 4, fx);
            }
            HASTE => {
                let t = ti.expect("unit spell");
                if !self.begin(i, 50, true) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                let h = &mut self.units[t].haste;
                *h = if *h > 0 { 1000 } else { h.saturating_add(1000) };
                self.blink(t);
            }
            UNHOLY_ARMOUR => {
                let t = ti.expect("unit spell");
                if !self.begin(i, 100, true) {
                    return;
                }
                self.spell_sound(spell, Some(t), tile);
                let u = &mut self.units[t];
                u.unholy_armor = 500;
                if u.hp >= 2 {
                    u.hp /= 2;
                }
                self.blink(t);
            }
            RUNES => {
                if !self.begin(i, 200, true) {
                    return;
                }
                self.spell_sound(spell, None, tile);
                let refund = (5 - self.place_runes(tile)) * 40;
                let m = &mut self.units[i].mana;
                *m = (*m as u32 + refund).min(255) as u8;
            }
            DEATH_AND_DECAY => {
                if !self.pay_mana(i, 25) {
                    return;
                }
                self.finish_cast(i, false);
                self.spell_sound(spell, None, tile);
                let c = centre_of_tile(tile);
                for _ in 0..5 {
                    let x = c.0 + ((self.rng.draw() % 5) as i32 - 2) * TILE_PX;
                    let y = c.1 + ((self.rng.draw() % 5) as i32 - 2) * TILE_PX;
                    self.new_fx(kind::CLOUD, caster, None, (x, y), 10, Fx::Cloud);
                }
            }
            _ => self.idle_pending(i),
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

    // ----- effects: creation ---------------------------------------------------------------------------------

    /// The cosmetic explosion of a unit with the explode flag (corpses spec, "Units with the explode flag").
    pub(crate) fn explosion(&mut self, unit: UnitId, pos: (i32, i32)) {
        self.new_fx(kind::EXPLOSION, unit, None, pos, 0, Fx::Cosmetic);
    }

    /// The cosmetic effect a missile of kind `missile` leaves at its impact at `pos`, if any (damage spec, "Impact
    /// effects"); it takes the lowest free slot, or isn't made when there is none.
    pub(crate) fn impact_fx(&mut self, missile: u8, owner: UnitId, pos: (i32, i32)) {
        if let Some(k) = impact_effect(missile) {
            self.new_fx(k, owner, None, pos, 0, Fx::Cosmetic);
        }
    }

    /// The cosmetic smoke of a unit's cannon shot (damage spec), at the missile's start point; none if no slot is free.
    pub(crate) fn cannon_smoke(&mut self, owner: UnitId, pos: (i32, i32)) {
        self.new_fx(kind::CANNON_SMOKE, owner, None, pos, 0, Fx::Cosmetic);
    }

    /// Puts an effect in the lowest free effect slot after the permanent ones; `None` when the table is full.
    fn new_fx(
        &mut self,
        kind: u8,
        owner: UnitId,
        target: Option<UnitId>,
        pos: (i32, i32),
        damage: u8,
        fx: Fx,
    ) -> Option<usize> {
        let m = crate::battle::Missile {
            kind,
            attacker: owner,
            target,
            from: pos,
            to: pos,
            pos,
            flight: 0,
            elapsed: 0,
            damage,
            splash: false,
            hits_air: false,
            fx,
        };
        let n = self
            .missiles
            .iter()
            .enumerate()
            .skip(slots::PERMANENT_EFFECTS)
            .find(|(_, e)| e.is_none())
            .map(|(n, _)| n)?;
        self.missiles[n] = Some(m);
        Some(n)
    }

    /// A cosmetic effect of kind 0x16 on a unit.
    fn blink(&mut self, t: usize) {
        let (id, c) = (self.units[t].id, self.units[t].centre_px());
        self.new_fx(kind::BLINK, id, None, c, 0, Fx::Cosmetic);
    }

    /// The exorcism or healing effect on a unit (spec, "Cosmetic effects").
    fn wisp(&mut self, k: u8, t: usize) {
        let u = &self.units[t];
        let b = self.stats_of(u.kind).box_size;
        let pos = (u.pos.0 + b.0 as i32 / 2, u.pos.1 + b.1 as i32 / 2 - 4);
        let id = u.id;
        self.new_fx(k, id, None, pos, 0, Fx::Wisp);
    }

    /// One blizzard shard of a chain: its start offset is two draws.
    fn start_shard(&mut self, caster: UnitId, land: (i32, i32), repeats: u8) -> bool {
        let ox = (self.rng.draw() % 3) as i32 * 11 - 11;
        let oy = (self.rng.draw() % 3) as i32 * 11 - 11;
        let start = (land.0 - 110 + ox, land.1 - 170 + oy);
        let fx = Fx::Shard {
            line: Line::new(start, land),
            repeats,
        };
        self.new_fx(kind::SHARD, caster, None, start, 10, fx).is_some()
    }

    // ----- spells --------------------------------------------------------------------------------------------

    fn holy_vision(&mut self, owner: u8, t: (i32, i32)) {
        const OFFSETS: [(i32, i32); 7] = [(0, 0), (0, -8), (0, 8), (6, 4), (6, -4), (-6, 4), (-6, -4)];
        let w = self.terrain.width as i32;
        let enhanced = self.rules.enhanced;
        let caster = self.units.iter().find(|u| u.owner == owner).map_or(UnitId(0), |u| u.id);
        for (dx, dy) in OFFSETS {
            let p = (t.0 + dx, t.1 + dy);
            let limit = if enhanced { w - 1 } else { w };
            if p.0 < 0 || p.1 < 0 || p.0 > limit || p.1 > limit {
                continue;
            }
            if p.0 < w && p.1 < w {
                self.reveal(owner, p, 9);
            }
            self.new_fx(
                kind::BLINK,
                caster,
                None,
                (p.0 * TILE_PX + 8, p.1 * TILE_PX + 8),
                0,
                Fx::Cosmetic,
            );
        }
        if enhanced {
            self.vision.countdown = 150;
        }
    }

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
                self.blink(si);
                let bi = self.index(b).expect("still there");
                self.units[bi].used = true;
            }
            if let Some(i) = self.index(caster) {
                self.units[i].mana -= 50;
            }
        }
    }

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
    /// the victim's share as its stored damage.
    fn launch_coil(&mut self, caster: UnitId, victim: UnitId, share: u8) {
        let (Some(a), Some(v)) = (self.unit(caster), self.unit(victim)) else {
            return;
        };
        let s = self.stats_of(a.kind);
        let (basic, pierce) = combat::attack(a.kind, &s, &self.players[a.owner as usize].upgrades, a.bloodlust > 0);
        let armour = combat::armour(&self.stats_of(v.kind), &self.players[v.owner as usize].upgrades);
        let (from, mut to) = (a.centre_px(), v.centre_px());
        let _ = combat::roll(&mut self.rng, basic, pierce, armour);
        to.0 += (self.rng.draw() & 7) as i32 - 3;
        to.1 += (self.rng.draw() & 7) as i32 - 3;
        let fx = Fx::Coil {
            line: Line::new(from, to),
        };
        self.new_fx(kind::COIL, caster, Some(victim), from, share, fx);
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
            self.new_fx(kind::RUNE, UnitId(0), None, centre_of_tile(tile), 0, Fx::Cosmetic);
            n += 1;
        }
        n
    }

    // ----- the timers pass -----------------------------------------------------------------------------------

    /// The timers pass at the end of phase 4 (spec, "Timers" and "Runes on the map").
    pub(crate) fn timers_pass(&mut self) {
        self.advance_runes();
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if u.dying || u.removing || u.script.anim == crate::world::RELEASED {
                continue;
            }
            let (kind, owner) = (u.kind, u.owner);
            let s = self.stats_of(kind);
            // Caster-ai spec, "Marks": the seven target marks last until the unit's next visit here.
            self.units[i].marks = 0;
            if kind == unit::BERSERKER && self.players[owner as usize].upgrades.marksmanship {
                // Our reading: the regeneration upgrade is the level line it shares with marksmanship.
                let u = &mut self.units[i];
                u.mana += 1;
                if u.mana >= 240 {
                    u.mana = 0;
                    if u.hp < s.hit_points {
                        u.hp += 1;
                    }
                }
            }
            let u = &self.units[i];
            if matches!(kind, unit::DEMOLITION | unit::SAPPER) && (u.invisibility > 0 || u.unholy_armor > 0) {
                self.kill(i);
                continue;
            }
            let u = &mut self.units[i];
            u.invisibility = u.invisibility.saturating_sub(1);
            u.unholy_armor = u.unholy_armor.saturating_sub(1);
            u.bloodlust = u.bloodlust.saturating_sub(1);
            u.flame_shield = u.flame_shield.saturating_sub(1);
            u.haste -= u.haste.signum();
            if s.flags & flags::CASTER != 0 {
                if u.mana < 255 {
                    u.mana_counter = u.mana_counter.saturating_sub(1);
                    if u.mana_counter == 0 {
                        u.mana_counter = 40;
                        u.mana += 1;
                    }
                }
            } else if u.decays {
                u.decay_count = u.decay_count.saturating_sub(1);
                if u.decay_count == 0 {
                    u.decay_count = s.decay_rate;
                    if u.decay_period > 0 {
                        u.decay_period -= 1;
                    } else if u.hp >= 5 {
                        u.hp -= 5;
                    } else {
                        self.kill(i);
                    }
                }
            }
        }
    }

    fn advance_runes(&mut self) {
        for r in 0..self.runes.len() {
            let Some(mut rune) = self.runes[r] else { continue };
            rune.life -= 1;
            if rune.life & 0xff == 1 {
                self.new_fx(kind::RUNE, UnitId(0), None, centre_of_tile(rune.tile), 0, Fx::Cosmetic);
            }
            let hit = self.occupant(Layer::Land, rune.tile.0, rune.tile.1);
            if let Some(ui) = hit.and_then(|id| self.index(id)) {
                self.runes[r] = None;
                self.sound_at_tile(rune.tile, Sound::Explode);
                self.new_fx(
                    kind::EXPLOSION,
                    UnitId(0),
                    None,
                    centre_of_tile(rune.tile),
                    0,
                    Fx::Cosmetic,
                );
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

    // ----- effects in flight ---------------------------------------------------------------------------------

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

    /// Returns whether the effect stays; when it doesn't, the slot has been freed.
    fn fx_update(&mut self, n: usize, m: &mut crate::battle::Missile, age: u16) -> bool {
        let owner = m.attacker;
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
            Fx::Wisp => match age {
                0 => true,
                1 => {
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
            Fx::Fireball { line, count } => {
                if age == 0 {
                    return true;
                }
                self.rng.draw();
                if *count > FIREBALL_COUNT_LIMIT {
                    let t = tile_of(m.pos);
                    self.hit_wall(t, m.damage);
                    return self.gone(n);
                }
                let speed = if line.remaining < 50 { 6 } else { 12 };
                line.step(&mut m.pos, speed);
                if line.remaining < 50 {
                    *count += 1;
                    if matches!(*count, 8 | 16 | 24 | 32 | 40) {
                        let pos = m.pos;
                        self.fx_splash(pos, m.damage, owner);
                        self.new_fx(kind::EXPLOSION, owner, None, pos, 0, Fx::Cosmetic);
                    }
                }
                true
            }
            Fx::Bolt { line } => {
                if age == 0 {
                    return true;
                }
                self.rng.draw();
                if line.remaining <= 0 {
                    if let Some(t) = m.target.filter(|&t| self.unit(t).is_some_and(|u| u.present())) {
                        // Sounds spec, "Other explosions": the bolt arrives and makes its flames.
                        self.sound_at_px(m.pos, Sound::Explode);
                        for (point, life) in FLAMES {
                            let c = self.unit(t).expect("checked").centre_px();
                            let p = FLAME_PATH[point as usize];
                            let pos = (c.0 + p.0, c.1 + p.1);
                            self.new_fx(kind::FLAME, t, Some(t), pos, 4, Fx::Flame { point, life });
                        }
                    }
                    return self.gone(n);
                }
                line.step(&mut m.pos, EFFECT_SPEED[kind::BOLT as usize]);
                true
            }
            Fx::Flame { point, life } => {
                if age == 0 {
                    return true;
                }
                let Some(c) = m
                    .target
                    .and_then(|t| self.unit(t))
                    .filter(|u| u.present())
                    .map(|u| u.centre_px())
                else {
                    return self.gone(n);
                };
                *point = (*point + 1) % 51;
                let p = FLAME_PATH[*point as usize];
                m.pos = (c.0 + p.0, c.1 + p.1);
                *life -= 1;
                if *life & 15 == 1 {
                    let pos = m.pos;
                    self.fx_splash(pos, m.damage, owner);
                }
                *life > 0 || self.gone(n)
            }
            Fx::Shard { line, repeats } => {
                if age < 12 {
                    return true;
                }
                if line.remaining > 0 {
                    line.step(&mut m.pos, EFFECT_SPEED[kind::SHARD as usize]);
                    self.rng.draw();
                    return true;
                }
                let (pos, repeats, land) = (m.pos, *repeats, m.to);
                self.fx_splash(pos, m.damage, owner);
                self.gone(n);
                if repeats > 0 && self.start_shard(owner, land, repeats - 1) {
                    self.rng.draw();
                }
                false
            }
            Fx::Cloud => {
                match age {
                    34..=250 if (age - 34).is_multiple_of(24) => {
                        let pos = m.pos;
                        self.fx_splash(pos, m.damage, owner);
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
                    self.fx_splash(pos, m.damage, owner);
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
                }
                true
            }
            Fx::Coil { line } => {
                if age == 0 {
                    return true;
                }
                line.step(&mut m.pos, EFFECT_SPEED[kind::COIL as usize]);
                self.rng.draw();
                if line.remaining > 0 {
                    return true;
                }
                if let Some(v) = m.target.and_then(|t| self.index(t)) {
                    self.damage(v, m.damage, owner);
                }
                self.gone(n)
            }
        }
    }

    /// The splash rule of the damage spec for spell effects (spec, "Splash"): armour is ignored, air units are
    /// scanned too, and the effect's owner is never hit.
    pub(crate) fn fx_splash(&mut self, pos: (i32, i32), stored: u8, owner: UnitId) {
        let tile = tile_of(pos);
        self.hit_wall(tile, stored);
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            self.hit_wall((tile.0 + dx, tile.1 + dy), stored / 4);
        }
        let mut hit: Vec<UnitId> = Vec::new();
        let mut consider = |w: &World, x: i32, y: i32| {
            for l in [Layer::Land, Layer::Air] {
                if let Some(id) = w.occupant(l, x, y)
                    && id != owner
                    && !hit.contains(&id)
                {
                    hit.push(id);
                }
            }
        };
        consider(self, tile.0, tile.1);
        for y in tile.1 - 3..=tile.1 + 3 {
            for x in tile.0 - 3..=tile.0 + 3 {
                consider(self, x, y);
            }
        }
        for id in hit {
            let Some(ti) = self.index(id) else { continue };
            let t = &self.units[ti];
            if t.dying {
                continue;
            }
            let c = t.centre_px();
            let d = (c.0 - pos.0).abs().max((c.1 - pos.1).abs());
            let base = match d {
                0..=22 => stored as u32,
                23..=42 => stored as u32 / 4,
                _ => continue,
            };
            let h = base.div_ceil(2);
            let dmg = h + self.rng.draw() % (h + 1);
            self.damage(ti, dmg.min(255) as u8, owner);
        }
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

    /// Strikes at once, as the casting animation would, with `order` set directly.
    fn strike(w: &mut World, id: UnitId, spell: u8, target: Option<UnitId>, tile: (i32, i32)) {
        let i = w.index(id).unwrap();
        w.units[i].order = Order::Cast { spell, target, tile };
        w.cast_strike(i);
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
        assert_eq!(w.unit(eye).unwrap().decay_period, 255);
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
        assert!(eye.tile != (5, 5) && chebyshev(eye.tile, (5, 5)) <= 3, "{:?}", eye.tile);
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
        assert!(hp(&w, own) < 40, "Classic mode hits its own undead");
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
        let fx = Fx::Fireball { line, count: 0 };
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
        assert_eq!(ended - 1, 54, "the flight ends at step 54");
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
        w.fx_splash(pos, 10, UnitId(0));
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
        run(&mut w, 2);
        assert_eq!(w.rng, advanced(before, 1), "one flight step, one draw");
        run(&mut w, 2);
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
        w.blink(i);
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
}
