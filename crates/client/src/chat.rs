//! The chat line in the status bar (spec `docs/specs/ui/messages.md`, "Chat line"), and what a typed line does
//! (`docs/specs/game/cheats.md`, "Entering a cheat during a game" and "Word codes").
//!
//! Enter opens the line; Enter sends it and Escape closes it without sending. A sent line is first checked as a cheat
//! ([`crate::cheats::submit`]); in single player a line that isn't one is dropped ("In single player nothing is
//! sent"). While the line is open the keys that type text don't act as game keys: the letters (command-panel hotkeys),
//! digits, + and − (`ui/selecting-and-commanding.md`, "Keys"), the space key (the messages spec's open question:
//! "most likely the chat line takes the key"), the numeric keypad (`ui/screen-and-cursors.md`: no keypad scrolling
//! while a chat line is typed), Enter, Escape and Backspace. Function keys, the arrow and paging keys, and every key
//! with Ctrl or Alt held keep working.
//!
//! Choices where the specs leave a gap:
//! - The label and the field's look come from `Rez\textbox.bin`, which has no spec: the label is our own
//!   ([`LABEL`]) and a `_` marks the typing position.
//! - Accepted characters are printable ASCII (space to `~`), and a line holds at most [`MAX_CHARS`] characters, the
//!   length of a message line (`messages::MAX_CHARS`).
//! - The fixed notices of the three developer words that are no command (`showinfo`, `showmemory`, `nopalette`) are
//!   the sim's own texts (`Word::notice`, the cheats spec's open-question default). `nopalette` toggles the stored
//!   colour-cycling bit; the other developer and local words toggle settings of [`LocalSettings`], which nothing in
//!   this client displays or uses yet (no network or demo playback, no debug displays).
//! - Multiplayer chat: in a network game a line that is no cheat is sent to everyone, or to the sender's allies when it
//!   starts with `/a ` or `/allies ` ([`recipients`]; spec gap, the messages spec leaves recipients open). It travels
//!   as its own network message (`net::proto::Msg::Chat`), outside the turns and the state hash. Under Enhanced rules
//!   the sender's own line joins the message list at once; under Classic rules the "chat sent" notice shows instead.

use tidewater_sim::cheats::Word;

use crate::cheats::{self, Outcome};
use crate::core::{self, Core};
use crate::input::{Input, Key, KeyEvent};
use crate::messages::{self, Text};
use crate::prefs::bit;

/// The label in front of the typed text.
pub const LABEL: &str = "Message: ";
/// The longest line, in characters.
pub const MAX_CHARS: usize = messages::MAX_CHARS;

/// Adds typed text to `line`: printable ASCII only, up to [`MAX_CHARS`].
pub fn type_text(line: &mut String, text: &str) {
    for c in text.chars() {
        if (' '..='~').contains(&c) && line.chars().count() < MAX_CHARS {
            line.push(c);
        }
    }
}

/// What the status bar shows for an open line.
pub fn shown(line: &str) -> String {
    format!("{LABEL}{line}_")
}

/// Whether `k` types into an open chat line instead of acting as a game key (when neither Ctrl nor Alt is held).
pub fn typing_key(k: Key) -> bool {
    !matches!(
        k,
        Key::F(1..=12)
            | Key::Up
            | Key::Down
            | Key::Left
            | Key::Right
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
            | Key::CapsLock
            | Key::PrintScreen
    )
}

/// Who a chat line is for and what it says. A line starting with `/a ` or `/allies ` goes to the sender's allies;
/// anything else goes to everyone (spec gap: `ui/messages.md` leaves the recipients to a later spec, and the
/// original's key for choosing them is not documented).
pub fn recipients(line: &str) -> (bool, &str) {
    let line = line.trim();
    for prefix in ["/allies ", "/a "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return (true, rest.trim());
        }
    }
    (false, line)
}

/// Developer and local settings toggled by typed words (`game/cheats.md`, "Word codes") that only this machine keeps.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalSettings {
    pub sync_check: bool,
    pub fast_demo: bool,
    pub net_profiling: bool,
    pub show_info: bool,
    pub show_memory: bool,
}

