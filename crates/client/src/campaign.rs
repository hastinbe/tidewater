//! The campaign's flow around a game: startup videos and title, the new-campaign choices, act screens, briefings,
//! the victory and defeat dialogs, the score screen, what comes after it and the endings. Spec:
//! `docs/specs/game/campaign-flow.md`. Mission entries are those of `docs/specs/game/victory.md` ("Mission rules").
//!
//! Game text is named by string table and entry number counting from 1, as in the spec; [`text_index`] turns an
//! entry into a table index. No Bevy types here.
//!
//! Defaults for the spec's open questions:
//! - The skip setting and the setting that skips the startup videos are off: the developer words that turn them on
//!   (`game/cheats.md`) aren't implemented yet.
//! - The movie archive is always available (a video the install lacks is skipped); the startup intro plays when the
//!   "play the intro" bit is set, which is every start (it isn't stored; `--no-intro` clears it).
//! - Ctrl isn't read at startup (the window doesn't exist yet), only at the later checks.
//! - The title and act screen tick is the normal game speed's cycle (40 ms).
//! - The ending videos are `HVict_M` / `OVict_M` (ToD) and `HVicX_M` / `OrcX_M` (BDP), before the finale.
//! - A type change doesn't count as a building lost; gathered resources are deliveries (`tidewater_sim`).
//! - A restart from the game menu puts the campaign total back as a defeat does.
//! - No cheats can be entered yet, so the rank is never the cheat rank in play.
//! - The multiplayer score screen isn't covered; only the single-player rows are built.

#![allow(
    dead_code,
    reason = "the spec's tables are kept whole; the dialog files, multiplayer and the game menu use the rest"
)]

use tidewater_sim::World;
use tidewater_sim::ai::controller;

/// Race 0, human.
pub const HUMAN: u8 = 0;
/// Race 1, orc.
pub const ORC: u8 = 1;

/// The table index of a 1-based entry number.
pub fn text_index(entry: u16) -> usize {
    entry.saturating_sub(1) as usize
}

// ---------------------------------------------------------------------------------------------------------------
// Videos

/// Video file names in the `Smk` folder, by video number ("Videos").
pub const VIDEOS: [&str; 13] = [
    "Blizzard", "Intro_M", "Smoke_M", "Burn_M", "Zepp_M", "Turtle_M", "Ships_M", "Demn_M", "HVict_M", "OVict_M",
    "IntroX_M", "HVicX_M", "OrcX_M",
];

/// Video numbers named by the flow.
pub mod video {
    pub const LOGO: u8 = 0;
    pub const INTRO: u8 = 1;
    pub const TOD_HUMAN_END: u8 = 8;
    pub const TOD_ORC_END: u8 = 9;
    pub const BDP_INTRO: u8 = 10;
    pub const BDP_HUMAN_END: u8 = 11;
    pub const BDP_ORC_END: u8 = 12;
}

/// The archive name of video `n`.
pub fn video_path(n: u8) -> String {
    format!("Smk\\{}.smk", VIDEOS[n as usize])
}

// ---------------------------------------------------------------------------------------------------------------
// Startup

/// What the program knows when it starts ("Startup", step 1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartupInput {
    pub ctrl: bool,
    /// The second, unidentified setting that skips the startup videos (open question; off).
    pub skip_videos: bool,
    /// The movie archive is available (open question: always).
    pub movies: bool,
    /// The "play the intro" preference bit.
    pub play_intro: bool,
}

/// The startup videos in order, and whether the "play the intro" bit is cleared.
pub fn startup_videos(i: StartupInput) -> (Vec<u8>, bool) {
    let mut v = Vec::new();
    let mut clear = false;
    if !i.ctrl && !i.skip_videos {
        v.push(video::LOGO);
        if i.movies && i.play_intro {
            clear = true;
            v.push(video::INTRO);
        }
    }
    (v, clear)
}

/// The title picture.
pub const TITLE_PICTURE: &str = "Art\\Title_Rel_BNE.pcx";
/// The title stays this many ticks unless a key or click ends it.
pub const TITLE_TICKS: u32 = 100;
/// The title's fade in, and its fade out when it ran its time.
pub const TITLE_FADE_MS: u32 = 250;
/// The title's music track.
pub const TITLE_MUSIC: u8 = 22;

/// The title's fade-out time: 250 ms, or 1 ms when a key or click ended it.
pub fn title_fade_out_ms(skipped: bool) -> u32 {
    if skipped { 1 } else { TITLE_FADE_MS }
}

/// The tick of the title and the act screen: an open question ("Tick lengths"). Our default: the cycle time of the
/// normal game speed under Enhanced rules.
pub fn screen_tick_ms() -> u32 {
    tidewater_sim::timing::ENHANCED_MS[tidewater_sim::timing::NORMAL as usize]
}

// ---------------------------------------------------------------------------------------------------------------
// Mission entries

/// Whether `entry` is a *Beyond the Dark Portal* mission (entries 28 to 51).
pub fn is_bdp(entry: u8) -> bool {
    entry >= 28
}

/// The race of a mission entry: human for even entries, orc for odd ones.
pub fn race_of(entry: u8) -> u8 {
    entry % 2
}

/// The mission number (from 1) of an entry.
pub fn mission_number(entry: u8) -> u8 {
    (entry % 28) / 2 + 1
}

/// The map of a mission entry ("Campaign maps").
pub fn map_name(entry: u8) -> String {
    let n = mission_number(entry);
    match (is_bdp(entry), race_of(entry)) {
        (false, HUMAN) => format!("Campaign\\Human\\Human{n:02}.pud"),
        (false, _) => format!("Campaign\\Orc\\Orc{n:02}.pud"),
        (true, HUMAN) => format!("Campaign\\XHuman\\2XHum{n:02}.pud"),
        (true, _) => format!("Campaign\\XOrc\\2XOrc{n:02}.pud"),
    }
}

