//! The campaign's flow around a game: startup videos and title, the new-campaign choices, act screens, briefings,
//! the victory and defeat dialogs, the score screen, what comes after it and the endings. Spec:
//! `docs/specs/game/campaign-flow.md`. Mission entries are those of `docs/specs/game/victory.md` ("Mission rules").
//!
//! Game text is named by string table and entry number counting from 1, as in the spec; [`text_index`] turns an
//! entry into a table index. No SDL types here.
//!
//! Defaults for the spec's open questions:
//! - The skip setting and the setting that skips the startup videos are off: the developer words that turn them on
//!   (`game/cheats.md`) aren't implemented yet.
//! - The movie archive is always available (a video the install lacks is skipped); the startup intro plays when the
//!   "play the intro" bit is set, which is every start (it isn't stored; `--no-intro` clears it).
//! - Ctrl isn't read at startup (the window doesn't exist yet), only at the later checks.
//! - The menu timer ([`MenuTimer`]) gives its first tick at once, as if the last one were long ago; when the clock went
//!   back it restarts with a tick at once (the next tick due at the current time).
//! - A fade blocks for its whole frame schedule: one 16 ms frame per frame time ([`fade_blocks_ms`]), which gives the
//!   spec's 272, 320, 416 and 624 ms; a key or click during a blocking fade waits and is handled when it ends.
//! - The title's music starts when its fade-in ends (the fade blocks, and the spec names the music after it).
//! - The ending videos are `HVict_M` / `OVict_M` (ToD) and `HVicX_M` / `OrcX_M` (BDP), before the finale.
//! - A type change doesn't count as a building lost; gathered resources are deliveries (`tidewater_sim`).
//! - A restart from the game menu puts the campaign total back as a defeat does.
//! - No cheats can be entered yet, so the rank is never the cheat rank in play.
//! - The multiplayer score screen's rows are built by [`mp_rows`] (`ui/multiplayer-score.md`: the local player's row
//!   first, then slots 0 to 7, name first). No network game exists yet, so nothing builds one in play; a row is made
//!   at the end of the game for each human still connected (spec gap: the row made when a player leaves early needs
//!   the leave notices, `net/leave-and-drop.md`).

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
/// The main menu's picture ("Startup", step 3): the main menu and every menu reached from it are drawn over it, in its
/// own palette (`ui/screen-and-cursors.md`, "Menu screens"). It isn't the title picture.
pub const MENU_PICTURE: &str = "Art\\TitleMenu_BNE.pcx";
/// The title stays this many menu-timer ticks unless a key or click ends it.
pub const TITLE_TICKS: u32 = 100;
/// The title's fade in, and its fade out when it ran its time.
pub const TITLE_FADE_MS: u32 = 250;
/// The title's music track.
pub const TITLE_MUSIC: u8 = 22;

/// The title's fade-out time: 250 ms, or 1 ms when a key or click ended it.
pub fn title_fade_out_ms(skipped: bool) -> u32 {
    if skipped { 1 } else { TITLE_FADE_MS }
}

/// What the title asks its screen to do ("Startup", step 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleCue {
    /// The fade-in ended: music track 22 starts.
    Music,
    /// The picture fades out over this many ms.
    FadeOut(u32),
    /// The fade-out ended: the main menu opens.
    Done,
}

/// The title: a 250 ms fade-in, then 100 menu-timer ticks or a key or click (Shift, Ctrl and Alt alone don't count;
/// the caller filters them), then the fade-out. Times are ms on the menu timer's clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TitleRun {
    ticks: u32,
    /// The running fade blocks until then.
    blocked_until: Option<f64>,
    fading_out: bool,
    /// A key or click is waiting.
    input: bool,
    done: bool,
}

impl TitleRun {
    /// The title opens at `now`: its fade-in starts.
    pub fn new(now: f64) -> Self {
        TitleRun {
            ticks: 0,
            blocked_until: Some(now + fade_blocks_ms(TITLE_FADE_MS) as f64),
            fading_out: false,
            input: false,
            done: false,
        }
    }