/// What the program does after a line was sent, besides the cheat command `cheats::submit` already queued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum After {
    Nothing,
    /// Mission select chose a mission.
    Mission(tidewater_sim::cheats::MissionSelect),
    /// A developer word that is no command: its fixed notice; `colour_cycling` says to toggle that preference.
    Notice {
        text: String,
        colour_cycling: bool,
    },
}

/// Applies an [`Outcome`] to the machine's own settings and says what else to do.
pub fn after(outcome: Outcome, local: &mut LocalSettings) -> After {
    let toggle = |local: &mut LocalSettings, w: Word| match w {
        Word::AllowSync => local.sync_check ^= true,
        Word::FastDemo => local.fast_demo ^= true,
        Word::NetProf => local.net_profiling ^= true,
        Word::ShowInfo => local.show_info ^= true,
        Word::ShowMemory => local.show_memory ^= true,
        _ => {}
    };
    match outcome {
        // Single player: dropped. (Multiplayer would send it as chat.)
        Outcome::Chat | Outcome::Sent => After::Nothing,
        Outcome::SelectMission(m) => After::Mission(m),
        Outcome::Local(w) => {
            toggle(local, w);
            After::Nothing
        }
        Outcome::Developer(w) => {
            toggle(local, w);
            After::Notice {
                text: w.notice().to_string(),
                colour_cycling: w == Word::NoPalette,
            }
        }
    }
}

