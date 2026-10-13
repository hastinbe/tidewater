//! The synced selection, control groups and the commands that act on them. Spec:
//! `docs/specs/ui/selecting-and-commanding.md`.
//!
//! Each player has a selection of up to 9 units and 18 control groups, all part of the game state. The interface
//! changes them with player commands ([`Command::Select`], [`Command::SetGroup`], [`Command::RecallGroup`]); the
//! right-click command and the commands from target mode, the buttons and the keys ([`Command::RightClick`],
//! [`Command::Issue`]) then act on the issuing player's synced selection. Picking units with the mouse is the client's
//! job (it needs the local view); this module holds what runs inside a cycle.
//!
//! Unit type numbers and flag bits are those of the public PUD specification (appendix A and the UDTA flags list).
//!
//! Our readings where the spec is open:
//! - Command 4 gives order 4 directly, command 10 is taken as an attack-move (order 10), commands 12 and 16 need a
//!   target unit, and command 27 without a target unit does nothing.
//! - The pause count of Enhanced multiplayer isn't limited (its starting value belongs to a later spec).
//! - Under Classic rules a speed step moves the stored 9-level setting between 0 and 6.

use tidewater_formats::udta::flags;

use crate::world::{Command, Order, PLAYERS, Unit, UnitId, World};
use crate::{slots, terrain};

/// Places in a selection and in a control group.
pub const PLACES: usize = 9;
/// Control groups per player; the keys use 0 to 9.
pub const GROUPS: usize = 18;
/// Highest speed level a speed key reaches.
pub const TOP_SPEED_ENHANCED: u8 = 8;
pub const TOP_SPEED_CLASSIC: u8 = 6;
/// Spread on an axis, in tiles, from which the formation offset no longer keeps the group's shape.
pub const FORMATION_SPREAD: i32 = 4;

/// Unit types named by the rules (public PUD specification, appendix A).
pub mod kind {
    pub const KNIGHT: u8 = 0x06;
    pub const OGRE: u8 = 0x07;
    pub const MAGE: u8 = 0x0a;
    pub const DEATH_KNIGHT: u8 = 0x0b;
    pub const PALADIN: u8 = 0x0c;
    pub const OGRE_MAGE: u8 = 0x0d;
    pub const DEMOLITION_SQUAD: u8 = 0x0e;
    pub const SAPPER: u8 = 0x0f;
    pub const TERON_GOREFIEND: u8 = 0x15;
    pub const DENTARG: u8 = 0x17;
    pub const KHADGAR: u8 = 0x18;
    pub const FLYING_MACHINE: u8 = 0x28;
    pub const ZEPPELIN: u8 = 0x29;
    pub const TURALYON: u8 = 0x2c;
    pub const EYE_OF_KILROGG: u8 = 0x2d;
    pub const CHOGALL: u8 = 0x31;
    pub const LOTHAR: u8 = 0x32;
    pub const GULDAN: u8 = 0x33;
    pub const UTHER: u8 = 0x34;
    /// The last type with a right-click action.
    pub const CRITTER: u8 = 0x39;
}

/// Right-click actions (UDTA "2nd mouse button action").
pub mod action {
    pub const NONE: u8 = 0;
    pub const ATTACK: u8 = 1;
    pub const MOVE: u8 = 2;
    pub const HARVEST: u8 = 3;
    pub const HAUL_OIL: u8 = 4;
    pub const DEMOLISH: u8 = 5;
    pub const SAIL: u8 = 6;
}

/// Flag bits 0 to 4: land, air, explode, sea, critter.
pub const MOBILE: u32 = flags::LAND | flags::AIR | flags::EXPLODE_WHEN_KILLED | flags::SEA | flags::CRITTER;

/// One player's synced selection and control groups.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlayerSelection {
    /// Under Classic rules a lost unit leaves its place empty; under Enhanced rules the rest close up.
    pub places: [Option<UnitId>; PLACES],
    pub groups: [Vec<UnitId>; GROUPS],
}

impl PlayerSelection {
    /// The selected units in place order.
    pub fn units(&self) -> impl Iterator<Item = UnitId> + '_ {
        self.places.iter().flatten().copied()
    }

    fn set(&mut self, units: impl IntoIterator<Item = UnitId>) {
        self.places = [None; PLACES];
        for (p, id) in self.places.iter_mut().zip(units) {
            *p = Some(id);
        }
    }
}

/// Whether a command number is one the game carries out from target mode, buttons or keys: 2 to 6, 8 to 17, 19, 23,
/// 24, 27, 29 and 38 to 56.
pub fn handled(command: u8) -> bool {
    matches!(command, 2..=6 | 8..=17 | 19 | 23 | 24 | 27 | 29 | 38..=56)
}

