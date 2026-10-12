//! Which sound file plays for each sound cue of the sim, and how loud. Spec: `docs/specs/ui/sounds.md`. No Bevy types
//! here; `main.rs` plays the result.
//!
//! The sim (`tidewater_sim::sound`) says what happened. This module does the rest of the spec: the file tables ("Data"),
//! the variant draws from the setup-shuffle generator ("Variant draws"), the human or orc sound set, the option bits,
//! the fog, inside and screen tests with their two levels ("How a sound is played") and the subtitles.
//!
//! Choices of our own where the spec leaves a gap: the digital volume runs from 0 to 255 and a level maps linearly to
//! loudness (the game's volume table isn't specified); plain sounds play at the normal level; a file's variant is
//! drawn before the option and position tests, so the draws happen for every cue, heard or not; the sword and chop
//! draws are made when the animation plays the sound rather than when the attack turns or the chop starts (the same
//! sounds, but the order of the generator's draws can differ; how the generator's state becomes a draw is itself an
//! open question of `core/timing-and-rng.md`). Overlapping sounds all play (open question "Sound file flags").

use tidewater_formats::pud::Era;
use tidewater_formats::udta::flags;
use tidewater_sim::sound::{CRITTER, Cue, Pick, Place, Sound};
use tidewater_sim::spells::spell;

/// The option bits of the preferences word ("Option bits in the preferences"). The labels are inferred (open question
/// "Option labels"); the bits each event tests are not in doubt.
pub mod option {
    pub const UNIT_SPEECH: u32 = 0x01;
    pub const ACKNOWLEDGEMENTS: u32 = 0x02;
    pub const BUILDING_SOUNDS: u32 = 0x04;
    pub const SUBTITLES: u32 = 0x4_0000;
    pub const ALL: u32 = UNIT_SPEECH | ACKNOWLEDGEMENTS | BUILDING_SOUNDS | SUBTITLES;
}

/// Subtitle entries of `Rez\network.tbl`, numbered from 1 ("Subtitles").
pub mod subtitle {
    pub const READY: u16 = 75;
    pub const WORK_COMPLETE: u16 = 76;
    pub const HELP_UNIT: u16 = 77;
    pub const HELP_BUILDING: u16 = 78;
    pub const BUILDING_DESTROYED: u16 = 79;
}

/// The sound set loaded at game start, from the local player's race ("Two sound sets").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundSet {
    Human,
    Orc,
}

impl SoundSet {
    /// From a race number of the map's `SIDE` section: 1 is orc, anything else human.
    pub fn of_race(race: u8) -> Self {
        if race == 1 { SoundSet::Orc } else { SoundSet::Human }
    }
}

/// A group of variant files: `n` numbered files `stem1` to `stemN`, or with `n` = 0 the single file `stem`. The human
/// and orc sets may name different stems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Files {
    human: &'static str,
    orc: &'static str,
    n: u8,
}

const fn one(f: &'static str) -> Files {
    Files { human: f, orc: f, n: 0 }
}

const fn many(f: &'static str, n: u8) -> Files {
    Files { human: f, orc: f, n }
}

const fn pair(human: &'static str, orc: &'static str, n: u8) -> Files {
    Files { human, orc, n }
}

const BUTTON: Files = one("Button");

impl Files {
    fn count(&self) -> u8 {
        self.n.max(1)
    }

    /// Variant `v` (from 0) in sound set `set`.
    fn name(&self, set: SoundSet, v: u8) -> String {
        let stem = match set {
            SoundSet::Human => self.human,
            SoundSet::Orc => self.orc,
        };
        if self.n == 0 {
            stem.to_string()
        } else {
            format!("{stem}{}", v.min(self.n - 1) + 1)
        }
    }
}

/// A type's voices ("Unit sounds"); `None` is the table's "none".
#[derive(Clone, Copy, Debug, Default)]
struct Voice {
    selected: Option<Files>,
    annoyed: Option<Files>,
    acknowledge: Option<Files>,
    ready: Option<Files>,
}

const FOOTMAN: Voice = Voice {
    selected: Some(pair("human\\hwhat", "orc\\owhat", 6)),
    annoyed: Some(pair("human\\hpissed", "orc\\opissed", 7)),
    acknowledge: Some(pair("human\\hyessir", "orc\\oyessir", 4)),
    ready: Some(pair("human\\hready", "orc\\oready", 0)),
};
const PEASANT: Voice = Voice {
    selected: Some(many("peasant\\pswhat", 4)),
    annoyed: Some(many("peasant\\pspissd", 7)),
    acknowledge: Some(many("peasant\\psyessr", 4)),
    ready: Some(one("peasant\\psready")),
};
const SIEGE: Voice = Voice {
    selected: Some(BUTTON),
    annoyed: None,
    acknowledge: Some(one("misc\\catyessr")),
    ready: Some(pair("human\\hready", "orc\\oready", 0)),
};
const HUMAN_TANKER: Voice = Voice {
    selected: Some(pair("ships\\hshpwht", "ships\\oshpwht", 3)),
    annoyed: Some(pair("ships\\hshppis", "ships\\oshppis", 3)),
    acknowledge: Some(one("ships\\foghorn")),
    ready: Some(pair("ships\\hshpredy", "ships\\oshpredy", 0)),
};
const SHIP: Voice = Voice {
    acknowledge: Some(pair("ships\\hshpyes", "ships\\oshpyes", 3)),
    ..HUMAN_TANKER
};

