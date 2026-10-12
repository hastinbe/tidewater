//! Damage arithmetic. Spec: `docs/specs/combat/damage.md`, "Effective attack values", "Damage roll" and "Splash on
//! impact", and the per-kind table of "Missile flight".

use tidewater_formats::udta::{UnitStats, flags};

use crate::rng::Rng;

/// Unit types by PUD number that the spec names.
pub mod kind {
    pub const ARCHER: u8 = 0x08;
    pub const AXE_THROWER: u8 = 0x09;
    pub const RANGER: u8 = 0x12;
    pub const BERSERKER: u8 = 0x13;
}

/// Missile weapon values (UDTA missile byte) with special meaning.
pub mod missile {
    pub const LIGHTNING: u8 = 0x00;
    pub const GRYPHON_HAMMER: u8 = 0x01;
    pub const DRAGON_BREATH: u8 = 0x02;
    pub const BIG_CANNON: u8 = 0x07;
    pub const TOUCH_OF_DEATH: u8 = 0x0a;
    pub const CATAPULT: u8 = 0x0d;
    pub const BALLISTA: u8 = 0x0e;
    pub const ARROW: u8 = 0x0f;
    pub const AXE: u8 = 0x10;
    pub const SUBMARINE: u8 = 0x11;
    pub const TURTLE: u8 = 0x12;
    pub const SMALL_CANNON: u8 = 0x18;
    pub const DEMON_FIRE: u8 = 0x1b;
    pub const NONE: u8 = 0x1d;
}

/// The flag the spec calls bit 2 (ballista and catapult), which takes the siege bonus.
const SIEGE_FLAG: u32 = 1 << 2;

/// Whether a missile kind splashes, and its minimum flight distance in tiles.
pub fn splash(m: u8) -> Option<u8> {
    match m {
        missile::BIG_CANNON | missile::SMALL_CANNON => Some(2),
        missile::CATAPULT | missile::BALLISTA => Some(3),
        _ => None,
    }
}

/// How a missile kind flies (spec, "Missile flight", table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flight {
    /// Pixels per flight step.
    pub speed: i32,
    /// Whether every step makes one more discarded draw while the travel is still short of the aim point.
    pub extra_draw: bool,
    /// Cycles from the landing step to the impact.
    pub delay: u16,
}

/// The flight table. The gryphon hammer and dragon breath have their own rule and are not in it.
pub fn flight(m: u8) -> Flight {
    let (speed, extra_draw, delay) = match m {
        missile::LIGHTNING => (16, false, 11),
        missile::BIG_CANNON => (12, false, 1),
        missile::TOUCH_OF_DEATH => (12, false, 16),
        missile::CATAPULT => (8, true, 5),
        missile::BALLISTA => (8, false, 1),
        missile::ARROW | missile::AXE => (12, false, 1),
        missile::SUBMARINE | missile::TURTLE => (16, false, 1),
        missile::SMALL_CANNON => (16, true, 1),
        missile::DEMON_FIRE => (12, false, 5),
        // Our reading: a kind the table doesn't list ("and others") flies like an arrow.
        _ => (12, false, 1),
    };
    Flight {
        speed,
        extra_draw,
        delay,
    }
}

/// The number of flight steps for a flight length `d` at `speed`: the step on which the remaining distance goes below
/// 0 is the last, so `floor(d / speed) + 1`.
pub fn flight_steps(d: i32, speed: i32) -> i32 {
    d / speed + 1
}

/// A player's upgrade levels (0 to 2) and researched abilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Upgrades {
    pub sword: u8,
    pub arrow: u8,
    pub shield: u8,
    pub ship_cannon: u8,
    pub ship_armor: u8,
    pub siege: u8,
    pub marksmanship: bool,
    pub longbow: bool,
    pub lighter_axes: bool,
    /// Ranger and berserker scouting (sight 9; vision spec, "Sight of a unit").
    pub ranger_scouting: bool,
    pub berserker_scouting: bool,
}

fn is_missile_trooper(kind: u8) -> bool {
    matches!(kind, kind::ARCHER | kind::AXE_THROWER | kind::RANGER | kind::BERSERKER)
}

/// An attacker's (basic, piercing) damage after upgrades and bloodlust.
pub fn attack(kind: u8, s: &UnitStats, up: &Upgrades, bloodlust: bool) -> (u32, u32) {
    let mut basic = s.basic_damage as u32;
    let mut pierce = s.piercing_damage as u32;
    if s.weapons_upgradable {
        basic += if s.flags & flags::SEA != 0 {
            up.ship_cannon as u32 * 5
        } else if s.flags & SIEGE_FLAG != 0 {
            up.siege as u32 * 15
        } else if is_missile_trooper(kind) {
            0
        } else {
            up.sword as u32 * 2
        };
    }
    if is_missile_trooper(kind) {
        pierce += up.arrow as u32;
    }
    if kind == kind::RANGER && up.marksmanship {
        pierce += basic;
        basic = 0;
    }
    if bloodlust {
        basic *= 2;
        pierce *= 2;
    }
    (basic, pierce)
}

/// A target's armour after upgrades.
pub fn armour(s: &UnitStats, up: &Upgrades) -> u32 {
    let bonus = match (s.armor_upgradable, s.flags & flags::SEA != 0) {
        (false, _) => 0,
        (true, true) => up.ship_armor as u32 * 5,
        (true, false) => up.shield as u32 * 2,
    };
    s.armor as u32 + bonus
}

