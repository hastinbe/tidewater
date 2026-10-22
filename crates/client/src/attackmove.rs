//! Smart attack-move (option `smart-attack-move`, `extras`, `docs/design/extras.md`): a right-click on open ground with
//! only fighting units selected sends them to attack-move there instead of moving. Our own design, not the original's.
//!
//! **What it sends.** The normal [`Command::Issue`] with command 8 and no target, exactly what the Attack button sends
//! for a click on the ground (`panel::target_command`); the sim turns it into its existing attack-move. The sim is not
//! changed, so the option is client-only. It is **C+L**: in a game with other players the session sets it for everyone,
//! because it saves an order (an edge).
//!
//! **When it applies.** All of these must hold, or the click is the normal right-click:
//! - the click is on open ground (no unit under the pointer is its target);
//! - every selected unit is the local player's, can attack, and is not a worker, an oil tanker or a building: a
//!   selection with a peasant or a transport in it still moves, so the workers do not run into a fight.

use tidewater_formats::udta::flags;
use tidewater_sim::{Command, UnitId, World};

/// What the rule needs to know about one selected unit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub own: bool,
    pub can_attack: bool,
    /// A peasant, a peon or an oil tanker.
    pub worker: bool,
    pub building: bool,
}

/// Whether a right-click on `target` (none: open ground) with these units selected becomes an attack-move.
pub fn applies(target: Option<UnitId>, selection: &[Facts]) -> bool {
    target.is_none()
        && !selection.is_empty()
        && selection
            .iter()
            .all(|f| f.own && f.can_attack && !f.worker && !f.building)
}

/// The facts of the selected units. A unit that is gone counts as not fighting, so the click stays a plain move.
pub fn facts(world: &World, player: u8, selection: &[UnitId]) -> Vec<Facts> {
    selection
        .iter()
        .map(|&id| {
            world.unit(id).map_or(Facts::default(), |u| {
                let f = world.stats.0.get(u.kind as usize).map_or(0, |s| s.flags);
                Facts {
                    own: u.owner == player,
                    can_attack: f & flags::CAN_ATTACK != 0,
                    worker: f & (flags::PEON | flags::TANKER) != 0,
                    building: u.building,
                }
            })
        })
        .collect()
}

/// The command the click sends: the Attack button's command for a click on the ground.
pub fn command(player: u8, tile: (i32, i32)) -> Command {
    crate::panel::target_command(crate::panel::Target::Attack, player, None, tile)
        .expect("attack on the ground is always a command")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIGHTER: Facts = Facts {
        own: true,
        can_attack: true,
        worker: false,
        building: false,
    };

    #[test]
    fn only_fighting_units_on_open_ground_attack_move() {
        assert!(applies(None, &[FIGHTER; 3]));
        assert!(!applies(None, &[]), "nothing selected");
        assert!(
            !applies(Some(UnitId(7)), &[FIGHTER]),
            "a unit under the pointer is a target"
        );
    }

    #[test]
    fn a_worker_a_non_fighter_or_a_foreign_unit_keeps_the_normal_move() {
        let worker = Facts {
            worker: true,
            ..FIGHTER
        };
        let transport = Facts {
            can_attack: false,
            ..FIGHTER
        };
        let foreign = Facts { own: false, ..FIGHTER };
        let building = Facts {
            building: true,
            ..FIGHTER
        };
        for odd in [worker, transport, foreign, building] {
            assert!(!applies(None, &[FIGHTER, odd]), "{odd:?} among fighters");
            assert!(!applies(None, &[odd]), "{odd:?} alone");
        }
    }

    #[test]
    fn it_sends_what_the_attack_button_sends_on_the_ground() {
        assert_eq!(
            command(2, (10, 12)),
            Command::Issue {
                player: 2,
                command: 8,
                target: None,
                tile: (10, 12)
            }
        );
    }
}
