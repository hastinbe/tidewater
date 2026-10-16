//! Spell casting by computer units: the spell step casters run before their goal in pass B of the army logic. Spec:
//! `docs/specs/ai/caster-ai.md`. Unit flag bits and spell bits are those of the public PUD specification.
//!
//! The spell itself goes through the spell order of `spells.rs` ([`World::give_spell`]).
//!
//! Open questions, with the spec's defaults:
//! - Free slots in the random search: a released slot keeps its type, tile and target marks ([`Stale`]), with owner
//!   14 and no used mark; a never-used slot reads as all zero with owner 14. The alliance table is applied
//!   literally, so owner 14 counts as allied only with itself.
//! - Polymorph from the random search is given with no target unit and fails as the spell order does for a unit
//!   spell without a target.
//! - The eye of Kilrogg keeps the old target tile and unit.
//! - Raise dead is implemented as specified and so never finds a body (bodies aren't on the ground layer).
//! - The mage's invisibility try sets the target's unholy-armour timer, with its effects.
//!
//! Our readings where the spec leaves room:
//! - The "used mark" is our `used` mark or `inside` (the used mark is also the "inside something" mark,
//!   `core/corpses.md`).
//! - The eye of Kilrogg's "old target" is the current order's target unit and tile; an order with a target unit but no
//!   tile takes the unit's tile, and an order with neither takes the caster's own tile.
//! - "Attack range" is the type's attack range in the unit data.

use std::collections::BTreeMap;

use tidewater_formats::udta::flags;

use crate::attack_ai::GoalKind;
use crate::slots;
use crate::spells::spell::*;
use crate::world::{Order, UnitId, World};

/// The seven target marks, as bits of [`crate::Unit::marks`].
pub mod mark {
    pub const DEATH_COIL: u8 = 1 << 0;
    pub const POLYMORPH: u8 = 1 << 1;
    pub const HOLY_VISION: u8 = 1 << 2;
    pub const EXORCISM: u8 = 1 << 3;
    pub const HEALING: u8 = 1 << 4;
    pub const RAISE_DEAD: u8 = 1 << 5;
    pub const WHIRLWIND: u8 = 1 << 6;
}

/// What a released unit slot still holds for the random search: its last occupant's type, tile and target marks.
/// Its owner is 14 and it carries no used mark.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Stale {
    pub kind: u8,
    pub tile: (i32, i32),
    pub marks: u8,
}

/// Counts for inspection tools (not game state): per player, the spell commands the caster AI gave by spell order
/// (38 to 56), its retreats, and every spell strike made (any owner).
#[derive(Clone, Debug, Default)]
pub struct CastCounts {
    pub commands: BTreeMap<(u8, u8), u32>,
    pub retreats: [u32; 8],
    pub strikes: [u32; 16],
}

/// PUD unit types the spec names.
mod unit {
    pub const MAGE: u8 = 0x0a;
    pub const DEATH_KNIGHT: u8 = 0x0b;
    pub const PALADIN: u8 = 0x0c;
    pub const OGRE_MAGE: u8 = 0x0d;
    pub const TERON: u8 = 0x15;
    pub const DENTARG: u8 = 0x17;
    pub const KHADGAR: u8 = 0x18;
    pub const TURALYON: u8 = 0x2c;
    pub const MAGE_TOWER: u8 = 0x50;
    pub const TEMPLE: u8 = 0x51;
    pub const DEAD_BODY: u8 = 0x69;
}

/// Side of half the nearby box: 31 x 31 tiles centred on the caster.
const NEARBY: i32 = 15;
/// Draws of one random try.
const RANDOM_DRAWS: usize = 10;
/// The stronger mana gate of the nearest-enemy blizzard and death and decay.
const AREA_GATE: u8 = 75;
/// Mana exactly this for the eye of Kilrogg and the paladin's holy vision.
const FULL: u8 = 255;
/// Mage and death knight retreat below this mana.
const RETREAT_MANA: u8 = 50;

/// The spell bit of a spell order, in the PUD text's numbering of the spells known word.
pub fn spell_bit(spell: u8) -> u32 {
    let n = match spell {
        HOLY_VISION => 0,
        HEALING => 1,
        EXORCISM => 3,
        FLAME_SHIELD => 4,
        FIREBALL => 5,
        SLOW => 6,
        INVISIBILITY => 7,
        POLYMORPH => 8,
        BLIZZARD => 9,
        EYE_OF_KILROGG => 10,
        BLOODLUST => 11,
        RAISE_DEAD => 13,
        DEATH_COIL => 14,
        WHIRLWIND => 15,
        HASTE => 16,
        UNHOLY_ARMOUR => 17,
        RUNES => 18,
        DEATH_AND_DECAY => 19,
        _ => return 0,
    };
    1 << n
}

/// How a try finds its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Search {
    Nearby,
    NearestEnemy,
    Random,
}

/// What a successful try marks on its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    None,
    Target(u8),
    Bloodlust,
    UnholyArmour,
    FlameShield,
    Speed(i16),
}

/// A candidate target: a unit in the table (`live`), or a released or never-used slot.
#[derive(Clone, Copy, Debug)]
struct Tgt {
    slot: u16,
    live: Option<usize>,
    kind: u8,
    owner: u8,
    tile: (i32, i32),
    marks: u8,
    used: bool,
}

/// A try's test on a target: the world, the caster's index and the target.
type Test<'a> = &'a dyn Fn(&World, usize, &Tgt) -> bool;

/// One try's fixed parts.
struct Try<'a> {
    spell: u8,
    search: Search,
    /// The try's own condition before the gates ("only if invisible", the stronger mana gates), checked with them.
    extra: bool,
    test: Test<'a>,
    /// Aim at the target's tile with no target unit (raise dead); the random search always does.
    tile_only: bool,
    mark: Mark,
}

impl World {
    /// The alliance table: a player counts itself and the neutral owner (15) as allied, and the players its alliance
    /// bits name. Owner 14 (free slots) counts as allied only with itself. Game setup's table already sets the neutral
    /// bit for every player, but worlds not built from a map start with no alliance bits, so the neutral case stays.
    fn alliance(&self, a: u8, b: u8) -> bool {
        b == slots::NEUTRAL || self.allied(a, b)
    }

    fn type_flags(&self, kind: u8) -> u32 {
        self.stats_of(kind).flags
    }

    fn tgt_live(&self, k: usize) -> Tgt {
        let u = &self.units[k];
        Tgt {
            slot: u.slot,
            live: Some(k),
            kind: u.kind,
            owner: u.owner,
            tile: u.tile,
            marks: u.marks,
            used: u.used || u.inside,
        }
    }

    /// What unit slot `s` holds, live or not.
    fn tgt_slot(&self, s: u16) -> Tgt {
        if let Ok(k) = self.units.binary_search_by_key(&s, |u| u.slot) {
            return self.tgt_live(k);
        }
        let st = self.stale.get(&s).copied().unwrap_or_default();
        Tgt {
            slot: s,
            live: None,
            kind: st.kind,
            owner: slots::FREE,
            tile: st.tile,
            marks: st.marks,
            used: false,
        }
    }