/// A voice with `what`, `pissed` and `yes` variant counts in one folder; `ready` is a full name or `None`.
const fn voice(sel: &'static str, sn: u8, ann: &'static str, an: u8, ack: &'static str, kn: u8) -> Voice {
    Voice {
        selected: Some(many(sel, sn)),
        annoyed: Some(many(ann, an)),
        acknowledge: Some(many(ack, kn)),
        ready: None,
    }
}

const fn ready(v: Voice, r: &'static str) -> Voice {
    Voice {
        ready: Some(one(r)),
        ..v
    }
}

const KNIGHT: Voice = voice("knight\\knwhat", 4, "knight\\knpissd", 3, "knight\\knyessr", 4);
const OGRE: Voice = voice("ogre\\ogwhat", 4, "ogre\\ogpissd", 5, "ogre\\ogyessr", 3);
const ARCHER: Voice = voice("elves\\ewhat", 4, "elves\\epissed", 3, "elves\\eyessir", 4);
const AXE_THROWER: Voice = voice("troll\\trwhat", 3, "troll\\trpissd", 3, "troll\\tryessr", 3);
const MAGE: Voice = voice("wizard\\wzwhat", 3, "wizard\\wzpissd", 3, "wizard\\wzyessr", 3);
const DEATH_KNIGHT: Voice = voice("deathknt\\dkwhat", 2, "deathknt\\dkpissd", 3, "deathknt\\dkyessr", 3);
const PALADIN: Voice = voice("paladin\\pkwhat", 4, "paladin\\pkpissd", 3, "paladin\\pkyessr", 4);
const OGRE_MAGE: Voice = voice("ogremage\\omwhat", 4, "ogremage\\ompissd", 3, "ogremage\\omyessr", 3);

/// "Unit sounds" by type. The critter (0x39) is by terrain and handled apart.
fn voice_of(kind: u8) -> Voice {
    let no_ready = |v: Voice| Voice { ready: None, ..v };
    let button = |ack: Files| Voice {
        selected: Some(BUTTON),
        acknowledge: Some(ack),
        ..Voice::default()
    };
    match kind {
        0x00 | 0x01 => FOOTMAN,
        0x02 => PEASANT,
        0x03 => ready(FOOTMAN, "peon\\pnready"),
        0x04 | 0x05 => SIEGE,
        0x06 => ready(KNIGHT, "knight\\knready"),
        0x07 => ready(OGRE, "ogre\\ogready"),
        0x08 | 0x12 => ready(ARCHER, "elves\\eready"),
        0x09 | 0x13 => ready(AXE_THROWER, "troll\\trready"),
        0x0a => ready(MAGE, "wizard\\wzready"),
        0x0b => ready(DEATH_KNIGHT, "deathknt\\dkready"),
        0x0c => ready(PALADIN, "paladin\\pkready"),
        0x0d => ready(OGRE_MAGE, "ogremage\\omready"),
        0x0e => ready(
            voice("dwarf\\dwhat", 2, "dwarf\\dwpissd", 3, "dwarf\\dwyessr", 5),
            "dwarf\\dwready",
        ),
        0x0f => ready(
            voice("goblin\\gowhat", 4, "goblin\\gopissd", 3, "goblin\\goyessr", 4),
            "goblin\\goready",
        ),
        0x10 => no_ready(PEASANT),
        0x11 => no_ready(FOOTMAN),
        0x14 => voice("aleria\\alwhat", 3, "aleria\\alpissd", 3, "aleria\\alyessr", 3),
        0x15 => voice("teron\\tewhat", 3, "teron\\tepissd", 3, "teron\\teyessr", 3),
        0x16 => voice("kurdran\\kuwhat", 3, "kurdran\\kupissd", 3, "kurdran\\kuyessr", 3),
        0x17 => voice("dentarg\\odwhat", 3, "dentarg\\odpissd", 3, "dentarg\\odyessr", 3),
        0x18 => voice("khadgar\\khwhat", 3, "khadgar\\khpissd", 3, "khadgar\\khyessr", 3),
        0x19 => voice("grom\\grwhat", 3, "grom\\grpissd", 3, "grom\\gryessr", 3),
        0x1a => HUMAN_TANKER,
        0x1b..=0x21 | 0x26 => SHIP,
        0x23 => voice("deathwng\\dewhat", 3, "deathwng\\depissd", 3, "deathwng\\deyessr", 3),
        0x27 => Voice {
            selected: FOOTMAN.selected,
            annoyed: Some(many("ships\\subpiss", 4)),
            ..SHIP
        },
        0x28 => Voice {
            annoyed: Some(many("gnome\\gnpissd", 5)),
            ready: Some(one("gnome\\gnready")),
            ..button(many("gnome\\gnyessr", 1))
        },
        0x29 => Voice {
            annoyed: Some(many("zeppelin\\gbpissd", 2)),
            ready: Some(one("zeppelin\\gbready")),
            ..button(many("zeppelin\\gbyessr", 1))
        },
        0x2a => Voice {
            selected: Some(one("griffon\\grwhat")),
            annoyed: None,
            acknowledge: Some(many("griffon\\griffon", 2)),
            ready: Some(one("griffon\\grwhat")),
        },
        0x2b => Voice {
            selected: Some(one("dragon\\drwhat")),
            annoyed: None,
            acknowledge: Some(many("dragon\\dryessr", 2)),
            ready: Some(one("dragon\\drready")),
        },
        0x2c => voice("turalyon\\tuwhat", 3, "turalyon\\tupissd", 3, "turalyon\\tuyessr", 3),
        0x2d => button(one("misc\\Eye of Killrog Move")),
        0x2e => voice("danath\\dnwhat", 3, "danath\\dnpisd", 3, "danath\\dnyessr", 3),
        0x2f => voice("kargath\\kawhat", 3, "kargath\\kapissd", 3, "kargath\\kayessr", 3),
        0x31 => OGRE_MAGE,
        0x32 => KNIGHT,
        0x33 => DEATH_KNIGHT,
        0x34 => PALADIN,
        0x35 => AXE_THROWER,
        0x37 => button(one("misc\\Skeleton Move")),
        0x38 => button(BUTTON),
        _ => Voice::default(),
    }
}

