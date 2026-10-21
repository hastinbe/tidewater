//! Starting a single-player game from a map: choosing the players, the custom-game choices, the starting alliances
//! and shared vision, creating the map's units, and the starting view, sound set and colours. Spec:
//! `docs/specs/game/setup.md`. Unit type numbers are those of the public PUD specification (appendix A).
//!
//! [`Start::choose`] makes steps 1 to 4 of the spec's "Order of the steps" (the header has been read by
//! [`Pud::parse`]); [`World::from_pud`](crate::World::from_pud) makes the rest.
//!
//! Our readings where the spec leaves room:
//! - Owner codes other than 2 to 7 (open question "Other owner codes"): code 0 becomes a human-controlled player and
//!   code 1 a computer player, since the spec says they test as those kinds; 8 and up are kept and match no kind.
//! - A campaign map with no code-5 slot fails like a custom game does.
//! - The campaign's race setting is the race of the chosen campaign: the entry's lowest bit (`game/victory.md`).
//! - The opponent count "map default" counts code-4 slots among slots 0 to 7, the slots the searches look at.
//! - Under Classic rules a "random" resource or terrain choice keeps the map's (the spec offers it only under
//!   Enhanced rules).
//! - A `UNIT` record whose owner isn't a slot (16 and up) is skipped.
//! - The starting view is clamped to the view range (0 to size − 14 on each axis) whether or not a start location
//!   set it.
//! - Race pairs are the human and orc versions the PUD specification lists side by side: types 0x00 to 0x13,
//!   0x1A to 0x21, 0x26 to 0x2B, 0x3A to 0x5B and 0x60 to 0x63 (even human, odd orc), and the walls 0x67 and 0x68.

use tidewater_formats::pud::{Era, Pud};

use crate::ai::controller;
use crate::rng::Rng;
use crate::victory::{Rule, mission_rule, mode};
use crate::world::{PLAYERS, World};

/// Slots the player choice and the custom choices cover.
const PLAYABLE: usize = 8;
/// The view is 14 tiles on each side ("Start-location view offset" is half of it).
pub const VIEW_TILES: i32 = 14;
const VIEW_OFFSET: i32 = 7;
/// Spells known at start: bits 5 and 14.
pub const SPELLS_KNOWN: u32 = 0x4020;
/// Resources before the map is read.
const DEFAULT_RESOURCES: u32 = 1000;

/// PUD unit types setup names.
pub mod kind {
    pub const PEASANT: u8 = 0x02;
    pub const PEON: u8 = 0x03;
    pub const OIL_PLATFORM: u8 = 0x56;
    pub const ORC_OIL_PLATFORM: u8 = 0x57;
    pub const GOLD_MINE: u8 = 0x5c;
    pub const OIL_PATCH: u8 = 0x5d;
    pub const HUMAN_START: u8 = 0x5e;
    pub const ORC_START: u8 = 0x5f;
    pub const CIRCLE_OF_POWER: u8 = 0x64;
    pub const WALL: u8 = 0x67;
    pub const ORC_WALL: u8 = 0x68;
    /// Town hall, great hall, keep, stronghold, castle and fortress: the units that store gold.
    pub const HALLS: [u8; 6] = [0x4a, 0x4b, 0x58, 0x59, 0x5a, 0x5b];
}

/// Race numbers (`SIDE`).
pub mod race {
    pub const HUMAN: u8 = 0;
    pub const ORC: u8 = 1;
    pub const NEUTRAL: u8 = 2;
}

/// The resources choice of a custom game.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Resources {
    #[default]
    MapDefault,
    Low,
    Medium,
    High,
    /// Enhanced rules only.
    Random,
}

/// The terrain choice of a custom game.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TerrainChoice {
    #[default]
    MapDefault,
    Set(Era),
    /// Enhanced rules only.
    Random,
}

/// The custom-scenario screen's choices ("Custom choices").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Custom {
    /// The local player's race: `None` is "map default", else [`race::HUMAN`] or [`race::ORC`].
    pub race: Option<u8>,
    /// The number of opponents: `None` is "map default".
    pub opponents: Option<u8>,
    pub resources: Resources,
    pub terrain: TerrainChoice,
    /// Units "one worker only" rather than "map default".
    pub one_worker: bool,
    pub fixed_starts: bool,
    pub fog_of_war: bool,
}

impl Default for Custom {
    fn default() -> Self {
        Custom {
            race: None,
            opponents: None,
            resources: Resources::MapDefault,
            terrain: TerrainChoice::MapDefault,
            one_worker: false,
            fixed_starts: false,
            fog_of_war: true,
        }
    }
}

/// How the map was started.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Game {
    /// A campaign map, with its mission table entry (`game/victory.md`, "Mission rules").
    Campaign {
        entry: u8,
    },
    Custom(Custom),
}

/// The options a new single-player game starts with. The default is a custom game with every choice at "map
/// default", under Enhanced rules.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Setup {
    pub game: Game,
    /// Enhanced rather than Classic rules (single player is Enhanced, `core/timing-and-rng.md`).
    pub enhanced: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            game: Game::Custom(Custom::default()),
            enhanced: true,
        }
    }
}

impl Setup {
    /// A normal start for a map named `name`: a campaign map when the name is one of the campaign's maps (a
    /// `Campaign` folder holding `HumanNN`, `OrcNN`, `2XHumNN` or `2XOrcNN`), otherwise a custom game with every
    /// choice at "map default".
    pub fn for_map(name: &str) -> Setup {
        let entry = campaign_entry(name);
        Setup {
            game: entry.map_or(Game::Custom(Custom::default()), |entry| Game::Campaign { entry }),
            ..Default::default()
        }
    }

    /// Reads options from text: `campaign:ENTRY`, or `custom` optionally followed by `:` and comma-separated choices
    /// (`race=human|orc`, `opponents=N`, `resources=low|medium|high|random`,
    /// `terrain=forest|winter|wasteland|swamp|random`, `units=one-worker`, `fixed-starts`, `fog=off`), and either
    /// may end with `,classic` for Classic rules.
    pub fn parse(s: &str) -> Result<Setup, String> {
        let (head, rest) = s.split_once(':').unwrap_or((s, ""));
        let mut setup = Setup::default();
        let mut custom = Custom::default();
        let mut opts = rest.split(',').map(str::trim).filter(|o| !o.is_empty());
        match head.trim() {
            "campaign" => {
                let entry = opts.next().ok_or("campaign needs an entry: campaign:ENTRY")?;
                let entry: u8 = entry.parse().map_err(|_| format!("bad campaign entry {entry:?}"))?;
                if entry > 51 {
                    return Err(format!("campaign entry {entry} is past 51"));
                }
                setup.game = Game::Campaign { entry };
            }
            "custom" => {}
            other => return Err(format!("unknown game {other:?} (campaign:ENTRY or custom)")),
        }
        for o in opts {
            let (k, v) = o.split_once('=').unwrap_or((o, ""));
            let custom_only = !matches!(k, "classic");
            if custom_only && matches!(setup.game, Game::Campaign { .. }) {
                return Err(format!("{k} is a custom-game choice"));
            }
            match (k, v) {
                ("classic", "") => setup.enhanced = false,
                ("race", "human") => custom.race = Some(race::HUMAN),
                ("race", "orc") => custom.race = Some(race::ORC),
                ("opponents", n) => custom.opponents = Some(n.parse().map_err(|_| format!("bad opponents {n:?}"))?),
                ("resources", r) => {
                    custom.resources = match r {
                        "low" => Resources::Low,
                        "medium" => Resources::Medium,
                        "high" => Resources::High,
                        "random" => Resources::Random,
                        _ => return Err(format!("bad resources {r:?}")),
                    }
                }
                ("terrain", t) => {
                    custom.terrain = match t {
                        "forest" => TerrainChoice::Set(Era::Forest),
                        "winter" => TerrainChoice::Set(Era::Winter),
                        "wasteland" => TerrainChoice::Set(Era::Wasteland),
                        "swamp" => TerrainChoice::Set(Era::Swamp),
                        "random" => TerrainChoice::Random,
                        _ => return Err(format!("bad terrain {t:?}")),
                    }
                }
                ("units", "one-worker") => custom.one_worker = true,
                ("fixed-starts", "") => custom.fixed_starts = true,
                ("fog", "off") => custom.fog_of_war = false,
                _ => return Err(format!("unknown choice {o:?}")),
            }
        }
        if let Game::Custom(c) = &mut setup.game {
            *c = custom;
        }
        Ok(setup)
    }