    /// Runs the title at `now`; `input` is a key or click since the last call.
    pub fn update(&mut self, timer: &mut MenuTimer, now: f64, input: bool) -> Vec<TitleCue> {
        let mut out = Vec::new();
        self.input |= input;
        if self.done {
            return out;
        }
        if let Some(end) = self.blocked_until {
            if now < end {
                return out;
            }
            self.blocked_until = None;
            if self.fading_out {
                self.done = true;
                out.push(TitleCue::Done);
                return out;
            }
            out.push(TitleCue::Music);
        }
        // Waiting input is handled before ticks.
        let skipped = std::mem::take(&mut self.input);
        if !skipped {
            while self.ticks < TITLE_TICKS && timer.poll(now) {
                self.ticks += 1;
            }
            if self.ticks < TITLE_TICKS {
                return out;
            }
        }
        let ms = title_fade_out_ms(skipped);
        self.fading_out = true;
        self.blocked_until = Some(now + fade_blocks_ms(ms) as f64);
        out.push(TitleCue::FadeOut(ms));
        out
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The menu timer and fades ("Terms")

/// The menu timer's period in ms.
pub const MENU_TICK_MS: f64 = 100.0;
/// A tick later than this after its due time restarts the schedule.
pub const MENU_LATE_MS: f64 = 300.0;
/// A due time further ahead of the clock than this means the clock went back.
pub const MENU_AHEAD_MS: f64 = 200.0;

/// The 100 ms real-time menu timer that drives the title and the act screen ("Terms", "Menu timer"). The caller asks
/// for ticks with [`MenuTimer::poll`] only while no input is waiting and no fade runs, with a real-time clock in ms;
/// the game speed, the game cycle and the frame rate don't enter. One timer serves every screen, so its schedule
/// carries over from one screen to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuTimer {
    /// When the next tick is due; `None` before the first tick (our choice: as if the last were long ago).
    due: Option<f64>,
}

impl MenuTimer {
    /// Whether a tick comes at `now`. A tick makes the next one due 100 ms after this one was due, or at once if this
    /// one came more than 300 ms late; so late ticks come back to back until the timer catches up, and a long wait is
    /// not made up. A due time more than 200 ms ahead of `now` (the clock went back) restarts the schedule at `now`.
    pub fn poll(&mut self, now: f64) -> bool {
        let due = match self.due {
            None => f64::NEG_INFINITY,
            Some(d) if d - now > MENU_AHEAD_MS => now,
            Some(d) => d,
        };
        if now < due {
            self.due = Some(due);
            return false;
        }
        self.due = Some(if now - due > MENU_LATE_MS {
            now
        } else {
            due + MENU_TICK_MS
        });
        true
    }

    /// When the next tick is due, once a tick has come.
    pub fn due(&self) -> Option<f64> {
        self.due
    }
}

/// The least length of a fade frame in ms ("Terms", "Fade").
pub const FADE_FRAME_MS: u32 = 16;

/// How long a fade of `ms` blocks: 16 ms frames at fade times 0, 16, 32, ... and a last one at exactly `ms` ("Terms",
/// "Fade": about `ms` + 16; 250 takes 272, 300 takes 320, 400 takes 416 and 600 takes 624).
pub fn fade_blocks_ms(ms: u32) -> u32 {
    (1 + ms.div_ceil(FADE_FRAME_MS)) * FADE_FRAME_MS
}

/// A screen's close that fades all sound first (`ui/music.md`, "Where music fades out"; `ui/palette-effects.md`,
/// "Screen fades"): choosing a campaign (then 200 ms to black), the briefing closing (100 ms), the score screen closing
/// (150 ms) and the main menu leaving the menus (200 ms). The fade of all sound blocks with the screen unchanged; the
/// fade to black starts when it has ended, and the screen goes on when that has ended too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundThenBlack {
    /// The fade to black's length in ms.
    pub black_ms: u32,
    /// When the fade to black ends, once it has started.
    black_end: Option<f64>,
}

/// What a [`SoundThenBlack`] close asks for at a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseCue {
    /// Nothing yet: a fade still runs.
    Wait,
    /// The fade to black starts now, over this many ms.
    FadeToBlack(u32),
    /// Both fades have ended: the screen goes on.
    Done,
}

impl SoundThenBlack {
    /// A close whose fade to black takes `black_ms`. The caller starts the fade of all sound with it.
    pub fn new(black_ms: u32) -> Self {
        SoundThenBlack {
            black_ms,
            black_end: None,
        }
    }

    /// The close at screen time `now`, with `sound_fading` whether the fade of all sound still runs.
    pub fn update(&mut self, sound_fading: bool, now: f64) -> CloseCue {
        match self.black_end {
            None if sound_fading => CloseCue::Wait,
            None if self.black_ms == 0 => CloseCue::Done,
            None => {
                self.black_end = Some(now + self.black_ms as f64);
                CloseCue::FadeToBlack(self.black_ms)
            }
            Some(end) if now >= end => CloseCue::Done,
            Some(_) => CloseCue::Wait,
        }
    }
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
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
/// When the picture fades in (and, once that fade ends, the act sound plays).
pub const ACT_PICTURE_AT: u8 = 50;
/// The text's fade-in at 60, and the picture's at 50.
pub const ACT_TEXT_FADE_MS: u32 = crate::palette::fades::ACT_TEXT_IN;
pub const ACT_PICTURE_FADE_MS: u32 = crate::palette::fades::ACT_PICTURE_IN;
/// The fade to black when the screen closes ("Closing").
pub const ACT_CLOSE_FADE_MS: u32 = crate::palette::fades::ACT_CLOSE_OUT;
/// The palette entries that hold the text colours (white while the rest is black, at 60).
pub const ACT_TEXT_COLOURS: std::ops::RangeInclusive<u8> = 1..=3;

/// What happens on an act screen tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActEvent {
    /// At 60: the text fades in over 300 ms.
    Text,
    /// At 50: the picture fades in over 600 ms; the act sound plays when that fade ends.
    Picture,
    /// The countdown reached 0.
    Close,
}