/// A type's attack sound ("Unit sounds", Attack column); "sword" is a draw of three.
fn attack_of(kind: u8) -> Option<Files> {
    Some(match kind {
        0x00 | 0x01 | 0x06 | 0x0c | 0x0e | 0x0f | 0x19 | 0x2c | 0x2e | 0x2f | 0x32 | 0x34 => many("misc\\sword", 3),
        0x02 | 0x03 | 0x10 | 0x11 => one("misc\\peonatak"),
        0x04 | 0x05 => one("misc\\catapult"),
        0x07 | 0x0d | 0x17 | 0x37 => one("misc\\punch"),
        0x08 | 0x12 | 0x14 => one("misc\\bowfire"),
        0x09 | 0x13 | 0x35 => one("misc\\axe"),
        0x0a | 0x18 | 0x1a..=0x22 | 0x26 | 0x27 | 0x31 | 0x33 => one("misc\\fireball"),
        0x0b | 0x15 => one("spells\\touchdrk"),
        _ => return None,
    })
}

/// The critter's files for a terrain ("Critters"): selected (also its acknowledgement), annoyed, death.
fn critter(era: Era) -> (&'static str, &'static str, &'static str) {
    match era {
        Era::Forest => ("misc\\sheep", "misc\\shpisd1", "misc\\sheep"),
        Era::Winter => ("misc\\seal", "misc\\sealpisd", "misc\\sealpisd"),
        Era::Wasteland => ("misc\\pig", "misc\\pigpissd", "misc\\pigpissd"),
        Era::Swamp => ("misc\\warthog", "misc\\wartpisd", "misc\\wartpisd"),
    }
}

/// "Building selection sounds", in the table's order; `None` falls through to `Button`.
fn building_select(kind: u8, wood_storage: bool) -> Option<Files> {
    Some(match kind {
        0x5c => one("bldg\\mine"),
        0x48 | 0x49 => one("bldg\\shipbell"),
        0x46 | 0x47 => pair("bldg\\aviary", "bldg\\dragon", 0),
        0x4e | 0x4f => one("bldg\\foundry"),
        0x44 | 0x45 => pair("bldg\\inventor", "bldg\\alchemst", 0),
        0x50 | 0x51 => pair("bldg\\wzrdtowr", "bldg\\dthtower", 0),
        0x3a | 0x3b => pair("bldg\\hfarm", "bldg\\ofarm", 0),
        0x52 | 0x53 => one("bldg\\smith"),
        0x3e | 0x3f => pair("bldg\\hchant", "bldg\\ochant", 0),
        0x42 | 0x43 => pair("bldg\\stables", "bldg\\ogrecamp", 0),
        0x56 | 0x57 => one("bldg\\oilplat"),
        0x54 | 0x55 => one("bldg\\oilrefin"),
        _ if wood_storage => one("bldg\\lumbmill"),
        _ => return None,
    })
}

/// "Deaths": the first rule that matches a unit's type and flags.
fn death(kind: u8, f: u32, era: Era) -> &'static str {
    if f & (flags::AIR | flags::EXPLODE_WHEN_KILLED | flags::KILLED_BY_INVISIBILITY) != 0 {
        "misc\\explode"
    } else if kind == CRITTER {
        critter(era).2
    } else if kind == 0x37 {
        "misc\\Skeleton Death"
    } else if f & flags::CRITTER != 0 {
        "orc\\odead"
    } else if f & flags::SEA != 0 {
        "ships\\shipsink"
    } else if kind.is_multiple_of(2) {
        "human\\hdead"
    } else {
        "orc\\odead"
    }
}

/// "Missile impacts": the sound for a missile kind, if any. The impact effects' explosion (kinds 0x15 and 0x1A) is a
/// separate `Sound::Explode` the sim cues at the effect's first update.
fn impact(kind: u8, damaging: bool) -> Option<&'static str> {
    match kind {
        0x00 | 0x01 | 0x02 | 0x05 | 0x1b if damaging => Some("misc\\firehit"),
        0x0e if damaging => Some("misc\\explode"),
        0x0f | 0x10 if damaging => Some("misc\\bowhit"),
        0x07 | 0x11 | 0x12 => Some("misc\\explode"),
        _ => None,
    }
}