    /// "The box test": a unit other than `x`, without the used mark, whose owner `p` counts as allied, in the
    /// (2d + 1) x (2d + 1) ground-layer tiles centred on `x`'s tile.
    fn company(&self, x: &Tgt, d: i32, p: u8) -> bool {
        let xid = x.live.map(|k| self.units[k].id);
        for y in x.tile.1 - d..=x.tile.1 + d {
            for xx in x.tile.0 - d..=x.tile.0 + d {
                let Some(id) = self.occupant(crate::Layer::Land, xx, y) else {
                    continue;
                };
                if Some(id) == xid {
                    continue;
                }
                let Some(u) = self.unit(id) else { continue };
                if !(u.used || u.inside) && self.alliance(p, u.owner) {
                    return true;
                }
            }
        }
        false
    }

    /// The nearby search: the 31 x 31 box round the caster, row by row from the top, ground layer only, skipping the
    /// caster and units with the used mark. No fog or detection test.
    fn nearby(&self, c: usize, test: Test) -> Option<Tgt> {
        let (cx, cy) = self.units[c].tile;
        let me = self.units[c].id;
        for y in cy - NEARBY..=cy + NEARBY {
            for x in cx - NEARBY..=cx + NEARBY {
                let Some(id) = self.occupant(crate::Layer::Land, x, y) else {
                    continue;
                };
                if id == me {
                    continue;
                }
                let Some(k) = self.index(id) else { continue };
                let t = self.tgt_live(k);
                if !t.used && test(self, c, &t) {
                    return Some(t);
                }
            }
        }
        None
    }

    /// Runs one try ("The tries"); true when it succeeds.
    fn spell_try(&mut self, c: usize, t: Try) -> bool {
        let owner = self.units[c].owner;
        let known = self.players[owner as usize].spells_known & spell_bit(t.spell) != 0;
        if !known || self.units[c].mana < Self::mana_cost(t.spell) || !t.extra {
            return false;
        }
        let (found, aim_unit) = match t.search {
            Search::Nearby => (self.nearby(c, t.test), !t.tile_only),
            Search::NearestEnemy => {
                let found = self
                    .enemy_search(c)
                    .and_then(|id| self.index(id))
                    .map(|k| self.tgt_live(k))
                    .filter(|x| (t.test)(self, c, x));
                (found, !t.tile_only)
            }
            Search::Random => {
                let n = self.rules.unit_slots as u32;
                let mut last = None;
                for _ in 0..RANDOM_DRAWS {
                    let s = (self.rng.draw() % n) as u16;
                    let x = self.tgt_slot(s);
                    if x.used || !(t.test)(self, c, &x) {
                        return false;
                    }
                    last = Some(x);
                }
                (last, false)
            }
        };
        let Some(x) = found else { return false };
        let target = if aim_unit {
            x.live.map(|k| self.units[k].id)
        } else {
            None
        };
        self.give_spell(c, t.spell, target, x.tile);
        *self.casts.commands.entry((owner, t.spell)).or_default() += 1;
        self.set_mark(&x, t.mark);
        true
    }

    fn set_mark(&mut self, x: &Tgt, m: Mark) {
        let Some(k) = x.live else {
            if let Mark::Target(b) = m {
                self.stale.entry(x.slot).or_default().marks |= b;
            }
            return;
        };
        let u = &mut self.units[k];
        match m {
            Mark::None => {}
            Mark::Target(b) => u.marks |= b,
            Mark::Bloodlust => u.bloodlust = 1,
            Mark::UnholyArmour => u.unholy_armor = 1,
            Mark::FlameShield => u.flame_shield = 1,
            Mark::Speed(v) => u.haste = v,
        }
    }

    /// The caster spell step (spec, "When the step runs"): true if it acts.
    pub(crate) fn caster_step(&mut self, c: usize) -> bool {
        match self.units[c].kind {
            unit::OGRE_MAGE | unit::DENTARG => self.ogre_mage_step(c),
            unit::PALADIN | unit::TURALYON => self.paladin_step(c),
            unit::MAGE | unit::KHADGAR => self.mage_step(c),
            unit::DEATH_KNIGHT | unit::TERON => self.death_knight_step(c),
            _ => false,
        }
    }

    fn invisible(&self, c: usize) -> bool {
        self.units[c].invisibility > 0
    }

    // ----- shared tests ------------------------------------------------------------------------------------------

    fn ally(&self, c: usize, x: &Tgt) -> bool {
        self.alliance(self.units[c].owner, x.owner)
    }

    fn tgt_has(&self, x: &Tgt, f: u32) -> bool {
        self.type_flags(x.kind) & f != 0
    }

    fn chasing(&self, x: &Tgt) -> bool {
        x.live
            .is_some_and(|k| matches!(self.units[k].order, Order::Chase { .. }))
    }

    fn range_1(&self, x: &Tgt) -> bool {
        self.stats_of(x.kind).attack_range == 1
    }

    /// Not allied; a worker or gold store; no company within 3 for the caster's owner (the nearest-enemy area tries).
    fn soft_target(&self, c: usize, x: &Tgt) -> bool {
        !self.ally(c, x)
            && self.tgt_has(x, flags::PEON | flags::GOLD_STORAGE)
            && !self.company(x, 3, self.units[c].owner)
    }

    /// Not allied; a building; no company within 3 for the caster's owner.
    fn lone_building(&self, c: usize, x: &Tgt) -> bool {
        !self.ally(c, x) && self.tgt_has(x, flags::BUILDING) && !self.company(x, 3, self.units[c].owner)
    }

    // ----- the steps -----------------------------------------------------------------------------------------------

    /// "Ogre-mage".
    fn ogre_mage_step(&mut self, c: usize) -> bool {
        let inv = self.invisible(c);
        let runes: Test = &|w, c, x| !w.ally(c, x) && w.tgt_has(x, flags::PEON);
        let bloodlust: Test = &|w, c, x| {
            !w.invisible(c)
                && w.ally(c, x)
                && w.tgt_has(x, flags::ORGANIC)
                && x.live.is_some_and(|k| w.units[k].bloodlust == 0)
                && w.chasing(x)
        };
        self.spell_try(c, Try::random(RUNES, inv, runes))
            || self.spell_try(c, Try::nearby(BLOODLUST, bloodlust, Mark::Bloodlust))
            || self.spell_try(c, Try::random(RUNES, true, runes))
            || self.eye_of_kilrogg(c)
    }

    /// Try 4 of the ogre-mage: the pending order becomes 48, keeping the old target, with no spell command.
    fn eye_of_kilrogg(&mut self, c: usize) -> bool {
        let u = &self.units[c];
        let known = self.players[u.owner as usize].spells_known & spell_bit(EYE_OF_KILROGG) != 0;
        if !known || u.mana < Self::mana_cost(EYE_OF_KILROGG) || u.mana != FULL {
            return false;
        }
        let (target, tile) = self.order_target(c);
        let owner = u.owner;
        self.units[c].pending = Some(Order::Cast {
            spell: EYE_OF_KILROGG,
            target,
            tile,
        });
        *self.casts.commands.entry((owner, EYE_OF_KILROGG)).or_default() += 1;
        true
    }