/// The act screen's countdown from 60 to 0, one step per menu-timer tick: the tick at 60 fades the text in, the one at
/// 50 the picture, and the one at 1 brings the countdown to 0, which closes the screen.
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
        let at = self.count;
        self.ticks += 1;
        self.count = self.count.saturating_sub(1);
        match (at, self.count) {
            (ACT_TICKS, _) => Some(ActEvent::Text),
            (ACT_PICTURE_AT, _) => Some(ActEvent::Picture),
            (_, 0) => Some(ActEvent::Close),
            _ => None,
        }
    }

    /// Esc, Enter, Space or a click close the screen, but not on its first tick (nor before it).
    pub fn may_close_early(&self) -> bool {
        self.ticks >= 1
    }

    /// Whether the picture is showing (its fade began at 50).
    pub fn picture_shown(&self) -> bool {
        self.count < ACT_PICTURE_AT
    }
}

/// What the act screen asks its screen to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActCue {
    /// The text fades in over 300 ms (the rest stays black).
    TextFadeIn,
    /// The picture fades in over 600 ms.
    PictureFadeIn,
    /// The picture's fade ended: the act sound plays.
    ActSound,
    /// The screen closes: everything fades to black over 400 ms.
    FadeToBlack,
    /// The fade to black ended: a fade of all sound (`ui/music.md`). Once that has ended, the video after (if any)
    /// or the briefing follows.
    FadeAllSound,
}

/// The act screen's run ("Act screens": "Timing", "Closing", "Closing early"): the countdown on the menu timer, the
/// blocking fades, waiting input and the close. Times are ms on the menu timer's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActRun {
    pub timer: ActTimer,
    /// The running fade blocks until then, and this cue follows it.
    blocked: Option<(f64, Option<ActCue>)>,
    /// A key or click is waiting.
    input: bool,
    closing: bool,
    done: bool,
}

impl ActRun {
    /// Runs the screen at `now`; `input` is Esc, Enter, Space or a click since the last call.
    pub fn update(&mut self, menu: &mut MenuTimer, now: f64, input: bool) -> Vec<ActCue> {
        let mut out = Vec::new();
        self.input |= input;
        while !self.done {
            if let Some((end, then)) = self.blocked {
                if now < end {
                    break;
                }
                self.blocked = None;
                if let Some(c) = then {
                    out.push(c);
                    self.done = c == ActCue::FadeAllSound;
                    continue;
                }
            }
            // Waiting input is handled before ticks; before the first tick it is dropped.
            if std::mem::take(&mut self.input) && self.timer.may_close_early() {
                self.close(now, &mut out);
                continue;
            }
            if !menu.poll(now) {
                break;
            }
            match self.timer.tick() {
                Some(ActEvent::Text) => {
                    out.push(ActCue::TextFadeIn);
                    self.block(now, ACT_TEXT_FADE_MS, None);
                }
                Some(ActEvent::Picture) => {
                    out.push(ActCue::PictureFadeIn);
                    self.block(now, ACT_PICTURE_FADE_MS, Some(ActCue::ActSound));
                }
                Some(ActEvent::Close) => self.close(now, &mut out),
                None => {}
            }
        }
        out
    }

    fn block(&mut self, now: f64, ms: u32, then: Option<ActCue>) {
        self.blocked = Some((now + fade_blocks_ms(ms) as f64, then));
    }

    fn close(&mut self, now: f64, out: &mut Vec<ActCue>) {
        if !self.closing {
            self.closing = true;
            out.push(ActCue::FadeToBlack);
            self.block(now, ACT_CLOSE_FADE_MS, Some(ActCue::FadeAllSound));
        }
    }

    /// The screen is closing (its fade to black began).
    pub fn closing(&self) -> bool {
        self.closing
    }

    /// The fade to black ended and the fade of all sound began.
    pub fn done(&self) -> bool {
        self.done
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

/// A button on the victory or defeat dialog, by the dialog data's names ("The victory and defeat dialogs"): the victory
/// dialog's "Victory" (hotkey V) and "Save Game" (hotkey F11), the defeat dialog's "OK" (hotkey O).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogButton {
    Victory,
    SaveGame,
    Ok,
}

impl DialogButton {
    /// The button's label.
    pub fn label(self) -> &'static str {
        match self {
            DialogButton::Victory => "Victory",
            DialogButton::SaveGame => "Save Game",
            DialogButton::Ok => "OK",
        }
    }

    /// The button that ends the game: "Victory" on the victory dialog, "OK" on the defeat dialog.
    pub fn ending(victory: bool) -> DialogButton {
        if victory {
            DialogButton::Victory
        } else {
            DialogButton::Ok
        }
    }
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
        DialogButton::Victory | DialogButton::Ok => DialogResult::End,
        DialogButton::SaveGame => DialogResult::OpenSave,
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
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

