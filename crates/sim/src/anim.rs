//! The animation-script interpreter. Spec: `docs/specs/core/animation-scripts.md`, "Running a script" and
//! "Instruction set".

use tidewater_formats::script::{Op, decode};

/// Pixel offsets of a 1-pixel step for each facing, north first and clockwise.
pub const FACING_VECTOR: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

/// Most instructions run in one update; a guard against scripts that jump in a loop without waiting.
const MAX_OPS: usize = 256;

/// A unit's place in its animation script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Script {
    pub pc: u16,
    pub wait: u8,
    pub anim: u8,
    pub frame: u8,
}

/// What one update did, for the order logic and the renderer to act on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    pub step_done: bool,
    pub strikes: u8,
    pub dx: i32,
    pub dy: i32,
    pub neutral: bool,
    /// A halt instruction ran.
    pub halted: bool,
}

/// Inputs that adjust waits.
#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    /// Haste/slow timer: positive is hasted, negative slowed.
    pub haste: i16,
    pub carrying: bool,
}

pub fn hasted(n: u8) -> u8 {
    (n / 2).max(1)
}

pub fn slowed(n: u8) -> u8 {
    n.wrapping_mul(2)
}

impl Script {
    /// Starts animation `anim` at script offset `pc`: the first instruction runs on the next update.
    pub fn start(&mut self, anim: u8, pc: u16) {
        self.anim = anim;
        self.pc = pc;
        self.wait = 1;
    }

    /// Runs one cycle of the script.
    pub fn update(&mut self, code: &[u8], facing: u8, m: Modifiers) -> Effects {
        let mut fx = Effects::default();
        self.wait = self.wait.wrapping_sub(1);
        if self.wait > 0 {
            return fx;
        }
        let (vx, vy) = FACING_VECTOR[facing as usize & 7];
        let adjust = |n: u8, haste: bool, slow: bool| {
            if haste && m.haste > 0 {
                hasted(n)
            } else if slow && m.haste < 0 {
                slowed(n)
            } else {
                n
            }
        };
        for _ in 0..MAX_OPS {
            let Some(op) = decode(code, self.pc as usize) else {
                // Off the end of the data: treat as a halt.
                return fx;
            };
            self.pc = self.pc.wrapping_add(op.size() as u16);
            match op {
                Op::StepDone => {
                    self.wait = 1;
                    fx.step_done = true;
                }
                Op::Wait(n) => self.wait = n,
                Op::Halt => {
                    self.wait = 0;
                    fx.halted = true;
                }
                Op::Jump(to) => self.pc = to,
                Op::Frame(f) => self.frame = f,
                Op::Move(d) | Op::MoveFrame(d, _) => {
                    fx.dx += vx * d as i32;
                    fx.dy += vy * d as i32;
                }
                Op::Move2(d) | Op::Move2Frame(d, _) => {
                    fx.dx += 2 * vx * d as i32;
                    fx.dy += 2 * vy * d as i32;
                }
                Op::WaitHasteSlow(n) => self.wait = adjust(n, true, true),
                Op::WaitHaste(n) => self.wait = adjust(n, true, false),
                Op::WaitSlow(n) => self.wait = adjust(n, false, true),
                Op::WaitCarry(n) => self.wait = adjust(n.wrapping_add(m.carrying as u8), true, true),
                Op::Strike => fx.strikes += 1,
                Op::Neutral => fx.neutral = true,
                Op::Sound | Op::Nop(_) => {}
            }
            if let Op::MoveFrame(_, f) | Op::Move2Frame(_, f) = op {
                self.frame = f;
            }
            if op.sets_wait() {
                return fx;
            }
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic footman-style move loop: step done, then 12 moves with the spec's per-cycle steps and waits.
    fn move_loop() -> Vec<u8> {
        let steps = [3, 3, 3, 2, 3, 2, 3, 3, 3, 2, 3, 2];
        let waits = [2, 1, 2, 1, 1, 1, 2, 1, 2, 1, 1, 1];
        let mut code = vec![0];
        for (i, (s, w)) in steps.iter().zip(waits).enumerate() {
            code.extend([6, *s, i as u8, 7, w]);
        }
        // The last wait is the step-done wait, so replace it with the jump back.
        code.truncate(code.len() - 2);
        code.extend([3, 0, 0]);
        code
    }

    #[test]
    fn footman_move_vector() {
        let code = move_loop();
        let mut s = Script::default();
        s.start(3, 0);
        let mut x = 100;
        let mut done_at = vec![];
        for cycle in 1..=33 {
            let fx = s.update(&code, 2, Modifiers::default());
            x += fx.dx;
            if fx.step_done {
                done_at.push(cycle);
            }
            if cycle == 2 {
                assert_eq!(x, 103);
            }
            if cycle == 17 {
                assert_eq!(x, 132);
            }
        }
        assert_eq!(done_at, [1, 17, 33]);
    }

    #[test]
    fn wait_adjustments() {
        assert_eq!(hasted(3), 1);
        assert_eq!(hasted(1), 1);
        assert_eq!(slowed(100), 200);
        assert_eq!(slowed(130), 4);
        let mut s = Script::default();
        s.start(3, 0);
        s.update(
            &[12, 2, 2],
            0,
            Modifiers {
                haste: 5,
                carrying: true,
            },
        );
        assert_eq!(s.wait, 1);
    }

    #[test]
    fn halt_sleeps_256_cycles() {
        let code = [2, 11];
        let mut s = Script::default();
        s.start(0, 0);
        s.update(&code, 0, Modifiers::default());
        let strikes: Vec<_> = (1..=256)
            .map(|_| s.update(&code, 0, Modifiers::default()).strikes)
            .collect();
        assert_eq!(strikes.iter().position(|&n| n > 0), Some(255));
    }

    #[test]
    fn nop_is_one_byte_and_moves_use_facing() {
        let mut s = Script::default();
        s.start(3, 0);
        let fx = s.update(&[20, 13, 2, 5, 0xff, 1, 1], 5, Modifiers::default());
        assert_eq!((fx.dx, fx.dy), (-3, 3));
        assert_eq!(s.wait, 1);
    }
}