/// The kind of game being played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKind {
    Campaign(u8),
    Custom,
    Multiplayer,
}

// ---------------------------------------------------------------------------------------------------------------
// Choosing a new campaign

/// The new-campaign dialog's four choices ("Choosing a new campaign").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    TodOrc,
    TodHuman,
    BdpOrc,
    BdpHuman,
}

impl Choice {
    pub const ALL: [Choice; 4] = [Choice::TodOrc, Choice::TodHuman, Choice::BdpOrc, Choice::BdpHuman];

    pub fn race(self) -> u8 {
        match self {
            Choice::TodOrc | Choice::BdpOrc => ORC,
            Choice::TodHuman | Choice::BdpHuman => HUMAN,
        }
    }

    pub fn first_entry(self) -> u8 {
        match self {
            Choice::TodOrc => 1,
            Choice::TodHuman => 0,
            Choice::BdpOrc => 29,
            Choice::BdpHuman => 28,
        }
    }

    pub fn bdp(self) -> bool {
        matches!(self, Choice::BdpOrc | Choice::BdpHuman)
    }

    /// Our label for the choice (the dialog's own text comes from its dialog file, which isn't read).
    pub fn label(self) -> &'static str {
        match self {
            Choice::TodOrc => "Tides of Darkness: Orc campaign",
            Choice::TodHuman => "Tides of Darkness: Human campaign",
            Choice::BdpOrc => "Beyond the Dark Portal: Orc campaign",
            Choice::BdpHuman => "Beyond the Dark Portal: Human campaign",
        }
    }
}

/// What the BDP intro does: `Ok(video)` to play before the mission (none while Ctrl is held), or `Err(())` when the
/// movie archive is missing and the game goes back to the main menu.
pub fn bdp_intro(movies: bool, ctrl: bool) -> Result<Option<u8>, ()> {
    if !movies {
        return Err(());
    }
    Ok((!ctrl).then_some(video::BDP_INTRO))
}

/// Whether the act screen and the briefing are skipped at a check ("Starting a mission").
pub fn screens_skipped(ctrl: bool, skip_setting: bool) -> bool {
    ctrl || skip_setting
}

// ---------------------------------------------------------------------------------------------------------------
// Act screens

/// One act screen ("The act screen").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Act {
    /// The mission entry it belongs to (the act's first mission).
    pub entry: u8,
    /// The `acts.tbl` entries: the act number and its title.
    pub lines: (u16, u16),
    /// The picture's name in `Art`.
    pub picture: &'static str,
    /// The video played after the screen.
    pub video: Option<u8>,
}

const fn act(entry: u8, a: u16, picture: &'static str, video: Option<u8>) -> Act {
    Act {
        entry,
        lines: (a, a + 1),
        picture,
        video,
    }
}

/// The act table.
pub const ACTS: [Act; 16] = [
    act(0, 1, "umaact1", None),
    act(8, 3, "umaact2", Some(2)),
    act(14, 5, "umaact3", Some(4)),
    act(22, 7, "umaact4", Some(6)),
    act(1, 9, "orcact1", None),
    act(9, 11, "orcact2", Some(3)),
    act(15, 13, "orcact3", Some(5)),
    act(23, 15, "orcact4", Some(7)),
    act(29, 17, "2xoact1", None),
    act(35, 19, "2xoact2", None),
    act(41, 21, "2xoact3", None),
    act(47, 23, "2xoact4", None),
    act(28, 25, "2xhact1", None),
    act(34, 27, "2xhact2", None),
    act(40, 29, "2xhact3", None),
    act(46, 31, "2xhact4", None),
];

/// The act screen of a mission entry, if it is the first mission of an act.
pub fn act_for(entry: u8) -> Option<Act> {
    ACTS.iter().copied().find(|a| a.entry == entry)
}

/// The act-1 picture of the same campaign and side, whose palette the act's picture uses.
pub fn act_palette_picture(a: &Act) -> &'static str {
    let first = match (is_bdp(a.entry), race_of(a.entry)) {
        (false, HUMAN) => 0,
        (false, _) => 1,
        (true, HUMAN) => 28,
        (true, _) => 29,
    };
    act_for(first).map_or(a.picture, |f| f.picture)
}

/// An `Art` picture's archive name.
pub fn art(name: &str) -> String {
    format!("Art\\{name}.pcx")
}

/// The act sound by campaign race.
pub fn act_sound(race: u8) -> &'static str {
    if race == HUMAN {
        "Sfx\\hact.wav"
    } else {
        "Sfx\\oact.wav"
    }
}

/// What the flow remembers since the program started: the last act screen shown and the last video played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub last_act: Option<u8>,
    pub last_video: Option<u8>,
}

impl Memory {
    /// The act screen to show for `entry`: only for the first mission of an act whose act isn't the last one shown.
    /// Showing it makes it the last act shown.
    pub fn act_to_show(&mut self, entry: u8) -> Option<Act> {
        let a = act_for(entry)?;
        if self.last_act == Some(a.entry) {
            return None;
        }
        self.last_act = Some(a.entry);
        Some(a)
    }

    /// The video after an act screen: the act's video unless it is the video played last, or a quit was asked for.
    pub fn video_after(&self, a: &Act, quit: bool) -> Option<u8> {
        a.video.filter(|&v| !quit && self.last_video != Some(v))
    }

    /// A video was played.
    pub fn played(&mut self, v: u8) {
        self.last_video = Some(v);
    }
}

/// The act screen's countdown.
pub const ACT_TICKS: u8 = 60;
/// When the picture fades in (and the act sound plays).
pub const ACT_PICTURE_AT: u8 = 50;
/// The text's fade-in at 60, and the picture's at 50.
pub const ACT_TEXT_FADE_MS: u32 = 300;
pub const ACT_PICTURE_FADE_MS: u32 = 600;
/// The palette entries that hold the text colours (white while the rest is black, at 60).
pub const ACT_TEXT_COLOURS: std::ops::RangeInclusive<u8> = 1..=3;

