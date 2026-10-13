//! The pure rules of automatic target acquisition and fleeing (orders spec, "Automatic target acquisition" and
//! "Fleeing"): the vertical-order sort, the reference tile of a candidate, its score and the flee offset.

/// Score bases ("Score").
pub const BASE_CANT_ATTACK: i32 = 0;
pub const BASE_OUT_OF_RANGE: i32 = 65536;
pub const BASE_IN_RANGE: i32 = 131072;
pub const BASE_IN_RANGE_AIR: i32 = 196608;

/// Pixel y that dying, dead and inside units sort at ("Vertical order").
pub const OUT_OF_LIST_Y: i32 = -32768;

/// One pass of the game's vertical sort: a stable insertion sort whose backward walk never compares the first two
/// entries, so only the pass's first step can change which entry is first ("Vertical order").
pub fn sort_pass<T>(list: &mut [T], y: impl Fn(&T) -> i32) {
    for i in 1..list.len() {
        if y(&list[i]) >= y(&list[i - 1]) {
            continue;
        }
        list.swap(i, i - 1);
        let mut j = i - 1;
        // The walk back stops before comparing entries 0 and 1, unless this is the pass's first step.
        while j >= 2 && y(&list[j]) < y(&list[j - 1]) {
            list.swap(j, j - 1);
            j -= 1;
        }
    }
}

/// One axis of a candidate building's reference tile, for a searcher at `from` ("Distance").
pub fn reference(from: i32, lo: i32, size: i32) -> i32 {
    if from <= lo {
        lo
    } else if from >= lo + 2 * (size / 2) {
        lo + size - 1
    } else {
        lo + size / 2
    }
}

/// What a candidate is to the searcher, for its base score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Threat {
    pub can_attack: bool,
    pub in_range: bool,
    /// An air unit or a submarine.
    pub air_or_sub: bool,
}

/// score = base + priority − floor(d² / 4), at least 1 ("Score").
pub fn score(t: Threat, priority: u8, d: i32) -> i32 {
    let base = match t {
        Threat { can_attack: false, .. } => BASE_CANT_ATTACK,
        Threat { in_range: false, .. } => BASE_OUT_OF_RANGE,
        Threat { air_or_sub: true, .. } => BASE_IN_RANGE_AIR,
        _ => BASE_IN_RANGE,
    };
    (base + priority as i32 - d * d / 4).max(1)
}

/// The unit step of a facing, 0 north, clockwise.
pub fn facing_step(f: u8) -> (i32, i32) {
    [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)][f as usize & 7]
}

/// The flee offset on one axis: `(r mod 8) − 2 + 4·k`, or `8·k` for two-tile movers ("Fleeing", step 4).
pub fn flee_offset(r: u32, k: i32, two_tile: bool) -> i32 {
    (r % 8) as i32 - 2 + if two_tile { 8 } else { 4 } * k
}

