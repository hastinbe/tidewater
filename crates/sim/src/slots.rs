//! Unit slot allocation. Spec: `docs/specs/core/unit-slots.md`, "Owner lists", "Creating a unit", "Death, corpses and
//! release" and "Changing owner".

use std::collections::VecDeque;

/// Unit table sizes.
pub const CLASSIC_SLOTS: usize = 600;
pub const ENHANCED_SLOTS: usize = 1600;
/// Effect table sizes.
pub const CLASSIC_EFFECTS: usize = 200;
pub const ENHANCED_EFFECTS: usize = 400;
/// Effect slots taken by the permanent entries created at map start.
pub const PERMANENT_EFFECTS: usize = 3;

pub const FREE: u8 = 14;
pub const NEUTRAL: u8 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotError {
    /// The owner is at the per-player cap, or no slot is free ("too many units").
    TooMany,
}

/// The owner lists, the free list among them. Each list is ordered most recently added first.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Slots {
    n: usize,
    lists: [VecDeque<u16>; 16],
}

impl Slots {
    /// All `n` slots free, added in slot order so the free list starts with slot `n − 1`.
    pub fn new(n: usize) -> Self {
        let mut lists: [VecDeque<u16>; 16] = Default::default();
        for s in 0..n as u16 {
            lists[FREE as usize].push_front(s);
        }
        Self { n, lists }
    }

    /// The number of unit slots in all.
    pub fn count(&self) -> usize {
        self.n
    }

    pub fn len(&self, owner: u8) -> usize {
        self.lists[owner as usize & 15].len()
    }

    /// An owner's slots, most recently added first.
    pub fn list(&self, owner: u8) -> impl Iterator<Item = u16> + '_ {
        self.lists[owner as usize & 15].iter().copied()
    }

    /// Players (owners 0 to 7) holding at least one slot.
    pub fn players(&self) -> usize {
        self.lists[..8].iter().filter(|l| !l.is_empty()).count()
    }

    /// The per-player cap.
    pub fn cap(&self) -> usize {
        match self.players() {
            0 | 1 => self.n,
            p => self.n / p,
        }
    }

    /// Takes the front free slot for `owner`.
    pub fn take(&mut self, owner: u8) -> Result<u16, SlotError> {
        if owner != NEUTRAL && self.len(owner) >= self.cap() {
            return Err(SlotError::TooMany);
        }
        let s = self.lists[FREE as usize].pop_front().ok_or(SlotError::TooMany)?;
        self.lists[owner as usize & 15].push_front(s);
        Ok(s)
    }

    /// Returns a slot to the front of the free list.
    pub fn release(&mut self, slot: u16, owner: u8) {
        self.unlink(slot, owner);
        self.lists[FREE as usize].push_front(slot);
    }

    /// Moves a slot to the front of another owner's list, without checking the cap.
    pub fn change_owner(&mut self, slot: u16, from: u8, to: u8) {
        self.unlink(slot, from);
        self.lists[to as usize & 15].push_front(slot);
    }

    /// After a load the lists are built again by walking slots 0 to N - 1 and adding each to the front of its owner's
    /// list, so every list, the free list included, is in descending slot order whatever order it had at the save
    /// (`core/unit-slots.md`, "Saved games").
    pub fn rebuild_descending(&mut self) {
        for l in &mut self.lists {
            l.make_contiguous().sort_unstable_by(|a, b| b.cmp(a));
        }
    }

    fn unlink(&mut self, slot: u16, owner: u8) {
        let l = &mut self.lists[owner as usize & 15];
        if let Some(i) = l.iter().position(|&s| s == slot) {
            l.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_puts_every_list_in_descending_slot_order() {
        let mut s = Slots::new(CLASSIC_SLOTS);
        let (a, b, c) = (s.take(0).unwrap(), s.take(0).unwrap(), s.take(1).unwrap());
        s.release(b, 0);
        let d = s.take(0).unwrap();
        s.change_owner(a, 0, 1);
        s.release(c, 1);
        s.release(a, 1);
        s.release(d, 0);
        assert_eq!(s.list(FREE).take(3).collect::<Vec<_>>(), [d, a, c], "598, 599, 597");
        s.rebuild_descending();
        assert_eq!(s.list(FREE).take(3).collect::<Vec<_>>(), [599, 598, 597]);
        assert_eq!(s.take(0), Ok(599));
    }

    #[test]
    fn creation_takes_slots_from_the_top_and_reuses_the_last_released() {
        let mut s = Slots::new(CLASSIC_SLOTS);
        let (a, b, c) = (s.take(0).unwrap(), s.take(0).unwrap(), s.take(0).unwrap());
        assert_eq!((a, b, c), (599, 598, 597));
        s.release(b, 0);
        assert_eq!(s.take(0), Ok(598));
    }

    #[test]
    fn caps() {
        let mut s = Slots::new(CLASSIC_SLOTS);
        s.take(0).unwrap();
        assert_eq!(s.cap(), 600);
        let last = s.take(1).unwrap();
        s.take(NEUTRAL).unwrap();
        assert_eq!(s.cap(), 300);
        while s.len(0) < 300 {
            s.take(0).unwrap();
        }
        assert_eq!(s.take(0), Err(SlotError::TooMany));
        s.take(NEUTRAL).unwrap();
        s.release(last, 1);
        assert_eq!(s.cap(), 600);
        let mut e = Slots::new(ENHANCED_SLOTS);
        for p in 0..4 {
            e.take(p).unwrap();
        }
        assert_eq!(e.cap(), 400);
        let mut c = Slots::new(CLASSIC_SLOTS);
        for p in 0..7 {
            c.take(p).unwrap();
        }
        assert_eq!(c.cap(), 85);
    }

    #[test]
    fn conversion_ignores_the_cap() {
        let mut s = Slots::new(4);
        let a = s.take(0).unwrap();
        s.take(1).unwrap();
        s.take(1).unwrap();
        assert_eq!(s.take(1), Err(SlotError::TooMany));
        s.change_owner(a, 0, 1);
        assert_eq!(s.len(1), 3);
    }
}