/// "Spells", by order number.
fn spell_file(n: u8) -> Option<&'static str> {
    Some(match n {
        spell::BLOODLUST => "spells\\blodlust",
        spell::DEATH_AND_DECAY => "spells\\decay",
        spell::DEATH_COIL => "spells\\dethcoil",
        spell::EXORCISM => "spells\\exorcism",
        spell::FLAME_SHIELD => "spells\\flamshld",
        spell::HASTE => "spells\\haste",
        spell::HEALING => "spells\\heal",
        spell::HOLY_VISION => "spells\\holyvisn",
        spell::BLIZZARD => "spells\\icestorm",
        spell::INVISIBILITY => "spells\\invisibl",
        spell::POLYMORPH => "spells\\morph",
        spell::SLOW => "spells\\slow",
        spell::UNHOLY_ARMOUR => "spells\\unhlyarm",
        spell::WHIRLWIND => "spells\\whrlwind",
        spell::RAISE_DEAD => "misc\\Raise Dead",
        spell::RUNES => "misc\\Runes",
        spell::EYE_OF_KILROGG => "misc\\Eye of Killrog",
        _ => return None,
    })
}

/// The archive name of a file as the tables write it: `Gamesfx\<name>.wav`, or `Sfx\Button.wav` for "Button".
pub fn path(name: &str) -> String {
    if name == "Button" {
        "Sfx\\Button.wav".to_string()
    } else {
        format!("Gamesfx\\{name}.wav")
    }
}

/// The subtitle a file carries, if any ("Subtitles"): every `...ready`/`...redy` file, the work-complete and
/// under-attack files and the building explosions.
pub fn subtitle_of(name: &str) -> Option<u16> {
    let n = name.to_ascii_lowercase();
    let stem = n.rsplit('\\').next().unwrap_or(&n);
    if stem.ends_with("ready") || stem.ends_with("redy") {
        return Some(subtitle::READY);
    }
    Some(match stem {
        "hwrkdone" | "owrkdone" | "pswrkdon" => subtitle::WORK_COMPLETE,
        "hhelp1" | "ohelp1" => subtitle::HELP_UNIT,
        "hhelp2" | "ohelp2" => subtitle::HELP_BUILDING,
        "bldexpl1" | "bldexpl2" | "bldexpl3" => subtitle::BUILDING_DESTROYED,
        _ => return None,
    })
}

/// The setup-shuffle generator (`core/timing-and-rng.md`, "Setup shuffles"): `t <- t * 1103515245 + 12345`. Which bits
/// make a draw is an open question there; we take bits 16 to 30, the usual 0 to 32767 output of this recurrence, as
/// the sounds spec's draws range over 0 to 32767.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shuffle {
    pub state: u32,
}

impl Shuffle {
    pub const MULTIPLIER: u32 = 1_103_515_245;
    pub const INCREMENT: u32 = 12_345;

    pub fn draw(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(Self::INCREMENT);
        (self.state >> 16) & 0x7fff
    }
}

/// Variant number for draw `r` among `k` variants ("Variant draws").
pub fn variant(r: u32, k: u8) -> u8 {
    ((r / 256) % k.max(1) as u32) as u8
}

/// The main view in tiles: top-left (x, y), width and height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl View {
    /// "On screen": both ends included, so one extra column and row count.
    pub fn on_screen(&self, (x, y): (i32, i32)) -> bool {
        self.x <= x && x <= self.x + self.w && self.y <= y && y <= self.y + self.h
    }
}

/// The player's sound settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The digital volume V, 0 to 255 (our range).
    pub volume: u8,
    /// Option bits ([`option`]).
    pub options: u32,
    /// The demo playback mode skips the tile test.
    pub demo: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            volume: 255,
            options: option::ALL,
            demo: false,
        }
    }
}

impl Settings {
    pub fn normal_level(&self) -> u8 {
        (self.volume as u32 * 99 / 100) as u8
    }

    pub fn quiet_level(&self) -> u8 {
        (self.volume as u32 * 24 / 100) as u8
    }
}

/// What one cue comes to: a file to play at a level (0 to 255), and a subtitle entry to show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heard {
    pub play: Option<(String, u8)>,
    pub subtitle: Option<u16>,
}

/// The file chosen for a cue, the option bit it tests, and whether it is heard off screen.
struct Choice {
    file: String,
    option: Option<u32>,
    off_screen: bool,
}

/// Sounds the interface makes by itself rather than cues of the sim: the placement click's
/// (`docs/specs/ui/placing-buildings.md`, "The click"). Both are plain sounds under no option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiSound {
    /// `misc\error`: a placement click on a site that fails the site check. Its only use.
    Error,
    /// `misc\thunk`: a placement click on an allowed site.
    Thunk,
}

impl UiSound {
    pub fn file(self) -> &'static str {
        match self {
            UiSound::Error => "misc\\error",
            UiSound::Thunk => "misc\\thunk",
        }
    }
}

/// Turns the sim's cues into files and levels.
#[derive(Clone, Debug)]
pub struct Director {
    pub settings: Settings,
    pub set: SoundSet,
    pub era: Era,
    pub shuffle: Shuffle,
}

impl Director {
    pub fn new(set: SoundSet, era: Era, seed: u32) -> Self {
        Director {
            settings: Settings::default(),
            set,
            era,
            shuffle: Shuffle { state: seed },
        }
    }