/// What happens on an act screen tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActEvent {
    /// The picture fades in over 600 ms and the act sound plays.
    Picture,
    Close,
}

/// The act screen's countdown from 60 to 0, one step per tick. At 60 (when it opens) only the text shows and fades
/// in over 300 ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActTimer {
    pub count: u8,
    ticks: u32,
}

impl Default for ActTimer {
    fn default() -> Self {
        ActTimer {
            count: ACT_TICKS,
            ticks: 0,
        }
    }
}

impl ActTimer {
    /// One tick.
    pub fn tick(&mut self) -> Option<ActEvent> {
        self.ticks += 1;
        self.count = self.count.saturating_sub(1);
        match self.count {
            ACT_PICTURE_AT => Some(ActEvent::Picture),
            0 => Some(ActEvent::Close),
            _ => None,
        }
    }

    /// Esc, Enter, Space or a click close the screen, but not on its first tick.
    pub fn may_close_early(&self) -> bool {
        self.ticks >= 1
    }

    /// Whether the picture is showing (from 50 on).
    pub fn picture_shown(&self) -> bool {
        self.count <= ACT_PICTURE_AT
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Briefings

/// The briefing's dialog file for a campaign race.
pub fn briefing_dialog(race: u8) -> &'static str {
    if race == HUMAN {
        "rez\\hdisptch.bin"
    } else {
        "rez\\odisptch.bin"
    }
}

/// The briefing's background picture, by mission number and race (the same for ToD and BDP).
pub fn briefing_background(entry: u8) -> &'static str {
    let human = race_of(entry) == HUMAN;
    match (mission_number(entry), human) {
        (1..=4, true) => "Art\\hdspatch.pcx",
        (1..=4, false) => "Art\\slash.pcx",
        (5..=7, true) => "Art\\ogreknit.pcx",
        (5..=7, false) => "Art\\odspatch.pcx",
        (8..=11, true) => "Art\\dethnite.pcx",
        (8..=11, false) => "Art\\mage.pcx",
        (_, true) => "Art\\dragon.pcx",
        (_, false) => "Art\\griffon.pcx",
    }
}

/// The objectives table.
pub const OBJECTIVES: &str = "rez\\objctivs.tbl";

/// The `objctivs.tbl` entries of a mission: its title (e + 60) and its objectives (e + 1).
pub fn objective_entries(entry: u8) -> (u16, u16) {
    (entry as u16 + 60, entry as u16 + 1)
}

/// The mission's story table; its entry 1 is the story text.
pub fn story_table(entry: u8) -> String {
    let n = mission_number(entry);
    match (is_bdp(entry), race_of(entry)) {
        (false, HUMAN) => format!("rez\\human{n}.tbl"),
        (false, _) => format!("rez\\orc{n}.tbl"),
        (true, HUMAN) => format!("rez\\2xhum{n}.tbl"),
        (true, _) => format!("rez\\2xorc{n}.tbl"),
    }
}

/// How many narration parts a mission has (Data, "Briefing narration parts per mission").
pub fn narration_parts(entry: u8) -> u8 {
    let n = mission_number(entry);
    let (ones, threes): (&[u8], &[u8]) = match (is_bdp(entry), race_of(entry)) {
        (false, HUMAN) => (&[6, 9, 10, 12, 14], &[]),
        (false, _) => (&[1, 2, 4, 5, 6, 8, 9, 10, 14], &[]),
        (true, HUMAN) => (&[], &[4, 6, 7, 10]),
        (true, _) => (&[8], &[1, 7, 10, 12]),
    };
    if ones.contains(&n) {
        1
    } else if threes.contains(&n) {
        3
    } else {
        2
    }
}

/// The narration's speech files, played in order.
pub fn narration_files(entry: u8) -> Vec<String> {
    let n = mission_number(entry);
    let side = if race_of(entry) == HUMAN { "hum" } else { "orc" };
    (1..=narration_parts(entry))
        .map(|k| {
            if is_bdp(entry) {
                format!("Speech\\war2x\\{side}{n}_{k}.wav")
            } else {
                format!("Speech\\{side}_{n}_{k}.wav")
            }
        })
        .collect()
}

/// The gap between narration parts.
pub const NARRATION_GAP_MS: u32 = 255;

/// The narration's total length T: each part's length plus 255, minus 255 (0 with no parts).
pub fn narration_total_ms(parts: &[u32]) -> u32 {
    let sum: u32 = parts.iter().map(|&l| l + NARRATION_GAP_MS).sum();
    sum.saturating_sub(NARRATION_GAP_MS)
}

/// When each narration part starts, in ms from the start.
pub fn narration_starts(parts: &[u32]) -> Vec<u32> {
    let mut at = 0;
    parts
        .iter()
        .map(|&l| {
            let s = at;
            at += l + NARRATION_GAP_MS;
            s
        })
        .collect()
}

/// The steps the story text scrolls: text height + panel height − half a line.
pub fn scroll_steps(text_h: u32, panel_h: u32, line_h: u32) -> u32 {
    (text_h + panel_h).saturating_sub(line_h / 2)
}

/// The scroll interval: T / steps ms, or 100 ms when that is 0.
pub fn scroll_interval_ms(total_ms: u32, steps: u32) -> u32 {
    match total_ms.checked_div(steps) {
        Some(0) | None => 100,
        Some(n) => n,
    }
}

/// The briefing's music track (if no music is playing).
pub fn briefing_music(race: u8) -> u8 {
    if race == HUMAN { 20 } else { 21 }
}

/// The story text's scrolling and the briefing's own close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scroll {
    /// Steps moved so far.
    pub moved: u32,
    pub steps: u32,
    pub interval_ms: u32,
}

