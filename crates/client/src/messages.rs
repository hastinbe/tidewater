//! Messages, notices, the status bar and the event history, from `docs/specs/ui/messages.md`: the 12-line message
//! list and the notice line over the map view (real-time expiry, emptied in place), how a message is split to the
//! 428-pixel width, the status bar's text, cost items and fixed prompts, and the Enhanced event history that the space
//! key steps through. The attack mark on the minimap is in [`crate::minimap`]. No Bevy types here.
//!
//! Texts are string-table entries numbered from 1: [`Text::Stat`] for `Rez\stat_txt.tbl`, [`Text::Net`] for
//! `Rez\network.tbl`.
//!
//! Defaults for the spec's open questions:
//! - Other failure messages (mana, return goods, affordability: stat_txt 435, 436, 438 to 441) go to the notice line.
//! - Pause notices fill in the player's name only: the entry is cut before its first placeholder other than `%s`
//!   (and trailing spaces and opening brackets go too), in every game that can pause.
//! - A space is the font's own space width.
//! - The message lines' second text setting (probably a shadow) is drawn as the font's own shadow shade.
//!
//! Our readings: a refused worker placement on arrival (`Event::CantPlace`) shows stat_txt 443 ("blocked"); a
//! trained unit with no room to come out (`Event::NoRoom`) shows network entry 67, the blocked-exit message of
//! `movement/placement.md`. A finished research has no text (its work-complete sound carries the subtitle). The
//! end-of-game lines are our own wording until the victory screen has a spec.

use tidewater_sim::cheats::Notice as CheatNotice;
use tidewater_sim::economy::{Event, Shortage};
use tidewater_sim::victory::Outcome;

/// List lines, newest last.
pub const LIST_LINES: usize = 12;
/// Line height in pixels.
pub const LINE_H: i32 = 20;
/// Top of list line 1 and of the notice line, on the 640 × 480 screen.
pub const FIRST_TOP: i32 = 194;
pub const NOTICE_TOP: i32 = 434;
/// Text left edge and clip edge (428 pixels of text).
pub const LEFT: i32 = 186;
pub const CLIP_RIGHT: i32 = 614;
// Only the chat line will use it (none yet); reachable from the library's `Core`, so not dead code.
pub const WIDTH: u32 = 428;
/// Characters a line holds.
pub const MAX_CHARS: usize = 99;
/// Time to live of every message and notice, in milliseconds.
pub const TTL: u64 = 7000;
/// Text colours: chat, and game and network notices.
#[cfg_attr(not(test), expect(dead_code, reason = "no chat line or network game yet"))]
pub const CHAT_COLOUR: u8 = 2;
pub const NOTICE_COLOUR: u8 = 4;

/// A string-table entry, numbered from 1, or our own text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    Stat(u16),
    Net(u16),
    Own(String),
}

/// Entries of `Rez\network.tbl`.
#[allow(
    dead_code,
    reason = "the player-gone, chat and unit-creation entries wait for network games and chat"
)]
pub mod net {
    pub const PLAYER_GONE: u16 = 63;
    pub const PLAYER_LEFT: u16 = 64;
    pub const PLAYER_DROPPED: u16 = 65;
    pub const CHAT_SENT: u16 = 66;
    pub const EXIT_BLOCKED: u16 = 67;
    pub const CANT_CREATE: u16 = 69;
    pub const SELECT_TARGET: u16 = 70;
    pub const SELECT_LOCATION: u16 = 71;
    pub const CHEAT_B: u16 = 72;
    pub const NEED_LUMBER: u16 = 73;
    pub const NEED_GOLD: u16 = 74;
    pub const CHEAT_ON: u16 = 80;
    pub const CHEAT_OFF: u16 = 81;
    pub const PAUSED: u16 = 86;
    pub const RESUMED: u16 = 87;
}

/// Entries of `Rez\stat_txt.tbl`.
pub mod stat {
    pub const BLOCKED: u16 = 443;
}