    /// One of a group's files, with a draw if it has several.
    fn draw(&mut self, f: Files) -> String {
        let k = f.count();
        let v = if k > 1 { variant(self.shuffle.draw(), k) } else { 0 };
        f.name(self.set, v)
    }

    fn choose(&mut self, s: Sound) -> Option<Choice> {
        use option::{ACKNOWLEDGEMENTS as ACK, BUILDING_SOUNDS as BLD, UNIT_SPEECH as SPEECH};
        let set = self.set;
        let (file, option) = match s {
            Sound::Select { kind, building, pick } => {
                let opt = Some(if building { BLD } else { ACK });
                let (sel, ann, _) = critter(self.era);
                let file = match pick {
                    Pick::Burning => "misc\\burning".to_string(),
                    Pick::Constructing => "misc\\constrct".to_string(),
                    Pick::Building { wood_storage } => {
                        building_select(kind, wood_storage).unwrap_or(BUTTON).name(set, 0)
                    }
                    Pick::Selected => self.draw(voice_of(kind).selected?),
                    Pick::Annoyed(n) => voice_of(kind).annoyed?.name(set, n.saturating_sub(1)),
                    Pick::CritterSelected => sel.to_string(),
                    Pick::CritterAnnoyed => ann.to_string(),
                };
                (file, opt)
            }
            Sound::Acknowledge { kind } if kind == CRITTER => (critter(self.era).0.to_string(), Some(ACK)),
            Sound::Acknowledge { kind } => (self.draw(voice_of(kind).acknowledge?), Some(ACK)),
            Sound::Ready { kind } => (self.draw(voice_of(kind).ready?), Some(SPEECH)),
            Sound::WorkComplete { peasant: true } => ("peasant\\pswrkdon".to_string(), Some(SPEECH)),
            Sound::WorkComplete { peasant: false } => {
                (pair("human\\hwrkdone", "orc\\owrkdone", 0).name(set, 0), Some(SPEECH))
            }
            Sound::Help { building } => {
                let f = if building {
                    pair("human\\hhelp2", "orc\\ohelp2", 0)
                } else {
                    pair("human\\hhelp1", "orc\\ohelp1", 0)
                };
                (f.name(set, 0), Some(SPEECH))
            }
            Sound::CritterExplode => ("misc\\explode".to_string(), Some(ACK)),
            Sound::Attack { kind } => (self.draw(attack_of(kind)?), None),
            Sound::Chop => {
                let f = self.draw(many("misc\\tree", 4));
                return Some(Choice {
                    file: f,
                    option: None,
                    off_screen: false,
                });
            }
            Sound::TowerFire { kind } => (
                match kind {
                    0x60 | 0x61 => "misc\\bowfire",
                    0x62 | 0x63 => "misc\\fireball",
                    _ => return None,
                }
                .to_string(),
                None,
            ),
            Sound::Death { kind, flags } => (death(kind, flags, self.era).to_string(), None),
            Sound::BuildingDeath => (self.draw(many("misc\\bldexpl", 3)), None),
            Sound::Impact { kind, damaging } => (impact(kind, damaging)?.to_string(), None),
            Sound::Explode => ("misc\\explode".to_string(), None),
            Sound::Construction => ("misc\\constrct".to_string(), Some(BLD)),
            Sound::Dock => ("misc\\dock".to_string(), None),
            Sound::Rescue => (pair("misc\\hrescue", "misc\\orescue", 0).name(set, 0), None),
            Sound::Capture => (pair("misc\\hcapture", "misc\\ocapture", 0).name(set, 0), None),
            Sound::Spell(n) => (spell_file(n)?.to_string(), None),
        };
        Some(Choice {
            file,
            option,
            off_screen: true,
        })
    }

    /// What the local player hears of an interface sound: a plain sound at the normal level, under no option, with no
    /// subtitle. No variant draw is made.
    pub fn hear_ui(&self, s: UiSound) -> Heard {
        let st = self.settings;
        Heard {
            play: (st.volume > 0).then(|| (s.file().to_string(), st.normal_level())),
            subtitle: None,
        }
    }