    /// Player `p`'s statistics at the end of a game: units and buildings are those alive now plus those lost, both
    /// read from the per-type counts (only completed units and buildings of a type that has one; spec "Statistics").
    pub fn of_world(w: &World, p: u8) -> Stats {
        let alive = |building: bool| {
            w.units
                .iter()
                .filter(|u| u.owner == p && u.building == building && !u.dying && !u.removing && World::counted(u))
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

/// How a multiplayer score row names its player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MpWho {
    /// A human player: the name the session holds for the slot.
    Human,
    /// A computer player: the nation name, a `network.tbl` entry by slot and race.
    Nation(u16),
}

/// One row of the multiplayer score screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MpRow {
    pub slot: u8,
    pub who: MpWho,
    /// "Ally" (36) or "Enemy" (35); none for the local player's own row.
    pub label: Option<u16>,
    pub stats: Stats,
}

/// The rows of a multiplayer score screen (`ui/multiplayer-score.md`, "Which players get a row", "Row text", "Row
/// order"): the local player first, then the other human and computer slots in slot order, 0 to 7. A computer player
/// is always "Enemy"; a human is "Ally" when the local player counts it allied. `start_controllers` are the slots'
/// controllers when the game started.
pub fn mp_rows(
    local: u8,
    start_controllers: &[u8; 8],
    races: &[u8; 8],
    allied: impl Fn(u8) -> bool,
    stats: impl Fn(u8) -> Stats,
) -> Vec<MpRow> {
    std::iter::once(local)
        .chain((0..8u8).filter(|&s| s != local))
        .filter_map(|slot| {
            let race = races[slot as usize];
            let (who, label) = match start_controllers[slot as usize] {
                _ if slot == local => (MpWho::Human, None),
                controller::HUMAN => (
                    MpWho::Human,
                    Some(if allied(slot) { label::ALLY } else { label::ENEMY }),
                ),
                controller::COMPUTER => (MpWho::Nation(name_entry(slot, race, false)), Some(label::ENEMY)),
                _ => return None,
            };
            Some(MpRow {
                slot,
                who,
                label,
                stats: stats(slot),
            })
        })
        .collect()
}

/// A multiplayer row's text: the name first, then " - " and the label for every row but the local player's.
pub fn mp_row_text(name: &str, label: Option<&str>) -> String {
    match label {
        Some(l) => format!("{name} - {l}"),
        None => name.to_string(),
    }
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
    /// After a multiplayer game's restart (not in the specs): the multiplayer screens, which Tidewater doesn't have.
    Multiplayer,
    /// A multiplayer game's score screen is followed by the lobby of the same session (`ui/multiplayer-score.md`,
    /// "What follows the score screen", MS-V8; `game/campaign-flow.md`). The music there: none starts (`ui/music.md`).
    Lobby,
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
        GameKind::Multiplayer => Next::Lobby,
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
/// The credits' own picture, whose palette they are shown in (`ui/screen-and-cursors.md`, "Pointers outside a game").
pub const CREDITS_PICTURE: &str = "Art\\credits.pcx";

// ---------------------------------------------------------------------------------------------------------------
// The credits screen (spec "Credits screen", round 72)

/// The credits screen's text: `rez\creditsBNE.tbl`. The spec numbers this table's entries from 0 (its title and body
/// pairs are entries 0 and 1, 2 and 3, 4 and 5 of the table's six), so they are table indices here.
pub const CREDITS_TABLE: &str = "rez\\creditsBNE.tbl";

/// A page of the credits screen: its picture and the table indices of its title and its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreditsPage {
    pub picture: &'static str,
    pub title: usize,
    pub body: usize,
}

/// The pages in order: when a page's body has scrolled off the top, the next replaces it (page 2 is the Beyond the Dark
/// Portal credits on `Art\2xcredit.pcx`). What follows page 2 wasn't watched (spec "Credits after page 2"; entries 4
/// and 5 aren't shown): the screen closes when its body has scrolled off, as the credits did before the spec.
pub const CREDITS_PAGES: [CreditsPage; 2] = [
    CreditsPage {
        picture: CREDITS_PICTURE,
        title: 0,
        body: 1,
    },
    CreditsPage {
        picture: "Art\\2xcredit.pcx",
        title: 2,
        body: 3,
    },
];

/// The rows text shows in, inclusive: a line crossing either is cut there. There is no horizontal cut.
pub const CREDITS_ROWS: (i32, i32) = (80, 354);
/// The title's font, and the body's.
pub const CREDITS_TITLE_FONT: &str = "Art\\font32.fnt";
pub const CREDITS_BODY_FONT: &str = "Art\\font12x.fnt";
/// The left edge of a title line and of a body line, and of a body line that begins with a tab (a 34-pixel indent).
pub const CREDITS_X: i32 = 140;
pub const CREDITS_TAB_X: i32 = 174;
/// The title's line pitch (font32's height) and the body's (an empty line takes one too).
pub const CREDITS_TITLE_PITCH: i32 = 35;
pub const CREDITS_BODY_PITCH: i32 = 25;
/// The title's first cell top on the first full frame after the fade-in, before the text has moved.
pub const CREDITS_TITLE_TOP: i32 = 274;
/// How far below the title's first cell top the body's first line (after its leading newlines) has its cell top.
pub const CREDITS_BODY_BELOW_TITLE: i32 = 129;
/// The pixels a space leaves between the glyphs either side of it: body font (font12x) and title font (font32).
pub const CREDITS_BODY_SPACE: u32 = 10;
pub const CREDITS_TITLE_SPACE: u32 = 18;
/// The width the title wraps at, from [`CREDITS_X`]. Not measured (spec "Credits title wrap": between about 315 and 450
/// pixels); our choice is 360, the column from x 140 to 499 that leaves 140 pixels on either side, which gives the
/// two-line titles the spec saw.
pub const CREDITS_TITLE_WRAP: u32 = 360;
/// The colours of the fonts' shades: the face (1 to 4) in greys, the shadow (5) black. Shades 6 and 7 aren't used by
/// the fonts; black.
pub const CREDITS_SHADES: [[u8; 3]; 8] = [
    [0, 0, 0],
    [255, 255, 255],
    [219, 219, 219],
    [187, 187, 187],
    [155, 155, 155],
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
];

/// The advance of a space that leaves `gap` pixels between the glyphs either side of it: a glyph's own advance already
/// leaves 1 pixel after it (`fnt`).
pub fn credits_space_advance(gap: u32) -> u32 {
    gap.saturating_sub(1)
}

/// One line of a credits page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditsLine {
    pub text: String,
    pub title: bool,
    /// Its cell's left edge.
    pub x: i32,
    /// Its cell top before the text has moved, from the page's title top.
    pub y: i32,
}