/// The formation offset ("Formation offset"): the tile each unit at `tiles` aims at for a click on `target`. `wall`
/// is whether the target tile has a wall flag; `map` the map size in tiles.
pub fn formation(tiles: &[(i32, i32)], target: (i32, i32), wall: bool, map: (i32, i32)) -> Vec<(i32, i32)> {
    let n = tiles.len() as i32;
    if wall || n == 0 {
        return vec![target; tiles.len()];
    }
    let axis = |a: fn(&(i32, i32)) -> i32| {
        let v: Vec<i32> = tiles.iter().map(a).collect();
        let (lo, hi) = (*v.iter().min().unwrap(), *v.iter().max().unwrap());
        (v.iter().sum::<i32>() / n, lo, hi)
    };
    let (mx, x0, x1) = axis(|t| t.0);
    let (my, y0, y1) = axis(|t| t.1);
    let inside = (x0..=x1).contains(&target.0) && (y0..=y1).contains(&target.1);
    if inside {
        return vec![target; tiles.len()];
    }
    let aim = |own: i32, mean: i32, lo: i32, hi: i32, to: i32, len: i32| {
        if hi - lo < FORMATION_SPREAD {
            (own + to - mean).clamp(0, (len - 1).max(0))
        } else {
            to
        }
    };
    tiles
        .iter()
        .map(|&(x, y)| (aim(x, mx, x0, x1, target.0, map.0), aim(y, my, y0, y1, target.1, map.1)))
        .collect()
}

impl World {
    fn flags_of_kind(&self, kind: u8) -> u32 {
        self.stats_of(kind).flags
    }

    /// Mobile: any of flag bits 0 to 4.
    pub fn is_mobile(&self, u: &Unit) -> bool {
        self.flags_of_kind(u.kind) & MOBILE != 0
    }

    /// Whether the target tile has a wall flag (0x04 or 0x08 of the movement word).
    pub(crate) fn wall_tile(&self, t: (i32, i32)) -> bool {
        use terrain::bits::{COMPUTER_WALL, HUMAN_WALL};
        self.terrain.contains(t.0, t.1) && self.terrain.word(t.0, t.1) & (HUMAN_WALL | COMPUTER_WALL) != 0
    }

    /// "Allied" as the right-click tables use it: the player itself, its allies, and the neutral player, who is
    /// allied to everyone.
    fn allied_to(&self, player: u8, owner: u8) -> bool {
        owner == slots::NEUTRAL || self.allied(player, owner)
    }

    /// The player's synced selection.
    pub fn selection(&self, player: u8) -> Vec<UnitId> {
        self.selections
            .get(player as usize)
            .map_or_else(Vec::new, |s| s.units().collect())
    }

    /// Applies the commands of this module; `false` for any other command.
    pub(crate) fn selection_command(&mut self, c: &Command) -> bool {
        match *c {
            Command::Select { player, ref units } => {
                if let Some(s) = self.selections.get_mut(player as usize) {
                    s.set(units.iter().copied());
                }
            }
            Command::RightClick { player, target, tile } => self.right_click(player, target, tile),
            Command::Issue {
                player,
                command,
                target,
                tile,
            } => self.issue(player, command, target, tile),
            Command::SetGroup { player, group } => self.set_group(player, group),
            Command::RecallGroup { player, group } => self.recall_group(player, group),
            Command::Speed { faster, .. } => {
                let top = if self.rules.enhanced {
                    TOP_SPEED_ENHANCED
                } else {
                    TOP_SPEED_CLASSIC
                };
                self.speed = if faster {
                    (self.speed + 1).min(top)
                } else {
                    self.speed.saturating_sub(1).min(top)
                };
            }
            Command::Pause { .. } => self.paused = !self.paused,
            _ => return false,
        }
        true
    }

    /// "Losing a unit from the selection": a unit that is dying, dead, removed or inside something leaves every
    /// player's synced selection. Under Enhanced rules the rest close up; under Classic rules its place stays empty.
    pub(crate) fn prune_selections(&mut self) {
        let enhanced = self.rules.enhanced;
        for p in 0..PLAYERS {
            let places = self.selections[p].places;
            let keep: Vec<bool> = places
                .iter()
                .map(|id| id.is_none_or(|id| self.unit(id).is_some_and(|u| u.present())))
                .collect();
            if keep.iter().all(|&k| k) {
                continue;
            }
            let s = &mut self.selections[p];
            if enhanced {
                s.set(places.iter().zip(&keep).filter(|(_, k)| **k).filter_map(|(id, _)| *id));
            } else {
                for (n, k) in keep.iter().enumerate() {
                    if !k {
                        s.places[n] = None;
                    }
                }
            }
        }
    }

    /// The units of the player's synced selection with the tile each aims at after the formation offset, for a
    /// command at `tile`.
    fn aims(&self, player: u8, tile: (i32, i32)) -> Vec<(usize, (i32, i32))> {
        let units: Vec<usize> = self
            .selection(player)
            .into_iter()
            .filter_map(|id| self.index(id))
            .collect();
        let tiles: Vec<(i32, i32)> = units.iter().map(|&i| self.units[i].tile).collect();
        let map = (self.terrain.width as i32, self.terrain.height as i32);
        let aims = formation(&tiles, tile, self.wall_tile(tile), map);
        units.into_iter().zip(aims).collect()
    }

