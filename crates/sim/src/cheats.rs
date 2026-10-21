//! Cheat codes: how a typed line is recognised, the cheat command, and the immediate effects. Spec:
//! `docs/specs/game/cheats.md`.
//!
//! - **Recognition** ([`interpret`], free of the world): a line is checked against the phrase table in order by its
//!   32-bit [`check_value`] (case and space/underscore don't matter, so many lines besides the published phrases
//!   match), then the developer [`Word`]s, then mission select ([`mission_select`]). The result says what to send:
//!   the new [`Flags`] for a [`Command::Cheat`](crate::Command::Cheat), plus any local-only change.
//! - **The command** ([`World::cheat_command`], run by `World::step`): sets the flags and runs "Effects when the flags
//!   change" in the spec's order. The flags live in [`Cheats`] and are hashed once any is on; the lasting effects read
//!   the matching [`Rules`](crate::Rules) fields, which the command keeps in step with the flags.
//! - **Lasting effects wired here:** build speed (existing), reveal map (existing), all spells (existing mana top-up),
//!   instant victory, instant defeat and no victory (existing), lumber (the chop counter jumps to 50, in `harvest`),
//!   invulnerability and damage (`battle`, `World::attack_values`), and the single-player fog-off of `showpath`.
//!
//! Gap choices (the spec leaves them open):
//! - Flag bits are our own layout ([`flag`]); only "less than" matters to the spec (entry 81 versus 80), and every
//!   code changes one bit, so any layout gives the same notices.
//! - The finale code is listed as a toggle but acts at once like a one-shot (step 7): its flag is cleared after the
//!   command, so it never fires twice.
//! - Notices A and C, and the three developer-word notices, use our own short texts ([`Notice`], [`Word::notice`]).
//! - "All spells" sets every spell known and allowed, every research allowed and the spells-being-researched word to
//!   zero, for every human-controlled player (`controller` 5). The research already running in a building is left.
//! - Invulnerability: attackers of human-controlled players keep their basic damage (upgrades, then bloodlust) and deal
//!   a flat piercing 200 that bloodlust doesn't double; every other attacker deals 0 and 0.
//! - Command-line words ([`from_arguments`]) only set persistent flags: one-shot codes are ignored there because no
//!   command would ever run them, and word codes without a flag are returned for the caller to apply.
//! - Mission select applies the number rule as `atoi` would: spaces, then decimal digits; no digits reads as 0, which
//!   is out of range but still uses the line up.
//! - Cheats disabled (a multiplayer lobby option, [`Rules::cheats_disabled`](crate::Rules)) defaults to off.
//! - The "cheated" mark ([`Cheats::used`]) is per machine (the local player) and is left out of the hash.
//!
//! Not yet here: the music change for disco ([`Event::Disco`] is raised; the client's music player is not built), the
//! screen switch for mission select and the finale ([`Event::Finale`], [`MissionSelect`]; campaign flow lives in the
//! client), the sync-check, demo-speed and profiling settings ([`Word`] only reports them), the colour-cycling
//! preference, the "skip startup videos" and "skip act screen" behaviours (their flags are modelled and carried across
//! games), the rank entry 39 reading of [`Cheats::used`], the multiplayer sender's name in front of a notice, and
//! the mission-select cheat command being carried out before the mission changes (an open question in the spec).

use crate::ai::controller;
use crate::economy::Event;
use crate::world::World;

/// The cheat flags as a bit set (our own layout).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(pub u32);

/// Flag bits.
pub mod flag {
    pub const MISSION_SELECT: u32 = 1 << 0;
    pub const RESOURCES: u32 = 1 << 1;
    pub const OIL: u32 = 1 << 2;
    pub const ALL_SPELLS: u32 = 1 << 3;
    pub const ALL_UPGRADES: u32 = 1 << 4;
    pub const REVEAL_MAP: u32 = 1 << 5;
    pub const BUILD_SPEED: u32 = 1 << 6;
    pub const INVULNERABLE: u32 = 1 << 7;
    pub const LUMBER: u32 = 1 << 8;
    pub const NOTICE_A: u32 = 1 << 9;
    pub const NOTICE_B: u32 = 1 << 10;
    pub const NOTICE_C: u32 = 1 << 11;
    pub const INSTANT_VICTORY: u32 = 1 << 12;
    pub const INSTANT_DEFEAT: u32 = 1 << 13;
    pub const FINALE: u32 = 1 << 14;
    pub const NO_VICTORY: u32 = 1 << 15;
    pub const DISCO: u32 = 1 << 16;
    /// The `showpath` debug flag.
    pub const SHOW_PATH: u32 = 1 << 17;
    /// "Skip the startup videos" (the `title` word).
    pub const SKIP_TITLE: u32 = 1 << 18;
    /// "Skip the act screen, briefing and score screens" (the `noglues` word).
    pub const NO_GLUES: u32 = 1 << 19;

