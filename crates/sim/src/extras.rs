//! The optional improvements that change the simulation (`docs/design/extras.md`, "Sim options").
//!
//! [`Extras`] lives in [`Rules::extras`](crate::Rules). Every field is **off by default**, and a game with every field
//! off runs exactly as the original rules say. The set is part of the game's state: it is hashed with the world
//! ([`World::state_hash`](crate::World::state_hash)), stored in a saved game with the rest of the rules, carried in a
//! replay's header (`RulesTag`) and will be carried in the multiplayer lobby's start message. A network or replay session
//! never reads it from the local settings: the session hands it over, so every machine runs the same rules.
//!
//! **No fields yet.** The struct is empty, so it adds no bytes to a saved game or a replay header (postcard writes a
//! struct with no fields as nothing) and nothing to the state hash; the save version and the replay version stay as
//! they are, and the snapshots are unchanged. **The first field added changes the layout**: bump the client's saved-game
//! `VERSION` and the replay format's `VERSION` in the same commit, and hash the field only when it is not at its default
//! (as the cheat flags are) so a game with it off still hashes as it always has.

use serde::{Deserialize, Serialize};

/// The sim-touching options a game runs with. See the module doc.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Extras {}

impl Extras {
    /// Whether every option is off (the original rules).
    pub fn is_default(&self) -> bool {
        *self == Extras::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_set_adds_no_bytes_to_a_save_or_a_header() {
        let bytes = postcard::to_allocvec(&Extras::default()).unwrap();
        assert!(bytes.is_empty(), "a field is added: bump the save and replay versions");
        let back: Extras = postcard::from_bytes(&bytes).unwrap();
        assert!(back.is_default());
    }

    #[test]
    fn an_empty_set_adds_nothing_to_the_hash() {
        use std::hash::{Hash, Hasher};
        struct Count(usize);
        impl Hasher for Count {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, b: &[u8]) {
                self.0 += b.len();
            }
        }
        let mut c = Count(0);
        Extras::default().hash(&mut c);
        assert_eq!(c.0, 0);
    }

    #[test]
    fn the_default_rules_have_every_extra_off() {
        assert!(crate::Rules::default().extras.is_default());
    }
}