    /// The custom choices, in a custom game.
    pub fn custom(&self) -> Option<&Custom> {
        match &self.game {
            Game::Custom(c) => Some(c),
            Game::Campaign { .. } => None,
        }
    }

    /// The mission table entry, on a campaign map.
    pub fn campaign(&self) -> Option<u8> {
        match self.game {
            Game::Campaign { entry } => Some(entry),
            Game::Custom(_) => None,
        }
    }

    fn one_worker(&self) -> bool {
        self.custom().is_some_and(|c| c.one_worker)
    }
}

/// The campaign mission entry of a map name such as `Campaign\Human\Human06.pud` (entry 10) or
/// `Campaign\XOrc\2XOrc03.pud` (entry 33), or `None`.
pub fn campaign_entry(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase();
    let mut parts = lower.split(['\\', '/']).filter(|p| !p.is_empty());
    if !parts.clone().any(|p| p == "campaign") {
        return None;
    }
    let file = parts.next_back()?;
    let stem = file.strip_suffix(".pud").unwrap_or(file);
    let (expansion, orc, number) = [
        ("2xhum", true, false),
        ("2xorc", true, true),
        ("human", false, false),
        ("orc", false, true),
    ]
    .into_iter()
    .find_map(|(prefix, x, o)| stem.strip_prefix(prefix).map(|n| (x, o, n)))?;
    let n: u8 = number.parse().ok()?;
    let most = if expansion { 12 } else { 14 };
    (1..=most)
        .contains(&n)
        .then(|| crate::victory::mission_entry(expansion, orc, n))
}

/// Units allowed, spells allowed to research and upgrades allowed for campaign mission table entry `entry` (Data,
/// "Campaign allowance table").
pub fn campaign_allowance(entry: u8) -> (u32, u32, u32) {
    match entry {
        0 | 1 => (0x0403_0003, 0, 0),
        2 | 3 => (0x0407_0013, 0, 0x1),
        4 | 5 => (0x4407_00D3, 0, 0x3),
        6 | 7 => (0x4667_01D3, 0, 0x143),
        8 | 9 => (0x6EE7_09D7, 0, 0x1157),
        10 | 13 => (0x6EEF_09DF, 0, 0xF317F),
        11 => (0, 0, 0),
        12 | 15 => (0x6EEF_0BDF, 0, 0xF33FF),
        14 => (0x7FEF_2BDF, 0x10_0003, 0xF33FF),
        16 => (0, 0x10_0003, 0),
        17 => (0x7FEF_2BDF, 0x10_0C00, 0xF33FF),
        18 => (0x7FEF_2BDF, 0x10_000B, 0xF33FF),
        19 => (0x7FEF_6BDF, 0x14_0C00, 0xF33FF),
        20 => (0x7FFF_6BFF, 0x10_005B, 0xF33FF),
        21 => (0x7FFF_6BFF, 0x15_2C00, 0xF33FF),
        22 => (0x7FFF_6FFF, 0x10_00DB, 0xF33FF),
        23 => (0x7FFF_6FFF, 0x15_AC00, 0xF33FF),
        24 | 26 => (0x7FFF_FFFF, 0x10_03DB, 0xF33FF),
        25 | 27 => (0x7FFF_FFFF, 0x1F_AC00, 0xF33FF),
        28 => (0x0000_0100, 0x1F_FFFE, 0xF3FFF),
        29 | 31 | 33 | 35 | 36 => (0x7FFF_7FFF, 0x1F_FFFF, 0xF3FFF),
        30 => (0x3FFF_7FFF, 0x1F_FFFF, 0xF3FFF),
        37 => (0x7FFF_3FFF, 0x1F_FFFF, 0xF3FFF),
        40 => (0x4445_00C2, 0x1F_FEFE, 0xE3FFF),
        45 => (0x7FFF_F91F, 0x1C_7FFF, 0xF3C3F),
        48 => (0x7FFF_FBFF, 0x1F_FFFF, 0xF3FFF),
        // 32, 34, 38, 39, 41-44, 46, 47, 49-51.
        _ => (0x7FFF_FFFF, 0x1F_FFFF, 0xF3FFF),
    }
}

/// A unit type converted to race `race`: an orc owner gets the orc version, any other race the human version, and
/// types without a race pair stay as they are.
pub fn for_race(k: u8, race: u8) -> u8 {
    let orc = race == race::ORC;
    let paired = matches!(k, 0x00..=0x13 | 0x1a..=0x21 | 0x26..=0x2b | 0x3a..=0x5b | 0x60..=0x63);
    if paired {
        (k & !1) | orc as u8
    } else if matches!(k, kind::WALL | kind::ORC_WALL) {
        if orc { kind::ORC_WALL } else { kind::WALL }
    } else {
        k
    }
}

/// One choice from a draw: 1 + floor(n·r / 32768), so 1 to n.
pub fn pick(r: u32, n: u32) -> u32 {
    1 + n * r / 32768
}

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("no player slot 0 to 7 can be played here (owner code 5)")]
    NoLocalSlot,
}

/// The players and choices of a new game, before the map's body is read (steps 1 to 4 of "Order of the steps").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub setup: Setup,
    /// The local player's slot.
    pub local: u8,
    /// The local player's race setting: 0 human, 1 orc (or 2 when a custom game's local slot is neutral).
    pub race_setting: u8,
    /// Each slot's controller after the choice ([`crate::ai::controller`] values): the local player human, the chosen
    /// opponents computer, and the other code-4 and code-5 slots nobody. Rescue codes 6 and 7 are kept until the
    /// map's units exist.
    pub controllers: [u8; PLAYERS],
    /// Each slot's race: 0 human, 1 orc, 2 neutral.
    pub races: [u8; PLAYERS],
    /// Each slot's gold, lumber and oil.
    pub resources: [(u32, u32, u32); PLAYERS],
    /// The tileset, after the terrain choice.
    pub era: Era,
    /// The game's generator after the slot choice's draws.
    pub rng: Rng,
    /// A multiplayer game (`Rules::multiplayer`). [`Start::choose`] starts a single-player game; a multiplayer start
    /// sets this.
    pub multiplayer: bool,
}

