//! Workers and tankers carrying goods (spec `docs/specs/ui/carried-goods.md`). No SDL types here.
//!
//! A worker or tanker that carries a load is drawn from a carrying sprite file; the frame numbers, facing columns and
//! mirroring stay as they are, only the file changes ([`file_for`]). The file is chosen again only when the unit's
//! carrying mark changes (with its current animation) or an animation is started by an order taking over or the unit
//! going idle (with the new order's animation, "The order list" of `orders/orders.md`); between those moments the
//! unit keeps its file ([`Carried`]). A worker whose file is chosen with animation 4 (attack, chop, mine) shows the
//! plain file, so a carrying worker given an attack, harvest or repair order shows no load until it next goes idle or
//! takes an order with another animation.
//!
//! The client sees the world once a game cycle, after its update ([`Carried::update`]). A cycle in which both the mark
//! changes and an order takes over (a worker felling its share of a tree, then given its return order in the same
//! update) chooses with the new order's animation, as the take-over comes last. A killed unit counts as not carrying
//! (the mark is cleared at the kill), so it dies in the plain file's death frames.
//!
//! Files (spec "Data"): `peong.grp` (gold), `peonl.grp` (lumber) and `tankero.grp` (oil), in `Art\Unit\Human\` for
//! unit type 2 (workers) or 0x1A (tankers) and in `Art\Unit\Orc\` for every other worker or tanker type: the folder
//! follows the type, not the owner. The plain file is the type's own sheet.

use std::collections::HashMap;
use std::mem::Discriminant;

use tidewater_formats::udta::flags;
use tidewater_sim::{Gather, Order, Unit, UnitId, World};

/// Animation numbers (`core/animation-scripts.md`): still, move, attack (also chopping and mining), landing.
pub const STILL: u8 = 2;
pub const MOVE: u8 = 3;
pub const ATTACK: u8 = 4;
pub const LANDING: u8 = 6;

/// The goods a carrying file shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Load {
    Gold,
    Lumber,
    Oil,
}

/// A carrying sprite file: its load and its race folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CarryFile {
    pub load: Load,
    /// `Art\Unit\Orc\` rather than `Art\Unit\Human\`.
    pub orc: bool,
}

impl CarryFile {
    /// Every carrying file, for loading.
    pub const ALL: [CarryFile; 6] = [
        CarryFile {
            load: Load::Gold,
            orc: false,
        },
        CarryFile {
            load: Load::Lumber,
            orc: false,
        },
        CarryFile {
            load: Load::Oil,
            orc: false,
        },
        CarryFile {
            load: Load::Gold,
            orc: true,
        },
        CarryFile {
            load: Load::Lumber,
            orc: true,
        },
        CarryFile {
            load: Load::Oil,
            orc: true,
        },
    ];

    /// The file's archive name.
    pub fn path(self) -> String {
        let folder = if self.orc { "Orc" } else { "Human" };
        let stem = match self.load {
            Load::Gold => "peong",
            Load::Lumber => "peonl",
            Load::Oil => "tankero",
        };
        format!("Art\\Unit\\{folder}\\{stem}.grp")
    }
}

/// Which carrier a unit type is: a worker (peon flag, type flags bit 8) or a tanker (tanker flag, bit 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carrier {
    Worker,
    Tanker,
}

impl Carrier {
    pub fn of(type_flags: u32) -> Option<Carrier> {
        if type_flags & flags::PEON != 0 {
            Some(Carrier::Worker)
        } else if type_flags & flags::TANKER != 0 {
            Some(Carrier::Tanker)
        } else {
            None
        }
    }
}

/// The file rule ("Which file"): the carrying file a unit of `kind` is drawn from, or `None` for its plain file.
/// Workers: carrying and not in animation 4, the gold file in gather mode gold and the lumber file otherwise. Tankers:
/// the oil file whenever carrying.
pub fn file_for(carrier: Carrier, kind: u8, carrying: bool, gather: Gather, anim: u8) -> Option<CarryFile> {
    if !carrying {
        return None;
    }
    match carrier {
        Carrier::Worker if anim == ATTACK => None,
        Carrier::Worker => Some(CarryFile {
            load: if gather == Gather::Gold {
                Load::Gold
            } else {
                Load::Lumber
            },
            orc: kind != 0x02,
        }),
        Carrier::Tanker => Some(CarryFile {
            load: Load::Oil,
            orc: kind != 0x1a,
        }),
    }
}