    /// Flags that act once: they clear as soon as the command has run.
    pub const ONE_SHOT: u32 = RESOURCES | OIL | NOTICE_A | NOTICE_B | NOTICE_C | FINALE;
    /// Flags that survive into the next game ("Flags across games").
    pub const KEPT: u32 = MISSION_SELECT | SKIP_TITLE | NO_GLUES;
}

impl Flags {
    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    /// The flags a new game starts with: everything is cleared except mission select and the two skip settings.
    pub fn for_new_game(self) -> Flags {
        Flags(self.0 & flag::KEPT)
    }

    /// The flags of attract-demo playback: the reveal-map flag is set.
    pub fn for_demo(self) -> Flags {
        Flags(self.for_new_game().0 | flag::REVEAL_MAP)
    }
}

/// The 32-bit check value of a line ("Check value"). A space counts as an underscore and A to Z as a to z.
pub fn check_value(line: &str) -> u32 {
    let mut h = 0u32;
    for &b in line.as_bytes() {
        let c = match b {
            b' ' => b'_',
            b'A'..=b'Z' => b + 32,
            _ => b,
        };
        h = (h << 2) ^ c as u32;
        if h >= 1 << 20 {
            h = (h & 0xf_ffff) ^ (h >> 12);
        }
    }
    ((line.len() as u32 & 0xff) << 24) + h
}

/// Whether a code toggles its flag or acts once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Toggle,
    OneShot,
}

/// One row of the phrase table.
#[derive(Clone, Copy, Debug)]
pub struct Phrase {
    /// A line that matches ("" for the code with no published phrase and no letters-only line).
    pub text: &'static str,
    pub check: u32,
    pub flag: u32,
    pub kind: Kind,
    /// Only recognised in single-player games.
    pub single_player: bool,
}

const fn row(text: &'static str, check: u32, flag: u32, kind: Kind, single_player: bool) -> Phrase {
    Phrase {
        text,
        check,
        flag,
        kind,
        single_player,
    }
}

/// The phrase codes in checking order ("Phrase codes"). Notice B has no published phrase; "183" matches it.
pub const PHRASES: [Phrase; 17] = {
    use Kind::*;
    [
        row("tigerlily", 0x0909_CE59, flag::MISSION_SELECT, Toggle, false),
        row("glittering prizes", 0x1107_9E4E, flag::RESOURCES, OneShot, false),
        row("valdez", 0x0601_A5AE, flag::OIL, OneShot, false),
        row(
            "every little thing she does",
            0x1B09_1606,
            flag::ALL_SPELLS,
            Toggle,
            false,
        ),
        row("deck me out", 0x0B09_B983, flag::ALL_UPGRADES, Toggle, false),
        row("on screen", 0x0900_C02A, flag::REVEAL_MAP, Toggle, false),
        row("make it so", 0x0A08_8C5B, flag::BUILD_SPEED, Toggle, false),
        row(
            "it is a good day to die",
            0x1708_F242,
            flag::INVULNERABLE,
            Toggle,
            false,
        ),
        row("hatchet", 0x0707_6FA0, flag::LUMBER, Toggle, false),
        row("day", 0x0300_07BD, flag::NOTICE_A, OneShot, false),
        row("183", 0x0300_03C3, flag::NOTICE_B, OneShot, false),
        row("ucla", 0x0400_1AA1, flag::NOTICE_C, OneShot, false),
        row("unite the clans", 0x0F0D_E3CB, flag::INSTANT_VICTORY, Toggle, true),
        row("you pitiful worm", 0x1006_3796, flag::INSTANT_DEFEAT, Toggle, true),
        row("there can be only one", 0x150F_F32E, flag::FINALE, OneShot, true),
        row("never a winner", 0x0E0B_5902, flag::NO_VICTORY, Toggle, true),
        row("disco", 0x0500_7893, flag::DISCO, Toggle, false),
    ]
};