impl Start {
    /// Chooses the players of `pud` and applies the custom choices, drawing from a generator seeded with `seed`.
    pub fn choose(pud: &Pud, setup: &Setup, seed: u32) -> Result<Start, SetupError> {
        let (codes, mut races, mut resources) = from_map(pud);
        let mut rng = Rng::new(seed);
        let mut era = pud.era;
        // Chosen: the local slot and the computer opponents.
        let mut computer = [false; PLAYERS];
        let local;
        let race_setting;
        match setup.game {
            Game::Campaign { entry } => {
                local = (0..PLAYABLE)
                    .find(|&s| codes[s] == controller::HUMAN)
                    .ok_or(SetupError::NoLocalSlot)?;
                for s in 0..PLAYERS {
                    computer[s] = codes[s] == controller::COMPUTER;
                }
                race_setting = entry & 1;
            }
            Game::Custom(c) => {
                let r = rng.draw() as usize;
                local = (0..PLAYABLE)
                    .map(|k| (r + k) % PLAYABLE)
                    .find(|&s| codes[s] == controller::HUMAN)
                    .ok_or(SetupError::NoLocalSlot)?;
                if let Some(r) = c.race {
                    races[local] = r;
                }
                race_setting = races[local];
                let wanted = c.opponents.map_or_else(
                    || {
                        (0..PLAYABLE)
                            .filter(|&s| codes[s] == controller::COMPUTER)
                            .count()
                            .max(1)
                    },
                    |n| n as usize,
                );
                let mut humans = [false; PLAYERS];
                humans[local] = true;
                place_opponents(&codes, &humans, &mut computer, wanted, c.fixed_starts, &mut rng);
                custom_choices(&c, setup.enhanced, &mut rng, &mut resources, &mut era);
            }
        }
        let mut humans = [false; PLAYERS];
        humans[local] = true;
        Ok(Start {
            setup: *setup,
            local: local as u8,
            race_setting,
            controllers: controllers(codes, &humans, &computer),
            races,
            resources,
            era,
            rng,
            multiplayer: false,
        })
    }

    /// "Choosing the players", multiplayer game: the lobby's humans take their slots (and races, unless neutral:
    /// "Reading the map", `SIDE`) with no draw, then the computer opponents are placed as custom step 3 places them,
    /// the humans' slots not counting as code 5, drawing from a generator seeded with the game's seed. With "Fixed
    /// Order" each Computer row instead takes its same-numbered slot (if the map gives it code 4 or 5), with no draw.
    /// The resources and the tileset are the map's.
    pub fn multiplayer(pud: &Pud, setup: &Setup, seed: u32, lobby: &Lobby) -> Result<Start, SetupError> {
        let (codes, mut races, resources) = from_map(pud);
        let mut humans = [false; PLAYERS];
        for &(slot, r) in &lobby.humans {
            let s = slot as usize;
            humans[s] = true;
            if r <= race::ORC {
                races[s] = r;
            }
        }
        if !lobby.humans.iter().any(|&(s, _)| s == lobby.local) {
            return Err(SetupError::NoLocalSlot);
        }
        let mut rng = Rng::new(seed);
        let mut computer = [false; PLAYERS];
        if lobby.fixed_order {
            // Round-59 corrections, "Fixed Order": no opponent placement and no draw; each Computer row takes the
            // slot with its own number if the map gives that slot code 4 or 5.
            for &row in &lobby.computers {
                let s = row as usize;
                let open = matches!(codes.get(s), Some(&controller::COMPUTER | &controller::HUMAN));
                if s < PLAYABLE && open && !humans[s] {
                    computer[s] = true;
                }
            }
        } else {
            // With no opponents placement stops before its first draw (round-59 corrections).
            place_opponents(&codes, &humans, &mut computer, lobby.computers.len(), false, &mut rng);
        }
        let local = lobby.local as usize;
        Ok(Start {
            setup: *setup,
            local: lobby.local,
            race_setting: races[local].min(race::ORC),
            controllers: controllers(codes, &humans, &computer),
            races,
            resources,
            era: pud.era,
            rng,
            multiplayer: true,
        })
    }

    /// The starting alliance table ("Starting alliances and shared vision"): bit b of entry a is set when a counts b
    /// as allied.
    pub fn alliances(&self) -> [u16; PLAYERS] {
        let c = &self.controllers;
        // A rescue-active slot starts allied with every human-controlled slot, both ways: in single player that is the
        // local player alone, in a network game every human slot, the same on every machine (G23, static study,
        // `ERRATA-audit-game.md` erratum 9).
        let local = self.local as usize;
        let human = |x: usize| x == local || (self.multiplayer && c[x] == controller::HUMAN);
        let mut table = [u16::MAX; PLAYERS];
        for a in 0..PLAYERS - 1 {
            for b in (0..PLAYERS - 1).filter(|&b| b != a) {
                let allied = c[a] == controller::RESCUE_PASSIVE
                    || c[b] == controller::RESCUE_PASSIVE
                    || (human(a) && c[b] == controller::RESCUE_ACTIVE)
                    || (human(b) && c[a] == controller::RESCUE_ACTIVE)
                    || (c[a] == controller::COMPUTER && c[b] == controller::COMPUTER);
                if !allied {
                    table[a] &= !(1 << b);
                }
            }
        }
        table
    }
}

/// The lobby's choices a multiplayer start takes as given (the lobby is a later spec).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Lobby {
    /// Each human's slot and race (0 human, 1 orc, 2 neutral).
    pub humans: Vec<(u8, u8)>,
    /// The local player's slot, one of the humans'.
    pub local: u8,
    /// The lobby rows set to Computer. With random starts only their number matters (the opponents placed by draw);
    /// with [`Lobby::fixed_order`] each takes the slot with its own row number.
    pub computers: Vec<u8>,
    /// The lobby's "Location: Fixed Order" (the fixed-starts setting): each human takes the slot of its own lobby row
    /// (as given in `humans`), each Computer row its same-numbered slot if the map gives it code 4 or 5, and there
    /// are no placement draws (setup spec, "Fixed Order", round-59 corrections).
    pub fixed_order: bool,
}

/// Each slot's owner code, race and (gold, lumber, oil).
type FromMap = ([u8; PLAYERS], [u8; PLAYERS], [(u32, u32, u32); PLAYERS]);

/// What the map gives every start: each slot's owner code (slot 15 passive), race and resources ("Reading the map").
fn from_map(pud: &Pud) -> FromMap {
    let mut codes = [controller::NOBODY; PLAYERS];
    for (c, &o) in codes.iter_mut().zip(pud.owners()) {
        *c = o;
    }
    codes[PLAYERS - 1] = controller::PASSIVE;
    let mut races = [race::NEUTRAL; PLAYERS];
    for (r, &s) in races.iter_mut().zip(pud.sides()) {
        *r = if s <= race::ORC { s } else { race::NEUTRAL };
    }
    let mut resources = [(DEFAULT_RESOURCES, DEFAULT_RESOURCES, DEFAULT_RESOURCES); PLAYERS];
    for (p, r) in resources.iter_mut().enumerate() {
        let (g, l, o) = pud.start_resources(p);
        *r = (g as u32, l as u32, o as u32);
    }
    (codes, races, resources)
}