/// The affordability message of a shortage (`economy/build-train.md`).
pub fn shortage_entry(s: Shortage) -> u16 {
    match s {
        Shortage::Food => 438,
        Shortage::Gold => 439,
        Shortage::Lumber => 440,
        Shortage::Oil => 441,
    }
}

/// The notice a sim event gives the local player `human`, if any.
pub fn event_notice(e: &Event, human: u8) -> Option<Text> {
    match *e {
        Event::CantAfford { player, shortage } if player == human => Some(Text::Stat(shortage_entry(shortage))),
        Event::NoRoom { player, .. } if player == human => Some(Text::Net(net::EXIT_BLOCKED)),
        Event::CantPlace { player, .. } if player == human => Some(Text::Stat(stat::BLOCKED)),
        Event::CantRepair { player, shortage } if player == human => Some(Text::Net(match shortage {
            Shortage::Lumber => net::NEED_LUMBER,
            _ => net::NEED_GOLD,
        })),
        Event::GameOver { player, outcome } => Some(Text::Own(match outcome {
            Outcome::Victory if player == human => "Victory!".to_string(),
            Outcome::Defeat if player == human => "You have been defeated".to_string(),
            Outcome::Defeat => format!("Player {} has been defeated", player + 1),
            _ => "The game has ended".to_string(),
        })),
        // Cheats spec, step 8 and the one-shot notices. The multiplayer sender's name in front is not shown yet: this
        // client has no network games.
        Event::Cheat { notice, .. } => Some(match notice {
            CheatNotice::On => Text::Net(net::CHEAT_ON),
            CheatNotice::Off => Text::Net(net::CHEAT_OFF),
            CheatNotice::B => Text::Net(net::CHEAT_B),
            n => Text::Own(n.text().unwrap_or_default().to_string()),
        }),
        _ => None,
    }
}

/// A table entry with the player's name filled in (the pause notices' default): `%s` takes the name, and the text
/// is cut before any other placeholder.
pub fn fill_name(template: &str, name: &str) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        if rest[i + 1..].starts_with('s') {
            out.push_str(name);
            rest = &rest[i + 2..];
        } else {
            let cut = out.trim_end_matches([' ', '(', '[']);
            return cut.to_string();
        }
    }
    out.push_str(rest);
    out
}

/// A chat line from a player as the list shows it.
#[cfg_attr(not(test), expect(dead_code, reason = "no chat line or network game yet"))]
pub fn chat_line(name: &str, text: &str) -> String {
    format!("{name}: {text}")
}

// --- the message area ----------------------------------------------------------------------------------------------

/// One line of the message area.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub colour: u8,
    /// Milliseconds by the system clock.
    pub expires: u64,
}

/// Splits `text` into pieces that each fit [`WIDTH`] pixels, by `width` (a string's width in the font).
// Only the chat line will use it (none yet); reachable from the library's `Core`, so not dead code.
pub fn split(text: &str, width: &dyn Fn(&str) -> u32) -> Vec<String> {
    let mut pieces = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        let mut has_word = false;
        // Words and the spaces before them.
        let mut rest = para;
        while !rest.is_empty() {
            let spaces = rest.len() - rest.trim_start_matches(' ').len();
            cur.push_str(&rest[..spaces]);
            rest = &rest[spaces..];
            if rest.is_empty() {
                break;
            }
            let end = rest.find(' ').unwrap_or(rest.len());
            let mut word = &rest[..end];
            rest = &rest[end..];
            if has_word && width(&format!("{cur}{word}")) > WIDTH {
                pieces.push(std::mem::take(&mut cur));
                has_word = false;
            }
            // Long word: break before the first character that would end past the width.
            while !has_word && width(&format!("{cur}{word}")) > WIDTH {
                let mut cut = 0;
                for (i, c) in word.char_indices() {
                    let next = i + c.len_utf8();
                    if width(&format!("{cur}{}", &word[..next])) > WIDTH {
                        break;
                    }
                    cut = next;
                }
                if cut == 0 && cur.is_empty() {
                    // Not even one character fits: take it anyway.
                    cut = word.chars().next().map_or(0, char::len_utf8);
                }
                cur.push_str(&word[..cut]);
                pieces.push(std::mem::take(&mut cur));
                word = &word[cut..];
                if word.is_empty() {
                    break;
                }
            }
            if !word.is_empty() {
                cur.push_str(word);
                has_word = true;
            }
        }
        if !cur.is_empty() || pieces.is_empty() || has_word {
            pieces.push(cur);
        }
    }
    pieces
        .into_iter()
        .map(|p| p.chars().take(MAX_CHARS).collect())
        .collect()
}

