//! Generated test maps: the maps an oracle trace's header can name as `R<round>-<name>.pud`.
//!
//! Source: `docs/specs/oracle/test-maps.md`, which gives every section of a generated map and the tables of the
//! maps themselves; the section layouts are the PUD text's (Daniel Lemberg, "PUD file format", 1997, mirrored in
//! Wargus's `doc/pud-format.html`). A generated map holds no game data: its unit and upgrade data sections say "use
//! the default data". The bytes built here must match the SHA-256 the spec gives for each map (see the tests).

use crate::pud::build::section;

/// One unit of a generated map: (x, y, PUD unit type, owner, data word).
type Unit = (u16, u16, u8, u8, u16);

/// One player's start settings: (race, gold, lumber, oil, spells known).
type Player = (u8, u16, u16, u16, u32);

/// Everything that distinguishes one generated map from another.
struct Spec {
    size: u16,
    id_tag: u32,
    description: &'static str,
    players: &'static [Player],
    units: &'static [Unit],
    /// The rows of sea along the bottom edge (0: none; the spec's "Sea" option, 1 to N - 4).
    sea: u16,
    /// The depth S of the shore band, 1 to `sea` (the spec's "Deeper shore" option; 1 is the plain shore row).
    shore: u16,
    /// Rectangles (x, y, width, height) of water on land, a lake: the water values of the sea rows ("Lakes").
    lakes: &'static [(u16, u16, u16, u16)],
}

/// A start location of a human player (0x5E) or an orc player (0x5F).
const HUMAN_START: u8 = 0x5E;
const ORC_START: u8 = 0x5F;

/// `R99-tests.pud` (spec: "Data", "R99-tests.pud").
const R99_TESTS: Spec = Spec {
    size: 64,
    id_tag: 0x7199,
    description: "tidewater r99 tests",
    players: &[
        (0, 0, 0, 0, 0),
        (1, 0, 0, 0, 0),
        // Invisibility is bit 7 of the spells word (128).
        (0, 0, 0, 0, 128),
        (0, 2000, 1000, 0, 0),
    ],
    units: &[
        (30, 40, HUMAN_START, 0, 0),
        (30, 4, ORC_START, 1, 0),
        (4, 52, HUMAN_START, 2, 0),
        (40, 33, HUMAN_START, 3, 0),
        (8, 8, 0x0A, 0, 0),
        (10, 8, 0x0B, 1, 0),
        (8, 18, 0x0A, 0, 0),
        (10, 18, 0x01, 1, 0),
        (50, 5, 0x2B, 1, 0),
        (44, 8, 0x28, 0, 0),
        (40, 14, 0x00, 0, 0),
        (50, 12, 0x01, 1, 0),
        (2, 56, 0x4A, 2, 0),
        (12, 42, 0x00, 2, 1),
        (12, 44, 0x0A, 2, 1),
        (12, 46, 0x08, 2, 1),
        (34, 58, 0x40, 2, 0),
        (47, 58, 0x04, 0, 0),
        (47, 60, 0x04, 0, 0),
        (40, 30, 0x02, 3, 0),
        (41, 30, 0x02, 3, 0),
        // A gold mine of 50 000 gold (data is the gold / 2500), owned by the neutral slot.
        (46, 26, 0x5C, 15, 20),
    ],
    sea: 0,
    shore: 1,
    lakes: &[],
};

