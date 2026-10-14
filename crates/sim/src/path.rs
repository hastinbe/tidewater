//! Path search: a straight line toward the goal, a wall-following walk around the first obstacle (left, then right),
//! and a table-driven shortening of the route. Spec: `docs/specs/movement/pathfinding.md`.

/// Most steps a search returns, and most straight-line steps it takes.
pub const MAX_STEPS: usize = 20;
/// Most moves of one wall-following walk.
const MAX_WALK: usize = 50;
/// A left walk scoring below this is taken without trying the right one.
const GOOD_SCORE: i32 = 8;

/// Direction vectors: 0 north, clockwise in eighths, y growing southward.
pub const DIRECTIONS: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

/// The direction of a single-tile offset, if it is one.
pub fn direction_of(dx: i32, dy: i32) -> Option<u8> {
    DIRECTIONS
        .iter()
        .position(|&v| v == (dx.signum(), dy.signum()))
        .map(|d| d as u8)
}

/// One search's inputs. Coordinates are tiles.
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub width: i32,
    pub height: i32,
    pub start: (i32, i32),
    pub goal: (i32, i32),
    /// The goal before rounding to even coordinates (equal to `goal` for one-tile movers): P is measured to it.
    pub raw_goal: (i32, i32),
    /// Tiles per step: 1, or 2 for ships and flyers.
    pub stride: i32,
    /// The goal zone as an inclusive rectangle (x0, y0, x1, y1): 3 x 3 around a tile goal, or a target's footprint
    /// widened by one tile.
    pub zone: (i32, i32, i32, i32),
    pub enhanced: bool,
}

impl Request {
    /// A request for a tile goal, its goal zone the 3 x 3 tiles around it.
    pub fn to_tile(width: i32, height: i32, start: (i32, i32), goal: (i32, i32), stride: i32) -> Self {
        let raw_goal = goal;
        let goal = if stride == 2 { (goal.0 & !1, goal.1 & !1) } else { goal };
        Self {
            width,
            height,
            start,
            goal,
            raw_goal,
            stride,
            zone: (goal.0 - 1, goal.1 - 1, goal.0 + 1, goal.1 + 1),
            enhanced: true,
        }
    }

    fn in_zone(&self, p: (i32, i32)) -> bool {
        let (mut x0, mut y0, mut x1, mut y1) = self.zone;
        if self.stride == 2 {
            // Pathfinding spec, "Step 1": the first row and column round down to even, the last up to even; a
            // two-tile mover stands only on even tiles, so the zone is the even tiles in that range.
            let up = |v: i32| (v + 1) & !1;
            (x0, y0, x1, y1) = (x0 & !1, y0 & !1, up(x1), up(y1));
        }
        (x0..=x1).contains(&p.0) && (y0..=y1).contains(&p.1)
    }

    fn contains(&self, p: (i32, i32)) -> bool {
        p.0 >= 0 && p.1 >= 0 && p.0 < self.width && p.1 < self.height
    }

    fn step(&self, p: (i32, i32), d: u8) -> (i32, i32) {
        let v = DIRECTIONS[(d & 7) as usize];
        (p.0 + v.0 * self.stride, p.1 + v.1 * self.stride)
    }
}

/// Tile tests the search needs. `blocked` applies the search's unit-ignoring rules; `blocked_strict` (for the
/// shortening's new tiles) counts every unit.
pub trait Map {
    fn blocked(&self, x: i32, y: i32) -> bool;
    fn blocked_strict(&self, x: i32, y: i32) -> bool;
}

impl<F: Fn(i32, i32) -> bool> Map for F {
    fn blocked(&self, x: i32, y: i32) -> bool {
        self(x, y)
    }
    fn blocked_strict(&self, x: i32, y: i32) -> bool {
        self(x, y)
    }
}

/// The straight line from `start` toward `goal` ("Line rule"): the direction of each step, until the goal is reached.
/// The deltas, M, m and the error term are in tiles even for a two-tile mover; each step moves one step vector, so a
/// two-tile mover reaches the goal after M / 2 steps.
pub(crate) fn line(start: (i32, i32), goal: (i32, i32), stride: i32) -> Vec<u8> {
    let (dx, dy) = (goal.0 - start.0, goal.1 - start.1);
    let (major, minor) = (dx.abs().max(dy.abs()), dx.abs().min(dy.abs()));
    let x_major = dx.abs() >= dy.abs();
    let (sx, sy) = (if dx >= 0 { 1 } else { -1 }, if dy >= 0 { 1 } else { -1 });
    let mut e = (major / 2).max(1);
    let steps = major / stride;
    let mut out = Vec::with_capacity(steps as usize);
    for _ in 0..steps {
        let (mut mx, mut my) = if x_major { (sx, 0) } else { (0, sy) };
        e -= minor;
        if e < 1 {
            if x_major {
                my = sy;
            } else {
                mx = sx;
            }
            e += major;
        }
        out.push(direction_of(mx, my).expect("a step moves"));
    }
    out
}