impl Scroll {
    pub fn new(total_ms: u32, text_h: u32, panel_h: u32, line_h: u32) -> Self {
        let steps = scroll_steps(text_h, panel_h, line_h);
        Scroll {
            moved: 0,
            steps,
            interval_ms: scroll_interval_ms(total_ms, steps),
        }
    }

    /// Moves to where the text is `elapsed_ms` after the start.
    pub fn at(&mut self, elapsed_ms: u64) {
        self.moved = ((elapsed_ms / self.interval_ms.max(1) as u64) as u32).min(self.steps);
    }

    /// The text has scrolled out of the panel.
    pub fn done(&self) -> bool {
        self.moved >= self.steps
    }
}

/// The briefing closes by itself once the text has scrolled out and the narration has finished.
pub fn briefing_closes(scroll: &Scroll, elapsed_ms: u64, narration_ms: u32) -> bool {
    scroll.done() && elapsed_ms >= narration_ms as u64
}

// ---------------------------------------------------------------------------------------------------------------
// Victory and defeat dialogs

/// The dialog files.
pub const VICTORY_DIALOG: &str = "rez\\wmission.bin";
pub const DEFEAT_DIALOG: &str = "rez\\lmission.bin";

/// The dialogs' small race picture.
pub fn dialog_picture(race: u8) -> &'static str {
    if race == HUMAN {
        "art\\h144x64.bmp"
    } else {
        "art\\o144x64.bmp"
    }
}

/// A button on the victory or defeat dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogButton {
    Continue,
    Save,
}

/// What a dialog button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    /// The game ends with the outcome.
    End,
    /// The save dialog opens; after a save the game ends, after a cancel the victory dialog is shown again.
    OpenSave,
}

pub fn dialog_button(b: DialogButton) -> DialogResult {
    match b {
        DialogButton::Continue => DialogResult::End,
        DialogButton::Save => DialogResult::OpenSave,
    }
}

/// After the save dialog: `true` when the game ends (saved), `false` when the victory dialog is shown again.
pub fn after_save(saved: bool) -> bool {
    saved
}

// ---------------------------------------------------------------------------------------------------------------
// The score screen

/// The victory bonus.
pub const VICTORY_BONUS: u32 = 500;

/// The campaign total and its value at the start of the mission ("Campaign total").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CampaignTotal {
    pub total: u32,
    pub at_start: u32,
}

impl CampaignTotal {
    /// A game starts: the total is 0 for a multiplayer or custom game or a campaign's first mission (entries 0, 1,
    /// 28, 29), otherwise it carries over. The value at the start is remembered.
    pub fn start_game(&mut self, game: GameKind) {
        if !matches!(game, GameKind::Campaign(e) if !matches!(e, 0 | 1 | 28 | 29)) {
            self.total = 0;
        }
        self.at_start = self.total;
    }

    /// Closes the mission's score: on a victory the mission score goes up by 500; then it is added to the total.
    /// Returns the closed mission score.
    pub fn close(&mut self, mission_score: u32, victory: bool) -> u32 {
        let s = mission_score + if victory { VICTORY_BONUS } else { 0 };
        self.total += s;
        s
    }

    /// A defeat puts the total back to its value at the start, before the mission is replayed.
    pub fn defeat(&mut self) {
        self.total = self.at_start;
    }

    /// A restart from the game menu: the total goes back as on a defeat (open question; default yes).
    pub fn restart(&mut self) {
        self.defeat();
    }
}

/// The score screen's background.
pub fn score_background(race: u8, victory: bool) -> &'static str {
    match (race == HUMAN, victory) {
        (true, true) => "Art\\hwinglu.pcx",
        (false, true) => "Art\\owinglu.pcx",
        (true, false) => "Art\\hlosglu.pcx",
        (false, false) => "Art\\olosglu.pcx",
    }
}

/// The score screen's music track.
pub fn score_music(race: u8, victory: bool) -> u8 {
    match (race == HUMAN, victory) {
        (true, true) => 14,
        (false, true) => 15,
        (true, false) => 12,
        (false, false) => 13,
    }
}

/// The score screen's dialog file by the number of players shown.
pub fn score_dialog(shown: usize) -> &'static str {
    match shown {
        0..=2 => "rez\\msnstat2.bin",
        3..=4 => "rez\\msnstat4.bin",
        5..=6 => "rez\\msnstat6.bin",
        _ => "rez\\msnstats.bin",
    }
}

/// The tick sound of the counting score, and the rank's sound.
pub const NUMBERS_SOUND: &str = "Sfx\\numbers.wav";
pub const RANK_SOUND: &str = "Sfx\\rank.wav";

/// The rows' labels in `network.tbl`.
pub mod label {
    pub const YOU: u16 = 34;
    pub const ENEMY: u16 = 35;
    pub const ALLY: u16 = 36;
}

/// The row label of slot `slot` (single player), or `None` when it isn't shown. `controllers` are the slots'
/// controllers as they were when the game started; `allied` tells whether the local player counts a slot allied.
pub fn row_label(slot: u8, local: u8, controllers: &[u8; 8], allied: impl Fn(u8) -> bool) -> Option<u16> {
    if slot == local {
        return Some(label::YOU);
    }
    if controllers[slot as usize] != controller::COMPUTER {
        return None;
    }
    Some(if allied(slot) { label::ALLY } else { label::ENEMY })
}

/// The `network.tbl` entry of a row's name, by slot and race ("Players shown").
pub fn name_entry(slot: u8, race: u8, bdp_campaign: bool) -> u16 {
    const HUMAN_NAMES: [u16; 8] = [50, 46, 52, 56, 58, 54, 48, 60];
    const ORC_NAMES: [u16; 8] = [51, 47, 53, 57, 59, 55, 49, 61];
    const BDP_ORC_NAMES: [u16; 8] = [94, 92, 95, 97, 98, 96, 93, 99];
    let s = slot as usize & 7;
    match (race == ORC, bdp_campaign) {
        (true, true) => BDP_ORC_NAMES[s],
        (true, false) => ORC_NAMES[s],
        _ => HUMAN_NAMES[s],
    }
}