/// A credits page's lines: the title wrapped by `wrap` (to [`CREDITS_TITLE_WRAP`]) at [`CREDITS_TITLE_PITCH`], then the
/// body from [`CREDITS_BODY_BELOW_TITLE`] at [`CREDITS_BODY_PITCH`], an empty line taking one pitch and a line that
/// begins with a tab starting at [`CREDITS_TAB_X`]. The body's leading newlines are where the spec's offset is
/// measured from, so they take no room; its trailing empty lines are left out. Body lines aren't wrapped (the spec
/// gives none; the table's fit).
pub fn credits_lines(title: &str, body: &str, wrap: &dyn Fn(&str) -> Vec<String>) -> Vec<CreditsLine> {
    let mut out: Vec<CreditsLine> = wrap(title)
        .into_iter()
        .enumerate()
        .map(|(i, text)| CreditsLine {
            text,
            title: true,
            x: CREDITS_X,
            y: i as i32 * CREDITS_TITLE_PITCH,
        })
        .collect();
    let body = body.trim_start_matches('\n').trim_end_matches(['\n', '\0']);
    if body.is_empty() {
        return out;
    }
    for (i, line) in body.split('\n').enumerate() {
        let (x, text) = match line.strip_prefix('\t') {
            Some(rest) => (CREDITS_TAB_X, rest),
            None => (CREDITS_X, line),
        };
        if text.is_empty() {
            continue;
        }
        out.push(CreditsLine {
            text: text.to_string(),
            title: false,
            x,
            y: CREDITS_BODY_BELOW_TITLE + i as i32 * CREDITS_BODY_PITCH,
        });
    }
    out
}

/// The steps (1 pixel each) after which a page's last line has scrolled off the top: its cell, `cell_h` high, has
/// left the text window. A page with no lines is done at once.
pub fn credits_page_steps(lines: &[CreditsLine], cell_h: i32) -> u32 {
    lines
        .last()
        .map_or(0, |l| (CREDITS_TITLE_TOP + l.y + cell_h - CREDITS_ROWS.0).max(0) as u32)
}

/// The steps the text has moved `elapsed_ms` after it started: one per [`CREDITS_STEP_MS`].
pub fn credits_moved(elapsed_ms: f64) -> u32 {
    (elapsed_ms.max(0.0) / CREDITS_STEP_MS as f64) as u32
}