/// Shortening table entries ("Shortening table").
#[derive(Clone, Copy)]
enum Merge {
    Keep,
    Swap,
    Into(u8),
    TwoOf(u8),
    Reverse,
}

fn merge(a: u8, b: u8) -> Merge {
    use Merge::*;
    const T: [[Merge; 8]; 8] = [
        [Keep, Swap, Into(1), Into(2), Reverse, Into(6), Into(7), Swap],
        [Keep, Keep, Keep, TwoOf(2), Into(2), Reverse, Into(0), TwoOf(0)],
        [Into(1), Swap, Keep, Swap, Into(3), Into(4), Reverse, Into(0)],
        [Into(2), TwoOf(2), Keep, Keep, Keep, TwoOf(4), Into(4), Reverse],
        [Reverse, Into(2), Into(3), Swap, Keep, Swap, Into(5), Into(6)],
        [Into(6), Reverse, Into(4), TwoOf(4), Keep, Keep, Keep, TwoOf(6)],
        [Into(7), Into(0), Reverse, Into(4), Into(5), Swap, Keep, Swap],
        [Keep, TwoOf(0), Into(0), Reverse, Into(6), TwoOf(6), Keep, Keep],
    ];
    T[a as usize & 7][b as usize & 7]
}

fn tiles(req: &Request, steps: &[u8]) -> Vec<(i32, i32)> {
    let mut p = req.start;
    let mut out = vec![p];
    for &d in steps {
        p = req.step(p, d);
        out.push(p);
    }
    out
}

/// Shortens a route in passes until a pass changes nothing. `None` if a pair steps straight back.
fn shorten(req: &Request, map: &impl Map, mut steps: Vec<u8>) -> Option<Vec<u8>> {
    loop {
        let mut changed = false;
        let mut j = 0;
        while j + 1 < steps.len() {
            let (a, b) = (steps[j], steps[j + 1]);
            let at = tiles(req, &steps[..j]).pop().expect("start");
            let usable = |t: (i32, i32), steps: &[u8]| {
                req.contains(t) && !map.blocked_strict(t.0, t.1) && !tiles(req, steps).contains(&t)
            };
            match merge(a, b) {
                Merge::Reverse => return None,
                Merge::Keep => {}
                Merge::Into(v) => {
                    steps.splice(j..j + 2, [v]);
                    changed = true;
                }
                Merge::Swap => {
                    if usable(req.step(at, b), &steps) {
                        steps[j] = b;
                        steps[j + 1] = a;
                        changed = true;
                    }
                }
                Merge::TwoOf(v) => {
                    if usable(req.step(at, v), &steps) {
                        steps[j] = v;
                        steps[j + 1] = v;
                        changed = true;
                    }
                }
            }
            j += 1;
        }
        if !changed {
            return Some(steps);
        }
    }
}

fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn minor(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().min((a.1 - b.1).abs())
}

/// Step 4's progress check for a walk from the start to `end` (not equal to it): the progress value, or `None` when
/// the walk is rejected. P (`end`) is measured to the goal before rounding, S to the rounded goal; the goal zone
/// plays no part.
fn progress(req: &Request, end: (i32, i32)) -> Option<i32> {
    let (s, g) = (req.start, req.goal);
    let (de, ds) = (chebyshev(end, req.raw_goal), chebyshev(s, g));
    if de < ds {
        Some(de)
    } else if de > ds {
        None
    } else {
        let m = minor(end, req.raw_goal);
        (m != 0 && m < minor(s, g)).then_some(m)
    }
}