/// "The halving search" (round-40 errata): the first and last entries (positions in `rows`, the tile rows of the list
/// after its skipped front entries) that the target search visits for the rows `top` to `bottom`, or `None` when it
/// visits nothing. The rows are only nearly sorted, so this can differ from an exact partition.
pub fn row_range(rows: &[i32], top: i32, bottom: i32) -> Option<(usize, usize)> {
    let (&first_row, &last_row) = (rows.first()?, rows.last()?);
    if top > last_row || bottom < first_row {
        return None;
    }
    let n = rows.len();
    let (mut lo, mut hi) = (0, n - 1);
    let first = loop {
        let mid = (lo + hi) / 2;
        let above = rows[mid] < top;
        if lo == hi || (above && rows.get(mid + 1).is_some_and(|&r| r >= top)) {
            break if above { mid + 1 } else { mid };
        }
        if above {
            lo = (lo + hi).div_ceil(2);
        } else {
            hi = (lo + hi - 1) / 2;
        }
    };
    // Round-41 errata: nothing is visited when the first entry found is below the bottom row, and otherwise the
    // last-entry search starts with low at the first entry found.
    if rows.get(first).is_none_or(|&r| r > bottom) {
        return None;
    }
    let (mut lo, mut hi) = (first, n - 1);
    let last = loop {
        let mid = (lo + hi) / 2;
        let below = rows[mid] > bottom;
        if lo == hi || (below && mid > 0 && rows[mid - 1] <= bottom) {
            break if below { mid.checked_sub(1)? } else { mid };
        }
        if below {
            hi = (lo + hi - 1) / 2;
        } else {
            lo = (lo + hi).div_ceil(2);
        }
    };
    if first > last {
        return None;
    }
    Some((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_halving_search_vector() {
        // Orders spec, round-40 vector: rows 1, 2, 5, 5, 6, top row 3, bottom row 5.
        assert_eq!(row_range(&[1, 2, 5, 5, 6], 3, 5), Some((2, 3)));
        assert_eq!(row_range(&[1, 2, 5, 5, 6], 7, 9), None);
        assert_eq!(row_range(&[4, 5], 0, 3), None);
        assert_eq!(row_range(&[], 0, 3), None);
        // Nearly sorted: a row-3 entry among row-4 entries is still visited.
        assert_eq!(row_range(&[1, 4, 3, 4, 4, 9], 2, 4), Some((1, 4)));
        // Round-41 errata: the last-entry search starts at the first entry found (2), so its first middle is 3 (row
        // 2, below 1) and it ends at 2. Starting at position 0 would have ended at 5.
        assert_eq!(row_range(&[0, 0, 1, 2, 0, 1], 1, 1), Some((2, 2)));
    }

    #[test]
    fn one_pass_moves_an_entry_up_to_second_place() {
        let mut l = vec![100, 200, 50];
        sort_pass(&mut l, |&y| y);
        assert_eq!(l, [100, 50, 200]);
        sort_pass(&mut l, |&y| y);
        assert_eq!(l, [50, 100, 200]);
        // The pass's first step can still change the first entry.
        let mut l = vec![200, 100, 300];
        sort_pass(&mut l, |&y| y);
        assert_eq!(l, [100, 200, 300]);
        // Stable for equal keys.
        let mut l = vec![(5, 'a'), (5, 'b'), (1, 'c'), (5, 'd')];
        sort_pass(&mut l, |p| p.0);
        assert_eq!(l, [(5, 'a'), (1, 'c'), (5, 'b'), (5, 'd')]);
    }

    #[test]
    fn building_reference_columns() {
        // Width 4 at column 10.
        let got: Vec<i32> = [8, 11, 13, 14].iter().map(|&c| reference(c, 10, 4)).collect();
        assert_eq!(got, [10, 12, 12, 13]);
        assert_eq!(reference(12, 10, 4), 12);
        // Width 3 at column 10: middle 11, last column from 12 on.
        assert_eq!([9, 11, 12].map(|c| reference(c, 10, 3)), [10, 11, 12]);
    }

    #[test]
    fn scores() {
        let attacker = |in_range, air_or_sub| Threat {
            can_attack: true,
            in_range,
            air_or_sub,
        };
        let p = 20;
        assert_eq!(score(attacker(true, false), p, 1), 131072 + 20);
        assert_eq!(score(attacker(false, false), p, 6), 65536 + 20 - 9);
        let farm = Threat {
            can_attack: false,
            in_range: true,
            air_or_sub: false,
        };
        assert_eq!(score(farm, 5, 6), 1);
        assert_eq!(score(farm, 30, 2), 29);
        assert!(score(attacker(true, true), 0, 4) > score(attacker(true, false), 255, 2));
    }

    #[test]
    fn flee_from_an_archer_facing_east() {
        let (kx, ky) = facing_step(2);
        assert_eq!((flee_offset(5, kx, false), flee_offset(2, ky, false)), (7, 0));
        assert_eq!(flee_offset(5, kx, true), 11);
    }
}