/// The message list and the notice line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Messages {
    /// List lines 1 to 12; 12 is the newest.
    pub list: [Line; LIST_LINES],
    pub notice: Line,
}

impl Messages {
    /// Empties, in place, every line whose expiry time has come.
    pub fn expire(&mut self, now: u64) {
        for l in self.list.iter_mut().chain(std::iter::once(&mut self.notice)) {
            if !l.text.is_empty() && now >= l.expires {
                l.text.clear();
            }
        }
    }

    /// Adds a message to the list: split to the width, each piece scrolling the list up by one.
    // Only the chat line will use it (none yet); reachable from the library's `Core`, so not dead code.
    pub fn add(&mut self, text: &str, colour: u8, ttl: u64, now: u64, width: &dyn Fn(&str) -> u32) {
        for piece in split(text, width) {
            self.list.rotate_left(1);
            self.list[LIST_LINES - 1] = Line {
                text: piece,
                colour,
                expires: now + ttl,
            };
        }
    }

    /// Shows a notice: it replaces the notice line, in colour 4; a time to live under 1000 ms becomes 7000.
    pub fn notice(&mut self, text: &str, ttl: u64, now: u64) {
        let ttl = if ttl < 1000 { TTL } else { ttl };
        self.notice = Line {
            text: text.chars().take(MAX_CHARS).collect(),
            colour: NOTICE_COLOUR,
            expires: now + ttl,
        };
    }

    /// The lines to draw: top y on the 640 × 480 screen, and the line.
    pub fn lines(&self) -> impl Iterator<Item = (i32, &Line)> {
        self.list
            .iter()
            .enumerate()
            .map(|(i, l)| (FIRST_TOP + LINE_H * i as i32, l))
            .chain(std::iter::once((NOTICE_TOP, &self.notice)))
            .filter(|(_, l)| !l.text.is_empty())
    }
}

// --- the status bar ------------------------------------------------------------------------------------------------

/// The status bar's cost items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cost {
    Gold,
    Lumber,
    Oil,
    Mana,
}

impl Cost {
    pub const ALL: [Cost; 4] = [Cost::Gold, Cost::Lumber, Cost::Oil, Cost::Mana];

    /// The item's x range from the bar's left edge, and its frame of `Art\icons.grp`.
    pub fn place(self) -> ((i32, i32), usize) {
        match self {
            Cost::Gold => ((290, 341), 0),
            Cost::Lumber => ((342, 393), 1),
            Cost::Oil => ((394, 445), 2),
            Cost::Mana => ((290, 341), 3),
        }
    }
}

/// Where the text starts, from the bar's left edge.
pub const STATUS_TEXT_X: i32 = 2;

/// The status bar's text and cost items.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusBar {
    pub text: String,
    /// Gold, lumber, oil and mana.
    pub values: [u32; 4],
    /// Whether the cost items can show (the text was set with a cost kind other than 0).
    pub costs: bool,
    /// The fixed prompt, while target or placement mode is on.
    pub prompt: Option<String>,
    /// The chat line's text while it is open (`crate::chat`): the bar shows it instead, and no cost items.
    pub chat: Option<String>,
}