/// The developer words ("Word codes").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    /// Toggles a network sync-check setting (this machine only).
    AllowSync,
    /// Toggles [`flag::SKIP_TITLE`].
    Title,
    /// Toggles [`flag::NO_GLUES`].
    NoGlues,
    /// Toggles [`flag::SHOW_PATH`]; turning it on in single player also turns fog of war off.
    ShowPath,
    /// Toggles a demo-speed setting (this machine only).
    FastDemo,
    /// Toggles a network profiling setting (this machine only).
    NetProf,
    /// Debug display, not a command.
    ShowInfo,
    /// Debug display, not a command.
    ShowMemory,
    /// The colour-cycling preference, not a command.
    NoPalette,
}

/// The words in checking order.
pub const WORDS: [(&str, Word); 9] = [
    ("allowsync", Word::AllowSync),
    ("title", Word::Title),
    ("noglues", Word::NoGlues),
    ("showpath", Word::ShowPath),
    ("fastdemo", Word::FastDemo),
    ("netprof", Word::NetProf),
    ("showinfo", Word::ShowInfo),
    ("showmemory", Word::ShowMemory),
    ("nopalette", Word::NoPalette),
];

impl Word {
    /// The word a line spells, ignoring letter case.
    pub fn parse(line: &str) -> Option<Word> {
        WORDS
            .iter()
            .find(|(w, _)| line.eq_ignore_ascii_case(w))
            .map(|&(_, w)| w)
    }

    /// The flag the word toggles, if any.
    pub fn flag(self) -> Option<u32> {
        match self {
            Word::Title => Some(flag::SKIP_TITLE),
            Word::NoGlues => Some(flag::NO_GLUES),
            Word::ShowPath => Some(flag::SHOW_PATH),
            _ => None,
        }
    }

    /// Whether the line becomes a cheat command (the first six); the last three act at once and send nothing.
    pub fn is_command(self) -> bool {
        !matches!(self, Word::ShowInfo | Word::ShowMemory | Word::NoPalette)
    }

    /// The fixed notice of a word that isn't a command (our own text; the spec leaves it open).
    pub fn notice(self) -> &'static str {
        match self {
            Word::ShowInfo => "Info display toggled",
            Word::ShowMemory => "Memory display toggled",
            Word::NoPalette => "Colour cycling toggled",
            _ => "",
        }
    }
}

/// A race and campaign: a mission-select prefix.
const PREFIXES: [(&str, u8, u8, bool); 4] = [
    ("human", 0, 14, false),
    ("orc", 1, 14, false),
    ("xhuman", 0, 12, true),
    ("xorc", 1, 12, true),
];

/// A mission chosen by mission select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionSelect {
    /// The campaign table entry: 2n - 2 + race (+ 28 for *Beyond the Dark Portal*).
    pub entry: u8,
    /// 0 human, 1 orc.
    pub race: u8,
    /// *Beyond the Dark Portal*.
    pub expansion: bool,
}

/// What mission select makes of a line: `None` if no prefix matches (the line is not used up); `Some(None)` if a prefix
/// matches but the number is out of range (the line is used up); `Some(Some(m))` for a mission.
pub fn mission_select(line: &str) -> Option<Option<MissionSelect>> {
    let lower = line.to_ascii_lowercase();
    let &(prefix, race, max, expansion) = PREFIXES.iter().find(|(p, ..)| lower.starts_with(p))?;
    let rest = lower[prefix.len()..].trim_start_matches(' ');
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n: u32 = digits.parse().unwrap_or(0);
    if n == 0 || n > max as u32 {
        return Some(None);
    }
    let entry = crate::victory::mission_entry(expansion, race == 1, n as u8);
    Some(Some(MissionSelect { entry, race, expansion }))
}

/// What a typed line needs to be checked.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    /// The flags as they stand on the typing machine.
    pub flags: Flags,
    pub multiplayer: bool,
    /// A custom or multiplayer game whose lobby options disable cheats.
    pub cheats_disabled: bool,
    pub enhanced: bool,
}

/// What a recognised cheat line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Typed {
    /// The flags to send in the cheat command.
    pub flags: Flags,
    /// A setting that changes at once on the typing machine only (sync check, demo speed, profiling).
    pub local: Option<Word>,
    /// Mission select: end this game and start the mission. The command is still sent.
    pub mission: Option<MissionSelect>,
}

/// The result of checking a typed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// Not a cheat: chat in multiplayer, dropped in single player.
    NotCheat,
    /// Send a cheat command; the line is not chat.
    Cheat(Typed),
    /// A word that changes a local setting and shows its notice. Not chat, no command.
    Developer(Word),
}

