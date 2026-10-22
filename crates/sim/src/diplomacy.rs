//! The diplomacy command (`docs/specs/game/diplomacy.md`, DP1 to DP8, DP11): a player's whole alliance row toward
//! players 0 to 7, shared-vision set and allied-victory setting, replaced in the command turn.
//!
//! Static study only (no trace has a diplomacy command). Choices where the spec leaves room:
//! - A command from a player 8 or above does nothing; the spec's players are 0 to 7.
//! - The alliance entry toward the sender itself is written like the others, so the entry can be cleared (DP7). Only
//!   the victory test reads it as written; the other readers of [`World::allied`] still count a player as allied with
//!   itself (the spec's open question "An own alliance entry of 0" leaves every other reader unstudied).
//! - The colour table of the spec's step 4 is display only and belongs to the client.

use crate::world::World;

impl World {
    /// Carries out a diplomacy command of `player` ("What it changes").
    ///
    /// Enhanced: the sender's row entries toward players 0 to 7 become "allied" exactly where `alliances` is not 0
    /// (slots 8 to 15 stay), its shared-vision set becomes `vision` whole (the sender's own entry included: nothing
    /// adds it back), and its allied-victory setting becomes `allied_victory`. Classic mode: the same, but shared
    /// vision never changes during play.
    pub(crate) fn diplomacy_command(&mut self, player: u8, alliances: [u8; 8], vision: u8, allied_victory: bool) {
        if player >= 8 {
            return;
        }
        let p = &mut self.players[player as usize];
        for (q, &value) in alliances.iter().enumerate() {
            if value != 0 {
                p.allies |= 1 << q;
            } else {
                p.allies &= !(1 << q);
            }
        }
        p.allied_victory = allied_victory;
        if self.rules.enhanced {
            p.shared_vision = vision;
            if vision & (1 << player) == 0 {
                self.vision_outsiders |= 1 << player;
            } else {
                self.vision_outsiders &= !(1 << player);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ai::controller;
    use crate::victory::Outcome;
    use crate::world::tests::field;
    use crate::{Command, World};

    /// A multiplayer world of four humans as game setup leaves it: each allied with itself and the slots 8 to 15, in
    /// its own shared-vision set, allied victory off.
    fn four() -> World {
        let mut w = field(32, 32);
        w.rules.multiplayer = true;
        for p in 0..4 {
            w.players[p].controller = controller::HUMAN;
            w.players[p].allies = 0xff00 | 1 << p;
            w.players[p].shared_vision = 1 << p;
        }
        w
    }

    fn send(player: u8, alliances: [u8; 8], vision: u8, allied_victory: bool) -> Command {
        Command::Diplomacy {
            player,
            alliances,
            vision,
            allied_victory,
        }
    }

    /// DP1: the row toward players 0 to 7 is replaced, the entries toward 8 to 15 stay, the set and the setting are
    /// replaced, and no other player's row changes.
    #[test]
    fn dp1_replaces_the_senders_settings_only() {
        let mut w = four();
        w.step(&[send(1, [0, 1, 1, 0, 0, 0, 0, 0], 0b110, true)]);
        assert_eq!(w.players[1].allies, 0xff00 | 0b110);
        assert_eq!((w.players[1].shared_vision, w.players[1].allied_victory), (0b110, true));
        assert_eq!(w.players[2].allies, 0xff00 | 1 << 2);
        assert_eq!(
            (w.players[2].shared_vision, w.players[2].allied_victory),
            (0b100, false)
        );
        assert!(w.allied(1, 2) && !w.allied(2, 1));
    }

    /// DP4: a raw value of 3 counts as allied. DP8: the entries toward slots 8 to 15 are never written.
    #[test]
    fn dp4_and_dp8_values_and_untouched_slots() {
        let mut w = four();
        w.players[1].allies = 0xff00 | 1 << 1;
        w.step(&[send(1, [0, 1, 3, 0, 0, 0, 0, 0], 0b10, false)]);
        assert!(w.allied(1, 2) && w.allied(1, 15) && w.allied(1, 8));
        w.players[1].allies &= !(1 << 15);
        w.step(&[send(1, [1, 1, 0, 0, 0, 0, 0, 0], 0b10, false)]);
        assert!(!w.allied(1, 15), "slot 15 is not written");
        assert!(w.allied(1, 0) && !w.allied(1, 2));
        // A command from a player of 8 or above does nothing.
        let before = w.state_hash();
        w.players[9].allies = 0;
        w.step(&[send(9, [1; 8], 0xff, true)]);
        assert_eq!(w.players[9].allies, 0);
        assert!(!w.players[9].allied_victory);
        let _ = before;
    }

    /// DP3: in Classic mode the alliances and the allied-victory setting change, shared vision never does.
    #[test]
    fn dp3_classic_mode_leaves_shared_vision_alone() {
        let mut w = four();
        w.rules.enhanced = false;
        w.step(&[send(1, [0, 1, 1, 0, 0, 0, 0, 0], 0b110, true)]);
        assert_eq!(w.players[1].allies, 0xff00 | 0b110);
        assert_eq!((w.players[1].shared_vision, w.players[1].allied_victory), (0b10, true));
        assert_eq!(w.vision_outsiders, 0);
    }

    /// The command is carried out in the command turn, before the cycle's phases, so the next test sees it.
    #[test]
    fn a_change_is_seen_by_the_next_test() {
        let mut w = four();
        assert!(!w.allied(0, 3));
        w.step(&[send(0, [1, 0, 0, 1, 0, 0, 0, 0], 0b1001, false)]);
        assert!(w.allied(0, 3));
        assert_eq!(w.shared_set(0), 0b1001);
    }

    /// DP5: a player outside its own set. Its own units' reveals clear the "seen" bits of the players in the set only,
    /// and its invisible units are detected by the set, not by the owner through it.
    #[test]
    fn dp5_a_player_may_leave_its_own_set() {
        let mut w = four();
        w.step(&[send(1, [0, 1, 0, 0, 0, 0, 0, 0], 0b100, false)]);
        assert_eq!(w.shared_set(1), 0b100, "nothing puts the sender back");
        w.vision.seen.fill(0xff);
        w.reveal(1, (10, 10), 2);
        assert!(!w.vision.unseen_by(2, 10, 10) && w.vision.unseen_by(1, 10, 10));
        let u = w.spawn(0, 1, (5, 5), 0).unwrap();
        let i = w.index(u).unwrap();
        w.units[i].invisibility = 50;
        w.step(&[]);
        assert_eq!(w.units[i].detected_by, 0b100);
        // A later command that holds the sender puts it back; so does a leave.
        w.step(&[send(1, [0, 1, 0, 0, 0, 0, 0, 0], 0b110, false)]);
        assert_eq!(w.shared_set(1), 0b110);
        w.step(&[send(1, [0, 1, 0, 0, 0, 0, 0, 0], 0b100, false)]);
        assert_eq!(w.shared_set(1), 0b100);
        w.step(&[Command::Leave {
            player: 1,
            dropped: false,
        }]);
        assert_eq!(w.shared_set(1), 0b10);
        assert_eq!(w.vision_outsiders, 0);
    }

    /// DP6: with humans 0 and 1 in, the check gives a result only when both have sent allied-with-both and allied
    /// victory on.
    #[test]
    fn dp6_and_dp7_allied_victory_needs_both_players_and_every_pairing() {
        let mut w = four();
        w.spawn(0, 0, (1, 1), 0).unwrap();
        w.spawn(0, 1, (8, 8), 0).unwrap();
        w.players[2].controller = controller::NOBODY;
        w.players[3].controller = controller::NOBODY;
        w.step(&[send(0, [1, 1, 0, 0, 0, 0, 0, 0], 0b11, true)]);
        w.multiplayer(0);
        assert!(!w.victory.over, "player 1 hasn't sent anything");
        // DP7: before the check that follows player 1's command, player 0 clears its own entry.
        w.step(&[
            send(1, [1, 1, 0, 0, 0, 0, 0, 0], 0b11, true),
            send(0, [0, 1, 0, 0, 0, 0, 0, 0], 0b11, true),
        ]);
        w.multiplayer(0);
        assert!(!w.victory.over, "the self pairing fails");
        w.step(&[send(0, [1, 1, 0, 0, 0, 0, 0, 0], 0b11, true)]);
        w.multiplayer(0);
        assert_eq!(w.players[0].result, Some(Outcome::Victory));
        // The check runs on the local player's machine; the others get the game-ended mark.
        assert!(w.victory.over);
    }

    /// Allied victory starts off for every player (DP9).
    #[test]
    fn dp9_allied_victory_starts_off() {
        let w = four();
        assert!(w.players.iter().all(|p| !p.allied_victory));
    }

    /// DP11: the sets and the settings are saved and restored, with the players left out of their own sets, and all
    /// of it is part of the sync hash.
    #[test]
    fn dp11_a_save_keeps_the_sets_and_settings() {
        let mut w = four();
        w.step(&[
            send(1, [0, 1, 1, 0, 0, 0, 0, 0], 0b100, false),
            send(0, [1, 0, 0, 0, 0, 0, 0, 0], 0b11, true),
        ]);
        let bytes = w.save_state();
        let mut loaded = four();
        loaded.restore(&bytes, 1).unwrap();
        for p in 0..4 {
            assert_eq!(loaded.players[p].allies, w.players[p].allies);
            assert_eq!(loaded.players[p].shared_vision, w.players[p].shared_vision);
            assert_eq!(loaded.players[p].allied_victory, w.players[p].allied_victory);
        }
        assert_eq!(loaded.vision_outsiders, 0b10);
        assert_eq!((loaded.shared_set(1), loaded.shared_set(0)), (0b100, 0b11));
    }

    #[test]
    fn the_sets_and_settings_are_in_the_state_hash() {
        let base = four().state_hash();
        let changed = |f: &dyn Fn(&mut World)| {
            let mut w = four();
            f(&mut w);
            w.state_hash() != base
        };
        assert!(changed(&|w| w.players[0].shared_vision = 0b11));
        assert!(changed(&|w| w.players[0].allied_victory = true));
        assert!(changed(&|w| w.players[0].allies &= !1));
        assert!(changed(&|w| w.vision_outsiders = 1));
    }
}