/// One shown player's statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub units: u32,
    pub buildings: u32,
    pub gold: u32,
    pub lumber: u32,
    pub oil: u32,
    pub kills: u32,
    pub razings: u32,
}

impl Stats {
    pub const NAMES: [&'static str; 7] = ["Units", "Buildings", "Gold", "Lumber", "Oil", "Kills", "Razings"];

    pub fn values(&self) -> [u32; 7] {
        [
            self.units,
            self.buildings,
            self.gold,
            self.lumber,
            self.oil,
            self.kills,
            self.razings,
        ]
    }

    /// Player `p`'s statistics at the end of a game: units and buildings are those alive now plus those lost.
    pub fn of_world(w: &World, p: u8) -> Stats {
        let alive = |building: bool| {
            w.units
                .iter()
                .filter(|u| u.owner == p && u.building == building && !u.dying && !u.removing)
                .count() as u32
        };
        let pl = &w.players[p as usize];
        Stats {
            units: alive(false) + pl.units_lost,
            buildings: alive(true) + pl.buildings_lost,
            gold: pl.gathered[0],
            lumber: pl.gathered[1],
            oil: pl.gathered[2],
            kills: pl.kills,
            razings: pl.razings,
        }
    }
}

/// One row of the score screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub slot: u8,
    pub label: u16,
    pub name: u16,
    pub stats: Stats,
}

/// The rows of a single-player score screen. `start_controllers` are the slots' controllers when the game started.
pub fn rows(w: &World, local: u8, start_controllers: &[u8; 8], bdp_campaign: bool) -> Vec<Row> {
    let allied = |q: u8| w.players[local as usize].allies & (1 << q) != 0;
    (0..8u8)
        .filter_map(|slot| {
            let label = row_label(slot, local, start_controllers, allied)?;
            let race = w.players[slot as usize].race;
            Some(Row {
                slot,
                label,
                name: name_entry(slot, race, bdp_campaign),
                stats: Stats::of_world(w, slot),
            })
        })
        .collect()
}

/// A bar's growth step: max / 40, at least 1, where max is the statistic's largest value among the shown players.
pub fn bar_step(max: u32) -> u32 {
    (max / 40).max(1)
}

/// The score's count-up step: total / 10. Our reading for a step of 0 (a total under 10): the total shows at once.
pub fn score_step(total: u32) -> u32 {
    total / 10
}

/// The value a count-up shows after `n` steps of `step` toward `to`.
pub fn counted(to: u32, step: u32, n: u32) -> u32 {
    if step == 0 { to } else { step.saturating_mul(n).min(to) }
}

/// The rank thresholds.
pub const RANK_THRESHOLDS: [u32; 19] = [
    2000, 5000, 8000, 18000, 28000, 40000, 55000, 70000, 85000, 105000, 125000, 145000, 165000, 185000, 205000, 230000,
    255000, 280000, 300000,
];
/// The rank text table and its cheat entry.
pub const RANK_TABLE: &str = "rez\\rank_txt.tbl";
pub const CHEAT_RANK: u16 = 39;

/// The `rank_txt.tbl` entry for a campaign total: 2i + 1 + race, with i the position of the first threshold above
/// the total (18 if none is), or 39 when the local player used a cheat this mission.
pub fn rank_entry(total: u32, race: u8, cheated: bool) -> u16 {
    if cheated {
        return CHEAT_RANK;
    }
    let i = RANK_THRESHOLDS.iter().position(|&t| t > total).unwrap_or(18) as u16;
    2 * i + 1 + race as u16
}

// ---------------------------------------------------------------------------------------------------------------
// After the score screen

/// Where the flow goes after the score screen ("After the score screen").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// The next mission: act screen check, briefing, game.
    Mission(u8),
    /// The same mission again.
    Replay(u8),
    Ending {
        bdp: bool,
        race: u8,
    },
    MainMenu,
    /// Back to the multiplayer screens (a later spec).
    Multiplayer,
}

pub fn after_score(game: GameKind, victory: bool) -> Next {
    match game {
        GameKind::Campaign(e) if !victory => Next::Replay(e),
        GameKind::Campaign(e @ (26 | 27)) => Next::Ending {
            bdp: false,
            race: race_of(e),
        },
        GameKind::Campaign(e @ (50 | 51)) => Next::Ending {
            bdp: true,
            race: race_of(e),
        },
        GameKind::Campaign(e) => Next::Mission(e + 2),
        GameKind::Custom => Next::MainMenu,
        GameKind::Multiplayer => Next::Multiplayer,
    }
}

