//! The idle look-around (reviewed spec `docs/specs/orders/orders.md`, "Idle look-around"): each run of a unit's idle
//! logic first turns it, makes a critter wander, or bobs a ship or flier, and those draws decide the generator's
//! sequence in every game (`docs/specs/core/timing-and-rng.md`, "Draw sites", rows "Idle look-around" and "Bob").

use tidewater_formats::udta::flags;

use crate::Command;
use crate::rng::Rng;
use crate::world::{Layer, World};

/// The critter's unit type (orders spec, "Who does what").
const CRITTER: u8 = 0x39;
/// Turn draws (`r AND 255`) from which a critter wanders, up to [`WANDER_TO`].
const WANDER_FROM: u32 = 4;
const WANDER_TO: u32 = 50;

/// A ship's or flier's bob state ("Bob"): both 0 when the unit is created.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bob {
    /// Runs of the idle logic left before the next bob draw.
    pub countdown: u8,
    /// The bob value b: the unit is drawn b pixels higher (`docs/specs/ui/map-drawing.md`, "Bob offset").
    pub value: u8,
}

impl Bob {
    /// One run of the idle logic: count down, or at 0 draw the new countdown and flip the value. Returns whether
    /// it drew.
    pub fn run(&mut self, rng: &mut Rng) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            return false;
        }
        let r = rng.draw();
        self.countdown = (r & 7) as u8 + 8;
        self.value = u8::from(self.value == 0);
        true
    }
}

/// Which rule a unit follows on each run of its idle logic ("Who does what").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Flag bit 2 (explode when killed: the catapults): no look-around.
    Nothing,
    /// Sea types and units on the air layer.
    Bob,
    /// Every other unit; the critter may wander.
    Turn,
}

/// The rule for a unit of type flags `f` on `layer`.
pub fn rule(f: u32, layer: Layer) -> Rule {
    if f & flags::EXPLODE_WHEN_KILLED != 0 {
        Rule::Nothing
    } else if f & flags::SEA != 0 || layer == Layer::Air {
        Rule::Bob
    } else {
        Rule::Turn
    }
}

/// What a turn draw does ("Turn").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    CounterClockwise,
    Clockwise,
    Wander,
    Nothing,
}

/// The effect of turn draw `r` for a unit that is (or isn't) the critter.
pub fn turn(r: u32, critter: bool) -> Turn {
    match r & 255 {
        0 => Turn::CounterClockwise,
        1..=3 => Turn::Clockwise,
        WANDER_FROM..=WANDER_TO if critter => Turn::Wander,
        _ => Turn::Nothing,
    }
}

/// The facing after `t` (facing order of the animation scripts spec, clockwise from north).
pub fn turned(facing: u8, t: Turn) -> u8 {
    match t {
        Turn::CounterClockwise => (facing + 7) % 8,
        Turn::Clockwise => (facing + 1) % 8,
        Turn::Wander | Turn::Nothing => facing,
    }
}

/// The wander target for draw `r2` from `tile` on a `size` map, or `None` when the clamped target is the critter's
/// own tile ("Wander").
pub fn wander_target(tile: (i32, i32), r2: u32, size: (i32, i32)) -> Option<(i32, i32)> {
    const D: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
    let (dx, dy) = D[(r2 & 7) as usize];
    let to = ((tile.0 + dx).clamp(0, size.0 - 1), (tile.1 + dy).clamp(0, size.1 - 1));
    (to != tile).then_some(to)
}

impl World {
    /// The look-around, the first thing each run of the idle logic does. Returns true if the unit's order changed
    /// (a critter that wandered), so the rest of the idle logic doesn't run on the old order.
    pub(crate) fn look_around(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        match rule(self.stats_of(u.kind).flags, u.layer) {
            Rule::Nothing => false,
            Rule::Bob => {
                self.units[i].bob.run(&mut self.rng);
                false
            }
            Rule::Turn => {
                let critter = u.kind == CRITTER;
                let t = turn(self.rng.draw(), critter);
                let u = &mut self.units[i];
                u.facing = turned(u.facing, t);
                t == Turn::Wander && self.wander(i)
            }
        }
    }

