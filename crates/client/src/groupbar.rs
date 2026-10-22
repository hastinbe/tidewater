//! The control-group bar (option `group-bar`, `extras`, `docs/design/extras.md`): a strip of small buttons, one for each
//! control group that has units, showing the group's number and how many units are in it. A click on a button recalls
//! the group, sending what the number key sends (`mouse::GroupKeys::press`, a double click centres the view). Our own
//! design, not the original's.
//!
//! **Where it is.** Over the map view's top left corner, starting 4 pixels in from the corner and one row high
//! ([`CELL_H`] pixels). That corner holds none of the original interface: the messages sit at the view's left edge
//! lower down (from 194 pixels down the screen), the notice line at its bottom, and the option's own clock (when on) at
//! the top right. Ten cells at most, so the strip is at most 400 pixels long and never reaches the clock.
//!
//! **What it reads.** The synced control groups (`World::selections`), the same lists the number keys recall. A unit
//! that is no longer in the game is not counted, and a group with none left shows no button.

use crate::hud::Rect;
use tidewater_sim::UnitId;

/// The groups the number keys reach: 0 to 9.
pub const GROUPS: usize = 10;
/// Cell size in pixels and the gap between cells.
pub const CELL_W: i32 = 36;
pub const CELL_H: i32 = 14;
pub const GAP: i32 = 4;
/// Distance of the strip from the view's top left corner.
pub const MARGIN: i32 = 4;

/// One button: the group and how many units it holds.
pub type Cell = (u8, usize);

/// The buttons to show: every group of 0 to 9 with units left, in group order. `alive` says whether a unit is still in
/// the game.
pub fn cells(groups: &[Vec<UnitId>], alive: impl Fn(UnitId) -> bool) -> Vec<Cell> {
    groups
        .iter()
        .take(GROUPS)
        .enumerate()
        .filter_map(|(g, units)| {
            let n = units.iter().filter(|&&u| alive(u)).count();
            (n > 0).then_some((g as u8, n))
        })
        .collect()
}

/// The text of a button: the group's number and its size, `1:5`.
pub fn label(cell: Cell) -> String {
    format!("{}:{}", cell.0, cell.1)
}

/// Where the `n`th button sits, for a strip whose area is the map view `view`.
pub fn cell_rect(view: Rect, n: usize) -> Rect {
    Rect::new(
        view.x + MARGIN + (CELL_W + GAP) * n as i32,
        view.y + MARGIN,
        CELL_W as u32,
        CELL_H as u32,
    )
}

/// The group whose button is under the point `(x, y)` (screen pixels), if any.
pub fn group_at(view: Rect, cells: &[Cell], x: f32, y: f32) -> Option<u8> {
    cells
        .iter()
        .enumerate()
        .find(|&(n, _)| cell_rect(view, n).contains(x, y))
        .map(|(_, &(g, _))| g)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Rect = Rect::new(176, 16, 448, 448);

    fn ids(n: impl IntoIterator<Item = u32>) -> Vec<UnitId> {
        n.into_iter().map(UnitId).collect()
    }

    #[test]
    fn only_groups_with_units_get_a_button_in_group_order() {
        let mut groups = vec![Vec::new(); 18];
        groups[3] = ids([1, 2, 3]);
        groups[1] = ids([9]);
        groups[12] = ids([4]);
        assert_eq!(
            cells(&groups, |_| true),
            vec![(1, 1), (3, 3)],
            "groups past 9 are not on the keys"
        );
        assert!(cells(&vec![Vec::new(); 18], |_| true).is_empty());
    }

    #[test]
    fn units_that_left_the_game_are_not_counted() {
        let mut groups = vec![Vec::new(); 18];
        groups[0] = ids([1, 2, 3]);
        groups[2] = ids([7]);
        assert_eq!(cells(&groups, |u| u.0 != 2 && u.0 != 7), vec![(0, 2)]);
    }

    #[test]
    fn the_label_is_the_number_and_the_count() {
        assert_eq!(label((4, 12)), "4:12");
    }

    #[test]
    fn the_strip_sits_in_the_views_top_left_and_never_leaves_the_view() {
        let first = cell_rect(VIEW, 0);
        assert_eq!((first.x, first.y), (180, 20));
        let last = cell_rect(VIEW, GROUPS - 1);
        assert!(last.x + last.w as i32 <= VIEW.x + VIEW.w as i32);
        // It stays above the first message line (194 pixels down the screen).
        assert!(first.y + (first.h as i32) < 194);
    }

    #[test]
    fn a_point_finds_its_button_and_the_gaps_find_none() {
        let cells = [(2u8, 5usize), (7, 1)];
        let second = cell_rect(VIEW, 1);
        assert_eq!(
            group_at(VIEW, &cells, second.x as f32 + 3.0, second.y as f32 + 3.0),
            Some(7)
        );
        let first = cell_rect(VIEW, 0);
        assert_eq!(group_at(VIEW, &cells, first.x as f32, first.y as f32), Some(2));
        let gap = first.x as f32 + CELL_W as f32 + 1.0;
        assert_eq!(group_at(VIEW, &cells, gap, first.y as f32 + 2.0), None);
        // A third slot has no button.
        let third = cell_rect(VIEW, 2);
        assert_eq!(group_at(VIEW, &cells, third.x as f32 + 1.0, third.y as f32 + 1.0), None);
        assert_eq!(group_at(VIEW, &[], first.x as f32, first.y as f32), None);
    }
}