    /// What the local player hears of `cue` with the main view at `view`.
    pub fn hear(&mut self, cue: &Cue, view: View) -> Heard {
        let Some(c) = self.choose(cue.sound) else {
            return Heard::default();
        };
        let st = self.settings;
        let level = match cue.place {
            None => st.normal_level(),
            Some(Place::Unit { inside: true, .. } | Place::Unit { hidden: true, .. }) => return Heard::default(),
            Some(Place::Tile { unseen: true, .. }) if !st.demo => return Heard::default(),
            Some(p) if view.on_screen(p.tile()) => st.normal_level(),
            Some(_) if c.off_screen => st.quiet_level(),
            Some(_) => return Heard::default(),
        };
        let option_on = c.option.is_none_or(|b| st.options & b != 0);
        let shown = st.options & option::SUBTITLES != 0 || st.volume == 0 || !option_on;
        Heard {
            subtitle: subtitle_of(&c.file).filter(|_| shown),
            play: (st.volume > 0 && option_on).then_some((c.file, level)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_sim::sound::annoyance_limit;

    /// The state whose next draw is `r` (with the low 16 bits of the next state 0).
    fn state_before(r: u32) -> u32 {
        // The multiplier is odd, so it has an inverse modulo 2^32 (Newton's iteration).
        let a = Shuffle::MULTIPLIER;
        let mut inv: u32 = a;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u32.wrapping_sub(a.wrapping_mul(inv)));
        }
        (r << 16).wrapping_sub(Shuffle::INCREMENT).wrapping_mul(inv)
    }

    fn director() -> Director {
        Director::new(SoundSet::Human, Era::Forest, 0)
    }

    const VIEW: View = View {
        x: 10,
        y: 10,
        w: 15,
        h: 11,
    };

    fn plain(sound: Sound) -> Cue {
        Cue { sound, place: None }
    }

    fn on_unit(sound: Sound, tile: (i32, i32)) -> Cue {
        Cue {
            sound,
            place: Some(Place::Unit {
                tile,
                inside: false,
                hidden: false,
            }),
        }
    }

    fn file(d: &mut Director, c: Cue) -> Option<String> {
        d.hear(&c, VIEW).play.map(|p| p.0)
    }

    fn select(kind: u8, building: bool, pick: Pick) -> Cue {
        plain(Sound::Select { kind, building, pick })
    }

    #[test]
    fn shuffle_follows_the_recurrence() {
        let mut s = Shuffle { state: 0 };
        assert_eq!(s.draw(), 0, "12345 has no bits above 15");
        assert_eq!(s.state, 12345);
        let mut s = Shuffle {
            state: state_before(1000),
        };
        assert_eq!(s.draw(), 1000);
    }

    #[test]
    fn footman_selected_with_r_1000() {
        assert_eq!(variant(1000, 6), 3);
        let mut d = director();
        d.shuffle.state = state_before(1000);
        let before = d.shuffle;
        assert_eq!(
            file(&mut d, select(0, false, Pick::Selected)).as_deref(),
            Some("human\\hwhat4")
        );
        let mut one = before;
        one.draw();
        assert_eq!(d.shuffle, one, "one draw");
    }

    #[test]
    fn annoyed_files_in_order_without_draws() {
        let mut d = director();
        let before = d.shuffle;
        let got: Vec<_> = (1..=7)
            .map(|n| file(&mut d, select(0, false, Pick::Annoyed(n))).unwrap())
            .collect();
        assert_eq!(got, (1..=7).map(|n| format!("human\\hpissed{n}")).collect::<Vec<_>>());
        assert_eq!(d.shuffle, before);
        assert_eq!(
            file(&mut d, select(6, false, Pick::Annoyed(3))).as_deref(),
            Some("knight\\knpissd3")
        );
    }

    #[test]
    fn the_sound_set_follows_the_local_player() {
        let mut orc = Director::new(SoundSet::Orc, Era::Forest, 0);
        assert!(
            file(&mut orc, select(1, false, Pick::Selected))
                .unwrap()
                .starts_with("orc\\owhat")
        );
        let mut human = director();
        assert!(
            file(&mut human, select(1, false, Pick::Selected))
                .unwrap()
                .starts_with("human\\hwhat")
        );
        // Captured pig farm, human set.
        assert_eq!(
            file(&mut human, select(0x3b, true, Pick::Building { wood_storage: false })).as_deref(),
            Some("bldg\\hfarm")
        );
    }

    #[test]
    fn buildings() {
        let mut d = director();
        let b = |kind, wood_storage| select(kind, true, Pick::Building { wood_storage });
        assert_eq!(file(&mut d, b(0x5c, false)).as_deref(), Some("bldg\\mine"));
        assert_eq!(file(&mut d, b(0x3c, false)).as_deref(), Some("Button"), "town hall");
        assert_eq!(file(&mut d, b(0x4c, true)).as_deref(), Some("bldg\\lumbmill"));
        assert_eq!(
            file(&mut d, select(0x3a, true, Pick::Constructing)).as_deref(),
            Some("misc\\constrct")
        );
        assert_eq!(
            file(&mut d, select(0x3e, true, Pick::Burning)).as_deref(),
            Some("misc\\burning")
        );
        assert_eq!(path("Button"), "Sfx\\Button.wav");
        assert_eq!(path("bldg\\mine"), "Gamesfx\\bldg\\mine.wav");
        // A building's selection sound is under the building sounds option, a unit's under acknowledgements.
        d.settings.options = option::ALL & !option::BUILDING_SOUNDS;
        assert_eq!(file(&mut d, b(0x5c, false)), None);
        assert!(
            file(&mut d, select(4, false, Pick::Selected)).is_some(),
            "ballista: Button under acks"
        );
    }

    #[test]
    fn critters_by_terrain() {
        let mut d = director();
        assert_eq!(
            file(&mut d, select(CRITTER, false, Pick::CritterSelected)).as_deref(),
            Some("misc\\sheep")
        );
        assert_eq!(
            file(&mut d, select(CRITTER, false, Pick::CritterAnnoyed)).as_deref(),
            Some("misc\\shpisd1")
        );
        let critter_flags = flags::LAND | flags::CRITTER;
        let dies = |era| {
            let mut d = Director::new(SoundSet::Human, era, 0);
            file(
                &mut d,
                on_unit(
                    Sound::Death {
                        kind: CRITTER,
                        flags: critter_flags,
                    },
                    (12, 12),
                ),
            )
        };
        assert_eq!(dies(Era::Forest).as_deref(), Some("misc\\sheep"));
        assert_eq!(dies(Era::Wasteland).as_deref(), Some("misc\\pigpissd"));
        assert_eq!(dies(Era::Winter).as_deref(), Some("misc\\sealpisd"));
        assert_eq!(
            file(&mut d, plain(Sound::CritterExplode)).as_deref(),
            Some("misc\\explode")
        );
    }

    #[test]
    fn death_rules() {
        let f = |kind, flags| death(kind, flags, Era::Forest);
        assert_eq!(f(0x37, flags::LAND | flags::CRITTER), "misc\\Skeleton Death");
        assert_eq!(
            f(0x38, flags::AIR | flags::CRITTER),
            "misc\\explode",
            "daemon: air flag"
        );
        assert_eq!(
            f(0x0f, flags::LAND | flags::KILLED_BY_INVISIBILITY),
            "misc\\explode",
            "sappers"
        );
        assert_eq!(f(0x1d, flags::SEA), "ships\\shipsink", "elven destroyer");
        assert_eq!(f(0x01, flags::LAND), "orc\\odead");
        assert_eq!(f(0x00, flags::LAND), "human\\hdead");
        assert_eq!(f(0x30, flags::LAND | flags::CRITTER), "orc\\odead");
    }

    #[test]
    fn building_destroyed_with_r_600() {
        let mut d = director();
        d.shuffle.state = state_before(600);
        let h = d.hear(&on_unit(Sound::BuildingDeath, (12, 12)), VIEW);
        assert_eq!(h.play.map(|p| p.0).as_deref(), Some("misc\\bldexpl3"));
        assert_eq!(h.subtitle, Some(subtitle::BUILDING_DESTROYED));
        d.settings.options &= !option::SUBTITLES;
        assert_eq!(d.hear(&on_unit(Sound::BuildingDeath, (12, 12)), VIEW).subtitle, None);
    }

    #[test]
    fn ready_and_subtitles() {
        let mut d = director();
        let h = d.hear(&plain(Sound::Ready { kind: 0 }), VIEW);
        assert_eq!(h.play, Some(("human\\hready".to_string(), d.settings.normal_level())));
        assert_eq!(h.subtitle, Some(subtitle::READY));
        d.settings.options = option::ALL & !option::SUBTITLES;
        assert_eq!(d.hear(&plain(Sound::Ready { kind: 0 }), VIEW).subtitle, None);
        d.settings.options = option::ALL & !option::SUBTITLES & !option::UNIT_SPEECH;
        let h = d.hear(&plain(Sound::Ready { kind: 0 }), VIEW);
        assert_eq!((h.play, h.subtitle), (None, Some(subtitle::READY)), "speech off");
        d.settings.options = option::ALL;
        let h = d.hear(&plain(Sound::Ready { kind: 0x2a }), VIEW);
        assert_eq!(
            (h.play.map(|p| p.0).as_deref(), h.subtitle),
            (Some("griffon\\grwhat"), None)
        );
        assert_eq!(
            file(&mut d, plain(Sound::Ready { kind: 0x1b })).as_deref(),
            Some("ships\\hshpredy")
        );
        assert_eq!(subtitle_of("ships\\oshpredy"), Some(subtitle::READY));
        // Volume 0: nothing plays, the subtitle shows even with the option off.
        d.settings.options = option::ALL & !option::SUBTITLES;
        d.settings.volume = 0;
        let h = d.hear(&plain(Sound::Ready { kind: 0 }), VIEW);
        assert_eq!((h.play, h.subtitle), (None, Some(subtitle::READY)));
    }

    #[test]
    fn work_complete() {
        let mut d = director();
        let h = d.hear(&plain(Sound::WorkComplete { peasant: true }), VIEW);
        assert_eq!(h.play.map(|p| p.0).as_deref(), Some("peasant\\pswrkdon"));
        assert_eq!(h.subtitle, Some(subtitle::WORK_COMPLETE));
        let mut orc = Director::new(SoundSet::Orc, Era::Forest, 0);
        assert_eq!(
            file(&mut orc, plain(Sound::WorkComplete { peasant: false })).as_deref(),
            Some("orc\\owrkdone")
        );
    }

    #[test]
    fn under_attack_files() {
        let mut d = director();
        let h = d.hear(&plain(Sound::Help { building: false }), VIEW);
        assert_eq!(
            (h.play.map(|p| p.0).as_deref(), h.subtitle),
            (Some("human\\hhelp1"), Some(subtitle::HELP_UNIT))
        );
        let mut orc = Director::new(SoundSet::Orc, Era::Forest, 0);
        let h = orc.hear(&plain(Sound::Help { building: true }), VIEW);
        assert_eq!(
            (h.play.map(|p| p.0).as_deref(), h.subtitle),
            (Some("orc\\ohelp2"), Some(subtitle::HELP_BUILDING))
        );
    }

    #[test]
    fn impacts() {
        let mut d = director();
        let at = |tile, unseen| Place::Tile { tile, unseen };
        let arrow = |damaging| Sound::Impact { kind: 0x0f, damaging };
        let c = |s, p| Cue {
            sound: s,
            place: Some(p),
        };
        assert_eq!(
            file(&mut d, c(arrow(true), at((12, 12), false))).as_deref(),
            Some("misc\\bowhit")
        );
        assert_eq!(file(&mut d, c(arrow(false), at((12, 12), false))), None);
        assert_eq!(file(&mut d, c(arrow(true), at((12, 12), true))), None, "unseen tile");
        d.settings.demo = true;
        assert!(
            file(&mut d, c(arrow(true), at((12, 12), true))).is_some(),
            "demo skips the test"
        );
        assert_eq!(impact(0x07, false), Some("misc\\explode"));
        assert_eq!(impact(0x0e, false), None);
        assert_eq!(impact(0x0d, true), None, "catapult rock");
        assert_eq!(impact(0x18, true), None, "small cannon");
    }

    #[test]
    fn screen_rules() {
        let mut d = director();
        let (normal, quiet) = (d.settings.normal_level(), d.settings.quiet_level());
        assert_eq!((normal, quiet), (252, 61));
        assert!(VIEW.on_screen((25, 21)));
        assert!(!VIEW.on_screen((26, 21)));
        let sword = Sound::Attack { kind: 0 };
        let lvl = |d: &mut Director, s, t| d.hear(&on_unit(s, t), VIEW).play.map(|p| p.1);
        assert_eq!(lvl(&mut d, sword, (25, 21)), Some(normal));
        assert_eq!(lvl(&mut d, sword, (26, 21)), Some(quiet));
        // Chopping off screen is silent, but the draw still happens.
        let before = d.shuffle;
        assert_eq!(lvl(&mut d, Sound::Chop, (26, 21)), None);
        assert_ne!(d.shuffle, before);
        assert!(lvl(&mut d, Sound::Chop, (12, 12)).is_some());
        // Inside a unit, or hidden by the fog: silent, no subtitle.
        let inside = Cue {
            sound: Sound::BuildingDeath,
            place: Some(Place::Unit {
                tile: (12, 12),
                inside: true,
                hidden: false,
            }),
        };
        assert_eq!(d.hear(&inside, VIEW), Heard::default());
        let hidden = Cue {
            place: Some(Place::Unit {
                tile: (12, 12),
                inside: false,
                hidden: true,
            }),
            ..inside
        };
        assert_eq!(d.hear(&hidden, VIEW), Heard::default());
    }

    #[test]
    fn options_gate_their_events() {
        let mut d = director();
        d.settings.options = 0;
        assert_eq!(file(&mut d, plain(Sound::Acknowledge { kind: 0 })), None);
        assert_eq!(file(&mut d, on_unit(Sound::Construction, (12, 12))), None);
        assert!(
            file(&mut d, on_unit(Sound::Explode, (12, 12))).is_some(),
            "explosions test no option"
        );
        d.settings.options = option::ALL;
        d.settings.volume = 0;
        assert_eq!(file(&mut d, on_unit(Sound::Explode, (12, 12))), None);
    }

    #[test]
    fn placement_click_sounds_are_plain() {
        let mut d = director();
        d.settings.options = 0;
        let h = d.hear_ui(UiSound::Error);
        assert_eq!(h.play, Some(("misc\\error".to_string(), d.settings.normal_level())));
        assert_eq!(h.subtitle, None);
        assert_eq!(
            d.hear_ui(UiSound::Thunk).play.map(|p| p.0).as_deref(),
            Some("misc\\thunk")
        );
        assert_eq!(path(UiSound::Error.file()), "Gamesfx\\misc\\error.wav");
        d.settings.volume = 0;
        assert_eq!(d.hear_ui(UiSound::Thunk), Heard::default());
    }

    #[test]
    fn other_files() {
        let mut d = director();
        let on = |s| on_unit(s, (12, 12));
        assert_eq!(
            file(&mut d, on(Sound::TowerFire { kind: 0x61 })).as_deref(),
            Some("misc\\bowfire")
        );
        assert_eq!(
            file(&mut d, on(Sound::TowerFire { kind: 0x63 })).as_deref(),
            Some("misc\\fireball")
        );
        assert_eq!(file(&mut d, on(Sound::Dock)).as_deref(), Some("misc\\dock"));
        assert_eq!(file(&mut d, on(Sound::Capture)).as_deref(), Some("misc\\hcapture"));
        assert_eq!(
            file(&mut d, on(Sound::Spell(spell::RAISE_DEAD))).as_deref(),
            Some("misc\\Raise Dead")
        );
        assert_eq!(file(&mut d, on(Sound::Spell(spell::FIREBALL))), None);
        assert_eq!(
            file(&mut d, plain(Sound::Spell(spell::EYE_OF_KILROGG))).as_deref(),
            Some("misc\\Eye of Killrog")
        );
        assert_eq!(
            file(&mut d, plain(Sound::Acknowledge { kind: 0x1a })).as_deref(),
            Some("ships\\foghorn")
        );
        assert_eq!(
            file(&mut d, plain(Sound::Acknowledge { kind: 0x28 })).as_deref(),
            Some("gnome\\gnyessr1")
        );
        assert_eq!(file(&mut d, plain(Sound::Acknowledge { kind: 0x22 })), None);
        assert!(
            file(&mut d, on(Sound::Attack { kind: 0 }))
                .unwrap()
                .starts_with("misc\\sword"),
            "sword"
        );
        assert_eq!(file(&mut d, on(Sound::Attack { kind: 0x16 })), None);
    }

    #[test]
    fn annoyance_limits_match_the_files() {
        for kind in 0..CRITTER {
            let l = voice_of(kind).annoyed.map_or(0, |f| f.count());
            assert_eq!(l, annoyance_limit(kind), "type {kind:#x}");
        }
    }
}