/// "Choosing the players", custom step 3 (also the multiplayer opponents): `wanted` opponents, each from a draw r
/// (0 with fixed starts) and a search from slot r mod 8 upward for a code-4 slot. When a search finds none, that draw
/// is spent and the rest come from the code-5 slots not taken by a human, a new draw per try, until none is left.
fn place_opponents(
    codes: &[u8; PLAYERS],
    humans: &[bool; PLAYERS],
    computer: &mut [bool; PLAYERS],
    wanted: usize,
    fixed_starts: bool,
    rng: &mut Rng,
) {
    let search = |computer: &[bool; PLAYERS], from: u32, code: u8| {
        (0..PLAYABLE)
            .map(|k| (from as usize + k) % PLAYABLE)
            .find(|&s| codes[s] == code && !computer[s] && !humans[s])
    };
    let mut draw = || if fixed_starts { 0 } else { rng.draw() };
    let (mut chosen, mut from_code_5) = (0, false);
    while chosen < wanted {
        if !from_code_5 {
            if let Some(s) = search(computer, draw(), controller::COMPUTER) {
                computer[s] = true;
                chosen += 1;
                continue;
            }
            from_code_5 = true;
        }
        let Some(s) = search(computer, draw(), controller::HUMAN) else {
            break;
        };
        computer[s] = true;
        chosen += 1;
    }
}

/// "Both": the humans human-controlled, the chosen opponents computer, every other code-4 or code-5 slot nobody;
/// codes 0 and 1 test as human and computer, and the rest are kept.
fn controllers(codes: [u8; PLAYERS], humans: &[bool; PLAYERS], computer: &[bool; PLAYERS]) -> [u8; PLAYERS] {
    let mut out = codes;
    for (s, c) in out.iter_mut().enumerate() {
        *c = if humans[s] {
            controller::HUMAN
        } else if computer[s] {
            controller::COMPUTER
        } else {
            match *c {
                controller::COMPUTER | controller::HUMAN => controller::NOBODY,
                0 => controller::HUMAN,
                1 => controller::COMPUTER,
                other => other,
            }
        };
    }
    out
}

/// The custom resources and terrain choices; the resource draw comes before the terrain draw.
fn custom_choices(
    c: &Custom,
    enhanced: bool,
    rng: &mut Rng,
    resources: &mut [(u32, u32, u32); PLAYERS],
    era: &mut Era,
) {
    let choice = match c.resources {
        Resources::Random if enhanced => match pick(rng.draw(), 3) {
            1 => Resources::Low,
            2 => Resources::Medium,
            _ => Resources::High,
        },
        Resources::Random => Resources::MapDefault,
        r => r,
    };
    let set = match choice {
        Resources::Low if enhanced => Some((2100, 1100, 1000)),
        Resources::Low => Some((2000, 1000, 1000)),
        Resources::Medium => Some((5000, 2000, 2000)),
        Resources::High => Some((10000, 5000, 5000)),
        _ if c.one_worker => {
            let (g, l, o) = resources[0];
            Some(if enhanced {
                (g.max(2100), l.max(1100), o.max(1000))
            } else {
                (g, l, o)
            })
        }
        _ => None,
    };
    if let Some(r) = set {
        resources[..PLAYABLE].fill(r);
    }
    *era = match c.terrain {
        TerrainChoice::Set(e) => e,
        TerrainChoice::Random if enhanced => match pick(rng.draw(), 4) {
            1 => Era::Forest,
            2 => Era::Winter,
            3 => Era::Wasteland,
            _ => Era::Swamp,
        },
        _ => *era,
    };
}

/// What setup decided that isn't game state: the starting view and the sound set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Started {
    /// The view's top-left tile at the start (`ui/screen-and-cursors.md`).
    pub view: (i32, i32),
    /// The saved view positions F2 to F4, which start as the starting view.
    pub saved_views: [(i32, i32); 3],
    /// The local player's race setting.
    pub race_setting: u8,
    /// Whether the orc sound set is loaded rather than the human set (`ui/sounds.md`).
    pub orc_sounds: bool,
}

impl World {
    /// Creates the map's units ("Creating the map's units") and returns the view a start location of the local
    /// player set, if any.
    pub(crate) fn create_map_units(&mut self, pud: &Pud, setup: &Setup) -> Option<(i32, i32)> {
        let mode_k = setup
            .campaign()
            .is_some_and(|e| matches!(mission_rule(e), Rule::Mission { mode: mode::K, .. }));
        let one_worker = setup.one_worker();
        let local = self.vision.local;
        let mut view = None;
        self.loading = true;
        for u in &pud.units {
            let Some(p) = self.players.get(u.owner as usize) else {
                continue;
            };
            let code = p.controller;
            if code == controller::NOBODY {
                continue;
            }
            let (x, y) = (u.x as i32, u.y as i32);
            if u.kind == kind::CIRCLE_OF_POWER && mode_k {
                self.victory.circle_region = self.terrain.region(x, y);
            }
            let create = if code == controller::PASSIVE {
                true
            } else if matches!(u.kind, kind::HUMAN_START | kind::ORC_START) {
                if u.owner == local {
                    view = Some(((x - VIEW_OFFSET).max(0), (y - VIEW_OFFSET).max(0)));
                }
                one_worker
            } else {
                !one_worker
            };
            if create {
                self.create_map_unit(u.kind, u.owner, (x, y), u.data);
            }
        }
        self.loading = false;
        view
    }

    /// Creates one map unit: converted to its owner's race (a start location becomes a worker), with the mine or
    /// platform amount or the held mark from the data value. A unit that can't be created is dropped.
    fn create_map_unit(&mut self, k: u8, owner: u8, tile: (i32, i32), data: u16) {
        let race = self.players[owner as usize].race;
        let k = match k {
            kind::HUMAN_START | kind::ORC_START if race == race::ORC => kind::PEON,
            kind::HUMAN_START | kind::ORC_START => kind::PEASANT,
            k => for_race(k, race),
        };
        let amount = matches!(
            k,
            kind::GOLD_MINE | kind::OIL_PATCH | kind::OIL_PLATFORM | kind::ORC_OIL_PLATFORM
        );
        let resources = if amount { crate::harvest::trips(data) } else { 0 };
        let Ok(id) = self.spawn(k, owner, tile, resources) else {
            return;
        };
        if let Some(i) = self.index(id) {
            self.units[i].held = !amount && data & 0xff != 0;
            self.held_goal(i);
        }
        if crate::economy::kind::is_hall(k) {
            self.players[owner as usize].had_hall = true;
        }
    }