    /// "Wander": a second draw picks a neighbouring tile, and the critter gets a smart move there. In the traces its
    /// order is already the move, with the take-over wait, at the end of the same update.
    fn wander(&mut self, i: usize) -> bool {
        let r2 = self.rng.draw();
        let size = (self.terrain.width as i32, self.terrain.height as i32);
        let u = &self.units[i];
        let Some(tile) = wander_target(u.tile, r2, size) else {
            return false;
        };
        let c = Command::Smart {
            player: u.owner,
            units: vec![u.id],
            target: None,
            tile,
        };
        let Some(o) = self.command_order(i, &c) else {
            return false;
        };
        self.command_at_once(i, o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Order;
    use crate::world::tests::{field, run};

    /// Draws `n` values from `state`, returning them and the state after.
    fn draws(state: u32, n: usize) -> (Vec<u32>, u32) {
        let mut r = Rng::new(state);
        let v = (0..n).map(|_| r.draw()).collect();
        (v, r.state)
    }

    /// Orders spec, "Human01, the first draws": each row's r, its turn and the state after the record.
    #[test]
    fn human01_first_draws() {
        // (draws in the record, facings before, state after)
        let rows: [(&[u32], &[u8], u32); 7] = [
            (&[9770], &[5], 0x262A_FAC9),
            (&[29433], &[2], 0xF2F9_299E),
            (&[30671, 27742], &[2, 5], 0xEC5E_DCE4),
            (&[25369], &[6], 0x6319_3335),
            (&[26483], &[7], 0x6773_BFFA),
            (&[10072, 2174, 4396], &[7, 0, 5], 0x912C_27E1),
            (&[26724], &[2], 0x6864_CF96),
        ];
        let mut state = 0xF55E_08A8;
        for (rs, facings, after) in rows {
            let (got, s) = draws(state, rs.len());
            assert_eq!(got, rs, "draws from {state:#x}");
            assert_eq!(s, after);
            for (&r, &f) in rs.iter().zip(facings) {
                // None of these is a turn: the facing stays.
                assert_eq!(turn(r, false), Turn::Nothing, "{r}");
                assert_eq!(turned(f, turn(r, false)), f);
            }
            state = s;
        }
        let ands: Vec<u32> = [9770, 29433, 30671, 27742, 25369, 26483, 10072, 2174, 4396, 26724]
            .iter()
            .map(|r| r & 255)
            .collect();
        assert_eq!(ands, [42, 249, 207, 94, 25, 115, 88, 126, 44, 100]);
    }

    /// Orders spec, "Human01, the first turns".
    #[test]
    fn human01_first_turns() {
        for (r, before, after) in [
            (7682, 6, 7),
            (13824, 2, 1),
            (17152, 2, 1),
            (4352, 5, 4),
            (7682, 4, 5),
            (14083, 7, 0),
        ] {
            assert_eq!(turned(before, turn(r, false)), after, "r {r}");
        }
        for (r, and) in [(7682, 2), (13824, 0), (17152, 0), (4352, 0), (14083, 3)] {
            assert_eq!(r & 255, and);
        }
    }

    /// Orders spec, "Human04, the first wanders" (map size 96).
    #[test]
    fn human04_first_wanders() {
        for (r, r2, from, to) in [
            (23301, 23615, (59, 44), (58, 43)),
            (12067, 4352, (63, 44), (63, 43)),
            (6160, 16610, (40, 79), (41, 79)),
            (19475, 8785, (37, 19), (38, 18)),
        ] {
            assert_eq!(turn(r, true), Turn::Wander, "{r}");
            assert_eq!(turn(r, false), Turn::Nothing, "only the critter wanders");
            assert_eq!(wander_target(from, r2, (96, 96)), Some(to));
        }
    }

    #[test]
    fn wander_targets_clamp_to_the_map() {
        // d = 0 (north) from the top row, d = 7 from the top-left corner: back to the own tile.
        assert_eq!(wander_target((5, 0), 0, (96, 96)), None);
        assert_eq!(wander_target((0, 0), 7, (96, 96)), None);
        // d = 1 (north-east) from the top row: only x changes.
        assert_eq!(wander_target((5, 0), 1, (96, 96)), Some((6, 0)));
        assert_eq!(wander_target((95, 95), 3, (96, 96)), None);
    }

    #[test]
    fn turn_table() {
        assert_eq!(turn(256, false), Turn::CounterClockwise);
        assert_eq!(
            (1..=3).map(|r| turn(r, false)).collect::<Vec<_>>(),
            [Turn::Clockwise; 3]
        );
        assert_eq!(turn(4, true), Turn::Wander);
        assert_eq!(turn(50, true), Turn::Wander);
        assert_eq!(turn(51, true), Turn::Nothing);
        assert_eq!(turn(4, false), Turn::Nothing);
        assert_eq!(turned(0, Turn::CounterClockwise), 7);
        assert_eq!(turned(7, Turn::Clockwise), 0);
        // 4 runs in 256 turn a land unit, 47 in 256 wander a critter.
        assert_eq!((0..256).filter(|&r| turn(r, false) != Turn::Nothing).count(), 4);
        assert_eq!((0..256).filter(|&r| turn(r, true) == Turn::Wander).count(), 47);
    }

    /// A generator whose next draw is `r`: one step back from a state with `r` in bits 16 to 30.
    fn yielding(r: u32) -> Rng {
        // Newton's iteration for the multiplier's inverse mod 2^32 (it is odd).
        let inv = (0..6).fold(1u32, |x, _| {
            x.wrapping_mul(2u32.wrapping_sub(Rng::MULTIPLIER.wrapping_mul(x)))
        });
        let g = Rng::new((r << 16).wrapping_sub(1).wrapping_mul(inv));
        assert_eq!({ g }.draw(), r);
        g
    }

    /// Orders spec, "Human04, bob": the destroyer's draws, countdowns and the cycles of its next draws, with one run
    /// of the idle logic every 5 cycles.
    #[test]
    fn human04_bob() {
        let mut bob = Bob::default();
        let mut n = 2;
        for (r, countdown, next) in [(21216, 8, 47), (17609, 9, 97), (11139, 11, 157), (9246, 14, 232)] {
            let value = bob.value;
            assert!(bob.run(&mut yielding(r)), "draws at n = {n}");
            assert_eq!(bob.countdown, countdown);
            assert_eq!(bob.value, u8::from(value == 0));
            let mut runs = 1;
            let mut quiet = Rng::new(0);
            while bob.countdown > 0 {
                assert!(!bob.run(&mut quiet));
                runs += 1;
            }
            assert_eq!(quiet, Rng::new(0), "no draw while counting down");
            assert_eq!(n + 5 * runs, next);
            n = next;
        }
    }

    #[test]
    fn bob_value_flips_and_counts_down() {
        let mut b = Bob::default();
        let mut rng = Rng::new(0);
        assert!(b.run(&mut rng));
        assert_eq!(b.value, 1);
        assert!((8..=15).contains(&b.countdown));
        let c = b.countdown;
        for _ in 0..c {
            assert!(!b.run(&mut rng));
        }
        assert!(b.run(&mut rng));
        assert_eq!(b.value, 0);
        b.value = 2;
        b.countdown = 0;
        b.run(&mut rng);
        assert_eq!(b.value, 0, "anything but 0 becomes 0");
    }

    #[test]
    fn rules_by_type() {
        assert_eq!(
            rule(flags::LAND | flags::EXPLODE_WHEN_KILLED, Layer::Land),
            Rule::Nothing
        );
        assert_eq!(rule(flags::SEA, Layer::Sea), Rule::Bob);
        assert_eq!(rule(flags::AIR, Layer::Air), Rule::Bob);
        assert_eq!(rule(flags::LAND, Layer::Land), Rule::Turn);
        assert_eq!(rule(flags::LAND | flags::CRITTER, Layer::Land), Rule::Turn);
    }

    /// An idle unit at rest draws once every 5 cycles, at its step done.
    #[test]
    fn an_idle_unit_draws_every_five_cycles() {
        let mut w = field(8, 8);
        let f = w.spawn(0, 0, (3, 3), 0).unwrap();
        let mut at = Vec::new();
        for n in 0..40u32 {
            let before = w.rng.state;
            w.step(&[]);
            if w.rng.state != before {
                at.push(n);
            }
        }
        assert!(at.windows(2).all(|p| p[1] - p[0] == 5), "{at:?}");
        assert_eq!(w.unit(f).unwrap().order, Order::Idle);
    }

    #[test]
    fn a_critter_wanders_with_the_take_over_wait() {
        let mut w = field(8, 8);
        w.stats.0[CRITTER as usize].flags = flags::LAND | flags::CRITTER;
        let c = w.spawn(CRITTER, 15, (3, 3), 0).unwrap();
        // Run until a wander gives the move, which shows at once with a wait of 3.
        for _ in 0..2000 {
            w.step(&[]);
            let u = w.unit(c).unwrap();
            if let Order::Move { goal, .. } = u.order {
                assert_eq!(u.script.wait, 3);
                assert_eq!(u.pending, None);
                let d = (goal.0 - 3, goal.1 - 3);
                assert!(d.0.abs() <= 1 && d.1.abs() <= 1 && d != (0, 0), "{goal:?}");
                return;
            }
        }
        panic!("the critter never wandered");
    }

    /// Timing-and-rng spec, scheduler rule 1 (vector "Oracle trace Human01"): the start-up cycles are full cycles,
    /// then the counter is reset, so the first normal cycle ends at 1.
    #[test]
    fn start_up_cycles_draw_then_the_counter_resets() {
        let mut w = field(8, 8);
        w.spawn(0, 0, (3, 3), 0).unwrap();
        // The footman's creation wait is 1 + (r AND 7); force 2 so its first step done is in the second cycle.
        w.units[0].script.wait = 2;
        let s = w.rng.state;
        w.start_up();
        assert_eq!(w.tick, 0);
        let mut one = Rng::new(s);
        one.draw();
        assert_eq!(w.rng, one, "one look-around draw, in the second start-up cycle");
        w.step(&[]);
        assert_eq!(w.tick, 1);
    }

    /// Orders spec, "Orders given when a unit is created": a building is created in order 2 with its first order
    /// pending; it takes over at the first step done with a wait of 3 and makes no look-around draw.
    #[test]
    fn a_building_takes_its_first_order_with_a_wait_of_3() {
        let mut w = field(16, 16);
        w.stats.0[0x4a].size = (4, 4);
        w.stats.0[0x4a].flags = flags::BUILDING;
        let h = w.spawn(0x4a, 0, (4, 4), 0).unwrap();
        assert_eq!(w.unit(h).unwrap().pending, Some(Order::Idle));
        let s = w.rng.state;
        for _ in 0..10 {
            w.step(&[]);
            let u = w.unit(h).unwrap();
            if u.pending.is_none() {
                assert_eq!(u.script.wait, 3);
                assert_eq!(w.rng.state, s);
                return;
            }
        }
        panic!("the first order never took over");
    }

    #[test]
    fn catapults_draw_nothing_and_ships_bob() {
        let mut w = field(8, 8);
        w.stats.0[4].flags = flags::LAND | flags::EXPLODE_WHEN_KILLED;
        w.spawn(4, 0, (3, 3), 0).unwrap();
        let s = w.rng.state;
        run(&mut w, 40);
        assert_eq!(w.rng.state, s, "a catapult at rest draws nothing");
        // A ship draws at its first run, then counts down 8 to 15 runs without drawing.
        w.stats.0[0x1a].flags = flags::SEA;
        let ship = w.spawn(0x1a, 0, (6, 6), 0).unwrap();
        let mut draws = 0;
        for _ in 0..40 {
            let before = w.rng;
            w.step(&[]);
            draws += u32::from(w.rng != before);
        }
        let b = w.unit(ship).unwrap().bob;
        assert_eq!((draws, b.value), (1, 1));
        assert!((1..=15).contains(&b.countdown));
    }
}
