//! Cheats on the client side. Spec: `docs/specs/game/cheats.md`.
//!
//! [`submit`] checks a line the player typed before it would go out as chat ([`tidewater_sim::cheats::interpret`]) and
//! says what the caller must do next ([`Outcome`]). The chat line (`chat`) calls it on Enter. The notices come back
//! through the sim's events (`messages::event_notice`).
//!
//! Defaults: cheats are enabled unless the world's [`Rules::cheats_disabled`](tidewater_sim::Rules) says a multiplayer
//! lobby turned them off (it is off by default, and has no effect in single player).
//!
//! The flow glue ([`plugin`]): the kept flags (mission select, `title`, `noglues`) live in the `Flow` between games
//! and start the next world's flags; `noglues` and `title` toggle `Flow::skip_screens` and `Flow::skip_videos`; the
//! finale event and a chosen mission (`Queue::mission`) leave the game for the campaign flow without a score screen.
//!
//! The disco flag's change (`Event::Disco`) changes the game's music track ([`play_disco`], `ui/music.md`, "Disco
//! cheat"): turning it on picks track 25, turning it off the campaign formula or a new draw from the setup-shuffle
//! generator; a playing track fades with all sound first. The local and developer words' settings are in `chat`.
//!
//! Not yet here: command-line words (`cheats::from_arguments`) are not read because the command line is parsed by
//! `clap`.

use tidewater_sim::cheats::{self, Context, Flags, Line, MissionSelect, Word};
use tidewater_sim::economy::Event;
use tidewater_sim::{Command, Rules};

use crate::core::Core;
use crate::frontend::{self, Flow, Step};
use crate::input::Key;

/// What to do with a typed line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Not a cheat: send it as chat in multiplayer, drop it in single player.
    Chat,
    /// A cheat command was queued; the line is used up.
    Sent,
    /// Queued like [`Outcome::Sent`], and a mission was chosen: end the game and start it (put it in [`Queue::mission`]).
    SelectMission(MissionSelect),
    /// Queued like [`Outcome::Sent`], and a local-only setting (sync check, demo speed, profiling) toggles on this
    /// machine at once.
    Local(Word),
    /// A developer word that is no command: toggle its setting and show the fixed notice (`Word::notice`).
    Developer(Word),
}

/// Checks `line` typed by `player` (the world's `rules` and current cheat `flags`) and queues the cheat command in `pending` if it is one.
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

/// Cheat outcomes for the flow, filled by `tick` and the chat line and read by [`run`].
#[derive(Default)]
pub struct Queue {
    /// The finale cheat fired: `bdp` says which ending.
    finale: Option<bool>,
    /// Mission select chose a mission.
    pub mission: Option<MissionSelect>,
    /// The disco flag changed to this.
    disco: Option<bool>,
}

impl Queue {
    /// Keeps the cheat events of one cycle.
    pub fn take_events(&mut self, events: &[Event]) {
        for e in events {
            match *e {
                Event::Finale { beyond } => self.finale = Some(beyond),
                Event::Disco { on } => self.disco = Some(on),
                _ => {}
            }
        }
    }
}

/// The disco cheat changes the game's track (music spec, "Disco cheat").
pub(crate) fn play_disco(c: &mut Core) {
    let Core {
        cheats: queue,
        sim,
        origin,
        speaker,
        jukebox,
        ..
    } = c;
    let Some(on) = queue.disco.take() else { return };
    let (Some(sim), Some(speaker)) = (sim.as_ref(), speaker.as_mut()) else {
        return;
    };
    let orc = sim.world.start.orc_sounds;
    let campaign = origin.as_ref().and_then(|o| o.setup.campaign());
    jukebox.disco(on, || {
        crate::music::game_track(orc, campaign, false, &mut speaker.director.shuffle)
    });
}

/// Starts the world's flags from the flow, keeps the flow's settings in step with the world's flags, and leaves the
/// game for a finale or a chosen mission.
pub(crate) fn run(c: &mut Core) {
    let Core {
        sim,
        sim_new,
        cheats: queue,
        flow,
        front,
        states: next,
        exit,
        input,
        ..
    } = c;
    let keys = &input.keys;
    let Some(sim) = sim.as_mut() else { return };
    // A game that is new since the last frame this ran (what Bevy's `is_added` told).
    if std::mem::take(sim_new) {
        // "Flags across games": a new game starts with the three kept flags only.
        // Cheats are a single-player feature (cheats spec): a network game starts with none, whatever the client kept,
        // so every machine starts from the same flags. Network cheats go through `Command::Cheat`.
        if !sim.world.rules.multiplayer {
            // A replay starts the flags the recorded game started with.
            let kept = sim
                .session
                .start_cheats()
                .map_or_else(|| flow.cheat_flags.for_new_game(), Flags);
            sim.world.start_cheats(kept);
        }
    }
    sync_flow(flow, sim.world.cheats.flags);
    let ctrl = keys.any_pressed([Key::ControlLeft, Key::ControlRight]);
    let steps = if let Some(m) = queue.mission.take() {
        // The game ends with no score screen and the mission starts with its act screen and briefing.
        vec![Step::Mission(m.entry)]
    } else if let Some(bdp) = queue.finale.take() {
        // An ending, not a victory: no score screen.
        vec![Step::Ending {
            bdp,
            race: sim.world.players[sim.human as usize].race,
        }]
    } else {
        return;
    };
    flow.steps = steps.into();
    match frontend::advance(flow, &front.install, ctrl) {
        Some(s) => next.set(s),
        None => *exit = true,
    }
}

/// The flow's settings follow the world's flags: `noglues` is "skip the act screen, briefing and score screens" and
/// `title` is "skip the startup videos".
fn sync_flow(flow: &mut Flow, flags: Flags) {
    flow.cheat_flags = flags;
    flow.skip_screens = flags.has(cheats::flag::NO_GLUES);
    flow.skip_videos = flags.has(cheats::flag::SKIP_TITLE);
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
    fn the_flow_follows_the_flags() {
        let mut flow = Flow::default();
        sync_flow(&mut flow, Flags(cheats::flag::NO_GLUES | cheats::flag::MISSION_SELECT));
        assert!(flow.skip_screens && !flow.skip_videos);
        assert_eq!(flow.cheat_flags.for_new_game(), flow.cheat_flags);
        sync_flow(&mut flow, Flags(cheats::flag::SKIP_TITLE));
        assert!(!flow.skip_screens && flow.skip_videos);
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