/// Attack range in tiles, with Longbow and Lighter Axes.
pub fn range(kind: u8, s: &UnitStats, up: &Upgrades) -> u32 {
    let bonus = (kind == kind::RANGER && up.longbow) || (kind == kind::BERSERKER && up.lighter_axes);
    s.attack_range as u32 + bonus as u32
}

/// The single-target damage roll. No draw is made when the total is 0.
pub fn roll(rng: &mut Rng, basic: u32, pierce: u32, armour: u32) -> u8 {
    let t = basic.saturating_sub(armour) + pierce;
    if t == 0 {
        return 0;
    }
    let h = t.div_ceil(2);
    (h + rng.draw() % (h + 1)).min(255) as u8
}

/// Splash damage to a unit `d` pixels (the larger axis offset) from the impact, or `None` if it isn't hit. Draws
/// only when the unit is hit.
pub fn splash_roll(rng: &mut Rng, stored: u8, d: i32, armour: u32) -> Option<u8> {
    let base = match d.abs() {
        0..=22 => stored as u32,
        23..=42 => stored as u32 / 4,
        _ => return None,
    };
    let v = base.checked_sub(armour).filter(|&v| v >= 1)?;
    let h = v.div_ceil(2);
    Some((h + rng.draw() % (h + 1)) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(basic: u8, pierce: u8, armor: u8) -> UnitStats {
        UnitStats {
            basic_damage: basic,
            piercing_damage: pierce,
            armor,
            weapons_upgradable: true,
            armor_upgradable: true,
            flags: flags::LAND,
            ..Default::default()
        }
    }

    /// The (lowest, highest) damage over every possible draw.
    fn spread(basic: u32, pierce: u32, armour: u32) -> (u8, u8) {
        let t = basic.saturating_sub(armour) + pierce;
        let h = t.div_ceil(2);
        (h as u8, (h + h) as u8)
    }

    #[test]
    fn footman_on_footman() {
        let f = unit(6, 3, 2);
        let none = Upgrades::default();
        let (b, p) = attack(0, &f, &none, false);
        assert_eq!(spread(b, p, armour(&f, &none)), (4, 8));
        assert_eq!(roll(&mut Rng::new(0), b, p, 2), 4);
        let mut r = Rng::new(0);
        r.draw();
        assert_eq!(roll(&mut r, b, p, 2), 5);
        let two = Upgrades {
            sword: 2,
            shield: 2,
            ..Default::default()
        };
        let (b, p) = attack(0, &f, &two, false);
        assert_eq!((b, armour(&f, &two)), (10, 6));
        assert_eq!(spread(b, p, 6), (4, 8));
    }

    #[test]
    fn missile_troopers() {
        let archer = unit(3, 6, 0);
        let (b, p) = attack(kind::ARCHER, &archer, &Upgrades::default(), false);
        assert_eq!(spread(b, p, 2), (4, 8));
        let up = Upgrades {
            arrow: 2,
            sword: 2,
            ..Default::default()
        };
        assert_eq!(attack(kind::ARCHER, &archer, &up, false), (3, 8));
        assert_eq!(spread(3, 8, 2), (5, 10));
        let mm = Upgrades {
            arrow: 0,
            marksmanship: true,
            ..Default::default()
        };
        assert_eq!(attack(kind::RANGER, &archer, &mm, false), (0, 9));
    }

    #[test]
    fn bloodlust_and_zero_totals() {
        let grunt = unit(6, 3, 2);
        assert_eq!(attack(1, &grunt, &Upgrades::default(), true), (12, 6));
        assert_eq!(spread(12, 6, 2), (8, 16));
        let mut r = Rng::new(0);
        assert_eq!(roll(&mut r, 2, 0, 2), 0);
        assert_eq!(r.state, 0, "no draw when the total is 0");
        let mut r = Rng::new(0);
        assert!((1..=2).contains(&roll(&mut r, 1, 0, 0)));
    }

    #[test]
    fn splash_vectors() {
        let mut r = Rng::new(0);
        let d = splash_roll(&mut r, 80, 10, 2).unwrap();
        assert!((39..=78).contains(&d));
        let d = splash_roll(&mut r, 80, 30, 2).unwrap();
        assert!((9..=18).contains(&d));
        let before = r.state;
        assert_eq!(splash_roll(&mut r, 80, 43, 2), None);
        assert_eq!(splash_roll(&mut r, 5, 30, 2), None);
        assert_eq!(r.state, before, "no draw for a miss");
    }

    #[test]
    fn siege_and_ships() {
        let mut cat = unit(80, 0, 0);
        cat.flags |= SIEGE_FLAG;
        assert_eq!(
            attack(
                5,
                &cat,
                &Upgrades {
                    siege: 2,
                    sword: 2,
                    ..Default::default()
                },
                false
            ),
            (110, 0)
        );
        let mut ship = unit(35, 0, 10);
        ship.flags = flags::SEA;
        let up = Upgrades {
            ship_cannon: 1,
            ship_armor: 2,
            shield: 2,
            ..Default::default()
        };
        assert_eq!(attack(0x20, &ship, &up, false), (40, 0));
        assert_eq!(armour(&ship, &up), 20);
    }
}