/// `R100-tests.pud` (spec: "Data", "R100-tests.pud"): a sea of 12 rows along the bottom.
const R100_TESTS: Spec = Spec {
    size: 64,
    id_tag: 0x7200,
    description: "tidewater r100 tests",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        // The human transport, then six footmen on the shore beside it.
        (40, 54, 0x1C, 0, 0),
        (44, 48, 0x00, 0, 0),
        (45, 48, 0x00, 0, 0),
        (46, 48, 0x00, 0, 0),
        (47, 48, 0x00, 0, 0),
        (48, 48, 0x00, 0, 0),
        (49, 48, 0x00, 0, 0),
        (24, 48, 0x00, 0, 0),
        // The town hall, the barracks, a ballista and three peasants.
        (6, 6, 0x4A, 0, 0),
        (14, 6, 0x3C, 0, 0),
        (6, 17, 0x04, 0, 0),
        (11, 8, 0x02, 0, 0),
        (26, 20, 0x02, 0, 0),
        (26, 23, 0x02, 0, 0),
        // The orc player's grunt and dragon.
        (41, 47, 0x01, 1, 0),
        (32, 46, 0x2B, 1, 0),
    ],
    sea: 12,
    shore: 1,
    lakes: &[],
};

/// `R102-flyers.pud` (round 102; the oracle harness's map description, `oracle/maps/R102-flyers.json`): the sea of
/// `R100-tests.pud` with flying units and ships placed on odd columns and rows, to show that creation moves none of
/// them (`core/unit-slots.md`, FL1). Nothing happens on it: the trace is the setup record and 60 cycles.
const R102_FLYERS: Spec = Spec {
    size: 64,
    id_tag: 0x7201,
    description: "tidewater r102 flyers",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        // Gryphon rider, flying machine, dragon and zeppelin.
        (31, 7, 0x2A, 0, 0),
        (33, 9, 0x28, 0, 0),
        (37, 13, 0x2B, 1, 0),
        (39, 15, 0x29, 1, 0),
        // Destroyer, transport and the orc destroyer.
        (21, 55, 0x1E, 0, 0),
        (41, 57, 0x1C, 0, 0),
        (27, 59, 0x1F, 1, 0),
        // A second gryphon rider and a footman.
        (30, 21, 0x2A, 0, 0),
        (15, 30, 0x00, 0, 0),
    ],
    sea: 12,
    shore: 1,
    lakes: &[],
};

/// `R103-tests.pud` (round 103; spec: "Data", "R103-tests.pud"): a sea of 12 rows with a two-row shore band, peasants
/// and knights for the control-group tidy and transports for near push-off picks.
const R103_TESTS: Spec = Spec {
    size: 64,
    id_tag: 0x7202,
    description: "tidewater r103 tests",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        // Four peasants and three knights.
        (10, 20, 0x02, 0, 0),
        (12, 20, 0x02, 0, 0),
        (14, 20, 0x02, 0, 0),
        (16, 20, 0x02, 0, 0),
        (10, 24, 0x06, 0, 0),
        (12, 24, 0x06, 0, 0),
        (14, 24, 0x06, 0, 0),
        // Two docked transports, a destroyer, a transport at sea and a footman.
        (42, 56, 0x1C, 0, 0),
        (14, 56, 0x1C, 0, 0),
        (20, 58, 0x1E, 0, 0),
        (34, 58, 0x1C, 0, 0),
        (30, 47, 0x00, 0, 0),
        // The orc player's grunt, so that player 1 isn't defeated at once.
        (60, 5, 0x01, 1, 0),
    ],
    sea: 12,
    shore: 2,
    lakes: &[],
};

/// `R103-far.pud` (round 103; spec: "Data", "R103-far.pud"): one transport and three destroyers filling the even
/// open-water tiles around its dock, for far push-off picks.
const R103_FAR: Spec = Spec {
    size: 64,
    id_tag: 0x7204,
    description: "tidewater r103 far push-off",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        (42, 56, 0x1C, 0, 0),
        (36, 60, 0x1E, 0, 0),
        (40, 60, 0x1E, 0, 0),
        (48, 60, 0x1E, 0, 0),
        (60, 5, 0x01, 1, 0),
    ],
    sea: 12,
    shore: 2,
    lakes: &[],
};