/// Opens, edits, sends and closes the chat line, and takes the typing keys from the game while it is open. Runs in the
/// game screen, after the pointer is placed (the pointer lock reads the held keys before the chat line takes the
/// typing keys).
pub(crate) fn chat_keys(c: &mut Core) {
    let running = core::game_running(c);
    let Core {
        chat_typed: typed,
        screen_ui: scr,
        sim,
        cheats: queue,
        stored,
        local,
        input,
        ..
    } = c;
    let Input { keys, typed: log, .. } = input;
    let events: Vec<KeyEvent> = log.read(typed).cloned().collect();
    let (Some(scr), Some(sim)) = (scr.as_mut(), sim.as_mut()) else {
        return;
    };
    if !running {
        return;
    }
    let held = keys.any_pressed([Key::ControlLeft, Key::ControlRight, Key::AltLeft, Key::AltRight]);
    let enter = [Key::Enter, Key::NumpadEnter];
    let Some(mut line) = scr.status.chat.take() else {
        if !held && keys.any_just_pressed(enter) {
            scr.status.chat = Some(String::new());
            enter.into_iter().for_each(|k| keys.reset(k));
        }
        return;
    };
    let mut open = true;
    let mut send = false;
    for e in events.iter().filter(|e| e.pressed) {
        match e.key {
            Key::Enter | Key::NumpadEnter => {
                send = true;
                break;
            }
            Key::Escape => {
                open = false;
                break;
            }
            Key::Backspace => {
                line.pop();
            }
            _ if held => {}
            _ => {
                if let Some(t) = &e.text {
                    type_text(&mut line, t);
                }
            }
        }
    }
    if !held {
        let taken: Vec<Key> = keys
            .get_pressed()
            .chain(keys.get_just_pressed())
            .copied()
            .filter(|&k| typing_key(k))
            .collect();
        taken.into_iter().for_each(|k| keys.reset(k));
    }
    if send {
        let (rules, flags, human) = (sim.world.rules.clone(), sim.world.cheats.flags, sim.human);
        // A `/diplomacy` line sends the diplomacy command (`game/diplomacy.md`); it is never chat.
        if let Some(me) = sim.world.players.get(human as usize) {
            let now = crate::diplomacy::Current {
                allies: me.allies,
                vision: me.shared_vision,
                victory: me.allied_victory,
            };
            if let Some(parsed) = crate::diplomacy::parse(&line, human, now) {
                match parsed {
                    Ok(command) => sim.pending.push(command),
                    Err(why) => sim
                        .notices
                        .push(Text::Own(format!("{}: {why}", crate::diplomacy::WORD))),
                }
                return;
            }
        }
        let outcome = cheats::submit(&rules, flags, human, &line, &mut sim.pending);
        // A line that is no cheat is chat in a network game; in single player it is dropped.
        if matches!(outcome, Outcome::Chat) && sim.session.is_network() {
            let (allies, text) = recipients(&line);
            sim.session.send_chat(allies, text);
        }
        match after(outcome, local) {
            After::Nothing => {}
            After::Mission(m) => queue.mission = Some(m),
            After::Notice { text, colour_cycling } => {
                if colour_cycling {
                    let on = stored.0.has(bit::COLOUR_CYCLING);
                    stored.0.set(bit::COLOUR_CYCLING, !on);
                }
                sim.notices.push(Text::Own(text));
            }
        }
        return;
    }
    if open {
        scr.status.chat = Some(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_sim::Rules;
    use tidewater_sim::cheats::Flags;

    #[test]
    fn typing_keeps_printable_ascii_up_to_the_limit() {
        let mut l = String::new();
        type_text(&mut l, "on scr\u{7f}een\té");
        assert_eq!(l, "on screen");
        type_text(&mut l, &"x".repeat(200));
        assert_eq!(l.chars().count(), MAX_CHARS);
        assert_eq!(shown("hi"), "Message: hi_");
    }

    #[test]
    fn game_keys_that_still_act_while_typing() {
        assert!(typing_key(Key::Letter('A')) && typing_key(Key::Digit(1)) && typing_key(Key::Minus));
        assert!(typing_key(Key::Numpad(8)) && typing_key(Key::Space) && typing_key(Key::Escape));
        assert!(!typing_key(Key::F(10)) && !typing_key(Key::Up) && !typing_key(Key::PageDown));
    }

    #[test]
    fn a_sent_line_does_what_the_cheats_spec_says() {
        let rules = Rules::default();
        let mut local = LocalSettings::default();
        let mut pending = Vec::new();
        // A cheat: the command is queued, nothing else.
        let o = cheats::submit(&rules, Flags(0), 0, "on screen", &mut pending);
        assert_eq!(after(o, &mut local), After::Nothing);
        assert_eq!(pending.len(), 1);
        // Not a cheat, single player: dropped.
        let o = cheats::submit(&rules, Flags(0), 0, "hello", &mut pending);
        assert_eq!(after(o, &mut local), After::Nothing);
        assert_eq!(pending.len(), 1);
        // nopalette: a fixed notice, the colour-cycling preference toggles, nothing sent.
        let o = cheats::submit(&rules, Flags(0), 0, "NoPalette", &mut pending);
        assert!(matches!(after(o, &mut local), After::Notice { colour_cycling: true, ref text } if !text.is_empty()));
        assert_eq!(pending.len(), 1);
        // showinfo: its notice and its local setting.
        let o = cheats::submit(&rules, Flags(0), 0, "showinfo", &mut pending);
        assert!(matches!(
            after(o, &mut local),
            After::Notice {
                colour_cycling: false,
                ..
            }
        ));
        assert!(local.show_info);
        // netprof: a cheat command and a setting of this machine.
        let o = cheats::submit(&rules, Flags(0), 0, "netprof", &mut pending);
        assert_eq!(after(o, &mut local), After::Nothing);
        assert!(local.net_profiling && pending.len() == 2);
    }

    #[test]
    fn a_slash_a_prefix_limits_a_line_to_allies() {
        assert_eq!(recipients("hello all"), (false, "hello all"));
        assert_eq!(recipients("/a  hold the ford"), (true, "hold the ford"));
        assert_eq!(recipients("/allies push now"), (true, "push now"));
        assert_eq!(recipients("/alliesx"), (false, "/alliesx"));
    }

    #[test]
    fn mission_select_chooses_the_mission() {
        let rules = Rules::default();
        let on = Flags(tidewater_sim::cheats::flag::MISSION_SELECT);
        let o = cheats::submit(&rules, on, 0, "orc5", &mut Vec::new());
        assert!(matches!(after(o, &mut LocalSettings::default()), After::Mission(m) if m.entry == 9));
    }
}