    /// The current order's target unit and tile (our reading of "the old target").
    fn order_target(&self, c: usize) -> (Option<UnitId>, (i32, i32)) {
        let u = &self.units[c];
        let unit = match u.order {
            Order::Follow { target }
            | Order::Guard { target }
            | Order::Attack { target }
            | Order::Chase { target }
            | Order::AttackInPlace { target }
            | Order::IdleReact { target }
            | Order::Repair { target } => Some(target),
            Order::TowerWatch { target } => target,
            Order::Harvest { source, .. } => source,
            Order::ReturnGoods { depot } => depot,
            Order::Enter { building, .. } => Some(building),
            Order::InsideSite { building } => building,
            Order::Inside { building } => building,
            Order::Board { transport } | Order::Aboard { transport } => Some(transport),
            Order::Demolish { at, .. } => at,
            Order::Cast { target, .. } => target,
            _ => None,
        };
        let tile = match u.order {
            Order::Move { goal, .. }
            | Order::MoveAttack { goal, .. }
            | Order::AttackMove { goal }
            | Order::Land { goal }
            | Order::Patrol { to: goal, .. } => Some(goal),
            Order::AttackWall { tile }
            | Order::AttackGround { tile }
            | Order::AttackGroundMove { tile, .. }
            | Order::Demolish { tile, .. }
            | Order::DemolishAt { tile }
            | Order::Cast { tile, .. } => Some(tile),
            Order::Harvest { source: None, tree, .. } => Some(tree),
            Order::Build { site, .. } => Some(site),
            _ => None,
        };
        let tile = tile
            .or_else(|| unit.and_then(|t| self.unit(t)).map(|t| t.tile))
            .unwrap_or(u.tile);
        (unit, tile)
    }

    /// "Paladin".
    fn paladin_step(&mut self, c: usize) -> bool {
        let inv = self.invisible(c);
        let full = self.units[c].mana == FULL;
        let exorcism: Test = &|w, c, x| !w.ally(c, x) && w.tgt_has(x, flags::UNDEAD) && x.marks & mark::EXORCISM == 0;
        let healing: Test = &|w, c, x| {
            !w.invisible(c)
                && w.ally(c, x)
                && w.tgt_has(x, flags::ORGANIC)
                && x.live.is_some_and(|k| w.units[k].hp < w.stats_of(x.kind).hit_points)
                && x.marks & mark::HEALING == 0
        };
        let vision: Test = &|w, c, x| !w.invisible(c) && !w.ally(c, x) && x.marks & mark::HOLY_VISION == 0;
        self.spell_try(c, Try::random(EXORCISM, inv, exorcism))
            || self.spell_try(c, Try::nearby(HEALING, healing, Mark::Target(mark::HEALING)))
            || self.spell_try(c, Try::nearby(EXORCISM, exorcism, Mark::Target(mark::EXORCISM)))
            || self.spell_try(
                c,
                Try {
                    mark: Mark::Target(mark::HOLY_VISION),
                    ..Try::random(HOLY_VISION, full, vision)
                },
            )
    }

    /// "Mage".
    fn mage_step(&mut self, c: usize) -> bool {
        let inv = self.invisible(c);
        let area = self.units[c].mana >= AREA_GATE;
        let soft: Test = &|w, c, x| w.soft_target(c, x);
        let lone: Test = &|w, c, x| w.lone_building(c, x);
        let caster_poly: Test = &|w, c, x| {
            let need = flags::CAN_ATTACK | flags::ORGANIC | flags::CASTER;
            !w.ally(c, x) && w.type_flags(x.kind) & need == need && x.marks & mark::POLYMORPH == 0
        };
        let poly: Test = &|w, c, x| {
            let need = flags::CAN_ATTACK | flags::ORGANIC;
            !w.ally(c, x) && w.type_flags(x.kind) & need == need && x.marks & mark::POLYMORPH == 0
        };
        let fireball: Test = &|w, c, x| {
            let me = &w.units[c];
            let hurt = me.hp < w.stats_of(me.kind).hit_points / 2;
            !w.ally(c, x) && (!w.company(x, 3, me.owner) || hurt)
        };
        let invisibility: Test = &|w, c, x| {
            !w.invisible(c)
                && w.ally(c, x)
                && !w.tgt_has(x, flags::BUILDING)
                && w.units[c].goal.kind == GoalKind::Attack
                && (w.tgt_has(x, flags::CASTER) || w.range_1(x))
        };
        let shield: Test = &|w, c, x| {
            if w.invisible(c)
                || x.live.is_none_or(|k| w.units[k].flame_shield != 0)
                || w.company(x, 2, w.units[c].owner)
                || w.tgt_has(x, flags::BUILDING)
            {
                return false;
            }
            if w.ally(c, x) {
                w.chasing(x) && w.range_1(x)
            } else {
                w.company(x, 1, x.owner)
            }
        };
        let slow: Test = &|w, c, x| {
            !w.invisible(c)
                && !w.ally(c, x)
                && !w.tgt_has(x, flags::BUILDING)
                && x.live
                    .is_some_and(|k| w.units[k].haste >= 0 && w.units[k].order != Order::Idle)
        };
        self.spell_try(c, Try::nearest(BLIZZARD, inv && area, soft))
            || self.spell_try(c, Try::random(POLYMORPH, inv, caster_poly))
            || self.spell_try(c, Try::nearby(POLYMORPH, poly, Mark::Target(mark::POLYMORPH)))
            || self.spell_try(c, Try::nearby(FIREBALL, fireball, Mark::None))
            || self.spell_try(c, Try::nearby(INVISIBILITY, invisibility, Mark::UnholyArmour))
            || self.spell_try(c, Try::nearest(BLIZZARD, area, lone))
            || self.spell_try(c, Try::nearby(FLAME_SHIELD, shield, Mark::FlameShield))
            || self.spell_try(c, Try::nearby(SLOW, slow, Mark::Speed(-1)))
            || self.retreat(c)
    }

