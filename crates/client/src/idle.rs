//! The idle-worker button and key (option `idle-worker`, `extras`, `docs/design/extras.md`): select the player's next
//! worker that has nothing to do. Our own design, not the original's.
//!
//! **Who counts.** A unit of the local player that is a worker (a peasant or peon: the unit table's worker flag), is
//! alive and in the open (not dying, removed, or inside a mine, platform or depot), and has no order and none waiting:
//! it stands doing nothing. A worker carrying goods that stands still counts; one walking, chopping, mining, building,
//! repairing or returning goods does not. Oil tankers are not workers here.
//!
//! **Which one.** Each press takes the idle worker after the one the last press took, by creation number, and wraps
//! round to the first. So repeated presses go through every idle worker once. The choice is the normal [`Select`]
//! command for that one unit, as a click on it would send, and the view centres on it.
//!
//! The key is **F6**, which the original leaves unbound (`mouse::key_action`): F1 help, F2 to F4 views, F5 options, F7 to
//! F9 the dialogs, F10 the menu, F11 and F12 save and load.
//!
//! [`Select`]: tidewater_sim::Command::Select

use tidewater_formats::udta::flags;
use tidewater_sim::{Order, UnitId, World};

/// The key that selects the next idle worker.
pub const KEY: u8 = 6;

/// What the rule needs to know about one unit.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
    pub owner: u8,
    pub worker: bool,
    pub building: bool,
    /// Dying, removed, released or inside a mine, platform or depot.
    pub gone: bool,
    /// The order is "idle" and none is waiting.
    pub no_order: bool,
}

/// Whether a unit is one of `player`'s idle workers (see the module doc).
pub fn is_idle_worker(f: &Facts, player: u8) -> bool {
    f.owner == player && f.worker && !f.building && !f.gone && f.no_order
}

/// The local player's idle workers, in creation order.
pub fn idle_workers(world: &World, player: u8) -> Vec<UnitId> {
    let mut v: Vec<UnitId> = world
        .units
        .iter()
        .filter(|u| {
            let facts = Facts {
                owner: u.owner,
                worker: world
                    .stats
                    .0
                    .get(u.kind as usize)
                    .is_some_and(|s| s.flags & flags::PEON != 0),
                building: u.building,
                gone: u.dying || u.removing || u.inside || u.script.anim == tidewater_sim::RELEASED,
                no_order: u.order == Order::Idle && u.pending.is_none(),
            };
            is_idle_worker(&facts, player)
        })
        .map(|u| u.id)
        .collect();
    v.sort();
    v
}

/// The idle worker a press takes: the first after `last` in `idle` (sorted), else the first.
pub fn next(idle: &[UnitId], last: Option<UnitId>) -> Option<UnitId> {
    let after = last.and_then(|l| idle.iter().copied().find(|&id| id > l));
    after.or_else(|| idle.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[u32]) -> Vec<UnitId> {
        v.iter().map(|&n| UnitId(n)).collect()
    }

    #[test]
    fn presses_go_through_every_idle_worker_and_wrap() {
        let idle = ids(&[3, 7, 12]);
        assert_eq!(next(&idle, None), Some(UnitId(3)));
        assert_eq!(next(&idle, Some(UnitId(3))), Some(UnitId(7)));
        assert_eq!(next(&idle, Some(UnitId(7))), Some(UnitId(12)));
        assert_eq!(next(&idle, Some(UnitId(12))), Some(UnitId(3)), "wraps");
    }

    #[test]
    fn a_last_worker_that_is_no_longer_idle_still_places_the_next_press() {
        let idle = ids(&[3, 12]);
        assert_eq!(next(&idle, Some(UnitId(7))), Some(UnitId(12)), "7 went to work");
        assert_eq!(next(&idle, Some(UnitId(99))), Some(UnitId(3)));
    }

    #[test]
    fn only_a_standing_worker_of_the_player_counts() {
        let idle = Facts {
            owner: 0,
            worker: true,
            no_order: true,
            ..Facts::default()
        };
        assert!(is_idle_worker(&idle, 0));
        assert!(!is_idle_worker(&idle, 1), "another player's");
        assert!(!is_idle_worker(&Facts { worker: false, ..idle }, 0), "a footman");
        assert!(!is_idle_worker(&Facts { building: true, ..idle }, 0), "a town hall");
        assert!(
            !is_idle_worker(&Facts { gone: true, ..idle }, 0),
            "dead or inside a mine"
        );
        assert!(
            !is_idle_worker(
                &Facts {
                    no_order: false,
                    ..idle
                },
                0
            ),
            "walking, mining, building"
        );
    }

    #[test]
    fn no_idle_worker_selects_nothing() {
        assert_eq!(next(&[], None), None);
        assert_eq!(next(&[], Some(UnitId(4))), None);
    }
}