    /// The starting view ("After the units"), clamped to the view range.
    pub(crate) fn starting_view(&self, from_start_location: Option<(i32, i32)>) -> (i32, i32) {
        let local = self.vision.local;
        let view = from_start_location.unwrap_or_else(|| {
            // Most recently created first, skipping dying or removed units.
            let mut mine: Vec<_> = self
                .units
                .iter()
                .filter(|u| u.owner == local && !u.dying && !u.removing)
                .collect();
            mine.sort_by_key(|u| std::cmp::Reverse(u.id));
            let at = mine
                .iter()
                .find(|u| kind::HALLS.contains(&u.kind))
                .or(mine.last())
                .map_or((0, 0), |u| u.tile);
            (at.0 - VIEW_OFFSET, at.1 - VIEW_OFFSET)
        });
        let most = (
            (self.terrain.width as i32 - VIEW_TILES).max(0),
            (self.terrain.height as i32 - VIEW_TILES).max(0),
        );
        (view.0.clamp(0, most.0), view.1.clamp(0, most.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_formats::pud::build::Map;

    fn pud(m: &Map) -> Pud {
        Pud::parse(&m.bytes()).unwrap()
    }

    fn owners(first: [u8; 8]) -> [u8; 16] {
        let mut o = [3; 16];
        o[..8].copy_from_slice(&first);
        o
    }

    fn custom(c: Custom) -> Setup {
        Setup {
            game: Game::Custom(c),
            enhanced: true,
        }
    }

    use controller::{COMPUTER, HUMAN, NOBODY, PASSIVE, RESCUE_ACTIVE, RESCUE_PASSIVE};

    #[test]
    fn campaign_takes_the_lowest_code_5_and_every_code_4() {
        let m = Map {
            owners: owners([3, 5, 4, 4, 5, 3, 3, 3]),
            ..Default::default()
        };
        let s = Start::choose(&pud(&m), &Setup::for_map("Campaign\\Human\\Human01.pud"), 0).unwrap();
        assert_eq!(s.local, 1);
        assert_eq!(s.controllers[..5], [NOBODY, HUMAN, COMPUTER, COMPUTER, NOBODY]);
        // No draws.
        assert_eq!(s.rng, Rng::new(0));
    }

    #[test]
    fn slot_15_is_passive_and_races_come_from_the_map() {
        let mut m = Map::default();
        m.owners[15] = 3;
        m.sides = [1; 16];
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:0").unwrap(), 0);
        // No code-5 slot: the default map can't be played.
        assert!(s.is_err());
        m.owners[0] = 5;
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:0").unwrap(), 0).unwrap();
        assert_eq!(s.controllers[15], PASSIVE);
        // The map's `SIDE` race is used as it is for all 16 slots (round-31 errata).
        assert!(s.races.iter().all(|&r| r == race::ORC));
        // Human01 and Human04: 1, then 0 fourteen times, then 2.
        m.sides = [0; 16];
        m.sides[0] = 1;
        m.sides[15] = 2;
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:0").unwrap(), 0).unwrap();
        assert_eq!(s.races[0], race::ORC);
        assert!(s.races[1..15].iter().all(|&r| r == race::HUMAN));
        assert_eq!(s.races[15], race::NEUTRAL);
        m.sides[15] = 7;
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:0").unwrap(), 0).unwrap();
        assert_eq!(s.races[15], race::NEUTRAL);
    }

    fn custom_vector(c: Custom) -> Start {
        let m = Map {
            owners: owners([5, 4, 4, 5, 3, 3, 3, 3]),
            ..Default::default()
        };
        Start::choose(&pud(&m), &custom(c), 0).unwrap()
    }

    fn draws_made(s: &Start) -> usize {
        let mut r = Rng::new(0);
        let mut n = 0;
        while r != s.rng {
            r.draw();
            n += 1;
            assert!(n < 50);
        }
        n
    }

    #[test]
    fn custom_opponents_map_default() {
        let s = custom_vector(Custom::default());
        assert_eq!(s.local, 0);
        assert_eq!(s.controllers[..4], [HUMAN, COMPUTER, COMPUTER, NOBODY]);
        assert_eq!(draws_made(&s), 3);
    }

    #[test]
    fn custom_opponents_fall_back_to_code_5() {
        let s = custom_vector(Custom {
            opponents: Some(3),
            ..Default::default()
        });
        assert_eq!(s.controllers[..4], [HUMAN, COMPUTER, COMPUTER, COMPUTER]);
        assert_eq!(draws_made(&s), 5);
        // A fourth opponent finds nothing left.
        let s = custom_vector(Custom {
            opponents: Some(4),
            ..Default::default()
        });
        assert_eq!(s.controllers[..4], [HUMAN, COMPUTER, COMPUTER, COMPUTER]);
        assert_eq!(draws_made(&s), 6);
    }

    #[test]
    fn fixed_starts_search_from_slot_0_without_draws() {
        let s = custom_vector(Custom {
            fixed_starts: true,
            ..Default::default()
        });
        assert_eq!(s.controllers[..4], [HUMAN, COMPUTER, COMPUTER, NOBODY]);
        assert_eq!(draws_made(&s), 1);
    }

    fn draws_from(seed: u32, s: &Start) -> usize {
        let mut r = Rng::new(seed);
        let mut n = 0;
        while r != s.rng {
            r.draw();
            n += 1;
            assert!(n < 50);
        }
        n
    }

    fn lobby(humans: &[u8], local: u8, opponents: usize) -> Lobby {
        Lobby {
            humans: humans.iter().map(|&s| (s, race::ORC)).collect(),
            local,
            // With random starts only the number of Computer rows matters; these row numbers are never used.
            computers: (0..opponents as u8).map(|k| 8 + k).collect(),
            fixed_order: false,
        }
    }

    /// Round-58 errata, "Choosing the players", multiplayer game (the Gold Rush BNE vector): seed 5, codes
    /// (5, human, human, 5, 3, 3, 3, 3) with the humans in slots 1 and 2, 2 opponents. Draw 1731 finds no code 4 and
    /// is spent; 32036 (mod 8 = 4) finds slot 0; 21622 (mod 8 = 6) finds slot 3.
    #[test]
    fn multiplayer_opponents_spend_one_draw_then_take_code_5_slots() {
        let m = Map {
            owners: owners([5, 5, 5, 5, 3, 3, 3, 3]),
            ..Default::default()
        };
        let mut r = Rng::new(5);
        assert_eq!([r.draw(), r.draw(), r.draw()], [1731, 32036, 21622]);
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &lobby(&[1, 2], 1, 2)).unwrap();
        assert_eq!(
            s.controllers[..8],
            [COMPUTER, HUMAN, HUMAN, COMPUTER, NOBODY, NOBODY, NOBODY, NOBODY]
        );
        assert_eq!((s.local, s.multiplayer, draws_from(5, &s)), (1, true, 3));
        // The lobby's race replaces the map's for the humans only.
        assert_eq!(s.races[..4], [race::HUMAN, race::ORC, race::ORC, race::HUMAN]);
    }

    /// Round-58 vectors, `mp-dark-paths-bne-1000` and `mp-rose-petal-bne-10000` (the setup draws only): codes
    /// (5, 5, 5, 3, ...) with humans in 0 and 1 and 1 opponent gives the computer slot 2 in 2 draws; codes
    /// (5, 5, 5, 5, 5, 5, 3, 3) with humans in 0 and 4 and 4 opponents gives slots 1, 2, 3 and 5 in 5 draws.
    #[test]
    fn multiplayer_placement_makes_one_plus_opponents_draws() {
        let m = Map {
            owners: owners([5, 5, 5, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &lobby(&[0, 1], 0, 1)).unwrap();
        assert_eq!(s.controllers[..4], [HUMAN, HUMAN, COMPUTER, NOBODY]);
        assert_eq!(draws_from(5, &s), 2);
        let m = Map {
            owners: owners([5, 5, 5, 5, 5, 5, 3, 3]),
            ..Default::default()
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &lobby(&[0, 4], 0, 4)).unwrap();
        let computers: Vec<usize> = (0..8).filter(|&p| s.controllers[p] == COMPUTER).collect();
        assert_eq!(computers, [1, 2, 3, 5]);
        assert_eq!(draws_from(5, &s), 5);
        // A local player that isn't one of the humans can't start.
        assert!(Start::multiplayer(&pud(&m), &Setup::default(), 5, &lobby(&[0, 4], 2, 1)).is_err());
    }

    /// Round-59 corrections (setup spec vectors, static study): with no computer opponents placement makes no draw,
    /// so the setup's first draws are the unit-creation draws.
    #[test]
    fn multiplayer_with_no_opponents_makes_no_placement_draw() {
        let m = Map {
            owners: owners([5, 5, 5, 5, 3, 3, 3, 3]),
            ..Default::default()
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &lobby(&[0, 1], 0, 0)).unwrap();
        assert_eq!(s.controllers[..4], [HUMAN, HUMAN, NOBODY, NOBODY]);
        assert_eq!(draws_from(5, &s), 0);
    }

    /// Round-59 corrections, "Fixed Order" (setup spec vectors, static study): humans in lobby rows 0 and 1 and row 2
    /// set to Computer on a map whose slot 2 has code 5 give humans in slots 0 and 1, a computer in slot 2 and no
    /// placement draw; a Computer row 3 whose map slot has code 3 gives no computer in slot 3.
    #[test]
    fn multiplayer_fixed_order_takes_same_numbered_slots_without_draws() {
        let m = Map {
            owners: owners([5, 5, 5, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let l = Lobby {
            humans: vec![(0, race::ORC), (1, race::ORC)],
            local: 0,
            computers: vec![2, 3],
            fixed_order: true,
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &l).unwrap();
        assert_eq!(s.controllers[..4], [HUMAN, HUMAN, COMPUTER, 3]);
        assert_eq!(draws_from(5, &s), 0);
        // Random starts on the same map: the first draw finds no code 4 and is spent, then each opponent draws for a
        // code-5 slot (only slot 2 is left, so the second try finds nothing).
        let r = Lobby {
            fixed_order: false,
            ..l
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &r).unwrap();
        assert_eq!(s.controllers[..3], [HUMAN, HUMAN, COMPUTER]);
        assert_eq!(draws_from(5, &s), 3);
        // Fixed Order: a code-4 slot takes its Computer row too, and a row with no Computer stays nobody.
        let m = Map {
            owners: owners([5, 4, 4, 5, 3, 3, 3, 3]),
            ..Default::default()
        };
        let l = Lobby {
            humans: vec![(0, race::ORC)],
            local: 0,
            computers: vec![2],
            fixed_order: true,
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &l).unwrap();
        assert_eq!(s.controllers[..4], [HUMAN, NOBODY, COMPUTER, NOBODY]);
        assert_eq!(draws_from(5, &s), 0);
    }

    /// Round-60 vectors (`mp-rose-petal-bne-fixed-3000`, the setup draws only): codes (5, 5, 5, 5, 5, 5, 3, 3), Fixed
    /// Order, humans in lobby rows 0 and 1, rows 2 to 5 set to Computer: computers in slots 2 to 5 and no placement
    /// draw, where random starts with 4 opponents make 1 + 4 = 5.
    #[test]
    fn fixed_order_makes_one_plus_opponents_fewer_draws() {
        let m = Map {
            owners: owners([5, 5, 5, 5, 5, 5, 3, 3]),
            ..Default::default()
        };
        let l = Lobby {
            humans: vec![(0, race::HUMAN), (1, race::ORC)],
            local: 0,
            computers: vec![2, 3, 4, 5],
            fixed_order: true,
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &l).unwrap();
        assert_eq!(
            s.controllers[..8],
            [HUMAN, HUMAN, COMPUTER, COMPUTER, COMPUTER, COMPUTER, NOBODY, NOBODY]
        );
        assert_eq!(draws_from(5, &s), 0);
        let r = Lobby {
            fixed_order: false,
            ..l
        };
        let s = Start::multiplayer(&pud(&m), &Setup::default(), 5, &r).unwrap();
        assert_eq!(draws_from(5, &s), 5);
    }

    #[test]
    fn your_race_replaces_the_local_slots() {
        let m = Map {
            owners: owners([5, 4, 3, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let s = Start::choose(
            &pud(&m),
            &custom(Custom {
                race: Some(race::ORC),
                ..Default::default()
            }),
            0,
        )
        .unwrap();
        assert_eq!((s.races[0], s.races[1], s.race_setting), (1, 0, 1));
    }

    #[test]
    fn random_choices() {
        assert_eq!(pick(20000, 3), 2);
        assert_eq!(pick(30000, 4), 4);
        // Seed 0: draws 0 (local), 346 (opponent), 130 (resources: Low), 10982 (terrain: winter).
        let m = Map {
            owners: owners([5, 4, 3, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let s = Start::choose(
            &pud(&m),
            &custom(Custom {
                resources: Resources::Random,
                terrain: TerrainChoice::Random,
                ..Default::default()
            }),
            0,
        )
        .unwrap();
        assert_eq!(
            (s.resources[0], s.resources[7], s.era),
            ((2100, 1100, 1000), (2100, 1100, 1000), Era::Winter)
        );
        assert_eq!(s.resources[8], (2000, 1000, 1000));
    }

    #[test]
    fn one_worker_takes_slot_0s_resources_raised_to_the_minimums() {
        let mut m = Map {
            owners: owners([5, 4, 3, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        m.gold[0] = 500;
        m.gold[1] = 9000;
        m.lumber[0] = 3000;
        let s = Start::choose(
            &pud(&m),
            &custom(Custom {
                one_worker: true,
                ..Default::default()
            }),
            0,
        )
        .unwrap();
        assert_eq!(s.resources[0], (2100, 3000, 1000));
        assert_eq!(s.resources[1], (2100, 3000, 1000));
    }

    #[test]
    fn alliance_vector() {
        let m = Map {
            owners: owners([5, 4, 4, 2, 6, 7, 3, 3]),
            ..Default::default()
        };
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:10").unwrap(), 0).unwrap();
        assert_eq!(s.controllers[3..6], [PASSIVE, RESCUE_PASSIVE, RESCUE_ACTIVE]);
        let t = s.alliances();
        let allied = |a: usize, b: usize| t[a] & (1 << b) != 0;
        assert!(allied(1, 2) && allied(2, 1));
        assert!(!allied(0, 1) && !allied(0, 2) && !allied(0, 3) && !allied(1, 0) && !allied(3, 0));
        assert!((0..16).all(|q| allied(4, q) && allied(q, 4)));
        assert!(allied(0, 5) && allied(5, 0));
        assert!(!allied(1, 5) && !allied(5, 1));
        assert!((0..16).all(|q| allied(q, 15) && allied(15, q) && allied(q, q)));
        // Passive computer and nobody slots: only rescue (passive), neutral and themselves.
        assert!((0..16).all(|q| allied(3, q) == matches!(q, 3 | 4 | 15)));
        assert!((0..16).all(|q| allied(6, q) == matches!(q, 4 | 6 | 15)));
    }

    /// G23 (static study): in a network game, with humans in slots 0 and 1 and code 7 in slot 2, slot 2 is allied both
    /// ways with both humans, whichever machine's local slot it is; in single player only the local player.
    #[test]
    fn g23_an_active_rescue_slot_allies_with_every_human() {
        let m = Map {
            owners: owners([5, 5, 7, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let mut s = Start::choose(&pud(&m), &Setup::parse("campaign:10").unwrap(), 0).unwrap();
        s.controllers[..3].copy_from_slice(&[HUMAN, HUMAN, RESCUE_ACTIVE]);
        s.multiplayer = true;
        for local in [0, 1] {
            s.local = local;
            let t = s.alliances();
            let allied = |a: usize, b: usize| t[a] & (1 << b) != 0;
            for h in [0, 1] {
                assert!(allied(h, 2) && allied(2, h), "local {local}: slot {h} and slot 2");
            }
            assert!(
                !allied(0, 1) && !allied(1, 0),
                "the humans aren't allied with each other"
            );
        }
        s.multiplayer = false;
        s.local = 0;
        let t = s.alliances();
        let allied = |a: usize, b: usize| t[a] & (1 << b) != 0;
        assert!(allied(0, 2) && allied(2, 0) && !allied(1, 2) && !allied(2, 1));
    }

    #[test]
    fn trace_start_up_alliances() {
        // Round-35 vectors. human10: slot 4 has code 6 and its units are rescued into the local player's slot; the
        // rescued units and the remaining slot-4 units are allied both ways. human5: a computer slot and a code-7
        // slot are not allied (code 7 is allied with the local player only), so the computer unit acquires.
        let m = Map {
            owners: owners([7, 2, 5, 3, 6, 2, 3, 3]),
            ..Default::default()
        };
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:0").unwrap(), 0).unwrap();
        let t = s.alliances();
        let allied = |a: usize, b: usize| t[a] & (1 << b) != 0;
        let local = 2;
        assert!(allied(local, 4) && allied(4, local));
        assert!(allied(local, 0) && allied(0, local));
        assert!(!allied(5, 0) && !allied(0, 5));
        assert!(!allied(1, 0) && !allied(0, 1));
    }

    #[test]
    fn other_owner_codes() {
        let m = Map {
            owners: owners([5, 0, 1, 9, 3, 3, 3, 3]),
            ..Default::default()
        };
        let s = Start::choose(&pud(&m), &Setup::parse("campaign:0").unwrap(), 0).unwrap();
        assert_eq!(s.controllers[..4], [HUMAN, HUMAN, COMPUTER, 9]);
    }

    #[test]
    fn races_convert_by_pair() {
        assert_eq!(for_race(0x00, race::ORC), 0x01);
        assert_eq!(for_race(0x01, race::NEUTRAL), 0x00);
        assert_eq!(for_race(0x4a, race::ORC), 0x4b);
        assert_eq!(for_race(0x5b, race::HUMAN), 0x5a);
        assert_eq!(for_race(0x67, race::ORC), 0x68);
        // Heroes, critters, mines, the circle and the portal have no pair.
        for k in [0x14, 0x19, 0x23, 0x39, 0x5c, 0x5d, 0x64, 0x65, 0x66] {
            assert_eq!((for_race(k, race::ORC), for_race(k, race::HUMAN)), (k, k));
        }
    }

    #[test]
    fn campaign_entries_from_map_names() {
        assert_eq!(campaign_entry("Campaign\\Human\\Human06.pud"), Some(10));
        assert_eq!(campaign_entry("Campaign\\Orc\\Orc05.pud"), Some(9));
        assert_eq!(campaign_entry("Campaign\\XHuman\\2XHum11.pud"), Some(48));
        assert_eq!(campaign_entry("Campaign\\XOrc\\2XOrc01.PUD"), Some(29));
        assert_eq!(campaign_entry("Maps\\Human01.pud"), None);
        assert_eq!(campaign_entry("Campaign\\Human\\Human15.pud"), None);
    }

    #[test]
    fn parses_options() {
        let s = Setup::parse(
            "custom:race=orc,opponents=2,resources=high,terrain=swamp,units=one-worker,fixed-starts,fog=off,classic",
        )
        .unwrap();
        assert!(!s.enhanced);
        let c = s.custom().unwrap();
        assert_eq!(c.race, Some(1));
        assert_eq!(c.opponents, Some(2));
        assert_eq!(c.resources, Resources::High);
        assert_eq!(c.terrain, TerrainChoice::Set(Era::Swamp));
        assert!(c.one_worker && c.fixed_starts && !c.fog_of_war);
        assert_eq!(Setup::parse("campaign:48").unwrap().campaign(), Some(48));
        assert!(Setup::parse("campaign:52").is_err());
        assert!(Setup::parse("campaign:3,race=orc").is_err());
        assert_eq!(Setup::parse("custom").unwrap(), Setup::default());
    }

    #[test]
    fn allowances_by_entry() {
        assert_eq!(campaign_allowance(0), (0x0403_0003, 0, 0));
        assert_eq!(campaign_allowance(13), campaign_allowance(10));
        assert_eq!(campaign_allowance(48), (0x7FFF_FBFF, 0x1F_FFFF, 0xF3FFF));
        assert_eq!(campaign_allowance(39), (0x7FFF_FFFF, 0x1F_FFFF, 0xF3FFF));
    }

    /// A world started on `m` with `setup` and seed 0, on the world tests' synthetic tileset and scripts. Peasants are
    /// land units here and halls 2 × 2.
    fn started(m: &Map, setup: &Setup) -> World {
        use tidewater_formats::udta::UnitStats;
        let p = pud(m);
        let start = Start::choose(&p, setup, 0).unwrap();
        let mut stats = crate::world::tests::stats();
        stats.0[kind::PEASANT as usize] = UnitStats {
            hit_points: 30,
            size: (1, 1),
            ..Default::default()
        };
        World::from_pud(
            &p,
            &crate::world::tests::tileset(),
            stats,
            crate::tech::default_upgrades(),
            crate::world::tests::scripts(),
            &start,
        )
        .unwrap()
    }

    fn units(w: &World) -> Vec<(u8, u8, (i32, i32))> {
        let mut v: Vec<_> = w.units.iter().map(|u| (u.kind, u.owner, u.tile)).collect();
        v.sort();
        v
    }

    fn campaign(entry: u8) -> Setup {
        Setup {
            game: Game::Campaign { entry },
            enhanced: true,
        }
    }

    /// X12 (`game/rescue.md`, "The rescued count"): every player's rescued count is 0 at a new game start, in a campaign
    /// mission with rescuable units and in a custom game alike, and the start-up cycles leave it there.
    #[test]
    fn every_rescued_count_is_zero_at_a_new_game_start() {
        let mut m = Map {
            size: 64,
            owners: owners([5, 3, 6, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        m.units = vec![(10, 10, kind::HUMAN_START, 0, 0), (20, 20, 0x4a, 2, 0)];
        for setup in [campaign(2), custom(Custom::default())] {
            let mut w = started(&m, &setup);
            assert!(w.players.iter().all(|p| p.rescued == 0));
            w.start_up();
            assert!(w.players.iter().all(|p| p.rescued == 0));
        }
    }

    #[test]
    fn one_worker_only() {
        let mut m = Map {
            size: 64,
            owners: owners([5, 4, 3, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        m.sides[1] = race::ORC;
        m.units = vec![
            (10, 10, kind::HUMAN_START, 0, 0),
            (20, 20, 0x4a, 0, 0),
            (50, 50, kind::ORC_START, 1, 0),
            (52, 52, 0x01, 1, 0),
            (53, 52, 0x01, 1, 0),
            (30, 30, kind::GOLD_MINE, 15, 4),
        ];
        let w = started(
            &m,
            &custom(Custom {
                one_worker: true,
                ..Default::default()
            }),
        );
        assert_eq!(
            units(&w),
            [
                (kind::PEASANT, 0, (10, 10)),
                (kind::PEON, 1, (50, 50)),
                (kind::GOLD_MINE, 15, (30, 30))
            ]
        );
        assert_eq!(w.start.view, (3, 3));
        assert_eq!(w.start.saved_views, [(3, 3); 3]);
    }

    /// Unit-slots spec, "Table sizes": a Classic start has 600 unit slots and 200 effect slots, an Enhanced one 1600
    /// and 400.
    #[test]
    fn classic_mode_has_the_smaller_tables() {
        let m = Map {
            size: 64,
            owners: owners([5, 3, 3, 3, 3, 3, 3, 3]),
            units: vec![(3, 20, kind::HUMAN_START, 0, 0)],
            ..Default::default()
        };
        for (enhanced, units, effects) in [(true, 1600, 400), (false, 600, 200)] {
            let setup = Setup {
                enhanced,
                ..campaign(0)
            };
            let w = started(&m, &setup);
            assert_eq!(
                (w.rules.unit_slots, w.rules.effect_slots, w.missiles.len()),
                (units, effects, effects),
                "enhanced {enhanced}"
            );
            assert!(!w.rules.multiplayer);
        }
    }

    #[test]
    fn a_local_start_location_sets_the_view_and_isnt_created() {
        let m = Map {
            size: 64,
            owners: owners([5, 3, 3, 3, 3, 3, 3, 3]),
            units: vec![(3, 20, kind::HUMAN_START, 0, 0)],
            ..Default::default()
        };
        let w = started(&m, &campaign(0));
        assert!(w.units.is_empty());
        assert_eq!(w.start.view, (0, 13));
    }

    #[test]
    fn the_view_finds_the_newest_gold_store_or_the_oldest_unit() {
        let mut m = Map {
            size: 128,
            owners: owners([5, 3, 3, 3, 3, 3, 3, 3]),
            units: vec![(30, 30, 0x4a, 0, 0), (60, 60, 0x58, 0, 0)],
            ..Default::default()
        };
        assert_eq!(started(&m, &campaign(0)).start.view, (53, 53));
        m.units = vec![(40, 30, 0x00, 0, 0), (20, 20, 0x00, 0, 0)];
        assert_eq!(started(&m, &campaign(0)).start.view, (33, 23));
        // Clamped to the view range, and (−7, −7) without units.
        m.units = vec![(127, 3, 0x00, 0, 0)];
        assert_eq!(started(&m, &campaign(0)).start.view, (114, 0));
        m.units.clear();
        assert_eq!(started(&m, &campaign(0)).start.view, (0, 0));
    }

    #[test]
    fn passive_owners_create_everything_and_nobody_slots_nothing() {
        let mut m = Map {
            size: 64,
            owners: owners([5, 3, 2, 3, 3, 3, 3, 3]),
            units: vec![
                (10, 10, kind::HUMAN_START, 2, 0),
                (12, 12, 0x00, 1, 0),
                (14, 14, kind::HUMAN_START, 1, 0),
            ],
            ..Default::default()
        };
        m.sides[2] = race::HUMAN;
        let w = started(&m, &campaign(0));
        assert_eq!(units(&w), [(kind::PEASANT, 2, (10, 10))]);
    }

    #[test]
    fn your_race_converts_the_local_slots_units() {
        let m = Map {
            size: 32,
            owners: owners([5, 4, 3, 3, 3, 3, 3, 3]),
            units: vec![(5, 5, 0x00, 0, 0), (8, 8, 0x4a, 0, 0), (12, 12, 0x00, 1, 0)],
            ..Default::default()
        };
        let w = started(
            &m,
            &custom(Custom {
                race: Some(race::ORC),
                ..Default::default()
            }),
        );
        assert_eq!(units(&w), [(0x00, 1, (12, 12)), (0x01, 0, (5, 5)), (0x4b, 0, (8, 8))]);
        assert_eq!(
            (w.players[0].race, w.start.race_setting, w.start.orc_sounds),
            (1, 1, true)
        );
    }

    #[test]
    fn amounts_and_held_marks() {
        let m = Map {
            size: 32,
            owners: owners([5, 3, 3, 3, 3, 3, 3, 3]),
            units: vec![
                (1, 1, kind::GOLD_MINE, 15, 4),
                (5, 5, kind::OIL_PLATFORM, 0, 3),
                (8, 8, 0x00, 0, 0x0101),
                (9, 9, 0x00, 0, 0x0100),
            ],
            ..Default::default()
        };
        let w = started(&m, &campaign(0));
        let at = |t: (i32, i32)| w.units.iter().find(|u| u.tile == t).unwrap();
        assert_eq!(at((1, 1)).resources, crate::harvest::trips(4));
        assert_eq!(
            (at((5, 5)).resources, at((5, 5)).held),
            (crate::harvest::trips(3), false)
        );
        assert!(at((8, 8)).held && !at((9, 9)).held);
    }

    #[test]
    fn rescue_slots_after_the_units() {
        let m = Map {
            size: 32,
            owners: owners([5, 3, 3, 3, 6, 7, 3, 3]),
            units: vec![
                (5, 5, 0x00, 4, 0),
                (8, 8, 0x00, 5, 0),
                (10, 10, kind::HUMAN_START, 4, 0),
            ],
            ..Default::default()
        };
        let w = started(&m, &campaign(0));
        assert_eq!((w.players[4].controller, w.players[5].controller), (PASSIVE, COMPUTER));
        // A rescue player's start location isn't a passive computer's: nothing is created for it.
        assert_eq!(w.units.len(), 2);
        assert!(w.units.iter().all(|u| u.rescuable));
        assert!(w.allied(0, 4) && w.allied(4, 0) && w.allied(0, 5) && w.allied(5, 0));
    }

    #[test]
    fn per_player_words_vision_and_colours() {
        let m = Map {
            size: 32,
            owners: owners([5, 4, 3, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let w = started(&m, &campaign(2));
        let p = &w.players[1];
        assert_eq!(
            (p.units_allowed, p.spells_known, p.spells_allowed, p.upgrades_allowed),
            (0x0407_0013, SPELLS_KNOWN, 0, 1)
        );
        let w = started(&m, &Setup::default());
        let p = &w.players[0];
        assert_eq!(
            (p.units_allowed, p.spells_known, p.spells_allowed, p.upgrades_allowed),
            (!0, 0x4020, !0, !0)
        );
        assert!((0..8).all(|q| w.players[q].shared_vision == 1 << q));
        assert!((0..16).all(|q| w.players[q].colour == q as u8));
        assert_eq!(w.vision.local, 0);
    }

    #[test]
    fn mission_48_loads_the_orc_sound_set() {
        let m = Map {
            size: 32,
            owners: owners([5, 3, 3, 3, 3, 3, 3, 3]),
            ..Default::default()
        };
        let w = started(&m, &campaign(48));
        assert_eq!((w.start.race_setting, w.start.orc_sounds), (0, true));
        assert!(!started(&m, &campaign(46)).start.orc_sounds);
        assert!(started(&m, &campaign(47)).start.orc_sounds);
    }

    #[test]
    fn mode_k_remembers_the_last_circles_region() {
        let mut m = Map {
            size: 64,
            owners: owners([5, 3, 3, 3, 3, 3, 3, 3]),
            units: vec![
                (10, 10, kind::CIRCLE_OF_POWER, 15, 0),
                (40, 40, kind::CIRCLE_OF_POWER, 15, 0),
            ],
            ..Default::default()
        };
        m.regions = (0..64 * 64).map(|n| (n / 64) as u16 + 0x100).collect();
        let w = started(&m, &campaign(17));
        assert_eq!(w.victory.circle_region, Some(0x100 + 40));
        assert_eq!(w.units.iter().filter(|u| u.kind == kind::CIRCLE_OF_POWER).count(), 2);
        assert_eq!(started(&m, &campaign(10)).victory.circle_region, None);
    }
}