    /// "Death knight".
    fn death_knight_step(&mut self, c: usize) -> bool {
        let inv = self.invisible(c);
        let area = self.units[c].mana >= AREA_GATE;
        let soft: Test = &|w, c, x| w.soft_target(c, x);
        let lone: Test = &|w, c, x| w.lone_building(c, x);
        let random_wind: Test = &|w, c, x| {
            !w.ally(c, x)
                && !w.company(x, 4, w.units[c].owner)
                && x.marks & mark::WHIRLWIND == 0
                && w.tgt_has(x, flags::PEON | flags::GOLD_STORAGE)
        };
        let body: Test = &|w, _, x| {
            x.kind == unit::DEAD_BODY
                && x.live.is_some_and(|k| {
                    let u = &w.units[k];
                    u.remains && !u.used && !u.removing
                })
                && x.marks & mark::RAISE_DEAD == 0
        };
        let armour: Test = &|w, c, x| {
            !w.invisible(c)
                && w.ally(c, x)
                && x.live.is_some_and(|k| {
                    let u = &w.units[k];
                    u.unholy_armor == 0 && u.hp <= w.stats_of(u.kind).hit_points / 2
                })
                && w.chasing(x)
        };
        let coil_crowd: Test = &|w, c, x| {
            !w.ally(c, x) && x.marks & mark::DEATH_COIL == 0 && w.tgt_has(x, flags::ORGANIC) && w.company(x, 1, x.owner)
        };
        let coil: Test = &|w, c, x| {
            !w.invisible(c) && !w.ally(c, x) && x.marks & mark::DEATH_COIL == 0 && w.tgt_has(x, flags::ORGANIC)
        };
        let wind: Test =
            &|w, c, x| !w.ally(c, x) && !w.company(x, 4, w.units[c].owner) && x.marks & mark::WHIRLWIND == 0;
        let haste: Test =
            &|w, c, x| !w.invisible(c) && w.ally(c, x) && x.live.is_some_and(|k| w.units[k].haste == 0) && w.chasing(x);
        self.spell_try(c, Try::nearest(DEATH_AND_DECAY, inv && area, soft))
            || self.spell_try(c, Try::random(WHIRLWIND, inv, random_wind))
            || self.spell_try(
                c,
                Try {
                    tile_only: true,
                    ..Try::nearby(RAISE_DEAD, body, Mark::Target(mark::RAISE_DEAD))
                },
            )
            || self.spell_try(c, Try::nearby(UNHOLY_ARMOUR, armour, Mark::UnholyArmour))
            || self.spell_try(c, Try::nearest(DEATH_AND_DECAY, area, lone))
            || self.spell_try(c, Try::nearby(DEATH_COIL, coil_crowd, Mark::Target(mark::DEATH_COIL)))
            || self.spell_try(c, Try::nearby(DEATH_COIL, coil, Mark::Target(mark::DEATH_COIL)))
            || self.spell_try(
                c,
                Try {
                    mark: Mark::Target(mark::WHIRLWIND),
                    ..Try::nearest(WHIRLWIND, true, wind)
                },
            )
            || self.spell_try(c, Try::nearby(HASTE, haste, Mark::Speed(1)))
            || self.retreat(c)
    }

    /// "Retreat (mage and death knight only)".
    fn retreat(&mut self, c: usize) -> bool {
        if self.units[c].mana >= RETREAT_MANA {
            return false;
        }
        let owner = self.units[c].owner;
        if let Some(a) = self.units[c].attacked_by {
            // Whether the attacker is still alive isn't tested.
            self.command_12(c, a);
            self.casts.retreats[owner as usize & 7] += 1;
            return true;
        }
        let Some(tile) = self
            .units
            .iter()
            .find(|u| u.owner == owner && matches!(u.kind, unit::MAGE_TOWER | unit::TEMPLE))
            .map(|u| u.tile)
        else {
            return false;
        };
        if self.set_goal(c, GoalKind::Hold, Some(tile)) {
            self.smart_move(c, None, tile);
        }
        self.casts.retreats[owner as usize & 7] += 1;
        true
    }
}

impl<'a> Try<'a> {
    fn nearby(spell: u8, test: Test<'a>, mark: Mark) -> Self {
        Try {
            spell,
            search: Search::Nearby,
            extra: true,
            test,
            tile_only: false,
            mark,
        }
    }

    fn nearest(spell: u8, extra: bool, test: Test<'a>) -> Self {
        Try {
            spell,
            search: Search::NearestEnemy,
            extra,
            test,
            tile_only: false,
            mark: Mark::None,
        }
    }