    /// "What a right-click command does".
    fn right_click(&mut self, player: u8, target: Option<UnitId>, tile: (i32, i32)) {
        for (i, aim) in self.aims(player, tile) {
            let u = &self.units[i];
            if u.owner != player || !self.is_mobile(u) || Some(u.id) == target || u.kind > kind::CRITTER {
                continue;
            }
            let Some(command) = self.right_click_command(i, player, target, tile) else {
                continue;
            };
            if self.rules.enhanced {
                self.units[i].remembered = None;
            }
            // The tree row gives command 23 with no target unit.
            let target = if command == 23 && target.is_none() {
                None
            } else {
                target
            };
            self.give_command(i, command, target, aim);
        }
    }

    /// The command a unit's right-click action chooses ("What a right-click command does", and the harvest table),
    /// or `None` for action 0.
    pub(crate) fn right_click_command(
        &self,
        i: usize,
        player: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    ) -> Option<u8> {
        let u = &self.units[i];
        let s = self.stats_of(u.kind);
        let t = target.and_then(|t| self.unit(t));
        let allied = t.map(|t| self.allied_to(player, t.owner));
        Some(match s.right_click {
            action::MOVE | action::SAIL => 3,
            action::ATTACK => {
                if s.flags & flags::CAN_ATTACK == 0 {
                    3
                } else if allied == Some(false) {
                    8
                } else if self.wall_tile(tile) {
                    11
                } else {
                    3
                }
            }
            action::HAUL_OIL => match t {
                Some(t) if t.owner == player => {
                    let tf = self.flags_of_kind(t.kind);
                    if u.carrying && tf & flags::OIL_STORAGE != 0 {
                        24
                    } else if !u.carrying && tf & flags::OIL_SOURCE != 0 {
                        23
                    } else {
                        3
                    }
                }
                _ => 3,
            },
            action::DEMOLISH => {
                if allied == Some(false) {
                    19
                } else {
                    3
                }
            }
            action::HARVEST => self.harvest_choice(u, player, t, tile),
            _ => return None,
        })
    }

    /// The harvest action's table (first match).
    fn harvest_choice(&self, u: &Unit, player: u8, t: Option<&Unit>, tile: (i32, i32)) -> u8 {
        use crate::Gather;
        let Some(t) = t else {
            let tree = self
                .terrain
                .region(tile.0, tile.1)
                .is_some_and(terrain::region::is_forest);
            return if tree && !u.carrying { 23 } else { 3 };
        };
        if !self.allied_to(player, t.owner) {
            return 8;
        }
        if t.owner != player {
            return 3;
        }
        let tf = self.flags_of_kind(t.kind);
        if !u.carrying && tf & flags::MINE != 0 {
            23
        } else if u.carrying
            && ((u.gather == Gather::Lumber && tf & flags::WOOD_STORAGE != 0)
                || (tf & flags::GOLD_STORAGE != 0 && tf & flags::WOOD_STORAGE == 0))
        {
            // Lumber to a lumber store; anything to a store taking gold but not lumber.
            24
        } else if t.building && t.hp < self.stats_of(t.kind).hit_points {
            27
        } else {
            3
        }
    }

    /// The command filter ("Which units take part in a command"): whether unit `u` takes `command`.
    pub fn takes(&self, u: &Unit, command: u8) -> bool {
        use kind::*;
        let f = self.flags_of_kind(u.kind);
        let has = |b: u32| f & b != 0;
        let attacks = has(flags::CAN_ATTACK);
        let caster = has(flags::CASTER);
        let caster_or_mage = has(flags::CASTER) || has(flags::MAGE_LIKE);
        match command {
            23 => has(flags::PEON | flags::TANKER) && !u.carrying,
            24 => has(flags::PEON | flags::TANKER) && u.carrying,
            17 => has(flags::GROUND_ATTACK) && attacks,
            4 | 8..=16 => attacks,
            19 => attacks && has(flags::KILLED_BY_INVISIBILITY),
            5 if self.rules.enhanced
                && [DEMOLITION_SQUAD, SAPPER, FLYING_MACHINE, ZEPPELIN, EYE_OF_KILROGG].contains(&u.kind) =>
            {
                self.is_mobile(u)
            }
            5 => attacks,
            2 => true,
            3 | 6 => self.is_mobile(u),
            27 => has(flags::PEON),
            29 => has(flags::TRANSPORT),
            38..=41 => caster && [PALADIN, TURALYON, LOTHAR, UTHER, KNIGHT].contains(&u.kind),
            42..=47 => caster_or_mage && [MAGE, KHADGAR].contains(&u.kind),
            48 | 49 | 55 => caster && [OGRE_MAGE, DENTARG, CHOGALL, OGRE].contains(&u.kind),
            50..=54 | 56 => caster_or_mage && [DEATH_KNIGHT, GULDAN, TERON_GOREFIEND].contains(&u.kind),
            _ => false,
        }
    }

