//! The optional improvements that change the simulation (`docs/design/extras.md`, "Sim options").
//!
//! [`Extras`] lives in [`Rules::extras`](crate::Rules). Every field is **off by default**, and a game with every field
//! off runs exactly as the original rules say. The set is part of the game's state: it is hashed with the world
//! ([`World::state_hash`](crate::World::state_hash)), stored in a saved game after the rest of the world's state, carried
//! in a replay's header (`RulesTag`) and in the multiplayer lobby's start message. A network or replay session never
//! reads it from the local settings: the session hands it over, so every machine runs the same rules.
//!
//! **The fields** (batch 1): rally points, the production queue, the selection limit and the speeds beyond Fastest.
//! Their rules live next to the code they extend (`rally.rs`, `queue.rs`, `select.rs`, `timing.rs`); this module holds
//! the switches and the state they keep ([`State`]).
//!
//! **Keeping the defaults honest.** `Rules::extras` is not one of the rules' own stored fields (`#[serde(skip)]`), and
//! [`State`] is not one of the world's: both are written after the world's stored fields (`world::save`), so the stored
//! layout of the world and its fingerprint are what they were before the options existed. The state hash and the
//! fingerprint add the options and their state only when an option is on.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::economy::{Cost, JobKind};
use crate::world::{PLAYERS, UnitId};

/// The selection limit that means "no limit".
pub const UNLIMITED: u8 = 255;

/// The most speed levels above Fastest.
pub const MAX_EXTRA_SPEEDS: u8 = 3;

/// The sim-touching options a game runs with. See the module doc.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Extras {
    /// Training buildings keep a rally point: trained units are sent there (`rally.rs`).
    pub rally_points: bool,
    /// Buildings queue up to five jobs, paid when queued (`queue.rs`).
    pub production_queue: bool,
    /// How many units a synced selection holds: 0 for the original nine, 12, 18 or [`UNLIMITED`] (`select.rs`).
    pub selection_limit: u8,
    /// How many speed levels there are above Fastest, 0 to [`MAX_EXTRA_SPEEDS`] (`timing.rs`). Enhanced rules only.
    pub extra_speeds: u8,
}

impl Extras {
    /// Whether every option is off (the original rules).
    pub fn is_default(&self) -> bool {
        *self == Extras::default()
    }

    /// How many units a synced selection holds: nine unless the option says otherwise.
    pub fn selection_cap(&self) -> usize {
        match self.selection_limit {
            0 => crate::select::PLACES,
            UNLIMITED => usize::MAX,
            n => usize::from(n).max(crate::select::PLACES),
        }
    }

    /// The options that are on, in words, for a lobby's or a log's description. Empty when every option is off.
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.rally_points {
            out.push("rally points".to_string());
        }
        if self.production_queue {
            out.push("production queue".to_string());
        }
        match self.selection_limit {
            0 => {}
            UNLIMITED => out.push("unlimited selection".to_string()),
            n => out.push(format!("selection of {n}")),
        }
        if self.extra_speeds > 0 {
            out.push(format!(
                "{} extra speed level(s)",
                self.extra_speeds.min(MAX_EXTRA_SPEEDS)
            ));
        }
        out
    }

    /// The highest speed level a speed key reaches under the rules in force.
    pub fn top_speed(&self, enhanced: bool) -> u8 {
        if enhanced {
            crate::select::TOP_SPEED_ENHANCED + self.extra_speed_levels(true)
        } else {
            crate::select::TOP_SPEED_CLASSIC
        }
    }

    /// The speed levels above Fastest this game has (none under Classic rules).
    pub fn extra_speed_levels(&self, enhanced: bool) -> u8 {
        if enhanced {
            self.extra_speeds.min(MAX_EXTRA_SPEEDS)
        } else {
            0
        }
    }
}

/// A rally point: where a building sends the units it trains.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rally {
    /// The unit it was set on, if any (a mine to work, a friendly unit to follow, an enemy to attack).
    pub target: Option<UnitId>,
    /// The tile it was set on.
    pub tile: (i32, i32),
}

/// A job waiting in a building's queue, already paid for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Queued {
    pub kind: JobKind,
    /// What was paid when it was queued: a cancel gives exactly this back.
    pub paid: Cost,
}

/// The state the options keep, outside the world's own stored fields. Empty whenever the options are off.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct State {
    /// Rally points by building.
    pub rally: BTreeMap<UnitId, Rally>,
    /// The jobs waiting behind each building's running one, next first.
    pub queue: BTreeMap<UnitId, Vec<Queued>>,
    /// Each player's selected units beyond the nine places of [`PlayerSelection`](crate::select::PlayerSelection), in
    /// order. Not stored in a saved game: a load starts with an empty selection.
    #[serde(skip)]
    pub more: [Vec<UnitId>; PLAYERS],
}

impl State {
    /// Whether nothing is kept.
    pub fn is_empty(&self) -> bool {
        self.rally.is_empty() && self.queue.is_empty() && self.more.iter().all(Vec::is_empty)
    }
}