/// The animation an order starts when it takes over ("The order list" of `orders/orders.md`, column "Animation").
pub fn order_animation(o: &Order) -> u8 {
    match o {
        Order::Attack { .. }
        | Order::AttackWall { .. }
        | Order::Chase { .. }
        | Order::TowerWatch { .. }
        | Order::AttackInPlace { .. }
        | Order::IdleReact { .. }
        | Order::AttackGround { .. }
        | Order::Demolish { .. }
        | Order::DemolishAt { .. }
        | Order::Harvest { .. }
        | Order::Repair { .. }
        | Order::Cast { .. } => ATTACK,
        Order::AttackGroundMove { .. } | Order::Enter { .. } => MOVE,
        Order::Land { .. } | Order::PushOff => LANDING,
        Order::Idle
        | Order::Move { .. }
        | Order::MoveAttack { .. }
        | Order::Patrol { .. }
        | Order::Follow { .. }
        | Order::Guard { .. }
        | Order::AttackMove { .. }
        | Order::StandGround
        | Order::ReturnGoods { .. }
        | Order::Inside { .. }
        | Order::Unload
        | Order::Docked
        | Order::Board { .. }
        | Order::Aboard { .. }
        | Order::Build { .. }
        | Order::InsideSite { .. } => STILL,
    }
}

/// What the rule reads from a worker or tanker in one look.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub carrier: Carrier,
    pub kind: u8,
    /// The carrying mark; a dying unit counts as not carrying.
    pub carrying: bool,
    pub gather: Gather,
    /// The animation running now.
    pub anim: u8,
    pub order: Order,
}

impl Look {
    pub fn of(world: &World, u: &Unit) -> Option<Look> {
        let f = world.stats.0.get(u.kind as usize).map_or(0, |s| s.flags);
        Some(Look {
            carrier: Carrier::of(f)?,
            kind: u.kind,
            carrying: u.carrying && !u.dying,
            gather: u.gather,
            anim: u.script.anim,
            order: u.order,
        })
    }

    fn file(&self, anim: u8) -> Option<CarryFile> {
        file_for(self.carrier, self.kind, self.carrying, self.gather, anim)
    }
}

/// A unit's file and what it was chosen from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chosen {
    carrying: bool,
    order: Discriminant<Order>,
    pub file: Option<CarryFile>,
}

impl Chosen {
    /// A unit seen for the first time (a new unit, or a loaded game): chosen with its order's animation.
    pub fn first(look: &Look) -> Chosen {
        Chosen {
            carrying: look.carrying,
            order: std::mem::discriminant(&look.order),
            file: look.file(order_animation(&look.order)),
        }
    }

    /// The unit seen again a cycle later ("When the file is chosen"): the mark changed (chosen with the current
    /// animation), then an order took over (chosen with its animation); otherwise the file stays.
    pub fn next(self, look: &Look) -> Chosen {
        let mut file = self.file;
        if look.carrying != self.carrying {
            file = look.file(look.anim);
        }
        let order = std::mem::discriminant(&look.order);
        if order != self.order {
            file = look.file(order_animation(&look.order));
        }
        Chosen {
            carrying: look.carrying,
            order,
            file,
        }
    }
}

/// Each worker's and tanker's chosen file, kept across the game's cycles.
#[derive(Default)]
pub struct Carried {
    units: HashMap<UnitId, Chosen>,
}

impl Carried {
    /// After a game cycle: every worker and tanker in the world is looked at once; units gone are forgotten.
    pub fn update(&mut self, world: &World) {
        let mut seen = HashMap::with_capacity(self.units.len());
        for u in &world.units {
            let Some(look) = Look::of(world, u) else { continue };
            let c = match self.units.get(&u.id) {
                Some(c) => c.next(&look),
                None => Chosen::first(&look),
            };
            seen.insert(u.id, c);
        }
        self.units = seen;
    }