/// One wall-following walk with turn sense `p` (step 3), judged and shortened (step 4). Returns the route's steps
/// and its score.
fn walk(
    req: &Request,
    map: &impl Map,
    prefix: &[u8],
    blocked_dir: u8,
    line_tiles: &[(i32, i32)],
    p: i8,
) -> Option<(Vec<u8>, i32)> {
    let turn = |d: u8, k: i32| ((d as i32 + k * p as i32).rem_euclid(8)) as u8;
    let goalish = |t: (i32, i32)| req.in_zone(t) || line_tiles.contains(&t);
    let mut steps = prefix.to_vec();
    let mut route = tiles(req, &steps);
    let t = *route.last().expect("start");
    let mut first = turn(blocked_dir, 1);
    let mut end = None;
    for _ in 0..MAX_WALK {
        let c = *route.last().expect("start");
        let mut found = None;
        for k in 0..8 {
            let d = turn(first, k);
            let n = req.step(c, d);
            if !req.contains(n) {
                return None;
            }
            if !map.blocked(n.0, n.1) {
                found = Some((d, n));
                break;
            }
        }
        let (d, n) = found?;
        if n == t {
            return None;
        }
        if let Some(k) = route[..route.len() - 1].iter().position(|&r| r == c) {
            route.truncate(k + 1);
            steps.truncate(k);
        }
        if goalish(n) {
            steps.push(d);
            end = Some(n);
            break;
        }
        if goalish(req.step(c, turn(d, 1))) {
            end = Some(c);
            break;
        }
        steps.push(d);
        route.push(n);
        first = turn(d, -2);
    }
    let end = end?;
    if end == req.start {
        return None;
    }
    let progress = progress(req, end)?;
    let steps = shorten(req, map, steps)?;
    let score = steps.len() as i32 + progress;
    Some((steps, score))
}