/// The part of a saved game the options add: the options and what they keep.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub extras: Extras,
    pub state: State,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_is_off_in_the_default_set() {
        let e = Extras::default();
        assert!(e.is_default());
        assert!(!e.rally_points && !e.production_queue);
        assert_eq!(e.selection_cap(), 9);
        assert_eq!(e.extra_speed_levels(true), 0);
    }

    #[test]
    fn the_set_round_trips_through_postcard() {
        let e = Extras {
            rally_points: true,
            production_queue: true,
            selection_limit: 18,
            extra_speeds: 2,
        };
        let bytes = postcard::to_allocvec(&e).unwrap();
        let back: Extras = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, e);
        assert!(!back.is_default());
    }

    #[test]
    fn the_set_has_a_layout_so_the_save_and_replay_versions_were_bumped() {
        // The set used to be empty and wrote no bytes. With fields it does, which is why the saved game's and the
        // replay's versions changed when the first one came (`savefile::VERSION`, `tidewater_replay::format::VERSION`).
        let bytes = postcard::to_allocvec(&Extras::default()).unwrap();
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn the_options_that_are_on_are_described() {
        assert!(Extras::default().describe().is_empty());
        let e = Extras {
            rally_points: true,
            production_queue: true,
            selection_limit: UNLIMITED,
            extra_speeds: 2,
        };
        assert_eq!(
            e.describe(),
            [
                "rally points",
                "production queue",
                "unlimited selection",
                "2 extra speed level(s)"
            ]
        );
    }

    #[test]
    fn the_selection_cap_follows_the_limit() {
        let cap = |n| {
            Extras {
                selection_limit: n,
                ..Extras::default()
            }
            .selection_cap()
        };
        assert_eq!((cap(0), cap(12), cap(18), cap(UNLIMITED)), (9, 12, 18, usize::MAX));
        // A limit below nine cannot take places away.
        assert_eq!(cap(4), 9);
    }

    #[test]
    fn extra_speeds_exist_only_in_enhanced_rules() {
        let e = Extras {
            extra_speeds: 3,
            ..Extras::default()
        };
        assert_eq!((e.extra_speed_levels(true), e.extra_speed_levels(false)), (3, 0));
        let e = Extras {
            extra_speeds: 9,
            ..Extras::default()
        };
        assert_eq!(e.extra_speed_levels(true), MAX_EXTRA_SPEEDS);
    }

    #[test]
    fn the_default_rules_have_every_extra_off() {
        assert!(crate::Rules::default().extras.is_default());
    }

    #[test]
    fn the_state_is_empty_by_default() {
        assert!(State::default().is_empty());
        let mut s = State::default();
        s.more[0].push(UnitId(1));
        assert!(!s.is_empty());
    }

    #[test]
    fn a_game_with_an_option_on_hashes_apart_and_a_game_with_all_off_does_not() {
        use crate::world::tests::field;
        let mut a = field(8, 8);
        let b = field(8, 8);
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.fingerprint(), b.fingerprint());
        a.rules.extras.rally_points = true;
        assert_ne!(
            a.state_hash(),
            b.state_hash(),
            "two machines with different options desync at once"
        );
        assert_ne!(a.fingerprint(), b.fingerprint());
        a.rules.extras = Extras::default();
        assert_eq!(a.state_hash(), b.state_hash());
        // What an option keeps is state too.
        a.rules.extras.production_queue = true;
        let h = a.state_hash();
        a.ext.rally.insert(
            UnitId(7),
            Rally {
                target: None,
                tile: (1, 2),
            },
        );
        assert_ne!(a.state_hash(), h);
    }

    #[test]
    fn the_options_and_their_state_come_back_from_a_saved_game() {
        use crate::world::tests::field;
        let mut a = field(8, 8);
        a.rules.extras = Extras {
            rally_points: true,
            production_queue: true,
            selection_limit: 18,
            extra_speeds: 2,
        };
        a.ext.rally.insert(
            UnitId(3),
            Rally {
                target: Some(UnitId(4)),
                tile: (5, 6),
            },
        );
        a.ext.queue.insert(
            UnitId(3),
            vec![Queued {
                kind: JobKind::Train(2),
                paid: [400, 0, 0],
            }],
        );
        // The selection is not kept by a load, so it is not stored.
        a.ext.more[0].push(UnitId(9));
        let bytes = a.save_state();
        let mut b = field(8, 8);
        b.restore(&bytes, 1).unwrap();
        assert_eq!(b.rules.extras, a.rules.extras);
        assert_eq!(b.ext.rally, a.ext.rally);
        assert_eq!(b.ext.queue, a.ext.queue);
        assert!(b.ext.more.iter().all(Vec::is_empty));
    }

    #[test]
    fn a_game_with_every_option_off_stores_the_same_world_as_before_the_options() {
        use crate::world::tests::field;
        let w = field(8, 8);
        let bytes = w.save_state();
        // The options' part is the last four bytes of the options plus three empty containers.
        let tail = postcard::to_allocvec(&w.extras_saved()).unwrap();
        assert!(bytes.ends_with(&tail));
        assert_eq!(tail, [0, 0, 0, 0, 0, 0]);
    }
}