    /// A command from target mode, the buttons or the keys, for the units of the player's synced selection that take
    /// it.
    fn issue(&mut self, player: u8, command: u8, target: Option<UnitId>, tile: (i32, i32)) {
        if !handled(command) || !self.terrain.contains(tile.0, tile.1) {
            return;
        }
        for (i, aim) in self.aims(player, tile) {
            let u = &self.units[i];
            if u.owner != player || Some(u.id) == target || !self.takes(u, command) {
                continue;
            }
            if self.rules.enhanced {
                self.units[i].remembered = None;
            }
            self.give_command(i, command, target, aim);
        }
    }

    /// Gives one unit command `command` with its target and aimed tile (orders spec, "Commands").
    fn give_command(&mut self, i: usize, command: u8, target: Option<UnitId>, tile: (i32, i32)) {
        let u = &self.units[i];
        if u.dying || u.building || self.stride_of(u.kind) == 0 {
            return;
        }
        let (player, units) = (u.owner, vec![u.id]);
        let c = match command {
            2 => Command::Stop { player, units },
            3 => Command::Smart {
                player,
                units,
                target,
                tile,
            },
            // Orders spec, "Commands" (round-49 errata): with a target unit, the unit walks to the target's tile and
            // records the unit as its target.
            4 => {
                let goal = target.and_then(|t| self.unit(t)).map_or(tile, |t| t.tile);
                return self.command(i, Order::MoveAttack { goal, target });
            }
            5 => Command::Patrol {
                player,
                units,
                to: tile,
            },
            6 => {
                let o = match target {
                    Some(target) => Order::Follow { target },
                    None => Order::Move { goal: tile },
                };
                return self.command(i, o);
            }
            8 | 10 => match target {
                Some(target) => Command::Attack { player, units, target },
                None if self.wall_tile(tile) => return self.command(i, Order::AttackWall { tile }),
                None => Command::AttackMove {
                    player,
                    units,
                    to: tile,
                },
            },
            9 | 11 => match target {
                Some(target) => Command::Attack { player, units, target },
                None if self.wall_tile(tile) => return self.command(i, Order::AttackWall { tile }),
                None => Command::Stop { player, units },
            },
            12 | 16 => {
                let Some(target) = target else { return };
                let o = if command == 12 {
                    Order::Chase { target }
                } else {
                    Order::IdleReact { target }
                };
                return self.command(i, o);
            }
            13 | 15 => Command::StandGround { player, units },
            14 => return self.command(i, Order::TowerWatch { target: None }),
            17 => Command::AttackGround { player, units, tile },
            19 => Command::Demolish {
                player,
                units,
                target,
                tile,
            },
            23 => Command::Harvest {
                player,
                units,
                target,
                tile,
            },
            24 => match target.filter(|&t| self.can_return_to(u, t)) {
                Some(depot) => {
                    return self.command(i, Order::ReturnGoods { depot: Some(depot) });
                }
                None => Command::ReturnGoods { player, units },
            },
            27 => {
                let Some(target) = target else { return };
                Command::Repair { player, units, target }
            }
            29 => Command::Unload { player, units, tile },
            38..=56 => {
                let id = u.id;
                return self.cast_command(player, id, command, target, tile);
            }
            _ => return,
        };
        if let Some(o) = self.command_order(i, &c) {
            self.command(i, o);
        }
    }

    /// Set (Ctrl+digit): the group is cleared and filled from the synced selection in order, stopping at the first
    /// unit that isn't the player's own.
    fn set_group(&mut self, player: u8, group: u8) {
        if group as usize >= GROUPS || player as usize >= PLAYERS {
            return;
        }
        let mut g = Vec::new();
        for id in self.selection(player) {
            if self.unit(id).is_none_or(|u| u.owner != player) {
                break;
            }
            g.push(id);
        }
        self.selections[player as usize].groups[group as usize] = g;
    }

    /// Recall (digit): the group is tidied, then its units that aren't inside anything or in orders 22, 25 or 26
    /// become the player's synced selection.
    fn recall_group(&mut self, player: u8, group: u8) {
        if group as usize >= GROUPS || player as usize >= PLAYERS {
            return;
        }
        let mut g = std::mem::take(&mut self.selections[player as usize].groups[group as usize]);
        let mut n = 0;
        while n < g.len() {
            let several = g.len() > 1;
            let ok = self
                .unit(g[n])
                .is_some_and(|u| u.alive() && u.owner == player && (!several || self.stats_of(u.kind).rect_selectable));
            if ok {
                n += 1;
            } else {
                // Each removal moves the group's last entry into the gap.
                g.swap_remove(n);
            }
        }
        let picked: Vec<UnitId> = g
            .iter()
            .copied()
            .filter(|&id| self.unit(id).is_some_and(|u| !u.inside && !busy(u.order)))
            .collect();
        let s = &mut self.selections[player as usize];
        s.groups[group as usize] = g;
        s.set(picked);
    }
}