/// Searches a path: up to [`MAX_STEPS`] step directions from the start. Empty means no path.
pub fn search(req: &Request, map: &impl Map) -> Vec<u8> {
    if req.enhanced && req.start == req.goal {
        return Vec::new();
    }
    let full = line(req.start, req.goal, req.stride);
    let mut at = req.start;
    for (i, &d) in full.iter().enumerate() {
        if i == MAX_STEPS {
            return full[..MAX_STEPS].to_vec();
        }
        let n = req.step(at, d);
        if !req.contains(n) || map.blocked(n.0, n.1) {
            let prefix = &full[..i];
            let line_tiles = tiles(&Request { start: at, ..*req }, &full[i..]);
            // Round-65 errata ("Step 2"; `xhuman5-30000` n = 26761): the line tiles stop before G, which counts only
            // through the goal zone.
            let line_tiles = &line_tiles[1..line_tiles.len() - 1];
            let left = walk(req, map, prefix, d, line_tiles, -1);
            if let Some((s, score)) = &left
                && *score < GOOD_SCORE
            {
                return s.iter().copied().take(MAX_STEPS).collect();
            }
            let right = walk(req, map, prefix, d, line_tiles, 1);
            let chosen = match (left, right) {
                (Some(l), Some(r)) => {
                    if r.1 < l.1 {
                        r.0
                    } else {
                        l.0
                    }
                }
                (Some(l), None) => l.0,
                (None, Some(r)) => r.0,
                (None, None) => prefix.to_vec(),
            };
            return chosen.into_iter().take(MAX_STEPS).collect();
        }
        at = n;
    }
    full
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: u8 = 0;
    const NE: u8 = 1;
    const E: u8 = 2;
    const SE: u8 = 3;
    const S: u8 = 4;
    const SW: u8 = 5;

    fn run(w: i32, h: i32, wall: &[(i32, i32)], s: (i32, i32), g: (i32, i32)) -> Vec<u8> {
        let blocked = |x: i32, y: i32| wall.contains(&(x, y));
        search(&Request::to_tile(w, h, s, g, 1), &blocked)
    }

    fn row(y: i32, xs: std::ops::RangeInclusive<i32>) -> Vec<(i32, i32)> {
        xs.map(|x| (x, y)).collect()
    }

    fn col(x: i32, ys: std::ops::RangeInclusive<i32>) -> Vec<(i32, i32)> {
        ys.map(|y| (x, y)).collect()
    }

    #[test]
    fn equal_scores_keep_the_left_walk() {
        // Spec vector (xorc01-death-coil, grunt 1497 at n = 356): from (72, 79) toward (58, 75) the line's first tile
        // (71, 79) is blocked. Left: SW to (71, 80), NW to (70, 79), next to the line tile (70, 78); "two of W"
        // through (71, 79) is refused, so 2 steps, distance 12, score 14. Right: NW to (71, 78), distance 13, score
        // 14. The tie keeps the left walk.
        let got = run(100, 100, &[(71, 79)], (72, 79), (58, 75));
        assert_eq!(got, [SW, 7]);
    }

    #[test]
    fn straight_lines() {
        assert_eq!(run(10, 10, &[], (0, 0), (5, 2)), [SE, E, E, SE, E]);
        let mut want = vec![E, E, E, SE];
        want.extend([E; 8]);
        want.push(SE);
        want.extend([E; 7]);
        assert_eq!(run(30, 10, &[], (0, 0), (25, 3)), want);
    }

    #[test]
    fn two_tile_lines_count_the_error_in_tiles() {
        // "Line rule": dx and dy are G − S in tiles, and each step moves one step vector. From (92, 14) to the even
        // goal (86, 0): M = 14, m = 6, e = 7. Step 1: e = 1, no minor move (N); step 2: e = −5, NW, e = 9; and so on.
        // Counting in step units (M = 7, m = 3, e = 3) would give NW first.
        const NW: u8 = 7;
        assert_eq!(line((92, 14), (86, 0), 2), [N, NW, N, NW, N, NW, N]);
        // A one-tile line is unchanged.
        assert_eq!(line((0, 0), (5, 2), 1), [SE, E, E, SE, E]);
    }

    #[test]
    fn walks_around_walls() {
        assert_eq!(run(5, 5, &row(2, 1..=3), (2, 0), (2, 4)), [SE, SE, SW]);
        assert_eq!(run(8, 4, &row(1, 1..=5), (2, 0), (2, 3)), [6, SW, SE]);
        assert_eq!(run(10, 6, &col(3, 1..=4), (1, 2), (6, 2)), [NE, NE, SE, S]);
        assert_eq!(run(6, 6, &col(2, 1..=5), (0, 3), (5, 3)), [NE, N, NE, SE]);
    }

    #[test]
    fn no_path() {
        let ring: Vec<_> = (1..=3)
            .flat_map(|y| (1..=3).map(move |x| (x, y)))
            .filter(|&p| p != (2, 2))
            .collect();
        assert!(run(6, 6, &ring, (2, 2), (0, 0)).is_empty());
        assert!(run(6, 6, &[], (2, 2), (2, 2)).is_empty());
    }

    #[test]
    fn shortening_resumes_after_a_merge() {
        let req = Request::to_tile(10, 10, (4, 4), (9, 9), 1);
        let open = |_: i32, _: i32| false;
        assert_eq!(shorten(&req, &open, vec![N, E, SW, SE]), Some(vec![SE]));
    }

    /// Pathfinding spec, "Step 1", the two-tile vectors: a 3 x 3 building at (6, 18) widens to columns 5 to 9 and
    /// rows 17 to 21, and a two-tile mover's zone is the even tiles from (4, 16) to (10, 22); with no target and
    /// G = (22, 8) it is the even tiles from (20, 6) to (24, 10).
    #[test]
    fn a_two_tile_movers_zone_rounds_out_to_even() {
        let mut req = Request::to_tile(64, 64, (14, 14), (8, 18), 2);
        req.zone = (5, 17, 9, 21);
        for x in (4..=10).step_by(2) {
            for y in (16..=22).step_by(2) {
                assert!(req.in_zone((x, y)), "({x}, {y})");
            }
        }
        for p in [(2, 16), (12, 16), (4, 14), (4, 24)] {
            assert!(!req.in_zone(p), "{p:?}");
        }
        let req = Request::to_tile(64, 64, (2, 2), (22, 8), 2);
        for x in (20..=24).step_by(2) {
            for y in (6..=10).step_by(2) {
                assert!(req.in_zone((x, y)), "({x}, {y})");
            }
        }
        for p in [(18, 8), (26, 8), (22, 4), (22, 12)] {
            assert!(!req.in_zone(p), "{p:?}");
        }
    }

    #[test]
    fn progress_ignores_the_goal_zone() {
        // A one-tile mover at S = (35, 4), G = (38, 5): P = (37, 9) is inside a widened hall zone but farther
        // from G than S (4 against 3): rejected.
        let mut req = Request::to_tile(64, 64, (35, 4), (38, 5), 1);
        req.zone = (36, 3, 41, 10);
        assert!(req.in_zone((37, 9)));
        assert_eq!(progress(&req, (37, 9)), None);
    }

    #[test]
    fn two_tile_movers_measure_p_to_the_unrounded_goal() {
        // S = (10, 10), ordered tile (13, 11): G = (12, 10), S's distance 2. P = (12, 12) is 1 from (13, 11).
        let req = Request::to_tile(64, 64, (10, 10), (13, 11), 2);
        assert_eq!(req.goal, (12, 10));
        assert_eq!(progress(&req, (12, 12)), Some(1));
        // Measured to the rounded G it would be equal (2) with minor-axis distance 0: a one-tile request there
        // rejects it.
        let rounded = Request::to_tile(64, 64, (10, 10), (12, 10), 1);
        assert_eq!(progress(&rounded, (12, 12)), None);
    }
}
