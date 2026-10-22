//! Minimap pings (option `minimap-ping`, `docs/design/extras.md`): Alt-click on the minimap in a network game marks
//! that place on the minimaps of your allies, as a marker that blinks for a few seconds. A ping is a network message
//! outside the turns and the state hash (`net::proto::Msg::Ping`), like chat, so it never touches the simulation and
//! is a no-op in a game with nobody else to tell.

/// How long a ping stays, in game cycles.
pub const LIFE: u32 = 125;
/// A marker is on for this many cycles, then off for as many.
pub const BLINK: u32 = 8;
/// The fewest cycles between two pings of the local player, so a held key cannot flood the network.
pub const GAP: u32 = 12;
/// The most pings kept at once; the oldest goes first.
pub const MAX: usize = 8;

/// One ping: who sent it, the tile, and the cycle it arrived in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ping {
    pub slot: u8,
    pub tile: (i32, i32),
    pub born: u32,
}

/// The pings on the minimap and the local player's last send.
#[derive(Clone, Debug, Default)]
pub struct Pings {
    list: Vec<Ping>,
    last_sent: Option<u32>,
}

impl Pings {
    /// Whether the local player may send a ping at cycle `tick` (the gap since the last one has passed).
    pub fn may_send(&self, tick: u32) -> bool {
        self.last_sent.is_none_or(|t| tick.wrapping_sub(t) >= GAP || tick < t)
    }

    /// Records that the local player sent a ping at `tick`.
    pub fn sent(&mut self, tick: u32) {
        self.last_sent = Some(tick);
    }

    /// Adds a ping from `slot` at `tile`, seen at `tick`. A newer ping from the same player replaces its older one.
    pub fn add(&mut self, slot: u8, tile: (i32, i32), tick: u32) {
        self.list.retain(|p| p.slot != slot);
        self.list.push(Ping { slot, tile, born: tick });
        if self.list.len() > MAX {
            self.list.remove(0);
        }
    }

    /// Drops the pings that have run out at `tick`.
    pub fn expire(&mut self, tick: u32) {
        self.list.retain(|p| tick.saturating_sub(p.born) < LIFE);
    }

    /// The pings whose marker is on at `tick`.
    pub fn shown(&self, tick: u32) -> Vec<Ping> {
        self.list
            .iter()
            .filter(|p| (tick.saturating_sub(p.born) / BLINK).is_multiple_of(2))
            .copied()
            .collect()
    }

    /// Whether any ping is waiting.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ping_blinks_and_runs_out() {
        let mut p = Pings::default();
        p.add(1, (10, 20), 100);
        assert_eq!(p.shown(100).len(), 1, "on at first");
        assert!(p.shown(100 + BLINK).is_empty(), "then off");
        assert_eq!(p.shown(100 + 2 * BLINK).len(), 1, "and on again");
        p.expire(100 + LIFE - 1);
        assert!(!p.is_empty());
        p.expire(100 + LIFE);
        assert!(p.is_empty());
    }

    #[test]
    fn a_player_has_one_ping_and_the_list_is_capped() {
        let mut p = Pings::default();
        p.add(1, (1, 1), 0);
        p.add(1, (2, 2), 5);
        assert_eq!(
            p.shown(5),
            [Ping {
                slot: 1,
                tile: (2, 2),
                born: 5
            }]
        );
        for s in 0..20 {
            p.add(s, (0, 0), 6);
        }
        assert_eq!(p.shown(6).len(), MAX);
    }

    #[test]
    fn sending_is_rate_limited() {
        let mut p = Pings::default();
        assert!(p.may_send(0));
        p.sent(0);
        assert!(!p.may_send(GAP - 1));
        assert!(p.may_send(GAP));
    }
}