/// Orders 22, 25 and 26: inside a building being constructed, entering or inside a mine or platform.
pub fn busy(o: Order) -> bool {
    matches!(o, Order::InsideSite { .. } | Order::Enter { .. } | Order::Inside { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gather;
    use crate::terrain::Terrain;
    use crate::world::tests::{scripts, tileset};
    use tidewater_formats::udta::{UnitStats, UnitTable};

    const FOOTMAN: u8 = 0x00;
    const GRUNT: u8 = 0x01;
    const PEASANT: u8 = 0x02;
    const PEON: u8 = 0x03;
    const CATAPULT: u8 = 0x05;
    const TANKER: u8 = 0x1a;
    const GRYPHON: u8 = 0x2a;
    const FARM: u8 = 0x3a;
    const BARRACKS: u8 = 0x3c;
    const HALL: u8 = 0x4a;
    const MILL: u8 = 0x4c;
    const PLATFORM: u8 = 0x56;
    const MINE: u8 = 0x5c;

    fn table() -> UnitTable {
        let mut t = vec![UnitStats::default(); 110];
        let unit = |f: u32, rc: u8| UnitStats {
            hit_points: 60,
            size: (1, 1),
            box_size: (31, 31),
            flags: f,
            right_click: rc,
            rect_selectable: true,
            ..Default::default()
        };
        let fight = flags::LAND | flags::CAN_ATTACK;
        t[FOOTMAN as usize] = unit(fight, action::ATTACK);
        t[GRUNT as usize] = unit(fight, action::ATTACK);
        t[PEASANT as usize] = unit(flags::LAND | flags::PEON | flags::CAN_ATTACK, action::HARVEST);
        t[PEON as usize] = t[PEASANT as usize];
        t[CATAPULT as usize] = unit(fight | flags::GROUND_ATTACK, action::ATTACK);
        t[TANKER as usize] = unit(flags::SEA | flags::TANKER, action::HAUL_OIL);
        t[GRYPHON as usize] = unit(flags::AIR | flags::CAN_ATTACK, action::ATTACK);
        t[kind::SAPPER as usize] = unit(fight | flags::KILLED_BY_INVISIBILITY, action::DEMOLISH);
        t[kind::FLYING_MACHINE as usize] = unit(flags::AIR, action::MOVE);
        t[kind::MAGE as usize] = unit(fight | flags::CASTER, action::ATTACK);
        t[kind::KNIGHT as usize] = unit(fight, action::ATTACK);
        let building = |f: u32, size: u16| UnitStats {
            hit_points: 400,
            size: (size, size),
            box_size: (size * 32 - 1, size * 32 - 1),
            flags: flags::BUILDING | f,
            ..Default::default()
        };
        t[FARM as usize] = building(0, 2);
        t[BARRACKS as usize] = building(0, 3);
        t[HALL as usize] = building(flags::GOLD_STORAGE | flags::WOOD_STORAGE, 4);
        t[MILL as usize] = building(flags::WOOD_STORAGE, 3);
        t[PLATFORM as usize] = building(flags::OIL_SOURCE, 2);
        t[MINE as usize] = building(flags::MINE, 3);
        UnitTable(t)
    }

    fn map() -> World {
        let tiles = vec![0u16; 64 * 64];
        let mut w = World::new(
            Terrain::from_map(&tileset(), &tiles, None, None, 64, 64, false),
            table(),
            scripts(),
            0,
        );
        w.players[0].controller = crate::ai::controller::HUMAN;
        w.players[1].controller = crate::ai::controller::HUMAN;
        w
    }

    fn spawn(w: &mut World, kind: u8, owner: u8, tile: (i32, i32)) -> UnitId {
        w.spawn(kind, owner, tile, 0).unwrap()
    }

    fn select(w: &mut World, units: &[UnitId]) {
        w.step(&[Command::Select {
            player: 0,
            units: units.to_vec(),
        }]);
    }

    fn rc(w: &World, id: UnitId, target: Option<UnitId>, tile: (i32, i32)) -> Option<u8> {
        w.right_click_command(w.index(id).unwrap(), 0, target, tile)
    }

    fn intent(w: &World, id: UnitId) -> Option<Order> {
        let u = w.unit(id).unwrap();
        u.pending.or(Some(u.order))
    }

    #[test]
    fn handled_commands() {
        let yes: Vec<u8> = (0..60).filter(|&c| handled(c)).collect();
        let mut want: Vec<u8> = (2..=6).chain(8..=17).chain([19, 23, 24, 27, 29]).collect();
        want.extend(38..=56);
        assert_eq!(yes, want);
    }

    #[test]
    fn formation_keeps_a_small_group_in_shape() {
        let tiles = [(10, 10), (11, 10), (12, 11)];
        assert_eq!(
            formation(&tiles, (30, 20), false, (64, 64)),
            vec![(29, 20), (30, 20), (31, 21)]
        );
    }

    #[test]
    fn formation_converges_a_wide_group() {
        assert_eq!(
            formation(&[(10, 5), (20, 5)], (40, 30), false, (64, 64)),
            vec![(40, 30), (40, 30)]
        );
    }

    #[test]
    fn formation_inside_the_box_or_at_a_wall_aims_at_the_tile() {
        let tiles = [(10, 10), (12, 12)];
        assert_eq!(formation(&tiles, (11, 11), false, (64, 64)), vec![(11, 11); 2]);
        assert_eq!(formation(&tiles, (30, 30), true, (64, 64)), vec![(30, 30); 2]);
    }

    #[test]
    fn formation_clamps_to_the_map() {
        // Mean (1, 0): offsets (61, 62); the first unit's aim is clamped.
        assert_eq!(
            formation(&[(0, 0), (2, 0)], (62, 62), false, (64, 64)),
            vec![(61, 62), (63, 62)]
        );
    }

    #[test]
    fn right_click_enemy_grunt_attacks_with_every_footman() {
        let mut w = map();
        let men: Vec<UnitId> = (0..3).map(|n| spawn(&mut w, FOOTMAN, 0, (10 + n, 10))).collect();
        let grunt = spawn(&mut w, GRUNT, 1, (20, 10));
        select(&mut w, &men);
        for &m in &men {
            assert_eq!(rc(&w, m, Some(grunt), (20, 10)), Some(8));
        }
        w.step(&[Command::RightClick {
            player: 0,
            target: Some(grunt),
            tile: (20, 10),
        }]);
        for &m in &men {
            assert_eq!(intent(&w, m), Some(Order::Attack { target: grunt }));
        }
    }

    #[test]
    fn harvest_table() {
        let mut w = map();
        let peasant = spawn(&mut w, PEASANT, 0, (5, 5));
        let barracks = spawn(&mut w, BARRACKS, 0, (10, 10));
        let mine = spawn(&mut w, MINE, slots::NEUTRAL, (20, 20));
        let hall = spawn(&mut w, HALL, 0, (30, 5));
        let mill = spawn(&mut w, MILL, 0, (30, 20));
        let grunt = spawn(&mut w, GRUNT, 1, (6, 5));
        // Damaged own barracks, empty-handed: repair.
        let b = w.index(barracks).unwrap();
        w.units[b].hp = 100;
        assert_eq!(rc(&w, peasant, Some(barracks), (10, 10)), Some(27));
        // A gold mine takes the "allied unit of another player" row: smart move.
        assert_eq!(rc(&w, peasant, Some(mine), (20, 20)), Some(3));
        // An enemy: attack.
        assert_eq!(rc(&w, peasant, Some(grunt), (6, 5)), Some(8));
        let p = w.index(peasant).unwrap();
        // Carrying gold to a hall at full hit points (it stores lumber too): repair check, so 3.
        w.units[p].carrying = true;
        w.units[p].gather = Gather::Gold;
        assert_eq!(rc(&w, peasant, Some(hall), (30, 5)), Some(3));
        // Carrying gold to a lumber mill at full hit points: 3.
        assert_eq!(rc(&w, peasant, Some(mill), (30, 20)), Some(3));
        // Carrying lumber to the hall: 24.
        w.units[p].gather = Gather::Lumber;
        assert_eq!(rc(&w, peasant, Some(hall), (30, 5)), Some(24));
        // A map gives a building flag bit 12 but not 18: a lumber carrier still returns there.
        w.stats.0[FARM as usize].flags |= flags::GOLD_STORAGE;
        let farm = spawn(&mut w, FARM, 0, (40, 40));
        assert_eq!(rc(&w, peasant, Some(farm), (40, 40)), Some(24));
    }

    #[test]
    fn harvest_on_a_tree_tile_has_no_target() {
        let mut w = map();
        let peon = spawn(&mut w, PEON, 0, (5, 5));
        let n = w.terrain.at(8, 5).unwrap();
        w.terrain.regions[n] = terrain::region::FOREST;
        assert_eq!(rc(&w, peon, None, (8, 5)), Some(23));
        assert_eq!(rc(&w, peon, None, (9, 5)), Some(3));
        let p = w.index(peon).unwrap();
        w.units[p].carrying = true;
        assert_eq!(rc(&w, peon, None, (8, 5)), Some(3));
    }

    #[test]
    fn tanker_and_sapper_rows() {
        let mut w = map();
        let tanker = spawn(&mut w, TANKER, 0, (4, 4));
        let platform = spawn(&mut w, PLATFORM, 0, (10, 10));
        assert_eq!(rc(&w, tanker, Some(platform), (10, 10)), Some(23));
        let sapper = spawn(&mut w, kind::SAPPER, 0, (20, 20));
        let farm = spawn(&mut w, FARM, 1, (24, 20));
        assert_eq!(rc(&w, sapper, Some(farm), (24, 20)), Some(19));
        assert_eq!(rc(&w, sapper, None, (30, 20)), Some(3));
    }

    #[test]
    fn right_click_on_a_wall_tile_attacks_it() {
        let mut w = map();
        let f = spawn(&mut w, FOOTMAN, 0, (4, 4));
        w.terrain.set_bits(8, 4, terrain::bits::HUMAN_WALL);
        assert_eq!(rc(&w, f, None, (8, 4)), Some(11));
        assert_eq!(rc(&w, f, None, (9, 4)), Some(3));
    }

    #[test]
    fn right_click_skips_enemies_and_the_target_itself() {
        let mut w = map();
        let mine = spawn(&mut w, FOOTMAN, 0, (4, 4));
        let other = spawn(&mut w, FOOTMAN, 0, (5, 4));
        let enemy = spawn(&mut w, GRUNT, 1, (8, 8));
        select(&mut w, &[enemy, mine, other]);
        let before = intent(&w, enemy);
        w.step(&[Command::RightClick {
            player: 0,
            target: Some(mine),
            tile: (4, 4),
        }]);
        assert_eq!(intent(&w, enemy), before, "not the player's own");
        assert_eq!(intent(&w, mine), Some(Order::Idle), "the target itself");
        assert_eq!(intent(&w, other), Some(Order::Follow { target: mine }));
    }

    #[test]
    fn right_click_moves_with_the_formation_offset() {
        let mut w = map();
        let men: Vec<UnitId> = [(10, 10), (11, 10), (12, 11)]
            .into_iter()
            .map(|t| spawn(&mut w, FOOTMAN, 0, t))
            .collect();
        select(&mut w, &men);
        w.step(&[Command::RightClick {
            player: 0,
            target: None,
            tile: (30, 20),
        }]);
        let goals: Vec<_> = men.iter().map(|&m| intent(&w, m)).collect();
        assert_eq!(
            goals,
            [(29, 20), (30, 20), (31, 21)].map(|goal| Some(Order::Move { goal }))
        );
    }

    fn filter(w: &World, id: UnitId, c: u8) -> bool {
        w.takes(w.unit(id).unwrap(), c)
    }

    #[test]
    fn command_filter() {
        let mut w = map();
        let footman = spawn(&mut w, FOOTMAN, 0, (1, 1));
        let catapult = spawn(&mut w, CATAPULT, 0, (2, 1));
        let flyer = spawn(&mut w, kind::FLYING_MACHINE, 0, (4, 4));
        let peasant = spawn(&mut w, PEASANT, 0, (6, 1));
        let sapper = spawn(&mut w, kind::SAPPER, 0, (7, 1));
        let mage = spawn(&mut w, kind::MAGE, 0, (8, 1));
        let knight = spawn(&mut w, kind::KNIGHT, 0, (9, 1));
        assert!(!filter(&w, footman, 17) && filter(&w, catapult, 17));
        assert!(filter(&w, sapper, 19) && !filter(&w, footman, 19));
        assert!(filter(&w, flyer, 5) && filter(&w, footman, 5), "Enhanced patrol");
        assert!(filter(&w, peasant, 23) && !filter(&w, footman, 23));
        assert!(filter(&w, peasant, 27) && !filter(&w, footman, 27));
        assert!(!filter(&w, peasant, 24));
        assert!(filter(&w, mage, 43) && !filter(&w, footman, 43) && !filter(&w, mage, 51));
        assert!(!filter(&w, knight, 39), "the knight lacks flag bit 17 in the data");
        assert!(filter(&w, flyer, 3) && filter(&w, footman, 2));
        w.rules.enhanced = false;
        assert!(!filter(&w, flyer, 5) && filter(&w, footman, 5), "Classic patrol");
    }

    #[test]
    fn target_mode_attack_ground_reaches_only_the_catapult() {
        let mut w = map();
        let footman = spawn(&mut w, FOOTMAN, 0, (1, 1));
        let catapult = spawn(&mut w, CATAPULT, 0, (2, 1));
        select(&mut w, &[footman, catapult]);
        w.step(&[Command::Issue {
            player: 0,
            command: 17,
            target: None,
            tile: (3, 1),
        }]);
        assert_eq!(intent(&w, footman), Some(Order::Idle));
        assert!(matches!(intent(&w, catapult), Some(Order::AttackGround { .. })));
    }

    #[test]
    fn target_mode_patrol_by_rule_set() {
        for enhanced in [true, false] {
            let mut w = map();
            w.rules.enhanced = enhanced;
            let footman = spawn(&mut w, FOOTMAN, 0, (1, 1));
            let flyer = spawn(&mut w, kind::FLYING_MACHINE, 0, (4, 4));
            select(&mut w, &[footman, flyer]);
            w.step(&[Command::Issue {
                player: 0,
                command: 5,
                target: None,
                tile: (40, 40),
            }]);
            assert!(matches!(intent(&w, footman), Some(Order::Patrol { .. })));
            assert_eq!(
                matches!(intent(&w, flyer), Some(Order::Patrol { .. })),
                enhanced,
                "enhanced {enhanced}"
            );
        }
    }

    #[test]
    fn unhandled_or_off_map_commands_do_nothing() {
        let mut w = map();
        let footman = spawn(&mut w, FOOTMAN, 0, (1, 1));
        select(&mut w, &[footman]);
        w.step(&[
            Command::Issue {
                player: 0,
                command: 7,
                target: None,
                tile: (5, 5),
            },
            Command::Issue {
                player: 0,
                command: 3,
                target: None,
                tile: (64, 5),
            },
        ]);
        assert_eq!(intent(&w, footman), Some(Order::Idle));
    }

    #[test]
    fn groups_set_and_recall() {
        let mut w = map();
        let knights: Vec<UnitId> = (0..4).map(|n| spawn(&mut w, FOOTMAN, 0, (n, 1))).collect();
        let other = spawn(&mut w, FOOTMAN, 0, (10, 10));
        select(&mut w, &knights);
        w.step(&[Command::SetGroup { player: 0, group: 1 }]);
        select(&mut w, &[other]);
        w.step(&[Command::RecallGroup { player: 0, group: 1 }]);
        assert_eq!(w.selection(0), knights);
    }

    #[test]
    fn setting_a_group_stops_at_the_first_foreign_unit() {
        let mut w = map();
        let a = spawn(&mut w, FOOTMAN, 0, (1, 1));
        let foe = spawn(&mut w, GRUNT, 1, (2, 1));
        let b = spawn(&mut w, FOOTMAN, 0, (3, 1));
        select(&mut w, &[a, foe, b]);
        w.step(&[Command::SetGroup { player: 0, group: 2 }]);
        assert_eq!(w.selections[0].groups[2], vec![a]);
    }

    #[test]
    fn recall_tidies_the_group() {
        let mut w = map();
        let units: Vec<UnitId> = (0..4).map(|n| spawn(&mut w, FOOTMAN, 0, (n, 1))).collect();
        w.selections[0].groups[3] = units.clone();
        // The second unit now belongs to someone else: the last entry moves into its place.
        let i = w.index(units[1]).unwrap();
        w.change_owner(i, 1);
        // The third is inside something: kept in the group, but not selected.
        let i = w.index(units[2]).unwrap();
        w.units[i].inside = true;
        w.step(&[Command::RecallGroup { player: 0, group: 3 }]);
        assert_eq!(w.selections[0].groups[3], vec![units[0], units[3], units[2]]);
        assert_eq!(w.selection(0), vec![units[0], units[3]]);
    }

    #[test]
    fn lost_units_leave_the_synced_selection() {
        for enhanced in [true, false] {
            let mut w = map();
            w.rules.enhanced = enhanced;
            let units: Vec<UnitId> = (0..3).map(|n| spawn(&mut w, FOOTMAN, 0, (n, 1))).collect();
            select(&mut w, &units);
            let i = w.index(units[0]).unwrap();
            w.units[i].inside = true;
            w.step(&[]);
            let places = w.selections[0].places;
            if enhanced {
                assert_eq!(&places[..3], &[Some(units[1]), Some(units[2]), None]);
            } else {
                assert_eq!(&places[..3], &[None, Some(units[1]), Some(units[2])]);
            }
        }
    }

    #[test]
    fn speed_steps_stop_at_the_limits() {
        let mut w = map();
        w.speed = TOP_SPEED_ENHANCED;
        w.step(&[Command::Speed {
            player: 0,
            faster: true,
        }]);
        assert_eq!(w.speed, TOP_SPEED_ENHANCED, "nothing changes at the top level");
        w.rules.enhanced = false;
        w.speed = 5;
        let up = Command::Speed {
            player: 0,
            faster: true,
        };
        w.step(&[up.clone(), up.clone(), up]);
        assert_eq!(w.speed, TOP_SPEED_CLASSIC);
        w.speed = 0;
        w.step(&[Command::Speed {
            player: 0,
            faster: false,
        }]);
        assert_eq!(w.speed, 0);
    }

    #[test]
    fn a_paused_cycle_changes_nothing_but_the_counter() {
        let mut w = map();
        let f = spawn(&mut w, FOOTMAN, 0, (1, 1));
        select(&mut w, &[f]);
        w.step(&[Command::Pause { player: 0 }]);
        let before = w.units.clone();
        let tick = w.tick;
        w.step(&[]);
        assert_eq!(w.tick, tick + 1);
        assert_eq!(w.units, before);
        w.step(&[Command::Pause { player: 0 }]);
        assert!(!w.paused);
    }
}