impl StatusBar {
    fn locked(&self, demo: bool) -> bool {
        demo || self.prompt.is_some()
    }

    /// The pointer is over a command button: its tooltip, cost kind, gold, lumber and oil costs, and mana cost.
    pub fn button(&mut self, text: &str, kind: u8, cost: [u32; 3], mana: u32, demo: bool) {
        if self.locked(demo) {
            return;
        }
        self.text = text.to_string();
        self.costs = kind != 0;
        match kind {
            1 => self.values[..3].copy_from_slice(&cost),
            2 => self.values = [cost[0], cost[1], cost[2], 0],
            3 => self.values = [0, 0, 0, mana],
            _ => {}
        }
    }

    /// The pointer is over a unit icon in the information panel: its name, no costs.
    pub fn name(&mut self, text: &str, demo: bool) {
        if self.locked(demo) {
            return;
        }
        self.text = text.to_string();
        self.costs = false;
    }

    /// The pointer is over nothing: the text is cleared and all four items set to 0.
    pub fn clear(&mut self, demo: bool) {
        if self.locked(demo) {
            return;
        }
        self.text.clear();
        self.values = [0; 4];
    }

    /// Sets (a mode starts) or clears (it ends) the fixed prompt; either hides the cost items.
    pub fn set_prompt(&mut self, prompt: Option<String>) {
        if prompt == self.prompt {
            return;
        }
        self.text = prompt.clone().unwrap_or_default();
        self.prompt = prompt;
        self.costs = false;
    }

    /// The cost items shown, with their values.
    pub fn shown(&self) -> Vec<(Cost, u32)> {
        if !self.costs || self.chat.is_some() {
            return Vec::new();
        }
        Cost::ALL
            .into_iter()
            .zip(self.values)
            .filter(|&(_, v)| v != 0)
            .collect()
    }

    /// The right edge of the text area, from the bar's left edge.
    pub fn text_right(&self) -> i32 {
        if self.costs { 289 } else { 341 }
    }
}

// --- the event history ---------------------------------------------------------------------------------------------

/// Entries the history keeps.
pub const HISTORY: usize = 8;
/// Presses of the space key closer than this go on to older entries.
pub const SPACE_MS: u64 = 1000;

/// The Enhanced rules' event history: view positions, newest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct History {
    pub entries: [(i32, i32); HISTORY],
    /// The entry the next space press goes to, from 0.
    pub step: usize,
    pub stamp: u64,
}

impl History {
    /// At the start of a game every entry is the starting view.
    pub fn new(start: (i32, i32)) -> Self {
        Self {
            entries: [start; HISTORY],
            step: 0,
            stamp: 0,
        }
    }

    /// A new event at `tile`.
    pub fn push(&mut self, tile: (i32, i32), now: u64) {
        self.entries.rotate_right(1);
        self.entries[0] = ((tile.0 - 7).max(0), (tile.1 - 7).max(0));
        self.step = 0;
        self.stamp = now;
    }