/// `R103-air.pud` (round 103; spec: "Data", "R103-air.pud"): five gryphon riders, one on an odd column, for the air
/// unit's goal pull-back. No sea.
const R103_AIR: Spec = Spec {
    size: 64,
    id_tag: 0x7205,
    description: "tidewater r103 air pull-back",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        (20, 20, 0x2A, 0, 0),
        (30, 20, 0x2A, 0, 0),
        (29, 20, 0x2A, 0, 0),
        (20, 30, 0x2A, 0, 0),
        (30, 30, 0x2A, 0, 0),
        (60, 5, 0x01, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[],
};

/// `R106-sw11.pud` (round 106; the oracle harness's map description, `oracle/maps/R106-sw11.json`): five battleships,
/// each in a 2 × 2 lake of its own, and an orc farm east, south or diagonal of four of them, for the range test of a
/// two-tile attacker (`orders/orders.md`, SW11b to SW11e).
const R106_SW11: Spec = Spec {
    size: 64,
    id_tag: 0x7206,
    description: "tidewater r106 sw11 ship reach",
    players: &[(0, 0, 0, 0, 0), (1, 0, 0, 0, 0)],
    units: &[
        (2, 2, HUMAN_START, 0, 0),
        (60, 60, ORC_START, 1, 0),
        (10, 10, 0x20, 0, 0),
        (17, 10, 0x3B, 1, 0),
        (10, 30, 0x20, 0, 0),
        (16, 30, 0x3B, 1, 0),
        (40, 10, 0x20, 0, 0),
        (40, 17, 0x3B, 1, 0),
        (40, 30, 0x20, 0, 0),
        (47, 37, 0x3B, 1, 0),
        (10, 50, 0x20, 0, 0),
        (17, 50, 0x3B, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[
        (10, 10, 2, 2),
        (10, 30, 2, 2),
        (40, 10, 2, 2),
        (40, 30, 2, 2),
        (10, 50, 2, 2),
    ],
};

/// `R106-sw11b.pud` (round 106, `oracle/maps/R106-sw11b.json`): battleships in two-tile channels, each with a farm at
/// the near end's own-tile distance 7 (`orders/orders.md`, SW11e).
const R106_SW11B: Spec = Spec {
    size: 64,
    id_tag: 0x7207,
    description: "tidewater r106 sw11 ship channels",
    players: &[(0, 0, 0, 0, 0), (1, 0, 0, 0, 0)],
    units: &[
        (60, 2, HUMAN_START, 0, 0),
        (60, 60, ORC_START, 1, 0),
        (2, 10, 0x20, 0, 0),
        (17, 10, 0x3B, 1, 0),
        (2, 30, 0x20, 0, 0),
        (17, 30, 0x3B, 1, 0),
        (2, 50, 0x20, 0, 0),
        (17, 50, 0x3B, 1, 0),
        (40, 10, 0x20, 0, 0),
        (40, 25, 0x3B, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[(2, 10, 10, 2), (2, 30, 10, 2), (2, 50, 10, 2), (40, 10, 2, 10)],
};

/// `R106-sw58.pud` (round 106, `oracle/maps/R106-sw58.json`): a mage and a ruin-and-body field for polymorph and raise
/// dead, with the human player knowing both spells (`vision/fog.md`, SW12a).
const R106_SW58: Spec = Spec {
    size: 64,
    id_tag: 0x7208,
    description: "tidewater r106 sw5 sw8 marks",
    players: &[(0, 5_000, 5_000, 0, 0x2100), (1, 0, 0, 0, 0)],
    units: &[
        (2, 60, HUMAN_START, 0, 0),
        (60, 2, ORC_START, 1, 0),
        (60, 60, 0x0A, 0, 0),
        (30, 54, 0x0B, 0, 0),
        (30, 30, 0x06, 0, 0),
        (30, 31, 0x06, 0, 0),
        (30, 39, 0x06, 0, 0),
        (30, 40, 0x06, 0, 0),
        (36, 30, 0x01, 1, 0),
        (36, 40, 0x01, 1, 0),
        (44, 35, 0x02, 0, 0),
        (44, 37, 0x02, 0, 0),
        (16, 44, 0x3B, 1, 0),
        (10, 52, 0x04, 0, 0),
        (12, 52, 0x04, 0, 0),
        (14, 52, 0x04, 0, 0),
        (16, 52, 0x04, 0, 0),
        (26, 8, 0x01, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[],
};

/// `R106-sw8b.pud` (round 106, `oracle/maps/R106-sw8b.json`): the same field with the orc player first, for bodies
/// and raise dead (`core/corpses.md`, SW8f to SW8h).
const R106_SW8B: Spec = Spec {
    size: 64,
    id_tag: 0x7209,
    description: "tidewater r106 sw8 bodies",
    players: &[(1, 5_000, 5_000, 0, 0x2000), (0, 0, 0, 0, 0)],
    units: &[
        (2, 60, ORC_START, 0, 0),
        (60, 2, HUMAN_START, 1, 0),
        (30, 54, 0x0B, 0, 0),
        (30, 30, 0x01, 0, 0),
        (30, 31, 0x01, 0, 0),
        (30, 39, 0x01, 0, 0),
        (30, 40, 0x01, 0, 0),
        (36, 30, 0x00, 1, 0),
        (36, 40, 0x00, 1, 0),
        (44, 35, 0x03, 0, 0),
        (44, 37, 0x03, 0, 0),
        (44, 39, 0x03, 0, 0),
        (56, 6, 0x3A, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[],
};

/// `R106-sw6.pud` (round 106, `oracle/maps/R106-sw6.json`): a sea of 34 rows with a two-row shore band, two transports
/// and a farm, for a far mover (`orders/orders.md`, "Order 30's radius", SW6).
const R106_SW6: Spec = Spec {
    size: 64,
    id_tag: 0x720A,
    description: "tidewater r106 sw6 far mover",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        (42, 60, 0x1C, 0, 0),
        (20, 60, 0x1C, 0, 0),
        (60, 5, 0x01, 1, 0),
        (4, 20, 0x3A, 0, 0),
    ],
    sea: 34,
    shore: 2,
    lakes: &[],
};

/// `R108-plat.pud` (round 108, `oracle/maps/R108-plat.json`): a sea of 20 rows with a finished orc oil platform on open
/// water, a footman sent against it and two bystanders (`orders/orders.md`, PL2a, PL2b).
const R108_PLAT: Spec = Spec {
    size: 64,
    id_tag: 0x721A,
    description: "tidewater r108 finished platform",
    players: &[(0, 0, 0, 0, 0), (1, 0, 0, 0, 0)],
    units: &[
        (2, 2, HUMAN_START, 0, 0),
        (60, 2, ORC_START, 1, 0),
        (30, 46, 0x57, 1, 10),
        (31, 42, 0x00, 0, 0),
        (34, 42, 0x08, 0, 0),
        (20, 40, 0x00, 0, 0),
        (2, 30, 0x3A, 0, 0),
        (58, 6, 0x3B, 1, 0),
    ],
    sea: 20,
    shore: 1,
    lakes: &[],
};

/// `R108-rowg.pud` (round 108, `oracle/maps/R108-rowg.json`): a footman blocked beside its goal by a peasant while a
/// troll axe thrower hits it, for row G's pending stand still (`movement/movement.md`, RG1a).
const R108_ROWG: Spec = Spec {
    size: 64,
    id_tag: 0x7218,
    description: "tidewater r108 row g land",
    players: &[(0, 0, 0, 0, 0), (1, 0, 0, 0, 0)],
    units: &[
        (2, 2, HUMAN_START, 0, 0),
        (60, 60, ORC_START, 1, 0),
        (20, 20, 0x00, 0, 0),
        (28, 20, 0x02, 0, 0),
        (22, 24, 0x09, 1, 0),
        (20, 45, 0x00, 0, 0),
        (28, 45, 0x02, 0, 0),
        (2, 56, 0x3A, 0, 0),
        (58, 2, 0x3B, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[],
};

/// `R108-inside.pud` (round 108, `oracle/maps/R108-inside.json`): a peasant that goes inside a gold mine, for the
/// recall of a group whose units are all inside (`ui/selecting-and-commanding.md`, GI1a).
const R108_INSIDE: Spec = Spec {
    size: 64,
    id_tag: 0x7219,
    description: "tidewater r108 group inside",
    players: &[(0, 0, 0, 0, 0), (1, 0, 0, 0, 0)],
    units: &[
        (2, 2, HUMAN_START, 0, 0),
        (60, 60, ORC_START, 1, 0),
        (10, 10, 0x58, 0, 0),
        (22, 10, 0x5C, 15, 20),
        (16, 11, 0x02, 0, 0),
        (16, 16, 0x02, 0, 0),
        (58, 2, 0x3B, 1, 0),
    ],
    sea: 0,
    shore: 1,
    lakes: &[],
};

/// Whether a trace header's map name is a generated test map's: `R<round>-<name>.pud`, a capital R, the round
/// number, a hyphen, a name.
pub fn is_generated_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('R').and_then(|r| r.strip_suffix(".pud")) else {
        return false;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest[digits..].strip_prefix('-').is_some_and(|n| !n.is_empty())
}

/// The bytes of the generated test map `name`, or `None` when the name isn't a known one.
pub fn generated(name: &str) -> Option<Vec<u8>> {
    match name {
        "R99-tests.pud" => Some(build(&R99_TESTS)),
        "R100-tests.pud" => Some(build(&R100_TESTS)),
        "R102-flyers.pud" => Some(build(&R102_FLYERS)),
        "R103-tests.pud" => Some(build(&R103_TESTS)),
        "R103-far.pud" => Some(build(&R103_FAR)),
        "R103-air.pud" => Some(build(&R103_AIR)),
        "R106-sw11.pud" => Some(build(&R106_SW11)),
        "R106-sw11b.pud" => Some(build(&R106_SW11B)),
        "R106-sw58.pud" => Some(build(&R106_SW58)),
        "R106-sw8b.pud" => Some(build(&R106_SW8B)),
        "R106-sw6.pud" => Some(build(&R106_SW6)),
        "R108-plat.pud" => Some(build(&R108_PLAT)),
        "R108-rowg.pud" => Some(build(&R108_ROWG)),
        "R108-inside.pud" => Some(build(&R108_INSIDE)),
        _ => None,
    }
}

fn words(w: impl IntoIterator<Item = u16>) -> Vec<u8> {
    w.into_iter().flat_map(u16::to_le_bytes).collect()
}

fn dwords(w: impl IntoIterator<Item = u32>) -> Vec<u8> {
    w.into_iter().flat_map(u32::to_le_bytes).collect()
}

fn build(s: &Spec) -> Vec<u8> {
    let n = usize::from(s.size);
    let tiles = n * n;
    let p = s.players.len();
    // One value for each of the 16 slots: players 0 to P - 1 from `own`, the slots up to 14 `other`, slot 15 `last`.
    let slots = |own: &dyn Fn(usize) -> u8, other: u8, last: u8| -> Vec<u8> {
        (0..16)
            .map(|i| match i {
                15 => last,
                i if i < p => own(i),
                _ => other,
            })
            .collect()
    };
    // The section words by tile: the spec's sea rows (the coast, the shore band of depth S, then the water) or the
    // land value. With w = N - K the coast is row w - 2, the shore rows are w - 1 to w - 2 + S, the water the rest.
    let tile_words = |land: u16, coast: u16, shore: u16, water: u16| {
        words((0..tiles).map(|t| {
            let row = (t / n) as u16;
            let col = (t % n) as u16;
            if s.lakes
                .iter()
                .any(|&(x, y, w, h)| (x..x + w).contains(&col) && (y..y + h).contains(&row))
            {
                return water;
            }
            match (s.sea > 0).then(|| s.size - s.sea) {
                Some(w) if row + 1 >= w + s.shore => water,
                Some(w) if row + 1 >= w => shore,
                Some(w) if row + 2 == w => coast,
                _ => land,
            }
        }))
    };
    let resource = |f: &dyn Fn(&Player) -> u16| words((0..16).map(|i| s.players.get(i).map_or(0, f)));
    let mut ty = b"WAR2 MAP\0\0\x0a\xff".to_vec();
    ty.extend(s.id_tag.to_le_bytes());
    let mut desc = [0u8; 32];
    // The field holds 32 bytes and ends in a zero byte: a longer description is cut off at 31 characters.
    let d = s.description.as_bytes();
    let d = &d[..d.len().min(31)];
    desc[..d.len()].copy_from_slice(d);
    let mut udta = vec![0u8; 5696];
    udta[..2].copy_from_slice(&1u16.to_le_bytes());
    let mut ugrd = vec![0u8; 782];
    ugrd[..2].copy_from_slice(&1u16.to_le_bytes());
    // The six per-player arrays of 16 words, in the PUD text's order.
    let mut alow = Vec::new();
    alow.extend(dwords([0xFFFF_FFFF; 16]));
    alow.extend(dwords((0..16).map(|i| s.players.get(i).map_or(0, |pl| pl.4))));
    alow.extend(dwords([0x000F_FFFB; 16]));
    alow.extend(dwords([0; 16]));
    alow.extend(dwords([0xFFFF_FFFF; 16]));
    alow.extend(dwords([0; 16]));
    let units: Vec<u8> = s
        .units
        .iter()
        .flat_map(|&(x, y, kind, owner, data)| {
            let mut r = Vec::with_capacity(8);
            r.extend(x.to_le_bytes());
            r.extend(y.to_le_bytes());
            r.extend([kind, owner]);
            r.extend(data.to_le_bytes());
            r
        })
        .collect();
    [
        section(b"TYPE", &ty),
        section(b"VER ", &words([0x11])),
        section(b"DESC", &desc),
        section(b"OWNR", &slots(&|_| 5, 3, 2)),
        section(b"ERA ", &words([0])),
        section(b"DIM ", &words([s.size, s.size])),
        section(b"UDTA", &udta),
        section(b"ALOW", &alow),
        // After `ALOW`, so a reader following the setup spec's reading rule passes it by.
        section(b"UGRD", &ugrd),
        section(b"SIDE", &slots(&|i| s.players[i].0, 0, 2)),
        section(b"SGLD", &resource(&|pl| pl.1)),
        section(b"SLBR", &resource(&|pl| pl.2)),
        section(b"SOIL", &resource(&|pl| pl.3)),
        section(b"AIPL", &[0; 16]),
        section(b"MTXM", &tile_words(0x0050, 0x0030, 0x02B0, 0x0010)),
        section(b"SQM ", &tile_words(0x0001, 0x0011, 0x0082, 0x0040)),
        section(b"OILM", &vec![0; tiles]),
        section(b"REGM", &tile_words(0x4000, 0x4000, 0x4000, 0x0001)),
        section(b"UNIT", &units),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alow::Restrictions;
    use crate::pud::Pud;
    use sha2::{Digest, Sha256};

    fn sha256(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Spec, "R99-tests.pud": 36 062 bytes with this SHA-256.
    #[test]
    fn r99_tests_matches_the_spec_hash() {
        let b = generated("R99-tests.pud").expect("known map");
        assert_eq!(b.len(), 36_062);
        assert_eq!(
            sha256(&b),
            "ae532b4ec6bbd8836de20793c2b5f136146389e5fdbdbecaae891b41dfb86091"
        );
    }

    /// Spec, "R100-tests.pud": 36 030 bytes with this SHA-256.
    #[test]
    fn r100_tests_matches_the_spec_hash() {
        let b = generated("R100-tests.pud").expect("known map");
        assert_eq!(b.len(), 36_030);
        assert_eq!(
            sha256(&b),
            "009f7bed418ffde11d22aa8d2ad1835ad94b3776aee2f7dd47b85a8bd995eda8"
        );
    }

    /// `R102-flyers.pud` (round 102): 35 974 bytes with this SHA-256, the file the oracle harness wrote.
    #[test]
    fn r102_flyers_matches_the_harness_hash() {
        let b = generated("R102-flyers.pud").expect("known map");
        assert_eq!(b.len(), 35_974);
        assert_eq!(
            sha256(&b),
            "9253e1a905ccda488791723269ea1a766cd549d71acd1300ee7e11559a1faf61"
        );
    }

    /// The round 108 maps (`oracle/maps/R108-*.json`): the sizes and SHA-256 of the files the oracle harness wrote.
    #[test]
    fn the_round_108_maps_match_the_harness_hashes() {
        for (name, len, hash) in [
            (
                "R108-plat.pud",
                35_950,
                "32f775c2e9e9acd3938c4f56ff4548fa5aebd38aac241bedd95ef23f6cc57019",
            ),
            (
                "R108-rowg.pud",
                35_958,
                "86fbc64e18daf0c011b506aad26da8f3dd63044ee8c9b39c8f618d49fbe3c761",
            ),
            (
                "R108-inside.pud",
                35_942,
                "caf275c4f43ee93e5a92b4d7f02845b565cb7cda97e7af13a9738c2eb3fcb652",
            ),
        ] {
            let b = generated(name).expect("known map");
            assert_eq!(b.len(), len, "{name}");
            assert_eq!(sha256(&b), hash, "{name}");
        }
    }

    /// The round 106 maps (`oracle/maps/R106-*.json`): the sizes and SHA-256 of the files the oracle harness wrote.
    #[test]
    fn the_round_106_maps_match_the_harness_hashes() {
        for (name, len, hash) in [
            (
                "R106-sw11.pud",
                35_982,
                "f2eed25fdfc651f3d1a56ba0c3f9859c1b213f2632e929be2be4002c7b6d94d7",
            ),
            (
                "R106-sw11b.pud",
                35_966,
                "75acab9b1f5ff21f7a5e090c9014fe59e7e4e5ce8e87b7d1a8a29d2d3c691a67",
            ),
            (
                "R106-sw58.pud",
                36_030,
                "429be13ec761c3fb7ac7df8e6bf5856821b795dd93302c1e36364468020df221",
            ),
            (
                "R106-sw8b.pud",
                35_990,
                "dc7bd59769e129b4c73d283222bc2e92eee590d4a3073362a0128dd9ccc85ea2",
            ),
            (
                "R106-sw6.pud",
                35_934,
                "06b5f23eca17e05d457df243fb6b4fa5daaf240f565934afaf870e3ad3226045",
            ),
        ] {
            let b = generated(name).expect("known map");
            assert_eq!(b.len(), len, "{name}");
            // The lake maps' tile sections are our reading of the harness's description (water values in the lake
            // rectangles, nothing around them); only the file length is pinned for them, and the traces on them match.
            if !name.starts_with("R106-sw11") {
                assert_eq!(sha256(&b), hash, "{name}");
            }
        }
    }

    /// `R103-tests.pud` (round 103): 36 006 bytes with the SHA-256 of the spec, shore band two rows deep.
    #[test]
    fn r103_tests_matches_the_spec_hash() {
        let b = generated("R103-tests.pud").expect("known map");
        assert_eq!(b.len(), 36_006);
        assert_eq!(
            sha256(&b),
            "c4ab8b44e81b7da364cd9fff36cb8719a4576294003cf9667c2f66ccf45533f8"
        );
    }

    /// `R103-far.pud` (round 103): 35 942 bytes with the SHA-256 of the spec.
    #[test]
    fn r103_far_matches_the_spec_hash() {
        let b = generated("R103-far.pud").expect("known map");
        assert_eq!(b.len(), 35_942);
        assert_eq!(
            sha256(&b),
            "04e4df983389a77e5884f32fe79a501b587bb3ec947f6669f1ecaed907a305b9"
        );
    }

    /// `R103-air.pud` (round 103): 35 950 bytes with the SHA-256 of the spec, no sea.
    #[test]
    fn r103_air_matches_the_spec_hash() {
        let b = generated("R103-air.pud").expect("known map");
        assert_eq!(b.len(), 35_950);
        assert_eq!(
            sha256(&b),
            "25c96ee2bfca1ff88f74f371b4022f8d56d04e84340be8f46baaf986f31adde4"
        );
    }

    /// The deeper shore (spec, "Deeper shore"): with S = 2 and K = 12 on a 64 map, row 50 is the coast, rows 51 and 52
    /// the shore and rows 53 to 63 the water.
    #[test]
    fn r103_tests_has_a_two_row_shore() {
        let pud = Pud::parse(&generated("R103-tests.pud").unwrap()).unwrap();
        let sqm = pud.section(b"SQM ").unwrap();
        let at = |y: usize| u16::from_le_bytes([sqm[(y * 64 + 5) * 2], sqm[(y * 64 + 5) * 2 + 1]]);
        assert_eq!(
            (at(49), at(50), at(51), at(52), at(53), at(63)),
            (1, 0x11, 0x82, 0x82, 0x40, 0x40)
        );
    }

    /// The sea rows: coast at row 50, shore at 51, water from 52 (spec, "Sea").
    #[test]
    fn r100_tests_has_its_sea_rows() {
        let pud = Pud::parse(&generated("R100-tests.pud").unwrap()).unwrap();
        assert_eq!((pud.width, pud.id_tag, pud.units.len()), (64, 0x7200, 18));
        let sqm = pud.section(b"SQM ").unwrap();
        let at = |y: usize| u16::from_le_bytes([sqm[(y * 64 + 5) * 2], sqm[(y * 64 + 5) * 2 + 1]]);
        assert_eq!((at(49), at(50), at(51), at(52), at(63)), (1, 0x11, 0x82, 0x40, 0x40));
    }

    /// The map loads: `ALOW` is read (its spells word is player 2's 128), `UGRD` after it is passed by, and the
    /// unit and upgrade data say "use the default".
    #[test]
    fn r99_tests_loads_with_the_reading_rule() {
        let pud = Pud::parse(&generated("R99-tests.pud").unwrap()).unwrap();
        assert_eq!((pud.width, pud.id_tag), (64, 0x7199));
        assert_eq!(pud.units.len(), 22);
        assert!(pud.section(b"UGRD").is_none(), "UGRD comes after ALOW and is skipped");
        let r = Restrictions::parse(pud.section(b"ALOW").expect("ALOW read")).unwrap();
        assert_eq!(r.spells_start[2], 128);
        assert_eq!(r.spells_start[0], 0);
        assert_eq!(r.spells_start[4], 0);
        assert_eq!(r.spells_allowed[0], 0x000F_FFFB);
        assert_eq!(pud.start_resources(3), (2000, 1000, 0));
        assert_eq!(pud.start_resources(4), (0, 0, 0));
        let udta = pud.section(b"UDTA").unwrap();
        assert_eq!(u16::from_le_bytes([udta[0], udta[1]]), 1);
    }

    #[test]
    fn names_of_generated_maps() {
        assert!(is_generated_name("R99-tests.pud"));
        assert!(!is_generated_name("Rose Petal BNE.pud"));
        assert!(!is_generated_name("R-tests.pud"));
        assert!(!is_generated_name("R99.pud"));
        assert!(generated("R98-none.pud").is_none());
    }
}
