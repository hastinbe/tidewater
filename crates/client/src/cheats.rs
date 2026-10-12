//! Typed cheat lines on the client side. Spec: `docs/specs/game/cheats.md`, "Entering a cheat during a game". No Bevy
//! types here.
//!
//! [`submit`] checks a line the player typed before it would go out as chat ([`tidewater_sim::cheats::interpret`]) and
//! says what the caller must do next ([`Outcome`]). The client has no chat line yet, so nothing calls it; when the
//! line exists it calls [`submit`] on Enter. The notices come back through the sim's events (`messages::event_notice`).
//!
//! Defaults: cheats are enabled unless the world's [`Rules::cheats_disabled`](tidewater_sim::Rules) says a multiplayer
//! lobby turned them off (it is off by default, and has no effect in single player).
//!
//! TODO (campaign flow is being built in `main.rs`): carry out [`Outcome::SelectMission`] (end the game and start the
//! mission with its act screen and briefing), `Event::Finale` (campaign ending) and `Event::Disco` (music track), and
//! apply the local settings of [`Outcome::Local`] and [`Outcome::Developer`].

use tidewater_sim::cheats::{self, Context, Flags, Line, MissionSelect, Word};
use tidewater_sim::{Command, Rules};

/// What to do with a typed line.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), expect(dead_code, reason = "no chat line yet"))]
pub enum Outcome {
    /// Not a cheat: send it as chat in multiplayer, drop it in single player.
    Chat,
    /// A cheat command was queued; the line is used up.
    Sent,
    /// Queued like [`Outcome::Sent`], and a mission was chosen: end the game and start it (TODO).
    SelectMission(MissionSelect),
    /// Queued like [`Outcome::Sent`], and a local-only setting (sync check, demo speed, profiling) toggles on this
    /// machine at once.
    Local(Word),
    /// A developer word that is no command: toggle its setting and show the fixed notice (`Word::notice`).
    Developer(Word),
}

/// Checks `line` typed by `player` (the world's `rules` and current cheat `flags`) and queues the cheat command in `pending` if it is one.
#[cfg_attr(not(test), expect(dead_code, reason = "no chat line yet"))]
pub fn submit(rules: &Rules, flags: Flags, player: u8, line: &str, pending: &mut Vec<Command>) -> Outcome {
    let cx = Context {
        flags,
        multiplayer: rules.multiplayer,
        cheats_disabled: rules.multiplayer && rules.cheats_disabled,
        enhanced: rules.enhanced,
    };
    match cheats::interpret(line, &cx) {
        Line::NotCheat => Outcome::Chat,
        Line::Developer(w) => Outcome::Developer(w),
        Line::Cheat(t) => {
            pending.push(Command::Cheat {
                player,
                flags: t.flags.0,
            });
            match (t.mission, t.local) {
                (Some(m), _) => Outcome::SelectMission(m),
                (None, Some(w)) => Outcome::Local(w),
                (None, None) => Outcome::Sent,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cheat_line_queues_a_command() {
        let rules = Rules::default();
        let mut pending = Vec::new();
        assert_eq!(submit(&rules, Flags(0), 0, "On Screen", &mut pending), Outcome::Sent);
        assert!(matches!(pending[..], [Command::Cheat { player: 0, flags }] if flags == cheats::flag::REVEAL_MAP));
        assert_eq!(
            submit(&rules, Flags(0), 0, "hello there", &mut Vec::new()),
            Outcome::Chat
        );
    }

    #[test]
    fn mission_select_and_local_words() {
        let rules = Rules::default();
        let mut pending = Vec::new();
        let on = Flags(cheats::flag::MISSION_SELECT);
        let out = submit(&rules, on, 0, "orc5", &mut pending);
        assert!(matches!(out, Outcome::SelectMission(m) if m.entry == 9));
        assert_eq!(
            submit(&rules, on, 0, "netprof", &mut pending),
            Outcome::Local(Word::NetProf)
        );
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn disabled_in_a_multiplayer_lobby() {
        let rules = Rules {
            multiplayer: true,
            cheats_disabled: true,
            ..Rules::default()
        };
        let mut pending = Vec::new();
        assert_eq!(submit(&rules, Flags(0), 0, "make it so", &mut pending), Outcome::Chat);
        assert!(pending.is_empty());
    }
}