    fn random(spell: u8, extra: bool, test: Test<'a>) -> Self {
        Try {
            spell,
            search: Search::Random,
            extra,
            test,
            tile_only: true,
            mark: Mark::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::attack_ai::Goal;
    use crate::rng::Rng;
    use crate::slots::Slots;
    use crate::world::tests::{field, run};
    use tidewater_formats::udta::UnitStats;

    const FOOTMAN: u8 = 0x00;
    const GRUNT: u8 = 0x01;
    const PEASANT: u8 = 0x02;
    const KNIGHT: u8 = 0x06;
    const DEMOLITION: u8 = 0x0e;
    const GRYPHON: u8 = 0x2a;
    const CHOGALL: u8 = 0x31;
    const SKELETON: u8 = 0x37;
    const CRITTER: u8 = 0x39;
    const FARM: u8 = 0x3a;
    use unit::*;

    fn fighter(hp: u16, f: u32, range: u8) -> UnitStats {
        UnitStats {
            hit_points: hp,
            size: (1, 1),
            box_size: (32, 32),
            flags: flags::LAND | flags::CAN_ATTACK | f,
            basic_damage: 6,
            piercing_damage: 3,
            attack_range: range,
            missile: crate::combat::missile::NONE,
            can_target: 7,
            react_range_computer: 6,
            sight: 9,
            ..Default::default()
        }
    }

    /// A 64 x 64 field; player 0 human, player 1 computer knowing every spell; no fog.
    fn game() -> World {
        let mut w = field(64, 64);
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        for p in 2..8 {
            w.players[p].controller = controller::NOBODY;
        }
        w.players[1].spells_known = !0;
        let o = flags::ORGANIC;
        let s = &mut w.stats.0;
        s[FOOTMAN as usize] = fighter(60, o, 1);
        s[GRUNT as usize] = fighter(60, o, 1);
        s[KNIGHT as usize] = fighter(90, o, 1);
        s[PEASANT as usize] = UnitStats {
            flags: flags::LAND | o | flags::PEON,
            ..fighter(30, 0, 1)
        };
        s[DEMOLITION as usize] = fighter(40, o, 1);
        s[MAGE as usize] = fighter(60, o | flags::CASTER, 2);
        s[DEATH_KNIGHT as usize] = fighter(60, flags::CASTER | flags::UNDEAD, 3);
        s[PALADIN as usize] = fighter(90, o | flags::CASTER, 1);
        s[OGRE_MAGE as usize] = fighter(90, o | flags::CASTER, 1);
        s[CHOGALL as usize] = fighter(90, o | flags::CASTER, 1);
        s[SKELETON as usize] = fighter(40, flags::UNDEAD, 1);
        s[CRITTER as usize] = UnitStats {
            flags: flags::LAND | flags::CRITTER,
            ..fighter(5, 0, 0)
        };
        s[GRYPHON as usize] = UnitStats {
            flags: flags::AIR | o | flags::CAN_ATTACK,
            ..fighter(100, 0, 4)
        };
        let building = |f: u32| UnitStats {
            hit_points: 400,
            size: (2, 2),
            flags: flags::BUILDING | f,
            ..Default::default()
        };
        s[FARM as usize] = building(0);
        s[MAGE_TOWER as usize] = building(0);
        s[TEMPLE as usize] = building(0);
        w.stride[PEASANT as usize] = 1;
        w.stride[GRYPHON as usize] = 2;
        w.rules.fog_of_war = false;
        w
    }

    /// A game with only `n` unit slots, so the random search's draws can be steered.
    fn small(n: usize) -> World {
        let mut w = game();
        w.rules.unit_slots = n;
        w.slots = Slots::new(n);
        w
    }

    /// Spawns a unit and runs the units' visibility (a new unit is hidden from everyone until then).
    fn put(w: &mut World, kind: u8, owner: u8, tile: (i32, i32)) -> UnitId {
        let id = w.spawn(kind, owner, tile, 0).unwrap();
        w.unit_visibility();
        id
    }

    fn caster(w: &mut World, kind: u8, tile: (i32, i32), mana: u8) -> UnitId {
        let id = put(w, kind, 1, tile);
        let u = um(w, id);
        (u.mana, u.mana_counter) = (mana, 255);
        id
    }

    fn ix(w: &World, id: UnitId) -> usize {
        w.index(id).unwrap()
    }

    fn at(w: &World, id: UnitId) -> &crate::Unit {
        w.unit(id).unwrap()
    }

    fn um(w: &mut World, id: UnitId) -> &mut crate::Unit {
        let i = ix(w, id);
        &mut w.units[i]
    }

    fn slot(w: &World, id: UnitId) -> u16 {
        at(w, id).slot
    }

    /// Runs the caster's spell step: (acted, draws made).
    fn cast_step(w: &mut World, id: UnitId) -> (bool, usize) {
        let before = w.rng;
        let acted = w.caster_step(ix(w, id));
        let mut r = before;
        let mut draws = 0;
        while r != w.rng {
            r.draw();
            draws += 1;
            assert!(draws < 100, "draws counted");
        }
        (acted, draws)
    }

    /// A generator whose next draws pick the slots `wanted` (mod `n`).
    fn seed_for(n: u32, wanted: &[u16]) -> Rng {
        let s = (0u32..)
            .find(|&s| {
                let mut r = Rng::new(s);
                wanted.iter().all(|&v| (r.draw() % n) as u16 == v)
            })
            .unwrap();
        Rng::new(s)
    }

    /// A generator whose next `k` draws (mod `n`) pass `ok`.
    fn seed_where(n: u32, k: usize, ok: impl Fn(&[u16]) -> bool) -> Rng {
        let s = (0u32..)
            .find(|&s| {
                let mut r = Rng::new(s);
                let d: Vec<u16> = (0..k).map(|_| (r.draw() % n) as u16).collect();
                ok(&d)
            })
            .unwrap();
        Rng::new(s)
    }

    fn cast(spell: u8, target: Option<UnitId>, tile: (i32, i32)) -> Option<Order> {
        Some(Order::Cast { spell, target, tile })
    }

    fn chasing(w: &mut World, id: UnitId) {
        um(w, id).order = Order::Chase { target: UnitId(999) };
    }

    // ----- terms ---------------------------------------------------------------------------------------------

    #[test]
    fn spell_bits_follow_the_pud_list() {
        let bits: Vec<u32> = (38..=56).map(|s| spell_bit(s).trailing_zeros()).collect();
        assert_eq!(
            bits,
            [0, 1, 32, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 18, 19]
        );
    }

    #[test]
    fn the_alliance_table_counts_neutral_as_allied_and_free_slots_as_nobodys() {
        let w = game();
        assert!(w.alliance(1, 1) && w.alliance(1, slots::NEUTRAL) && w.alliance(0, slots::NEUTRAL));
        assert!(!w.alliance(1, 0) && !w.alliance(1, slots::FREE));
        assert!(w.alliance(slots::FREE, slots::FREE) && w.alliance(slots::FREE, slots::NEUTRAL));
    }

    #[test]
    fn other_casters_never_act() {
        let mut w = game();
        let c = caster(&mut w, CHOGALL, (10, 10), 255);
        let g = put(&mut w, GRUNT, 1, (12, 10));
        chasing(&mut w, g);
        assert_eq!(cast_step(&mut w, c), (false, 0));
    }

    #[test]
    fn an_unknown_spell_or_too_little_mana_fails_the_gates_without_draws() {
        let mut w = game();
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 210);
        let g = put(&mut w, GRUNT, 1, (12, 10));
        chasing(&mut w, g);
        w.players[1].spells_known = 0;
        assert_eq!(cast_step(&mut w, c), (false, 0), "nothing known");
        w.players[1].spells_known = !0;
        um(&mut w, c).mana = 49;
        assert_eq!(cast_step(&mut w, c), (false, 0), "below bloodlust's 50 and runes' 200");
        um(&mut w, c).mana = 50;
        assert_eq!(cast_step(&mut w, c), (true, 0));
    }

    // ----- ogre-mage -----------------------------------------------------------------------------------------

    #[test]
    fn bloodlust_on_an_own_grunt_in_combat() {
        let mut w = game();
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 120);
        let g = put(&mut w, GRUNT, 1, (15, 10));
        chasing(&mut w, g);
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(BLOODLUST, Some(g), (15, 10)));
        assert_eq!(at(&w, g).bloodlust, 1);
        // A real timer: it runs out in this cycle's timers pass.
        w.timers_pass();
        assert_eq!(at(&w, g).bloodlust, 0);
    }

    #[test]
    fn a_second_ogre_mage_in_the_same_pass_skips_the_marked_grunt() {
        let mut w = game();
        let a = caster(&mut w, OGRE_MAGE, (10, 10), 210);
        let b = caster(&mut w, OGRE_MAGE, (10, 12), 120);
        let g = put(&mut w, GRUNT, 1, (12, 11));
        chasing(&mut w, g);
        for id in [a, b] {
            let i = ix(&w, id);
            w.set_goal(i, GoalKind::Hold, None);
        }
        let before = w.rng;
        w.run_goals();
        // `b` was created second, so it has the lower slot and runs first.
        assert_eq!(at(&w, b).pending, cast(BLOODLUST, Some(g), (12, 11)));
        assert_eq!(at(&w, g).bloodlust, 1);
        // `a`'s bloodlust test fails on the grunt (timer not 0); with 210 mana it goes on to the runes try's draws.
        assert_eq!(at(&w, a).pending, None);
        assert_ne!(w.rng, before);
    }

    #[test]
    fn runes_fail_on_an_allied_slot_after_one_draw() {
        let mut w = small(8);
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 210);
        let f = put(&mut w, FOOTMAN, 1, (40, 40));
        w.rng = seed_for(8, &[slot(&w, f)]);
        assert_eq!(cast_step(&mut w, c), (false, 1));
        assert_eq!(at(&w, c).pending, None);
    }

    #[test]
    fn an_invisible_ogre_mage_tries_runes_twice() {
        let mut w = small(8);
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 210);
        let f = put(&mut w, FOOTMAN, 1, (40, 40));
        let g = put(&mut w, GRUNT, 1, (12, 10));
        chasing(&mut w, g);
        um(&mut w, c).invisibility = 100;
        w.rng = seed_for(8, &[slot(&w, f), slot(&w, f)]);
        // Try 1 (1 draw), try 2 (invisible: fails), try 3 (1 draw).
        assert_eq!(cast_step(&mut w, c), (false, 2));
        assert_eq!(at(&w, g).bloodlust, 0);
    }

    #[test]
    fn runes_cast_at_the_tenth_slot_drawn() {
        let mut w = small(4);
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 210);
        let p = put(&mut w, PEASANT, 0, (30, 30));
        let q = put(&mut w, PEASANT, 0, (33, 31));
        let (sp, sq) = (slot(&w, p), slot(&w, q));
        w.rng = seed_where(4, 10, |d| d.iter().all(|&s| s == sp || s == sq) && d[9] == sq);
        assert_eq!(cast_step(&mut w, c), (true, 10));
        assert_eq!(at(&w, c).pending, cast(RUNES, None, (33, 31)));
    }

    #[test]
    fn eye_of_kilrogg_at_exactly_255_keeps_the_old_target() {
        let mut w = game();
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 255);
        w.players[1].spells_known = spell_bit(EYE_OF_KILROGG);
        um(&mut w, c).order = Order::Move {
            goal: (30, 20),
            target: None,
        };
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(EYE_OF_KILROGG, None, (30, 20)));
        let e = put(&mut w, GRUNT, 0, (13, 14));
        um(&mut w, c).order = Order::Chase { target: e };
        assert!(w.caster_step(ix(&w, c)));
        assert_eq!(at(&w, c).pending, cast(EYE_OF_KILROGG, Some(e), (13, 14)));
        // No top-up from the all-spells cheat, and 254 mana isn't enough.
        um(&mut w, c).pending = None;
        um(&mut w, c).mana = 254;
        w.rules.all_spells_cheat = true;
        assert_eq!(cast_step(&mut w, c), (false, 0));
        assert_eq!(at(&w, c).mana, 254);
    }

    // ----- paladin -------------------------------------------------------------------------------------------

    #[test]
    fn healing_takes_the_first_tile_of_the_scan() {
        let mut w = game();
        let c = caster(&mut w, PALADIN, (20, 20), 30);
        let near = put(&mut w, FOOTMAN, 1, (20, 19));
        let far = put(&mut w, FOOTMAN, 1, (5, 5));
        um(&mut w, near).hp = 50;
        um(&mut w, far).hp = 40;
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(HEALING, Some(far), (5, 5)));
        assert_eq!(at(&w, far).marks, mark::HEALING);
        assert_eq!(at(&w, near).marks, 0);
        // The mark keeps a second paladin off it; the timers pass clears it.
        let d = caster(&mut w, PALADIN, (20, 22), 30);
        assert!(w.caster_step(ix(&w, d)));
        assert_eq!(at(&w, d).pending, cast(HEALING, Some(near), (20, 19)));
        w.timers_pass();
        assert_eq!((at(&w, far).marks, at(&w, near).marks), (0, 0));
    }

    #[test]
    fn the_nearby_box_is_31_tiles_wide_and_skips_air_used_and_the_caster() {
        let mut w = game();
        let c = caster(&mut w, PALADIN, (20, 20), 30);
        um(&mut w, c).hp = 10;
        let out = put(&mut w, FOOTMAN, 1, (36, 20));
        let air = put(&mut w, GRYPHON, 1, (24, 24));
        let inside = put(&mut w, FOOTMAN, 1, (21, 21));
        for id in [out, air, inside] {
            um(&mut w, id).hp = 5;
        }
        um(&mut w, inside).inside = true;
        assert_eq!(cast_step(&mut w, c), (false, 0));
        let edge = put(&mut w, FOOTMAN, 1, (35, 35));
        um(&mut w, edge).hp = 5;
        assert!(w.caster_step(ix(&w, c)));
        assert_eq!(at(&w, c).pending, cast(HEALING, Some(edge), (35, 35)));
    }

    #[test]
    fn exorcism_at_an_unseen_undead_enemy_drops_the_target_unit() {
        let mut w = game();
        let c = caster(&mut w, PALADIN, (20, 20), 30);
        let s = put(&mut w, SKELETON, 0, (25, 22));
        // Unseen units count: no fog or detection test.
        um(&mut w, s).hidden_from = 0xff;
        um(&mut w, s).invisibility = 50;
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(EXORCISM, None, (25, 22)));
        assert_eq!(at(&w, s).marks, mark::EXORCISM);
    }

    #[test]
    fn holy_vision_fails_on_a_used_slot_after_four_draws() {
        let mut w = small(8);
        let c = caster(&mut w, PALADIN, (20, 20), 255);
        let e: Vec<UnitId> = (0..4).map(|k| put(&mut w, FOOTMAN, 0, (40 + k, 40))).collect();
        um(&mut w, e[3]).inside = true;
        let s: Vec<u16> = e.iter().map(|&id| slot(&w, id)).collect();
        w.rng = seed_for(8, &s);
        assert_eq!(cast_step(&mut w, c), (false, 4));
    }

    #[test]
    fn holy_vision_can_pass_ten_free_slots_and_marks_the_last() {
        let mut w = small(8);
        let c = caster(&mut w, PALADIN, (20, 20), 255);
        w.stale.insert(
            2,
            Stale {
                kind: FARM,
                tile: (9, 8),
                marks: 0,
            },
        );
        w.rng = seed_where(8, 10, |d| d.iter().all(|&s| s != 7) && d[9] == 2);
        assert_eq!(cast_step(&mut w, c), (true, 10));
        assert_eq!(at(&w, c).pending, cast(HOLY_VISION, None, (9, 8)));
        assert_eq!(w.stale[&2].marks, mark::HOLY_VISION);
        // The timers pass doesn't visit a free slot, so the mark stays and slot 2 now fails the test.
        w.timers_pass();
        assert_eq!(w.stale[&2].marks, mark::HOLY_VISION);
        w.rng = seed_for(8, &[2]);
        assert_eq!(cast_step(&mut w, c), (false, 1));
        // Exactly 255 is needed.
        um(&mut w, c).mana = 254;
        assert_eq!(cast_step(&mut w, c), (false, 0));
    }

    // ----- mage ----------------------------------------------------------------------------------------------

    /// A mage at `mage` with `hp` that knows everything but polymorph, and an enemy grunt at (20, 10).
    fn fireball_case(mage: (i32, i32), hp: u16) -> (World, UnitId) {
        let mut w = game();
        w.players[1].spells_known = !spell_bit(POLYMORPH);
        let c = caster(&mut w, MAGE, mage, 120);
        um(&mut w, c).hp = hp;
        put(&mut w, GRUNT, 0, (20, 10));
        (w, c)
    }

    #[test]
    fn fireball_vectors() {
        let (mut w, c) = fireball_case((10, 10), 60);
        put(&mut w, FOOTMAN, 1, (22, 10));
        assert_eq!(
            cast_step(&mut w, c),
            (false, 0),
            "an own footman 2 tiles from the grunt"
        );
        let (mut w, c) = fireball_case((10, 10), 29);
        put(&mut w, FOOTMAN, 1, (22, 10));
        assert_eq!(cast_step(&mut w, c), (true, 0), "29 < 30");
        assert_eq!(at(&w, c).pending, cast(FIREBALL, None, (20, 10)));
        let (mut w, c) = fireball_case((10, 10), 30);
        put(&mut w, FOOTMAN, 1, (22, 10));
        assert!(!cast_step(&mut w, c).0, "30 isn't below 30");
        let (mut w, c) = fireball_case((18, 10), 60);
        assert!(!cast_step(&mut w, c).0, "the mage itself is company");
        let (mut w, c) = fireball_case((14, 10), 60);
        put(&mut w, CRITTER, slots::NEUTRAL, (20, 12));
        assert!(!cast_step(&mut w, c).0, "a neutral critter is company");
        let (mut w, c) = fireball_case((14, 10), 60);
        assert!(cast_step(&mut w, c).0, "a lone grunt");
    }

    #[test]
    fn invisibility_marks_the_unholy_armour_timer() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 200);
        um(&mut w, c).goal = Goal {
            kind: GoalKind::Attack,
            ..Default::default()
        };
        let k = put(&mut w, KNIGHT, 1, (12, 12));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(INVISIBILITY, Some(k), (12, 12)));
        assert_eq!((at(&w, k).unholy_armor, at(&w, k).invisibility), (1, 0));
        // Without the attack goal nothing is chosen.
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 200);
        put(&mut w, KNIGHT, 1, (12, 12));
        assert_eq!(cast_step(&mut w, c), (false, 0));
    }

    #[test]
    fn an_invisibility_mark_kills_a_demolition_squad() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 200);
        um(&mut w, c).goal.kind = GoalKind::Attack;
        let d = put(&mut w, DEMOLITION, 1, (12, 12));
        assert!(w.caster_step(ix(&w, c)));
        assert_eq!(at(&w, c).pending, cast(INVISIBILITY, Some(d), (12, 12)));
        w.timers_pass();
        assert!(at(&w, d).dying);
    }

    #[test]
    fn blizzard_tests_only_the_nearest_enemy() {
        // The nearest enemy is a moving grunt and a farm is further away: try 6 tests only the grunt; slow follows.
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 80);
        let g = put(&mut w, GRUNT, 0, (14, 10));
        um(&mut w, g).order = Order::Move {
            goal: (0, 0),
            target: None,
        };
        put(&mut w, FARM, 0, (40, 40));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(SLOW, Some(g), (14, 10)));
        assert_eq!(at(&w, g).haste, -1);
    }

    #[test]
    fn blizzard_at_a_lone_building() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 80);
        put(&mut w, FARM, 0, (14, 10));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(BLIZZARD, None, (14, 10)));
        // Below 75, try 6 is skipped; nothing else fits a farm.
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 70);
        put(&mut w, FARM, 0, (14, 10));
        assert_eq!(cast_step(&mut w, c), (false, 0));
        // An own unit within 3 of its top-left tile protects it.
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 80);
        put(&mut w, FARM, 0, (14, 10));
        put(&mut w, FOOTMAN, 1, (17, 13));
        assert_eq!(cast_step(&mut w, c), (false, 0));
    }

    #[test]
    fn an_invisible_mage_blizzards_a_lone_worker() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 75);
        um(&mut w, c).invisibility = 9;
        put(&mut w, PEASANT, 0, (40, 10));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(BLIZZARD, None, (40, 10)));
    }

    #[test]
    fn an_invisible_mages_polymorph_fails_on_a_non_caster() {
        let mut w = small(8);
        let c = caster(&mut w, MAGE, (10, 10), 200);
        um(&mut w, c).invisibility = 9;
        let f = put(&mut w, FOOTMAN, 0, (40, 40));
        w.rng = seed_for(8, &[slot(&w, f)]);
        assert_eq!(cast_step(&mut w, c), (false, 1));
    }

    #[test]
    fn polymorph_from_the_random_search_has_no_target_and_fails_in_the_order() {
        let mut w = small(2);
        let c = caster(&mut w, MAGE, (10, 10), 200);
        um(&mut w, c).invisibility = 9;
        let e = put(&mut w, MAGE, 0, (40, 40));
        let s = slot(&w, e);
        w.rng = seed_where(2, 10, |d| d.iter().all(|&x| x == s));
        assert_eq!(cast_step(&mut w, c), (true, 10));
        assert_eq!(at(&w, c).pending, cast(POLYMORPH, None, (40, 40)));
        assert_eq!(at(&w, e).marks, 0, "the random polymorph sets no mark");
        run(&mut w, 30);
        let u = at(&w, c);
        assert_eq!((u.order, u.mana), (Order::Idle, 200));
        assert!(!at(&w, e).dying);
    }

    #[test]
    fn polymorph_nearby_marks_its_target() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 200);
        let g = put(&mut w, GRUNT, 0, (12, 12));
        assert!(w.caster_step(ix(&w, c)));
        assert_eq!(at(&w, c).pending, cast(POLYMORPH, Some(g), (12, 12)));
        assert_eq!(at(&w, g).marks, mark::POLYMORPH);
    }

    #[test]
    fn flame_shield_rules() {
        // An own melee unit in combat with nobody of ours within 2.
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 80);
        let k = put(&mut w, KNIGHT, 1, (15, 10));
        chasing(&mut w, k);
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(FLAME_SHIELD, Some(k), (15, 10)));
        assert_eq!(at(&w, k).flame_shield, 1);
        // An enemy needs company of its own within 1, and none of ours within 2.
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 80);
        let g = put(&mut w, GRUNT, 0, (15, 10));
        put(&mut w, GRUNT, 0, (16, 11));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(FLAME_SHIELD, Some(g), (15, 10)));
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 80);
        put(&mut w, GRUNT, 0, (15, 10));
        put(&mut w, GRUNT, 0, (16, 11));
        put(&mut w, FOOTMAN, 1, (17, 12));
        assert_eq!(
            cast_step(&mut w, c),
            (false, 0),
            "an own footman within 2 of both grunts"
        );
    }

    #[test]
    fn the_command_is_refused_but_the_try_still_acts() {
        let mut w = game();
        let c = caster(&mut w, PALADIN, (20, 20), 30);
        let f = put(&mut w, FOOTMAN, 1, (21, 21));
        um(&mut w, f).hp = 1;
        um(&mut w, c).order = Order::InsideSite {
            building: Some(UnitId(999)),
        };
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, None);
        assert_eq!(at(&w, f).marks, mark::HEALING);
    }

    // ----- retreat -------------------------------------------------------------------------------------------

    #[test]
    fn retreat_attacks_the_recorded_attacker_without_an_alive_test() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 40);
        let f = put(&mut w, FOOTMAN, 0, (12, 10));
        um(&mut w, c).attacked_by = Some(f);
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, Some(Order::Chase { target: f }));
        // An attacker no longer in play: the step still acts, but the command fails its target check and leaves a
        // pending idle (orders spec, "Commands forget a stale attacker", round-47 errata).
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 40);
        um(&mut w, c).attacked_by = Some(UnitId(777));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, Some(Order::Idle));
    }

    #[test]
    fn retreat_holds_at_the_lowest_slot_tower_whatever_its_state() {
        let mut w = game();
        let c = caster(&mut w, MAGE, (10, 10), 40);
        put(&mut w, MAGE_TOWER, 1, (40, 40));
        let low = put(&mut w, MAGE_TOWER, 1, (30, 20));
        um(&mut w, low).dying = true;
        assert_eq!(cast_step(&mut w, c), (true, 0));
        let u = at(&w, c);
        assert_eq!(u.goal.kind, GoalKind::Hold);
        let goal = u.goal.tile.unwrap();
        assert!((goal.0 - 30).abs() <= 2 && (goal.1 - 20).abs() <= 2, "{goal:?}");
        assert!(u.pending.is_some());
    }

    #[test]
    fn no_temple_no_retreat_and_the_goal_runs() {
        let mut w = game();
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 40);
        let i = ix(&w, c);
        w.set_goal(i, GoalKind::Hold, Some((40, 40)));
        assert_eq!(cast_step(&mut w, c), (false, 0));
        // In pass B the hold goal runs: past its leash, the knight is sent back to its post.
        w.run_goals();
        assert!(at(&w, c).pending.is_some());
        // Either building type will do for either caster.
        put(&mut w, MAGE_TOWER, 1, (30, 30));
        um(&mut w, c).pending = None;
        assert!(w.caster_step(ix(&w, c)));
        // At 50 mana there is no retreat.
        um(&mut w, c).mana = 50;
        assert_eq!(cast_step(&mut w, c), (false, 0));
    }

    #[test]
    fn a_caster_that_acts_skips_its_goal() {
        let mut w = game();
        let c = caster(&mut w, OGRE_MAGE, (10, 10), 120);
        let g = put(&mut w, GRUNT, 1, (12, 10));
        chasing(&mut w, g);
        let i = ix(&w, c);
        w.set_goal(i, GoalKind::Hold, Some((40, 40)));
        w.run_goals();
        assert_eq!(at(&w, c).pending, cast(BLOODLUST, Some(g), (12, 10)));
    }

    // ----- death knight --------------------------------------------------------------------------------------

    #[test]
    fn unholy_armour_at_half_hp() {
        let mut w = game();
        w.players[1].spells_known = spell_bit(UNHOLY_ARMOUR);
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 120);
        let g = put(&mut w, GRUNT, 1, (12, 10));
        chasing(&mut w, g);
        um(&mut w, g).hp = 31;
        assert_eq!(cast_step(&mut w, c), (false, 0), "31 > 30");
        um(&mut w, g).hp = 30;
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(UNHOLY_ARMOUR, Some(g), (12, 10)));
        assert_eq!(at(&w, g).unholy_armor, 1);
    }

    #[test]
    fn death_coil_wants_a_crowd_first_then_anyone() {
        // A lone grunt: try 6 rejects it, try 7 takes it.
        let mut w = game();
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 120);
        let g = put(&mut w, GRUNT, 0, (14, 10));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(DEATH_COIL, None, (14, 10)));
        assert_eq!(at(&w, g).marks, mark::DEATH_COIL);
        // Invisible, try 7 is out, so the lone grunt is left; one with company is taken by try 6.
        let mut w = game();
        w.players[1].spells_known = spell_bit(DEATH_COIL);
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 120);
        um(&mut w, c).invisibility = 9;
        put(&mut w, GRUNT, 0, (14, 10));
        assert_eq!(cast_step(&mut w, c), (false, 0));
        put(&mut w, GRUNT, 0, (14, 11));
        assert!(w.caster_step(ix(&w, c)));
        assert_eq!(at(&w, c).pending, cast(DEATH_COIL, None, (14, 10)));
    }

    #[test]
    fn whirlwind_at_the_nearest_enemy_marks_it() {
        let mut w = game();
        w.players[1].spells_known = spell_bit(WHIRLWIND);
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 120);
        let g = put(&mut w, GRUNT, 0, (16, 10));
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(WHIRLWIND, None, (16, 10)));
        assert_eq!(at(&w, g).marks, mark::WHIRLWIND);
        // Within 4 of the target, the caster is company.
        let mut w = game();
        w.players[1].spells_known = spell_bit(WHIRLWIND);
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 120);
        put(&mut w, GRUNT, 0, (14, 10));
        assert_eq!(cast_step(&mut w, c), (false, 0));
    }

    #[test]
    fn haste_on_an_own_unit_in_combat() {
        let mut w = game();
        w.players[1].spells_known = spell_bit(HASTE);
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 60);
        let g = put(&mut w, GRUNT, 1, (12, 10));
        chasing(&mut w, g);
        assert_eq!(cast_step(&mut w, c), (true, 0));
        assert_eq!(at(&w, c).pending, cast(HASTE, Some(g), (12, 10)));
        assert_eq!(at(&w, g).haste, 1);
    }

    #[test]
    fn raise_dead_never_finds_a_body() {
        let mut w = game();
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 60);
        let fs: Vec<UnitId> = (0..4).map(|k| put(&mut w, FOOTMAN, 0, (11 + k, 11))).collect();
        for &f in &fs {
            let i = ix(&w, f);
            w.kill(i);
        }
        run(&mut w, 15);
        assert!(fs.iter().all(|&f| at(&w, f).remains && at(&w, f).kind == DEAD_BODY));
        um(&mut w, c).mana = 60;
        let g = put(&mut w, GRUNT, 1, (9, 9));
        chasing(&mut w, g);
        assert!(w.caster_step(ix(&w, c)));
        assert_eq!(at(&w, c).pending, cast(HASTE, Some(g), (9, 9)));
        assert!(fs.iter().all(|&f| at(&w, f).marks == 0));
    }

    #[test]
    fn an_invisible_death_knights_whirlwind_reads_free_slots() {
        let mut w = small(4);
        w.players[1].spells_known = spell_bit(WHIRLWIND);
        let c = caster(&mut w, DEATH_KNIGHT, (10, 10), 120);
        um(&mut w, c).invisibility = 9;
        // Slot 1 last held a peasant at (40, 40); slot 2 a gold store at (50, 50).
        w.stats.0[FARM as usize].flags |= flags::GOLD_STORAGE;
        for (s, kind, tile) in [(1, PEASANT, (40, 40)), (2, FARM, (50, 50))] {
            w.stale.insert(s, Stale { kind, tile, marks: 0 });
        }
        w.rng = seed_where(4, 10, |d| d.iter().all(|&s| s == 1 || s == 2) && d[9] == 2);
        assert_eq!(cast_step(&mut w, c), (true, 10));
        assert_eq!(at(&w, c).pending, cast(WHIRLWIND, None, (50, 50)));
        // A never-used slot (type 0, a footman) is no worker: the try ends at once.
        um(&mut w, c).pending = None;
        w.rng = seed_for(4, &[0]);
        assert_eq!(cast_step(&mut w, c), (false, 1));
    }

    // ----- released slots ------------------------------------------------------------------------------------

    #[test]
    fn a_released_slot_keeps_type_tile_and_marks_until_reused() {
        let mut w = small(8);
        let m = put(&mut w, MAGE, 0, (5, 5));
        let s = slot(&w, m);
        um(&mut w, m).marks = mark::POLYMORPH;
        let i = ix(&w, m);
        w.kill(i);
        for _ in 0..1000 {
            if w.unit(m).is_none() {
                break;
            }
            w.step(&[]);
        }
        assert!(w.unit(m).is_none());
        let want = Stale {
            kind: MAGE,
            tile: (5, 5),
            marks: mark::POLYMORPH,
        };
        assert_eq!(w.stale.get(&s), Some(&want));
        let t = w.tgt_slot(s);
        assert_eq!((t.owner, t.kind, t.used, t.live), (slots::FREE, MAGE, false, None));
        // A never-used slot: all zero, owner 14.
        let t = w.tgt_slot(0);
        assert_eq!((t.owner, t.kind, t.tile, t.marks), (slots::FREE, 0, (0, 0), 0));
        // Reuse clears it.
        let n = put(&mut w, FOOTMAN, 0, (6, 6));
        assert_eq!(slot(&w, n), s);
        assert!(!w.stale.contains_key(&s));
    }
}
