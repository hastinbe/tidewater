//! The simulation's random number generator. Spec: `docs/specs/core/timing-and-rng.md`, "Random number generator".

/// One shared generator; every simulation draw goes through it, in game order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rng {
    pub state: u32,
}

impl Rng {
    pub const MULTIPLIER: u32 = 22_695_477;

    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Steps the state, then returns bits 16 to 30 of it (0 to 32767).
    pub fn draw(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(1);
        (self.state >> 16) & 0x7fff
    }

    /// An integer from `lo` to `hi` inclusive, from one draw.
    pub fn ranged(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (hi - lo + 1) * self.draw() / 32768
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draws(seed: u32) -> Vec<u32> {
        let mut r = Rng::new(seed);
        (0..6).map(|_| r.draw()).collect()
    }

    #[test]
    fn spec_vectors() {
        assert_eq!(draws(0), [0, 346, 130, 10982, 1090, 11656]);
        assert_eq!(draws(1), [346, 130, 10982, 1090, 11656, 7117]);
        assert_eq!(draws(0x1234_5678), [5470, 22030, 16144, 7932, 18879, 26633]);
        assert_eq!(draws(0x68E7_7800), [28283, 3278, 20871, 28228, 18503, 1342]);
        let mut r = Rng::new(0);
        (0..6).for_each(|_| {
            r.draw();
        });
        assert_eq!(r.state, 0xAD88_9112);
    }

    #[test]
    fn ranged_vectors() {
        let mut r = Rng::new(0);
        assert_eq!((0..6).map(|_| r.ranged(0, 1)).collect::<Vec<_>>(), [0; 6]);
        let mut r = Rng::new(0);
        assert_eq!((0..6).map(|_| r.ranged(1, 4)).collect::<Vec<_>>(), [1, 1, 1, 2, 1, 2]);
    }
}