/// Checks a typed line ("Entering a cheat during a game"): phrases in order, then words, then mission select.
pub fn interpret(line: &str, cx: &Context) -> Line {
    if cx.cheats_disabled {
        return Line::NotCheat;
    }
    let check = check_value(line);
    let found = PHRASES
        .iter()
        .find(|p| p.check == check && (!p.single_player || !cx.multiplayer));
    if let Some(p) = found {
        let flags = match p.kind {
            Kind::Toggle => cx.flags.0 ^ p.flag,
            Kind::OneShot => cx.flags.0 | p.flag,
        };
        return Line::Cheat(Typed {
            flags: Flags(flags),
            local: None,
            mission: None,
        });
    }
    if let Some(word) = Word::parse(line) {
        if !word.is_command() {
            return Line::Developer(word);
        }
        return match word.flag() {
            Some(f) => Line::Cheat(Typed {
                flags: Flags(cx.flags.0 ^ f),
                local: None,
                mission: None,
            }),
            None => Line::Cheat(Typed {
                flags: cx.flags,
                local: Some(word),
                mission: None,
            }),
        };
    }
    if cx.flags.has(flag::MISSION_SELECT)
        && !(cx.enhanced && cx.multiplayer)
        && let Some(m) = mission_select(line)
    {
        return Line::Cheat(Typed {
            flags: cx.flags,
            local: None,
            mission: m,
        });
    }
    Line::NotCheat
}

/// Checks the program's arguments ("Command-line arguments"): the flags they set (persistent ones only), and the words
/// with no flag for the caller to apply. Phrases are taken as for single player.
pub fn from_arguments<'a>(args: impl IntoIterator<Item = &'a str>) -> (Flags, Vec<Word>) {
    let mut flags = 0u32;
    let mut words = Vec::new();
    for a in args {
        let check = check_value(a);
        if let Some(p) = PHRASES.iter().find(|p| p.check == check) {
            if p.kind == Kind::Toggle {
                flags ^= p.flag;
            }
        } else if let Some(w) = Word::parse(a) {
            match w.flag() {
                Some(f) => flags ^= f,
                None => words.push(w),
            }
        }
    }
    (Flags(flags), words)
}

/// A notice a cheat command shows (`Event::Cheat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// `network.tbl` entry 80.
    On,
    /// `network.tbl` entry 81.
    Off,
    /// Notice A: our own short text.
    A,
    /// `network.tbl` entry 72.
    B,
    /// Notice C: our own short text.
    C,
}

impl Notice {
    /// The text of notices A and C (our own).
    pub fn text(self) -> Option<&'static str> {
        match self {
            Notice::A => Some("Cheat enabled"),
            Notice::C => Some("Go Bruins"),
            _ => None,
        }
    }
}

/// The cheat state of a game.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cheats {
    pub flags: Flags,
    /// The mission counts as one in which the local player cheated (rank entry 39, campaign-flow spec). Per machine, so
    /// not hashed.
    pub used: bool,
}

/// All spell and research bits.
const ALL_BITS: u32 = !0;
/// Upgrade groups 0 to 6 are levelled (to level 2); 7 to 10 are one-step (to level 1).
const LEVELLED_GROUPS: usize = 7;
const RESOURCE_GIFT: [u32; 3] = [10_000, 5_000, 5_000];
const OIL_GIFT: u32 = 5_000;
/// Piercing damage dealt under the invulnerability cheat by human-controlled attackers, set after upgrades and not
/// doubled by bloodlust (their basic damage is worked out as usual).
pub const CHEAT_PIERCING: u32 = 200;

impl World {
    /// Sets the flags at the start of a game or from the program's arguments: no effects run, no notices show. Pass
    /// [`Flags::for_new_game`] of the previous game's flags.
    pub fn start_cheats(&mut self, flags: Flags) {
        let old = self.cheats.flags;
        self.cheats.flags = flags;
        self.sync_cheat_rules(old, flags);
    }

