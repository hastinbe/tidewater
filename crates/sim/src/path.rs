//! Tile-grid path search. A placeholder of our own design (8-way A* with octile costs) until the movement spec
//! describes the original behaviour.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Cost of a straight step; a diagonal costs 14 (about sqrt 2 times as much).
const STRAIGHT: u32 = 10;
const DIAGONAL: u32 = 14;
/// Bound on expanded nodes, so a hopeless search on a large map stays cheap.
const MAX_EXPANDED: usize = 16_384;

fn heuristic(a: (i32, i32), b: (i32, i32)) -> u32 {
    let (dx, dy) = ((a.0 - b.0).unsigned_abs(), (a.1 - b.1).unsigned_abs());
    STRAIGHT * dx.max(dy) + (DIAGONAL - STRAIGHT) * dx.min(dy)
}

/// Finds a path of tiles from `start` (excluded) to `goal` (included) on a `w` x `h` grid. If `goal` is unreachable,
/// the path ends at the reachable tile closest to it. Ties break by insertion order, so results are deterministic.
pub fn find(
    w: i32,
    h: i32,
    start: (i32, i32),
    goal: (i32, i32),
    passable: impl Fn(i32, i32) -> bool,
) -> Vec<(i32, i32)> {
    let idx = |p: (i32, i32)| (p.1 * w + p.0) as usize;
    let n = (w * h) as usize;
    let mut cost = vec![u32::MAX; n];
    let mut from = vec![u32::MAX; n];
    let mut open = BinaryHeap::new();
    let mut seq = 0u32;
    cost[idx(start)] = 0;
    open.push(Reverse((heuristic(start, goal), seq, start)));
    let mut best = (heuristic(start, goal), start);
    let mut expanded = 0;
    while let Some(Reverse((_, _, p))) = open.pop() {
        if p == goal {
            best = (0, p);
            break;
        }
        expanded += 1;
        if expanded > MAX_EXPANDED {
            break;
        }
        let c = cost[idx(p)];
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)] {
            let q = (p.0 + dx, p.1 + dy);
            if q.0 < 0 || q.1 < 0 || q.0 >= w || q.1 >= h || !passable(q.0, q.1) {
                continue;
            }
            // No corner cutting: a diagonal needs both orthogonal neighbours free.
            if dx != 0 && dy != 0 && !(passable(p.0 + dx, p.1) && passable(p.0, p.1 + dy)) {
                continue;
            }
            let nc = c + if dx != 0 && dy != 0 { DIAGONAL } else { STRAIGHT };
            if nc < cost[idx(q)] {
                cost[idx(q)] = nc;
                from[idx(q)] = idx(p) as u32;
                let hq = heuristic(q, goal);
                if hq < best.0 {
                    best = (hq, q);
                }
                seq += 1;
                open.push(Reverse((nc + hq, seq, q)));
            }
        }
    }
    let mut path = Vec::new();
    let mut p = best.1;
    while p != start {
        path.push(p);
        let f = from[idx(p)];
        if f == u32::MAX {
            break;
        }
        p = ((f as i32) % w, (f as i32) / w);
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goes_around_a_wall() {
        // A wall at x = 2 from y = 0 to 3; the gap is at y = 4.
        let wall = |x: i32, y: i32| !(x == 2 && y < 4);
        let p = find(5, 5, (0, 0), (4, 0), wall);
        assert_eq!(p.last(), Some(&(4, 0)));
        assert!(p.iter().all(|&(x, y)| wall(x, y)));
        assert!(p.contains(&(2, 4)));
    }

    #[test]
    fn unreachable_goal_gets_closest_tile() {
        let p = find(5, 1, (0, 0), (4, 0), |x, _| x != 3);
        assert_eq!(p, vec![(1, 0), (2, 0)]);
    }
}