    /// The space key: the view position to go to (to be clamped to the view range), under Enhanced rules only.
    pub fn space(&mut self, now: u64, enhanced: bool) -> Option<(i32, i32)> {
        if !enhanced {
            return None;
        }
        if now.saturating_sub(self.stamp) > SPACE_MS {
            self.step = 0;
        }
        let v = self.entries[self.step];
        self.step = (self.step + 1) % HISTORY;
        self.stamp = now;
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character 10 pixels wide.
    fn w10(s: &str) -> u32 {
        s.chars().count() as u32 * 10
    }

    fn texts(m: &Messages) -> Vec<&str> {
        m.list.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn one_message_expires_at_7000() {
        let mut m = Messages::default();
        m.add("A", NOTICE_COLOUR, TTL, 0, &w10);
        assert_eq!(m.list[11].text, "A");
        assert_eq!(m.lines().next().map(|(y, _)| y), Some(414));
        m.expire(6999);
        assert_eq!(m.list[11].text, "A");
        m.expire(7000);
        assert_eq!(m.list[11].text, "");
    }

    #[test]
    fn lines_empty_in_place() {
        let mut m = Messages::default();
        m.add("A", 4, TTL, 0, &w10);
        m.add("B", 4, TTL, 2000, &w10);
        assert_eq!((m.list[10].text.as_str(), m.list[11].text.as_str()), ("A", "B"));
        assert_eq!((m.list[10].expires, m.list[11].expires), (7000, 9000));
        m.expire(7000);
        assert_eq!((m.list[10].text.as_str(), m.list[11].text.as_str()), ("", "B"));
        m.add("C", 4, TTL, 8000, &w10);
        assert_eq!(&texts(&m)[9..], ["", "B", "C"]);
    }

    #[test]
    fn long_messages_split() {
        // 60 short words of 10 px ("ab" + space = 30 px per word): 600 px or so.
        let words = vec!["ab"; 20].join(" ");
        assert_eq!(w10(&words), 590);
        let mut m = Messages::default();
        m.add(&words, 4, TTL, 0, &w10);
        let (a, b) = (&m.list[10], &m.list[11]);
        // 14 words with their spaces = 14 * 30 = 420 px; the 15th would end at 440.
        assert_eq!(a.text, format!("{} ", vec!["ab"; 14].join(" ")));
        assert_eq!(b.text, ["ab"; 6].join(" "));
        assert_eq!(a.expires, b.expires);
        // One 500-pixel word: broken at 428 pixels (42 characters).
        let pieces = split(&"x".repeat(50), &w10);
        assert_eq!(pieces, ["x".repeat(42), "x".repeat(8)]);
        // A line feed always ends a piece; a piece is cut to 99 characters.
        assert_eq!(split("a\nb", &w10), ["a", "b"]);
        assert_eq!(split(&"y".repeat(150), &|_| 0)[0].len(), 99);
    }

    #[test]
    fn thirteen_messages() {
        let mut m = Messages::default();
        for i in 0..13 {
            m.add(&format!("m{i}"), 4, TTL, 0, &w10);
        }
        assert_eq!(m.list[0].text, "m1");
        assert_eq!(m.list[11].text, "m12");
    }

    #[test]
    fn notices() {
        let mut m = Messages::default();
        m.notice("You cannot build there.", 0, 0);
        assert_eq!(m.lines().next().map(|(y, l)| (y, l.colour)), Some((434, 4)));
        m.notice("second", 0, 3000);
        assert_eq!((m.notice.text.as_str(), m.notice.expires), ("second", 10_000));
        m.expire(9999);
        assert_eq!(m.notice.text, "second");
        m.expire(10_000);
        assert_eq!(m.notice.text, "");
        // Real time: a 10-second pause right after a notice outlasts it.
        m.notice("x", 500, 0);
        m.expire(10_000);
        assert!(m.lines().next().is_none());
        m.notice("y", 2000, 0);
        assert_eq!(m.notice.expires, 2000);
    }

    #[test]
    fn chat() {
        let mut m = Messages::default();
        m.add(&chat_line("Player 2", "hi"), CHAT_COLOUR, TTL, 0, &w10);
        assert_eq!((m.list[11].text.as_str(), m.list[11].colour), ("Player 2: hi", 2));
    }

    #[test]
    fn event_texts() {
        let e = Event::CantAfford {
            player: 0,
            shortage: Shortage::Gold,
        };
        assert_eq!(event_notice(&e, 0), Some(Text::Stat(439)));
        assert_eq!(event_notice(&e, 1), None);
        let e = Event::CantRepair {
            player: 0,
            shortage: Shortage::Lumber,
        };
        assert_eq!(event_notice(&e, 0), Some(Text::Net(73)));
        assert_eq!(
            event_notice(&Event::NoRoom { player: 0, kind: 0 }, 0),
            Some(Text::Net(67))
        );
        let cheat = |notice| Event::Cheat { sender: 0, notice };
        assert_eq!(event_notice(&cheat(CheatNotice::On), 0), Some(Text::Net(80)));
        assert_eq!(event_notice(&cheat(CheatNotice::Off), 0), Some(Text::Net(81)));
        assert_eq!(event_notice(&cheat(CheatNotice::B), 0), Some(Text::Net(72)));
        assert!(matches!(event_notice(&cheat(CheatNotice::A), 0), Some(Text::Own(_))));
        assert_eq!(
            fill_name("%s paused the game. (%d timeouts remaining)", "Ann"),
            "Ann paused the game."
        );
        assert_eq!(fill_name("%s resumed the game.", "Ann"), "Ann resumed the game.");
    }

    #[test]
    fn status_bar_costs() {
        let mut s = StatusBar::default();
        s.button("Train Footman", 1, [600, 0, 0], 0, false);
        assert_eq!(s.text, "Train Footman");
        assert_eq!(s.shown(), [(Cost::Gold, 600)]);
        assert_eq!(s.text_right(), 289);
        s.button("Holy Vision", 3, [0; 3], 70, false);
        assert_eq!(s.shown(), [(Cost::Mana, 70)]);
        // Straight from a spell button to a kind-1 button: the mana cost stays.
        s.button("Train Footman", 1, [600, 0, 0], 0, false);
        assert_eq!(s.shown(), [(Cost::Gold, 600), (Cost::Mana, 70)]);
        s.clear(false);
        assert_eq!((s.text.as_str(), s.shown()), ("", vec![]));
        s.button("Upgrade", 2, [500, 100, 0], 0, false);
        assert_eq!(s.shown(), [(Cost::Gold, 500), (Cost::Lumber, 100)]);
        s.button("Stop", 0, [0; 3], 0, false);
        assert_eq!((s.shown(), s.text_right()), (vec![], 341));
        s.name("Footman", false);
        assert_eq!((s.text.as_str(), s.shown()), ("Footman", vec![]));
        assert_eq!(Cost::Mana.place(), ((290, 341), 3));
        // Demo playback leaves the text.
        s.button("Train", 1, [1, 0, 0], 0, true);
        assert_eq!(s.text, "Footman");
    }

    #[test]
    fn fixed_prompts() {
        let mut s = StatusBar::default();
        s.button("Train Footman", 1, [600, 0, 0], 0, false);
        s.set_prompt(Some("Select Target".into()));
        assert_eq!((s.text.as_str(), s.shown()), ("Select Target", vec![]));
        s.button("Attack", 0, [0; 3], 0, false);
        s.clear(false);
        assert_eq!(s.text, "Select Target");
        s.set_prompt(None);
        assert_eq!((s.text.as_str(), s.shown()), ("", vec![]));
        s.button("Attack", 0, [0; 3], 0, false);
        assert_eq!(s.text, "Attack");
    }

    #[test]
    fn event_history() {
        let mut h = History::new((10, 10));
        assert_eq!(h.entries, [(10, 10); 8]);
        h.push((40, 30), 5000);
        assert_eq!(h.space(5000, true), Some((33, 23)));
        h.push((3, 50), 6000);
        assert_eq!(h.entries[0], (0, 43));
        let mut h = History::new((0, 0));
        for (i, t) in [(20, 20), (30, 30), (40, 40)].into_iter().enumerate() {
            h.push(t, i as u64);
        }
        let got: Vec<_> = [10_000, 10_500, 11_000, 13_000]
            .into_iter()
            .map(|t| h.space(t, true).unwrap())
            .collect();
        assert_eq!(got, [(33, 33), (23, 23), (13, 13), (33, 33)]);
        assert_eq!(h.space(20_000, false), None, "Classic rules");
    }
}