    /// Carries out a cheat command from `player` carrying the new flags ("Carrying out a cheat command").
    pub fn cheat_command(&mut self, player: u8, new: Flags) {
        if player == self.vision.local && new.0 != 0 {
            self.cheats.used = true;
        }
        if self.rules.multiplayer && self.rules.cheats_disabled {
            // The generator is advanced once even though nothing happens.
            self.rng.draw();
            return;
        }
        let old = self.cheats.flags;
        self.cheats.flags = new;
        self.sync_cheat_rules(old, new);
        let changed = |bit: u32| (old.0 ^ new.0) & bit != 0;
        // 1. Music.
        if changed(flag::DISCO) {
            self.events.push(Event::Disco {
                on: new.has(flag::DISCO),
            });
        }
        // 2. Reveal: the minimap and fog drawing are refreshed.
        if changed(flag::REVEAL_MAP) {
            self.vision.generation = self.vision.generation.wrapping_add(1);
        }
        // The `showpath` word's side effect (fog/fog-off cheat), in single player only.
        if changed(flag::SHOW_PATH) && new.has(flag::SHOW_PATH) && !self.rules.multiplayer {
            self.cheat_clear_fog();
        }
        // 3. All spells.
        if new.has(flag::ALL_SPELLS) {
            for p in self.players.iter_mut().filter(|p| p.controller == controller::HUMAN) {
                p.spells_known = ALL_BITS;
                p.spells_allowed = ALL_BITS;
                p.upgrades_allowed = ALL_BITS;
                p.spells_researching = 0;
            }
        }
        // 4. All upgrades.
        if new.has(flag::ALL_UPGRADES) {
            for owner in 0..self.players.len() {
                if self.players[owner].controller != controller::HUMAN {
                    continue;
                }
                for (g, level) in self.players[owner].levels.iter_mut().enumerate() {
                    *level = if g < LEVELLED_GROUPS { 2 } else { 1 };
                }
                self.sync_upgrades(owner as u8);
            }
        }
        // 5. Resources and 6. oil, for all 16 players.
        if new.has(flag::RESOURCES) {
            for p in &mut self.players {
                p.gold = p.gold.saturating_add(RESOURCE_GIFT[0]);
                p.lumber = p.lumber.saturating_add(RESOURCE_GIFT[1]);
                p.oil = p.oil.saturating_add(RESOURCE_GIFT[2]);
            }
        }
        if new.has(flag::OIL) {
            for p in &mut self.players {
                p.oil = p.oil.saturating_add(OIL_GIFT);
            }
        }
        // The one-shot flags clear at once.
        self.cheats.flags = Flags(new.0 & !flag::ONE_SHOT);
        let playing = !self.loading;
        // 7. The first of these that applies ends the steps.
        let fixed = [
            (flag::NOTICE_A, Notice::A),
            (flag::NOTICE_B, Notice::B),
            (flag::NOTICE_C, Notice::C),
        ]
        .into_iter()
        .find(|&(f, _)| new.has(f));
        if let Some((_, notice)) = fixed {
            if playing {
                self.events.push(Event::Cheat { sender: player, notice });
            }
            return;
        }
        if new.has(flag::FINALE) && !self.rules.multiplayer {
            // The game ends in a campaign ending; it isn't a victory.
            let beyond = self.victory.campaign.is_some_and(|e| e >= 28);
            self.events.push(Event::Finale { beyond });
            return;
        }
        if new.has(flag::FINALE) {
            return;
        }
        // 8. The on or off notice.
        if playing {
            let notice = if self.cheats.flags.0 < old.0 {
                Notice::Off
            } else {
                Notice::On
            };
            self.events.push(Event::Cheat { sender: player, notice });
        }
    }

    /// Copies the changed flags into the rules the lasting effects read.
    fn sync_cheat_rules(&mut self, old: Flags, new: Flags) {
        let r = &mut self.rules;
        for (bit, field) in [
            (flag::REVEAL_MAP, &mut r.reveal_map),
            (flag::BUILD_SPEED, &mut r.build_speed_cheat),
            (flag::INVULNERABLE, &mut r.invulnerable_cheat),
            (flag::LUMBER, &mut r.lumber_cheat),
            (flag::ALL_SPELLS, &mut r.all_spells_cheat),
            (flag::INSTANT_VICTORY, &mut r.instant_victory),
            (flag::INSTANT_DEFEAT, &mut r.instant_defeat),
            (flag::NO_VICTORY, &mut r.disable_victory),
        ] {
            if (old.0 ^ new.0) & bit != 0 {
                *field = new.has(bit);
            }
        }
    }