/// Restarting the scenario from the game menu ("Restart"): the same mission again with no score screen, or the main
/// menu for a game that isn't a campaign mission (our choice; the menu spec covers those).
pub fn after_restart(game: GameKind) -> Next {
    match game {
        GameKind::Campaign(e) => Next::Replay(e),
        GameKind::Custom => Next::MainMenu,
        GameKind::Multiplayer => Next::Multiplayer,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Endings

/// An ending: its video, then the finale dialog and the credits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ending {
    /// Open question default: by file name.
    pub video: u8,
    pub picture: &'static str,
    pub music: u8,
    /// The `finale.tbl` entry scrolled up: k + 1 with k = race + 2 for BDP and race for ToD.
    pub finale_entry: u16,
    pub narration: Vec<&'static str>,
    pub credits: &'static str,
}

pub const FINALE_DIALOG: &str = "rez\\finaldlg.bin";
pub const FINALE_TABLE: &str = "rez\\finale.tbl";
/// The credits move one step every 40 ms.
pub const CREDITS_STEP_MS: u32 = 40;

pub fn ending(bdp: bool, race: u8) -> Ending {
    let human = race == HUMAN;
    let k = race as u16 + if bdp { 2 } else { 0 };
    Ending {
        video: match (bdp, human) {
            (false, true) => video::TOD_HUMAN_END,
            (false, false) => video::TOD_ORC_END,
            (true, true) => video::BDP_HUMAN_END,
            (true, false) => video::BDP_ORC_END,
        },
        picture: if human { "art\\humanv.pcx" } else { "art\\orcv.pcx" },
        music: if human { 16 } else { 17 },
        finale_entry: k + 1,
        narration: match (bdp, human) {
            (false, true) => vec!["Speech\\humvict.wav"],
            (false, false) => vec!["Speech\\orcvict.wav"],
            (true, true) => vec!["Speech\\war2x\\humvict1.wav", "Speech\\war2x\\humvict2.wav"],
            (true, false) => vec![
                "Speech\\war2x\\orcvict1.wav",
                "Speech\\war2x\\orcvict2.wav",
                "Speech\\war2x\\orcvict3.wav",
            ],
        },
        credits: if bdp { "rez\\crelst2x.tbl" } else { "rez\\crdt_lst.tbl" },
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Music

/// The archive name of a music track the flow starts (Data, "Music tracks started here").
pub fn music_file(track: u8) -> Option<&'static str> {
    Some(match track {
        12 => "Music\\HDEFEAT.WAV",
        13 => "Music\\ODEFEAT.WAV",
        14 => "Music\\HVICTORY.WAV",
        15 => "Music\\OVICTORY.WAV",
        16 => "Music\\HUMAN5.WAV",
        17 => "Music\\HUMAN6.WAV",
        20 => "Music\\HWARROOM.WAV",
        21 | 22 => "Music\\OWARROOM.WAV",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Text

/// Splits game text into lines no wider than `width`: newlines break, words wrap, and the formatting bytes (below
/// 0x20, other than newline and tab) are dropped. `measure` gives a string's width.
pub fn wrap(text: &str, width: u32, measure: &dyn Fn(&str) -> u32) -> Vec<String> {
    let clean: String = text
        .chars()
        .filter(|&c| c == '\n' || c >= ' ')
        .map(|c| if c == '\t' { ' ' } else { c })
        .collect();
    let mut out = Vec::new();
    for para in clean.split('\n') {
        let mut line = String::new();
        for word in para.split(' ').filter(|w| !w.is_empty()) {
            let tried = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure(&tried) > width {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = tried;
            }
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_and_entries_round_trip() {
        assert_eq!(map_name(0), "Campaign\\Human\\Human01.pud");
        assert_eq!(map_name(27), "Campaign\\Orc\\Orc14.pud");
        assert_eq!(map_name(29), "Campaign\\XOrc\\2XOrc01.pud");
        assert_eq!(map_name(50), "Campaign\\XHuman\\2XHum12.pud");
        for e in 0..52 {
            assert_eq!(tidewater_sim::setup::campaign_entry(&map_name(e)), Some(e), "entry {e}");
        }
    }

    #[test]
    fn startup_plays_the_logo_then_the_intro_once() {
        let i = StartupInput {
            movies: true,
            play_intro: true,
            ..Default::default()
        };
        assert_eq!(startup_videos(i), (vec![0, 1], true));
        let again = StartupInput { play_intro: false, ..i };
        assert_eq!(startup_videos(again), (vec![0], false));
        let ctrl = StartupInput { ctrl: true, ..i };
        assert_eq!(startup_videos(ctrl), (vec![], false));
        assert_eq!(title_fade_out_ms(true), 1);
        assert_eq!(title_fade_out_ms(false), 250);
    }

    /// Test vector: new campaign, ToD human, Ctrl not held.
    #[test]
    fn tod_human_starts_with_act_one() {
        let c = Choice::TodHuman;
        assert_eq!((c.race(), c.first_entry(), c.bdp()), (HUMAN, 0, false));
        let mut m = Memory::default();
        let a = m.act_to_show(0).unwrap();
        assert_eq!((a.lines, a.picture), ((1, 2), "umaact1"));
        assert_eq!(m.video_after(&a, false), None);
        assert_eq!(map_name(c.first_entry()), "Campaign\\Human\\Human01.pud");
        assert!(screens_skipped(true, false), "Ctrl held: no act screen, no briefing");
    }

    /// Test vector: new campaign, BDP orc, archive available.
    #[test]
    fn bdp_orc_plays_its_intro_then_act_one() {
        let c = Choice::BdpOrc;
        assert_eq!(c.first_entry(), 29);
        assert_eq!(bdp_intro(true, false), Ok(Some(10)));
        assert_eq!(bdp_intro(true, true), Ok(None));
        assert_eq!(bdp_intro(false, false), Err(()));
        let a = Memory::default().act_to_show(29).unwrap();
        assert_eq!((a.lines, a.picture), ((17, 18), "2xoact1"));
        assert_eq!(map_name(29), "Campaign\\XOrc\\2XOrc01.pud");
    }

    /// Test vector: ToD human mission 4 won.
    #[test]
    fn after_human_mission_4_comes_act_two_and_video_2() {
        assert_eq!(after_score(GameKind::Campaign(6), true), Next::Mission(8));
        let mut m = Memory {
            last_act: Some(0),
            last_video: None,
        };
        let a = m.act_to_show(8).unwrap();
        assert_eq!(a.lines, (3, 4));
        assert_eq!(m.video_after(&a, false), Some(2));
        assert_eq!(m.video_after(&a, true), None, "no video after a quit");
        m.played(2);
        assert_eq!(m.video_after(&a, false), None, "not the video played last");
        assert_eq!(briefing_background(8), "Art\\ogreknit.pcx");
    }

    /// Test vector: ToD orc mission 5 lost.
    #[test]
    fn a_lost_mission_is_replayed_without_its_act_screen() {
        let e = 9;
        assert_eq!(score_background(ORC, false), "Art\\olosglu.pcx");
        assert_eq!(score_music(ORC, false), 13);
        assert_eq!(after_score(GameKind::Campaign(e), false), Next::Replay(e));
        let mut m = Memory::default();
        assert!(m.act_to_show(e).is_some());
        assert!(m.act_to_show(e).is_none(), "act already shown");
        assert_eq!(act_for(11), None, "not an act's first mission");
    }

    /// Test vectors: the endings.
    #[test]
    fn endings() {
        assert_eq!(
            after_score(GameKind::Campaign(26), true),
            Next::Ending {
                bdp: false,
                race: HUMAN
            }
        );
        let e = ending(false, HUMAN);
        assert_eq!((e.video, e.finale_entry, e.credits), (8, 1, "rez\\crdt_lst.tbl"));
        assert_eq!(
            after_score(GameKind::Campaign(51), true),
            Next::Ending { bdp: true, race: ORC }
        );
        let e = ending(true, ORC);
        assert_eq!((e.video, e.finale_entry, e.narration.len()), (12, 4, 3));
        assert_eq!(ending(false, ORC).finale_entry, 2);
        assert_eq!(ending(true, HUMAN).finale_entry, 3);
        assert_eq!(e.music, 17);
        assert_eq!(after_score(GameKind::Campaign(50), false), Next::Replay(50));
    }

    /// Test vector: briefing for BDP human 4.
    #[test]
    fn briefing_for_bdp_human_4() {
        let e = tidewater_sim::victory::mission_entry(true, false, 4);
        assert_eq!(e, 34);
        assert_eq!(
            narration_files(e),
            [
                "Speech\\war2x\\hum4_1.wav",
                "Speech\\war2x\\hum4_2.wav",
                "Speech\\war2x\\hum4_3.wav"
            ]
        );
        assert_eq!(story_table(e), "rez\\2xhum4.tbl");
        assert_eq!(objective_entries(e), (94, 35));
        assert_eq!(briefing_dialog(HUMAN), "rez\\hdisptch.bin");
        assert_eq!(briefing_music(HUMAN), 20);
        assert_eq!(briefing_music(ORC), 21);
    }

    #[test]
    fn narration_files_and_parts() {
        assert_eq!(narration_files(0), ["Speech\\hum_1_1.wav", "Speech\\hum_1_2.wav"]);
        assert_eq!(narration_files(1), ["Speech\\orc_1_1.wav"]);
        assert_eq!(narration_parts(4), 2, "orc 3 has two");
        assert_eq!(narration_parts(10), 1, "human 6 has one");
        assert_eq!(narration_parts(43), 1, "BDP orc 8 has one");
        assert_eq!(narration_parts(29), 3, "BDP orc 1 has three");
        assert_eq!(narration_parts(30), 2, "BDP human 2 has two");
        let total: u32 = (0..52).map(|e| narration_parts(e) as u32).sum();
        assert_eq!(total, 23 + 19 + 28 + 27, "ToD human, ToD orc, BDP human, BDP orc");
    }

    /// Test vector: narration parts of 10,000 and 5,000 ms.
    #[test]
    fn narration_total_and_scroll() {
        assert_eq!(narration_total_ms(&[10_000, 5_000]), 15_255);
        assert_eq!(narration_total_ms(&[]), 0);
        assert_eq!(narration_starts(&[10_000, 5_000]), [0, 10_255]);
        // Text 400 px, panel 200 px, line 10 px: 595 steps of 15255 / 595 = 25 ms.
        assert_eq!(scroll_steps(400, 200, 10), 595);
        assert_eq!(scroll_interval_ms(15_255, 595), 25);
        assert_eq!(scroll_interval_ms(100, 595), 100, "0 becomes 100 ms");
        let mut s = Scroll::new(15_255, 400, 200, 10);
        s.at(25 * 594);
        assert!(!s.done());
        s.at(25 * 595);
        assert!(s.done());
        assert!(!briefing_closes(&s, 14_875, 15_255), "the narration is still playing");
        assert!(briefing_closes(&s, 15_255, 15_255));
    }

    #[test]
    fn briefing_backgrounds() {
        assert_eq!(briefing_background(0), "Art\\hdspatch.pcx");
        assert_eq!(briefing_background(1), "Art\\slash.pcx");
        assert_eq!(briefing_background(13), "Art\\odspatch.pcx");
        assert_eq!(briefing_background(14), "Art\\dethnite.pcx");
        assert_eq!(briefing_background(21), "Art\\mage.pcx");
        assert_eq!(briefing_background(22), "Art\\dragon.pcx");
        assert_eq!(briefing_background(27), "Art\\griffon.pcx");
        assert_eq!(
            briefing_background(50),
            "Art\\dragon.pcx",
            "BDP mission 12 uses the same table"
        );
    }

    #[test]
    fn act_countdown() {
        let mut t = ActTimer::default();
        assert!(!t.may_close_early(), "not on its first tick");
        assert!(!t.picture_shown());
        let events: Vec<_> = (0..60).map(|_| t.tick()).collect();
        assert!(t.may_close_early());
        assert_eq!(events[9], Some(ActEvent::Picture), "at 50");
        assert_eq!(events[59], Some(ActEvent::Close), "at 0");
        assert_eq!(events.iter().flatten().count(), 2);
        assert_eq!(act_sound(HUMAN), "Sfx\\hact.wav");
        assert_eq!(act_sound(ORC), "Sfx\\oact.wav");
    }

    #[test]
    fn act_palettes_come_from_act_one_of_the_same_side() {
        assert_eq!(act_palette_picture(&act_for(22).unwrap()), "umaact1");
        assert_eq!(act_palette_picture(&act_for(15).unwrap()), "orcact1");
        assert_eq!(act_palette_picture(&act_for(47).unwrap()), "2xoact1");
        assert_eq!(act_palette_picture(&act_for(46).unwrap()), "2xhact1");
        assert_eq!(ACTS.iter().filter(|a| a.video.is_some()).count(), 6);
    }

    /// Test vectors: ranks.
    #[test]
    fn ranks() {
        assert_eq!(rank_entry(0, HUMAN, false), 1);
        assert_eq!(rank_entry(0, ORC, false), 2);
        assert_eq!(rank_entry(2000, HUMAN, false), 3);
        assert_eq!(rank_entry(299_999, ORC, false), 38);
        assert_eq!(rank_entry(300_000, HUMAN, false), 37);
        assert_eq!(rank_entry(5_000, HUMAN, true), 39);
    }

    /// Test vectors: mission 2, total 4000 at the start, kills worth 900; won, then lost.
    #[test]
    fn the_campaign_total() {
        let mut t = CampaignTotal {
            total: 4000,
            at_start: 0,
        };
        t.start_game(GameKind::Campaign(2));
        assert_eq!(t.at_start, 4000);
        let mut won = t;
        assert_eq!(won.close(900, true), 1400);
        assert_eq!(won.total, 5400);
        assert_eq!(score_step(won.total), 540);
        assert_eq!(counted(5400, 540, 3), 1620);
        assert_eq!(counted(5400, 540, 11), 5400);
        let mut lost = t;
        assert_eq!(lost.close(900, false), 900);
        lost.defeat();
        assert_eq!(lost.total, 4000, "back to 4000 for the replay");
        lost.start_game(GameKind::Campaign(2));
        assert_eq!(lost.at_start, 4000);
    }

    #[test]
    fn a_restart_replays_with_the_total_put_back() {
        let mut t = CampaignTotal {
            total: 4000,
            at_start: 0,
        };
        t.start_game(GameKind::Campaign(2));
        t.total = 4700;
        t.restart();
        assert_eq!(t.total, 4000);
        assert_eq!(after_restart(GameKind::Campaign(2)), Next::Replay(2));
    }

    /// Test vector: custom game won.
    #[test]
    fn a_custom_game_starts_at_zero_and_returns_to_the_menu() {
        let mut t = CampaignTotal {
            total: 9000,
            at_start: 9000,
        };
        t.start_game(GameKind::Custom);
        assert_eq!(t.total, 0);
        let mut first = CampaignTotal {
            total: 9000,
            at_start: 0,
        };
        first.start_game(GameKind::Campaign(28));
        assert_eq!(first.total, 0, "a campaign's first mission");
        assert_eq!(after_score(GameKind::Custom, true), Next::MainMenu);
        assert_eq!(after_score(GameKind::Multiplayer, false), Next::Multiplayer);
    }

    /// Test vectors: rows and names.
    #[test]
    fn rows_labels_and_names() {
        let mut c = [controller::NOBODY; 8];
        c[0] = controller::HUMAN;
        c[1] = controller::COMPUTER;
        c[2] = controller::COMPUTER;
        c[3] = controller::PASSIVE;
        let allied = |q: u8| q == 1;
        assert_eq!(row_label(0, 0, &c, allied), Some(34));
        assert_eq!(row_label(1, 0, &c, allied), Some(36), "an allied computer");
        assert_eq!(row_label(2, 0, &c, allied), Some(35));
        assert_eq!(row_label(3, 0, &c, allied), None);
        assert_eq!(name_entry(0, ORC, true), 94);
        assert_eq!(name_entry(3, ORC, true), 97);
        assert_eq!(name_entry(0, HUMAN, true), 50);
        assert_eq!(name_entry(7, ORC, false), 61);
        assert_eq!(score_dialog(2), "rez\\msnstat2.bin");
        assert_eq!(score_dialog(3), "rez\\msnstat4.bin");
        assert_eq!(score_dialog(6), "rez\\msnstat6.bin");
        assert_eq!(score_dialog(7), "rez\\msnstats.bin");
    }

    #[test]
    fn bars_grow_in_fortieths() {
        assert_eq!(bar_step(400), 10);
        assert_eq!(bar_step(39), 1);
        assert_eq!(bar_step(0), 1);
        assert_eq!(counted(25, bar_step(400), 2), 20);
        assert_eq!(counted(25, bar_step(400), 3), 25);
        assert_eq!(counted(7, score_step(7), 0), 7, "a step of 0 shows the total at once");
    }

    #[test]
    fn dialog_buttons() {
        assert_eq!(dialog_button(DialogButton::Continue), DialogResult::End);
        assert_eq!(dialog_button(DialogButton::Save), DialogResult::OpenSave);
        assert!(
            !after_save(false),
            "Save, then cancel: the victory dialog is shown again"
        );
        assert_eq!(dialog_picture(ORC), "art\\o144x64.bmp");
    }

    #[test]
    fn music_files() {
        assert_eq!(
            music_file(TITLE_MUSIC),
            music_file(21),
            "the title shares the orc briefing's file"
        );
        assert_eq!(music_file(14), Some("Music\\HVICTORY.WAV"));
        assert_eq!(music_file(1), None);
    }

    #[test]
    fn videos() {
        assert_eq!(video_path(0), "Smk\\Blizzard.smk");
        assert_eq!(video_path(video::BDP_INTRO), "Smk\\IntroX_M.smk");
        assert_eq!(video_path(12), "Smk\\OrcX_M.smk");
        assert_eq!(text_index(1), 0);
    }

    #[test]
    fn wrapping() {
        let w = |s: &str| s.len() as u32;
        assert_eq!(wrap("aa bb cc", 5, &w), ["aa bb", "cc"]);
        assert_eq!(wrap("a\n\nb", 9, &w), ["a", "", "b"]);
        assert_eq!(wrap("x\u{4}y\u{1} z", 9, &w), ["xy z"]);
        assert_eq!(wrap("toolongword b", 3, &w), ["toolongword", "b"]);
    }
}