/// Where the credits are `elapsed_ms` after the first page's text started, given each page's [`credits_page_steps`]:
/// the page shown, the steps its text has moved (at most its own), and whether the last page is done. A page starts,
/// at its first position, on the step after the one that took the page before off; there is no fade between pages
/// (none was described).
pub fn credits_at(steps: &[u32], elapsed_ms: f64) -> (usize, u32, bool) {
    let mut moved = credits_moved(elapsed_ms);
    for (page, &n) in steps.iter().enumerate() {
        if moved < n || page + 1 == steps.len() {
            return (page, moved.min(n), moved >= n);
        }
        moved -= n;
    }
    (0, 0, true)
}

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
        assert_eq!(events[0], Some(ActEvent::Text), "at 60");
        assert_eq!(events[10], Some(ActEvent::Picture), "at 50");
        assert_eq!(events[59], Some(ActEvent::Close), "the tick at 1 brings it to 0");
        assert_eq!(events.iter().flatten().count(), 3);
        assert!(t.picture_shown());
        assert_eq!(act_sound(HUMAN), "Sfx\\hact.wav");
        assert_eq!(act_sound(ORC), "Sfx\\oact.wav");
    }

    #[test]
    fn a_close_fades_all_sound_then_the_screen() {
        // Choosing a campaign (round 74): the fade of all sound blocks, then 200 ms to black, then the act screen.
        let mut c = SoundThenBlack::new(200);
        assert_eq!(c.update(true, 0.0), CloseCue::Wait);
        assert_eq!(
            c.update(true, 500.0),
            CloseCue::Wait,
            "the screen stays while the sound fades"
        );
        assert_eq!(c.update(false, 520.0), CloseCue::FadeToBlack(200));
        assert_eq!(c.update(false, 700.0), CloseCue::Wait);
        assert_eq!(c.update(false, 720.0), CloseCue::Done);
        // Nothing playing: the fade of all sound returns at once and the screen fades out straight away.
        let mut c = SoundThenBlack::new(100);
        assert_eq!(c.update(false, 40.0), CloseCue::FadeToBlack(100));
        assert_eq!(c.update(false, 140.0), CloseCue::Done);
        assert_eq!(SoundThenBlack::new(0).update(false, 0.0), CloseCue::Done);
    }

    #[test]
    fn the_menu_timer_ticks_every_100_ms_of_its_due_time() {
        let mut m = MenuTimer::default();
        assert!(m.poll(1_000.0), "the first tick comes at once");
        assert!(m.poll(1_000.0), "and the schedule restarts: the next is due at once");
        assert!(!m.poll(1_099.0));
        assert!(m.poll(1_100.0));
        // Late by up to 300 ms: the late ticks come back to back until caught up.
        let ticks = std::iter::from_fn(|| m.poll(1_450.0).then_some(())).count();
        assert_eq!(ticks, 3, "due at 1200, 1300 and 1400");
        assert_eq!(m.due(), Some(1_500.0), "no time lost");
        // Exactly 300 ms late still catches up.
        assert!(m.poll(1_800.0));
        assert_eq!(m.due(), Some(1_600.0));
    }

    #[test]
    fn a_tick_more_than_300_ms_late_restarts_the_schedule() {
        let mut m = MenuTimer::default();
        m.poll(0.0);
        m.poll(0.0);
        assert_eq!(m.due(), Some(100.0));
        assert!(m.poll(401.0), "301 ms late");
        assert_eq!(m.due(), Some(401.0), "the next tick is due at once");
        assert!(m.poll(401.0));
        assert_eq!(m.due(), Some(501.0), "the lost time isn't made up");
        assert!(!m.poll(401.0));
    }

    #[test]
    fn the_clock_going_back_restarts_the_schedule_at_the_current_time() {
        let mut m = MenuTimer::default();
        m.poll(1_000.0);
        m.poll(1_000.0);
        assert_eq!(m.due(), Some(1_100.0));
        assert!(!m.poll(900.0), "200 ms ahead: wait");
        assert!(m.poll(899.0), "more than 200 ms ahead: due now");
        assert_eq!(m.due(), Some(999.0));
    }

    #[test]
    fn fades_block_for_their_frames() {
        assert_eq!(fade_blocks_ms(250), 272);
        assert_eq!(fade_blocks_ms(300), 320);
        assert_eq!(fade_blocks_ms(400), 416);
        assert_eq!(fade_blocks_ms(600), 624);
        assert_eq!(fade_blocks_ms(1), 32, "a frame at 0 and one at 1");
    }

    /// Runs an act screen opened at 0 ms, 1 ms at a time until its fade of all sound begins, with input at the given
    /// times. Returns the cues and the time of each tick by the countdown it was taken at (index 60 to 1).
    fn run_act(mut menu: MenuTimer, input_at: &[u32]) -> (Vec<(u32, ActCue)>, [Option<u32>; 61]) {
        let mut run = ActRun::default();
        let mut cues = Vec::new();
        let mut ticks = [None; 61];
        for t in 0..20_000u32 {
            let before = run.timer.count;
            for c in run.update(&mut menu, t as f64, input_at.contains(&t)) {
                cues.push((t, c));
            }
            // At most one fade per update, so a tick at countdown n is the only change from it.
            for n in (run.timer.count + 1..=before).rev() {
                ticks[n as usize] = Some(t);
            }
            if run.done() {
                break;
            }
        }
        (cues, ticks)
    }

    #[test]
    fn act_screen_timeline_vector() {
        // "Act screen timeline": the screen opens long after the last menu-timer tick.
        let (cues, ticks) = run_act(MenuTimer::default(), &[]);
        assert_eq!(
            cues,
            [
                (0, ActCue::TextFadeIn),
                (1_120, ActCue::PictureFadeIn),
                (1_744, ActCue::ActSound),
                (6_444, ActCue::FadeToBlack),
                (6_860, ActCue::FadeAllSound),
            ]
        );
        assert_eq!(ticks[60], Some(0));
        assert_eq!(
            (ticks[59], ticks[58]),
            (Some(320), Some(320)),
            "back to back after the text fade"
        );
        assert_eq!((ticks[57], ticks[51]), (Some(420), Some(1_020)));
        assert_eq!(ticks[50], Some(1_120));
        assert_eq!(
            (ticks[49], ticks[48]),
            (Some(1_744), Some(1_744)),
            "back to back after the picture fade"
        );
        assert_eq!((ticks[47], ticks[1]), (Some(1_844), Some(6_444)));
        // The orc act sound (110,852 samples at 22,050 Hz) has ended when the fade of all sound begins.
        let sound_ends = cues[2].0 + 110_852 * 1_000 / 22_050;
        assert_eq!(sound_ends, 6_771, "about 6.77 s");
        assert!(sound_ends < cues[4].0);
        // The fade of all sound from full volume: its last step 540 ms later, so the briefing opens at about 7.4 s.
        let mut music = crate::music::Music::new(100);
        music.start(Some(20));
        music.fade_stop(true, None);
        let ms = (1..).find(|_| music.update(1, None).contains(&crate::music::Action::Stop));
        assert_eq!(ms.map(|ms| 6_860 + ms), Some(7_400));
    }

    #[test]
    fn act_screen_opened_soon_after_a_tick_catches_up_instead() {
        let mut menu = MenuTimer::default();
        menu.poll(-150.0);
        menu.poll(-150.0);
        assert_eq!(menu.due(), Some(-50.0));
        let (cues, ticks) = run_act(menu, &[]);
        assert_eq!(ticks[60], Some(0));
        assert_eq!((ticks[59], ticks[58], ticks[57]), (Some(320), Some(320), Some(320)));
        assert_eq!(ticks[56], Some(350));
        let close = cues.iter().find(|c| c.1 == ActCue::FadeToBlack).unwrap().0;
        assert_eq!(close, 6_274, "0.17 s earlier, within the spec's 6.24 to 6.32 s");
    }

    #[test]
    fn a_key_closes_the_act_screen_but_not_before_its_first_tick() {
        // A key with the first tick is dropped.
        let (cues, _) = run_act(MenuTimer::default(), &[0]);
        assert_eq!(cues[0], (0, ActCue::TextFadeIn));
        assert_eq!(cues[3].1, ActCue::FadeToBlack);
        assert_eq!(cues[3].0, 6_444);
        // A key during the text fade waits for it, then closes: no picture, no act sound.
        let (cues, ticks) = run_act(MenuTimer::default(), &[100]);
        assert_eq!(
            cues,
            [
                (0, ActCue::TextFadeIn),
                (320, ActCue::FadeToBlack),
                (736, ActCue::FadeAllSound)
            ]
        );
        assert_eq!(ticks[59], None, "input is handled before ticks");
        // A key after the picture: the act sound still plays until the fade to black ends.
        let (cues, _) = run_act(MenuTimer::default(), &[2_000]);
        assert_eq!(
            &cues[2..],
            [
                (1_744, ActCue::ActSound),
                (2_000, ActCue::FadeToBlack),
                (2_416, ActCue::FadeAllSound)
            ]
        );
    }

    fn run_title(input_at: Option<u32>) -> Vec<(u32, TitleCue)> {
        let mut menu = MenuTimer::default();
        let mut run = TitleRun::new(0.0);
        let mut out = Vec::new();
        for t in 0..20_000u32 {
            for c in run.update(&mut menu, t as f64, input_at == Some(t)) {
                out.push((t, c));
                if c == TitleCue::Done {
                    return out;
                }
            }
        }
        out
    }

    #[test]
    fn title_runs_100_ticks_after_its_fade_in() {
        // The fade-in blocks 272 ms; the first tick comes at once, the next back to back, then 98 × 100 ms.
        assert_eq!(
            run_title(None),
            [
                (272, TitleCue::Music),
                (10_072, TitleCue::FadeOut(250)),
                (10_344, TitleCue::Done)
            ]
        );
        assert_eq!(
            run_title(Some(5_000)),
            [
                (272, TitleCue::Music),
                (5_000, TitleCue::FadeOut(1)),
                (5_032, TitleCue::Done)
            ]
        );
        // A key during the fade-in waits for it.
        assert_eq!(
            run_title(Some(100)),
            [
                (272, TitleCue::Music),
                (272, TitleCue::FadeOut(1)),
                (304, TitleCue::Done)
            ]
        );
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
        assert_eq!(after_score(GameKind::Multiplayer, false), Next::Lobby);
        assert_eq!(
            after_score(GameKind::Multiplayer, true),
            Next::Lobby,
            "MS-V8: the creator, after a victory"
        );
        assert_eq!(after_restart(GameKind::Multiplayer), Next::Multiplayer);
    }

    /// MS-V1 and MS-V8: the local player's row first, names first, "Ally" and "Enemy" labels, computers always Enemy.
    #[test]
    fn multiplayer_rows_put_the_local_player_first() {
        let mut c = [controller::NOBODY; 8];
        c[0] = controller::HUMAN;
        c[1] = controller::HUMAN;
        c[2] = controller::HUMAN;
        c[3] = controller::COMPUTER;
        c[4] = controller::PASSIVE;
        let mut races = [HUMAN; 8];
        races[3] = ORC;
        let rows = mp_rows(1, &c, &races, |q| q == 0, |_| Stats::default());
        let order: Vec<u8> = rows.iter().map(|r| r.slot).collect();
        assert_eq!(order, [1, 0, 2, 3]);
        let labels: Vec<Option<u16>> = rows.iter().map(|r| r.label).collect();
        assert_eq!(labels, [None, Some(36), Some(35), Some(35)]);
        assert_eq!(rows[3].who, MpWho::Nation(name_entry(3, ORC, false)));
        assert_eq!(mp_row_text("Bob", None), "Bob");
        assert_eq!(mp_row_text("Ann", Some("Ally")), "Ann - Ally");
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
        assert_eq!(dialog_button(DialogButton::Victory), DialogResult::End);
        assert_eq!(dialog_button(DialogButton::Ok), DialogResult::End);
        assert_eq!(dialog_button(DialogButton::SaveGame), DialogResult::OpenSave);
        // The names from the dialog data (round 70).
        assert_eq!(DialogButton::ending(true).label(), "Victory");
        assert_eq!(DialogButton::ending(false).label(), "OK");
        assert_eq!(DialogButton::SaveGame.label(), "Save Game");
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

    // The spec's "Credits screen" (round 72).

    #[test]
    fn credits_page_lines_are_placed_as_the_spec_measured() {
        // A title wrapped onto two lines, then a body with the table's two leading newlines, a heading, a tabbed name,
        // a blank line, another heading, and trailing newlines.
        let wrap_at_space = |s: &str| s.splitn(2, ' ').map(str::to_string).collect::<Vec<_>>();
        let lines = credits_lines("First Second", "\n\nHead\n\tName\n\nNext\n\n\n", &wrap_at_space);
        let got: Vec<(&str, bool, i32, i32)> = lines.iter().map(|l| (l.text.as_str(), l.title, l.x, l.y)).collect();
        assert_eq!(
            got,
            [
                // Title: x 140, pitch 35.
                ("First", true, 140, 0),
                ("Second", true, 140, 35),
                // Body: first line 129 below the title's first cell top; pitch 25; a tab starts at x 174.
                ("Head", false, 140, 129),
                ("Name", false, 174, 154),
                // An empty line takes one pitch: the heading after it is 50 below the line before it.
                ("Next", false, 140, 204),
            ]
        );
    }

    #[test]
    fn credits_title_starts_inside_the_window_and_the_text_moves_one_step_per_40_ms() {
        assert_eq!(CREDITS_TITLE_TOP, 274);
        assert_eq!(CREDITS_ROWS, (80, 354));
        assert!(CREDITS_ROWS.0 < CREDITS_TITLE_TOP && CREDITS_TITLE_TOP < CREDITS_ROWS.1);
        assert_eq!(credits_moved(0.0), 0);
        assert_eq!(credits_moved(39.9), 0);
        assert_eq!(credits_moved(40.0), 1);
        assert_eq!(credits_moved(41.0 * 37.0), 37);
        assert_eq!(credits_moved(-5.0), 0, "nothing moves before the start (the fade-in)");
    }

    #[test]
    fn a_credits_page_is_done_when_its_last_line_has_left_the_window() {
        let line = |y| CreditsLine {
            text: "x".into(),
            title: false,
            x: CREDITS_X,
            y,
        };
        // Last cell top at 274 + 204 = 478; with a 17-pixel cell its bottom passes row 80 after 478 + 17 - 80 steps.
        assert_eq!(credits_page_steps(&[line(0), line(204)], 17), 415);
        assert_eq!(credits_page_steps(&[], 17), 0);
    }

    #[test]
    fn credits_spaces_and_colours() {
        // A glyph leaves 1 pixel after itself, so a space that leaves 10 (font12x) or 18 (font32) advances 9 or 17.
        assert_eq!(credits_space_advance(CREDITS_BODY_SPACE), 9);
        assert_eq!(credits_space_advance(CREDITS_TITLE_SPACE), 17);
        assert_eq!(
            CREDITS_SHADES[1..6],
            [
                [255, 255, 255],
                [219, 219, 219],
                [187, 187, 187],
                [155, 155, 155],
                [0, 0, 0]
            ]
        );
        assert_eq!((CREDITS_X, CREDITS_TAB_X), (140, 174));
        assert_eq!((CREDITS_TITLE_PITCH, CREDITS_BODY_PITCH), (35, 25));
        assert_eq!(
            (CREDITS_TITLE_FONT, CREDITS_BODY_FONT),
            ("Art\\font32.fnt", "Art\\font12x.fnt")
        );
    }

    #[test]
    fn credits_title_wraps_as_the_spec_saw() {
        // Pinned: our wrap width (not measured): 360 pixels from x 140. A synthetic measure of 15 pixels a character
        // stands in for the font: 21 characters (315) fit, 30 (450) don't, the spec's bounds.
        let w = |s: &str| s.len() as u32 * 15;
        assert_eq!(CREDITS_TITLE_WRAP, 360);
        assert_eq!(
            wrap("aaaaaaaaaa aaaaaaaaaa aaaaaaaa", CREDITS_TITLE_WRAP, &w),
            ["aaaaaaaaaa aaaaaaaaaa", "aaaaaaaa"]
        );
    }

    #[test]
    fn credits_screen_page_one() {
        assert_eq!(CREDITS_TABLE, "rez\\creditsBNE.tbl");
        assert_eq!(
            CREDITS_PAGES[0],
            CreditsPage {
                picture: "Art\\credits.pcx",
                title: 0,
                body: 1
            }
        );
    }

    #[test]
    fn credits_page_two_is_the_beyond_the_dark_portal_credits_and_follows_page_one() {
        assert_eq!(CREDITS_PAGES.len(), 2, "pinned: nothing after page 2 (not watched)");
        assert_eq!(
            CREDITS_PAGES[1],
            CreditsPage {
                picture: "Art\\2xcredit.pcx",
                title: 2,
                body: 3
            }
        );
        let steps = [10, 5];
        assert_eq!(credits_at(&steps, 0.0), (0, 0, false));
        assert_eq!(credits_at(&steps, 9.0 * 40.0), (0, 9, false));
        // Page 1's last line leaves on step 10: page 2 replaces it at its first position.
        assert_eq!(credits_at(&steps, 10.0 * 40.0), (1, 0, false));
        assert_eq!(credits_at(&steps, 14.0 * 40.0), (1, 4, false));
        assert_eq!(
            credits_at(&steps, 15.0 * 40.0),
            (1, 5, true),
            "the last page done: the screen closes"
        );
        assert_eq!(credits_at(&steps, 99.0 * 40.0), (1, 5, true));
        assert_eq!(credits_at(&[], 0.0), (0, 0, true));
    }
}