    /// An attacker's (basic, piercing) damage, with the invulnerability cheat applied ("Cheat" in the damage spec).
    pub(crate) fn attack_values(&self, a: &crate::world::Unit, s: &tidewater_formats::udta::UnitStats) -> (u32, u32) {
        let owner = &self.players[a.owner as usize];
        let (basic, pierce) = crate::combat::attack(a.kind, s, &owner.upgrades, a.bloodlust > 0);
        if !self.rules.invulnerable_cheat {
            (basic, pierce)
        } else if owner.controller == controller::HUMAN {
            (basic, CHEAT_PIERCING)
        } else {
            (0, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::field;

    fn cx(flags: u32) -> Context {
        Context {
            flags: Flags(flags),
            multiplayer: false,
            cheats_disabled: false,
            enhanced: true,
        }
    }

    fn typed(line: &str, c: &Context) -> Typed {
        match interpret(line, c) {
            Line::Cheat(t) => t,
            other => panic!("{line:?} gave {other:?}"),
        }
    }

    #[test]
    fn every_phrase_has_its_check_value() {
        for p in PHRASES {
            assert_eq!(check_value(p.text), p.check, "{}", p.text);
        }
    }

    #[test]
    fn case_and_underscores_do_not_matter() {
        assert_eq!(check_value("ON_SCREEN"), 0x0900_C02A);
        assert_eq!(check_value("On Screen"), 0x0900_C02A);
    }

    #[test]
    fn collisions_share_a_code() {
        assert_eq!(check_value("are"), 0x0300_07BD);
        assert_eq!(check_value("are"), check_value("day"));
        // Notice B matches "183", and no line of letters only.
        for a in b'a'..=b'z' {
            for b in b'a'..=b'z' {
                for c in b'a'..=b'z' {
                    let s = String::from_utf8(vec![a, b, c]).unwrap();
                    assert_ne!(check_value(&s), 0x0300_03C3, "{s}");
                }
            }
        }
        assert_eq!(typed("are", &cx(0)).flags, Flags(flag::NOTICE_A));
    }

    #[test]
    fn phrases_toggle_or_act_once() {
        assert_eq!(typed("on screen", &cx(0)).flags, Flags(flag::REVEAL_MAP));
        assert_eq!(typed("on screen", &cx(flag::REVEAL_MAP)).flags, Flags(0));
        assert_eq!(
            typed("valdez", &cx(flag::REVEAL_MAP)).flags,
            Flags(flag::REVEAL_MAP | flag::OIL)
        );
    }

    #[test]
    fn single_player_codes_are_chat_in_multiplayer() {
        let mut c = cx(0);
        assert_eq!(typed("unite the clans", &c).flags, Flags(flag::INSTANT_VICTORY));
        c.multiplayer = true;
        assert_eq!(interpret("unite the clans", &c), Line::NotCheat);
        assert_eq!(typed("make it so", &c).flags, Flags(flag::BUILD_SPEED));
        c.cheats_disabled = true;
        assert_eq!(interpret("make it so", &c), Line::NotCheat);
    }

    #[test]
    fn words() {
        assert_eq!(typed("ShowPath", &cx(0)).flags, Flags(flag::SHOW_PATH));
        assert_eq!(typed("title", &cx(flag::SKIP_TITLE)).flags, Flags(0));
        let t = typed("fastdemo", &cx(flag::BUILD_SPEED));
        assert_eq!((t.flags, t.local), (Flags(flag::BUILD_SPEED), Some(Word::FastDemo)));
        assert_eq!(interpret("nopalette", &cx(0)), Line::Developer(Word::NoPalette));
        assert_eq!(interpret("show path", &cx(0)), Line::NotCheat);
    }

    #[test]
    fn mission_select_vectors() {
        let entry = |l| mission_select(l).map(|m| m.map(|m| m.entry));
        assert_eq!(entry("orc5"), Some(Some(9)));
        assert_eq!(entry("xorc 3"), Some(Some(33)));
        assert_eq!(entry("Human1"), Some(Some(0)));
        assert_eq!(entry("xhuman12"), Some(Some(50)));
        assert_eq!(entry("xhuman13"), Some(None));
        assert_eq!(entry("human15"), Some(None));
        assert_eq!(entry("orc"), Some(None));
        assert_eq!(entry("orc0"), Some(None));
        assert_eq!(entry("orc 7 now"), Some(Some(13)));
        assert_eq!(entry("peasant"), None);
    }

    #[test]
    fn mission_select_needs_its_flag() {
        assert_eq!(interpret("human3", &cx(0)), Line::NotCheat);
        let on = cx(flag::MISSION_SELECT);
        let t = typed("orc5", &on);
        assert_eq!(t.flags, on.flags);
        assert_eq!(t.mission.map(|m| m.entry), Some(9));
        // A prefix with a bad number is used up, with no mission.
        assert_eq!(typed("xhuman13", &on).mission, None);
        let mut mp = on;
        mp.multiplayer = true;
        assert_eq!(interpret("orc5", &mp), Line::NotCheat);
    }

    #[test]
    fn arguments_set_persistent_flags_only() {
        let (f, words) = from_arguments(["title", "tigerlily", "valdez", "fastdemo", "--other"]);
        assert_eq!(f, Flags(flag::SKIP_TITLE | flag::MISSION_SELECT));
        assert_eq!(words, [Word::FastDemo]);
    }

    #[test]
    fn only_three_flags_carry_over() {
        let all = Flags(!0);
        assert_eq!(all.for_new_game(), Flags(flag::KEPT));
        assert_eq!(all.for_demo(), Flags(flag::KEPT | flag::REVEAL_MAP));
    }

    fn notices(w: &mut World) -> Vec<Notice> {
        std::mem::take(&mut w.events)
            .into_iter()
            .filter_map(|e| match e {
                Event::Cheat { notice, .. } => Some(notice),
                _ => None,
            })
            .collect()
    }

    fn human_world() -> World {
        let mut w = field(8, 8);
        w.players[0].controller = controller::HUMAN;
        w.vision.local = 0;
        w
    }

    #[test]
    fn toggling_shows_on_then_off() {
        let mut w = human_world();
        w.cheat_command(0, Flags(flag::REVEAL_MAP));
        assert!(w.rules.reveal_map && w.cheats.used);
        assert_eq!(notices(&mut w), [Notice::On]);
        w.cheat_command(0, Flags(0));
        assert!(!w.rules.reveal_map);
        assert_eq!(notices(&mut w), [Notice::Off]);
    }

    #[test]
    fn resources_go_to_all_sixteen_and_the_flag_clears() {
        let mut w = human_world();
        w.cheat_command(0, Flags(flag::RESOURCES));
        assert!(
            w.players
                .iter()
                .all(|p| (p.gold, p.lumber, p.oil) == (10_000, 5_000, 5_000))
        );
        assert_eq!(w.cheats.flags, Flags(0));
        assert_eq!(notices(&mut w), [Notice::On]);
        w.cheat_command(0, Flags(flag::OIL));
        assert_eq!(w.players[15].oil, 10_000);
    }

    #[test]
    fn fixed_notices_end_the_steps() {
        let mut w = human_world();
        w.cheat_command(0, Flags(flag::NOTICE_B));
        assert_eq!(notices(&mut w), [Notice::B]);
        w.cheat_command(0, Flags(flag::NOTICE_C | flag::NOTICE_A));
        assert_eq!(notices(&mut w), [Notice::A]);
    }

    #[test]
    fn upgrades_and_spells_for_humans_only() {
        let mut w = human_world();
        w.players[1].controller = controller::COMPUTER;
        w.players[0].spells_researching = 4;
        w.cheat_command(0, Flags(flag::ALL_UPGRADES | flag::ALL_SPELLS));
        let p = &w.players[0];
        assert_eq!(p.levels, [2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1]);
        assert_eq!((p.upgrades.sword, p.upgrades.arrow, p.upgrades.siege), (2, 2, 2));
        assert!(p.upgrades.marksmanship && p.upgrades.longbow);
        assert_eq!((p.spells_known, p.spells_allowed, p.spells_researching), (!0, !0, 0));
        assert_eq!(w.players[1].spells_known, 0);
        assert_eq!(w.players[1].levels, [0; crate::tech::LINES]);
        assert!(w.rules.all_spells_cheat);
    }

    #[test]
    fn disabled_cheats_only_draw_once() {
        let mut w = human_world();
        w.rules.multiplayer = true;
        w.rules.cheats_disabled = true;
        let mut r = w.rng;
        r.draw();
        w.cheat_command(0, Flags(flag::BUILD_SPEED));
        assert_eq!(w.rng, r);
        assert!(!w.rules.build_speed_cheat && w.events.is_empty());
        // The mission still counts as cheated.
        assert!(w.cheats.used);
    }

    /// X2 (`game/cheats.md`, "Cheat-used mark"; G06 and G19, traced round 90): a new game starts with the mark clear
    /// (every new game start builds a fresh world, so replay, restart and mission select all get that), the local
    /// player's cheat command sets it, and a load leaves it as the running program had it: the mark isn't saved, so it
    /// is the template's (the client hands over the one from the game before).
    #[test]
    fn the_cheat_used_mark_is_clear_at_a_new_game_and_a_load_keeps_it() {
        let mut w = human_world();
        assert!(!w.cheats.used, "a new game starts clear");
        // Mission select's own command carries the flags unchanged, with the mission-select flag on.
        w.cheat_command(0, Flags(flag::MISSION_SELECT));
        assert!(w.cheats.used);
        let bytes = w.save_state();
        let mut loaded = human_world();
        loaded.restore(&bytes, 1).unwrap();
        assert!(
            !loaded.cheats.used,
            "the mark isn't in the save: a template without it stays clear"
        );
        // G19 (`human1-human01-cheat-load-game-50`): the program had it set before the load, so it is set after.
        let mut loaded = human_world();
        loaded.cheats.used = true;
        loaded.restore(&bytes, 1).unwrap();
        assert!(loaded.cheats.used, "a load keeps the mark");
        // The mark is per machine and not in the sync hash.
        let (mut a, b) = (human_world(), human_world());
        a.cheats.used = true;
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn another_players_cheat_is_not_ours() {
        let mut w = human_world();
        w.rules.multiplayer = true;
        w.cheat_command(2, Flags(flag::BUILD_SPEED));
        assert!(w.rules.build_speed_cheat && !w.cheats.used);
    }

    #[test]
    fn finale_in_single_player_only() {
        let mut w = human_world();
        w.cheat_command(0, Flags(flag::FINALE));
        assert_eq!(w.events, [Event::Finale { beyond: false }]);
        assert_eq!(w.cheats.flags, Flags(0));
        w.events.clear();
        w.victory.campaign = Some(33);
        w.cheat_command(0, Flags(flag::FINALE));
        assert_eq!(w.events, [Event::Finale { beyond: true }]);
        w.events.clear();
        w.rules.multiplayer = true;
        w.cheat_command(0, Flags(flag::FINALE));
        assert!(w.events.is_empty());
    }

    #[test]
    fn disco_raises_a_music_event() {
        let mut w = human_world();
        w.cheat_command(0, Flags(flag::DISCO));
        assert!(w.events.contains(&Event::Disco { on: true }));
    }

    #[test]
    fn showpath_turns_fog_off_in_single_player() {
        let mut w = human_world();
        w.cheat_command(0, Flags(flag::SHOW_PATH));
        assert!(!w.rules.fog_of_war);
        let mut m = human_world();
        m.rules.multiplayer = true;
        m.cheat_command(0, Flags(flag::SHOW_PATH));
        assert!(m.rules.fog_of_war);
    }

    /// Damage spec, round-59 vectors: a human player's attacker keeps its basic damage (upgrades, then bloodlust) and
    /// gets a flat piercing 200; anyone else's deals 0 and 0.
    #[test]
    fn invulnerability_sets_attack_values() {
        let mut w = human_world();
        // Footman (basic 6, piercing 3) and archer (basic 3, piercing 6) of the synthetic table, weapons upgradable.
        w.stats.0[0].weapons_upgradable = true;
        w.stats.0[8].weapons_upgradable = true;
        let footman = w.spawn(0, 0, (1, 1), 0).unwrap();
        let archer = w.spawn(8, 0, (3, 3), 0).unwrap();
        let computer = w.spawn(0, 1, (5, 5), 0).unwrap();
        w.players[1].controller = controller::COMPUTER;
        for p in 0..2 {
            w.players[p].upgrades.sword = 1;
            w.players[p].upgrades.arrow = 1;
        }
        for id in [footman, computer] {
            let i = w.index(id).unwrap();
            w.units[i].bloodlust = 100;
        }
        w.cheat_command(0, Flags(flag::INVULNERABLE));
        let values = |w: &World, id| {
            let u = w.unit(id).unwrap().clone();
            w.attack_values(&u, &w.stats_of(u.kind))
        };
        // Sword level 1 and bloodlust: basic (6 + 2) x 2 = 16, piercing 200 (not 400, not 203).
        assert_eq!(values(&w, footman), (16, CHEAT_PIERCING));
        // Arrow level 1, no bloodlust: basic 3, piercing 200.
        assert_eq!(values(&w, archer), (3, CHEAT_PIERCING));
        // A computer player's footman, upgrades and bloodlust or not: 0 and 0.
        assert_eq!(values(&w, computer), (0, 0));
        // Cheat off: the usual values.
        w.cheat_command(0, Flags(0));
        assert_eq!(values(&w, footman), (16, 6));
        assert_eq!(values(&w, archer), (3, 7));
    }

    #[test]
    fn flags_take_part_in_the_hash() {
        let mut a = human_world();
        let b = human_world();
        assert_eq!(a.state_hash(), b.state_hash());
        a.cheat_command(0, Flags(flag::BUILD_SPEED));
        a.cheat_command(0, Flags(0));
        // Back to no flags: nothing else changed, so the hash matches again.
        assert_eq!(a.state_hash(), b.state_hash());
        a.cheat_command(0, Flags(flag::NO_GLUES));
        assert_ne!(a.state_hash(), b.state_hash());
    }
}