    /// The carrying file `u` is drawn from, `None` for its plain file. A unit not looked at yet is chosen as on its
    /// first look.
    pub fn file(&self, world: &World, u: &Unit) -> Option<CarryFile> {
        if u.dying {
            return None;
        }
        match self.units.get(&u.id) {
            Some(c) => c.file,
            None => Chosen::first(&Look::of(world, u)?).file,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::sheet_frame;

    const PEASANT: u8 = 0x02;
    const PEON: u8 = 0x03;
    const HUMAN_TANKER: u8 = 0x1a;
    const ORC_TANKER: u8 = 0x1b;

    fn look(kind: u8, carrying: bool, gather: Gather, anim: u8, order: Order) -> Look {
        let carrier = if matches!(kind, PEASANT | PEON) {
            Carrier::Worker
        } else {
            Carrier::Tanker
        };
        Look {
            carrier,
            kind,
            carrying,
            gather,
            anim,
            order,
        }
    }

    fn path(c: &Chosen) -> Option<String> {
        c.file.map(CarryFile::path)
    }

    const HARVEST_TREE: Order = Order::Harvest {
        source: None,
        tree: (5, 5),
        chopping: true,
    };
    const RETURN: Order = Order::ReturnGoods { depot: None };

    #[test]
    fn carriers_come_from_the_type_flags() {
        assert_eq!(Carrier::of(flags::LAND | flags::PEON), Some(Carrier::Worker));
        assert_eq!(Carrier::of(flags::SEA | flags::TANKER), Some(Carrier::Tanker));
        assert_eq!(Carrier::of(flags::LAND), None, "the mark plays no part for other types");
    }

    #[test]
    fn the_file_paths() {
        let paths: Vec<String> = CarryFile::ALL.iter().map(|f| f.path()).collect();
        assert_eq!(
            paths,
            [
                "Art\\Unit\\Human\\peong.grp",
                "Art\\Unit\\Human\\peonl.grp",
                "Art\\Unit\\Human\\tankero.grp",
                "Art\\Unit\\Orc\\peong.grp",
                "Art\\Unit\\Orc\\peonl.grp",
                "Art\\Unit\\Orc\\tankero.grp",
            ]
        );
    }

    #[test]
    fn a_felling_peasant_walks_home_in_the_lumber_file() {
        // Chopping: not carrying, plain file.
        let chop = look(PEASANT, false, Gather::Lumber, ATTACK, HARVEST_TREE);
        let c = Chosen::first(&chop);
        assert_eq!(c.file, None);
        // The felling and the return order's take-over in the same update: the lumber file.
        let felled = look(PEASANT, true, Gather::Lumber, STILL, RETURN);
        let c = c.next(&felled);
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Human\\peonl.grp"));
        // Walking south (facing 4): column 4 plus the move script's frames, not mirrored.
        let c = c.next(&look(PEASANT, true, Gather::Lumber, MOVE, RETURN));
        assert!(c.file.is_some(), "a move step doesn't choose again");
        let frames: Vec<_> = [0, 5, 10, 0, 15, 20]
            .iter()
            .map(|&f| sheet_frame(false, 4, f, 65).unwrap())
            .collect();
        assert_eq!(frames, [4, 9, 14, 4, 19, 24].map(|f| (f, false)));
    }

    #[test]
    fn the_mark_set_while_chopping_keeps_the_plain_file() {
        // The mark set with animation 4 still running and no take-over yet: the plain file stays.
        let c = Chosen::first(&look(PEASANT, false, Gather::Lumber, ATTACK, HARVEST_TREE));
        let c = c.next(&look(PEASANT, true, Gather::Lumber, ATTACK, HARVEST_TREE));
        assert_eq!(c.file, None);
        // Going idle chooses again: the lumber file.
        let c = c.next(&look(PEASANT, true, Gather::Lumber, STILL, Order::Idle));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Human\\peonl.grp"));
    }

    #[test]
    fn a_peon_leaving_a_mine_carries_gold_facing_west() {
        let inside = Order::Inside { building: None };
        let c = Chosen::first(&look(PEON, false, Gather::Gold, STILL, inside));
        let c = c.next(&look(PEON, true, Gather::Gold, STILL, inside));
        let c = c.next(&look(PEON, true, Gather::Gold, STILL, RETURN));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Orc\\peong.grp"));
        assert_eq!(
            sheet_frame(false, 6, 0, 65),
            Some((2, true)),
            "west: column 2, mirrored"
        );
        // Delivered at a great hall: the plain file.
        let inside_hall = Order::Inside { building: None };
        let c = c.next(&look(PEON, false, Gather::Gold, STILL, inside_hall));
        assert_eq!(c.file, None);
    }

    #[test]
    fn a_carrying_peasant_given_an_attack_shows_no_load_until_idle() {
        let c = Chosen::first(&look(PEASANT, true, Gather::Lumber, STILL, Order::Idle));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Human\\peonl.grp"));
        let attack = Order::Attack { target: UnitId(7) };
        let c = c.next(&look(PEASANT, true, Gather::Lumber, STILL, attack));
        assert_eq!(c.file, None, "chosen with the attack order's animation 4");
        // Walking to the target (move steps) and swinging: still the plain file.
        let c = c.next(&look(PEASANT, true, Gather::Lumber, MOVE, attack));
        let c = c.next(&look(PEASANT, true, Gather::Lumber, ATTACK, attack));
        assert_eq!(c.file, None);
        let c = c.next(&look(PEASANT, true, Gather::Lumber, STILL, Order::Idle));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Human\\peonl.grp"));
    }

    #[test]
    fn a_gold_carrying_peasant_sent_to_a_tree_shows_the_plain_file() {
        let c = Chosen::first(&look(PEASANT, true, Gather::Gold, STILL, Order::Idle));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Human\\peong.grp"));
        let c = c.next(&look(PEASANT, true, Gather::Gold, MOVE, HARVEST_TREE));
        assert_eq!(c.file, None, "order 23 takes over with animation 4");
    }

    #[test]
    fn a_killed_carrier_dies_in_the_plain_file() {
        // `Look::of` counts a dying unit as not carrying.
        let c = Chosen::first(&look(PEASANT, true, Gather::Gold, STILL, Order::Idle));
        let c = c.next(&look(PEASANT, false, Gather::Gold, 1, Order::Idle));
        assert_eq!(c.file, None);
        let frames: Vec<_> = [50, 55, 60]
            .iter()
            .map(|&f| sheet_frame(false, 0, f, 65).unwrap().0)
            .collect();
        assert_eq!(frames, [50, 55, 60]);
        // A sunk tanker: frames 5 and 10 of its plain file.
        let t = Chosen::first(&look(HUMAN_TANKER, true, Gather::None, STILL, RETURN));
        assert_eq!(path(&t).as_deref(), Some("Art\\Unit\\Human\\tankero.grp"));
        let t = t.next(&look(HUMAN_TANKER, false, Gather::None, 1, RETURN));
        assert_eq!(t.file, None);
    }

    #[test]
    fn a_tanker_carries_oil_whatever_its_animation() {
        let inside = Order::Inside { building: None };
        let c = Chosen::first(&look(ORC_TANKER, false, Gather::None, STILL, inside));
        let c = c.next(&look(ORC_TANKER, true, Gather::None, ATTACK, inside));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Orc\\tankero.grp"));
        let c = c.next(&look(ORC_TANKER, true, Gather::None, MOVE, RETURN));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Orc\\tankero.grp"));
        let c = c.next(&look(ORC_TANKER, false, Gather::None, STILL, inside));
        assert_eq!(c.file, None, "delivered");
    }

    #[test]
    fn the_folder_follows_the_type_not_the_owner() {
        // A peasant (type 2) carrying gold, whoever owns it: the owner isn't an input of the rule.
        let c = Chosen::first(&look(PEASANT, true, Gather::Gold, STILL, Order::Idle));
        assert_eq!(path(&c).as_deref(), Some("Art\\Unit\\Human\\peong.grp"));
        // Any worker type but 2 uses the orc folder.
        let other = file_for(Carrier::Worker, 0x40, true, Gather::Gold, STILL);
        assert_eq!(other.map(CarryFile::path).as_deref(), Some("Art\\Unit\\Orc\\peong.grp"));
        let tanker = file_for(Carrier::Tanker, 0x41, true, Gather::None, STILL);
        assert_eq!(
            tanker.map(CarryFile::path).as_deref(),
            Some("Art\\Unit\\Orc\\tankero.grp")
        );
    }

    #[test]
    fn a_new_order_of_the_same_kind_keeps_the_file() {
        // A carrying worker in a move given another move: the same order animation, the same file.
        let mv = |g| Order::Move { goal: g, target: None };
        let c = Chosen::first(&look(PEASANT, true, Gather::Gold, MOVE, mv((1, 1))));
        let c2 = c.next(&look(PEASANT, true, Gather::Gold, MOVE, mv((9, 9))));
        assert_eq!(c2.file, c.file);
        assert!(c.file.is_some());
    }
}
