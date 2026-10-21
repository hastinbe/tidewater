//! Finding where two worlds that should be equal differ (`twtool desync-diff`, `docs/design/multiplayer-determinism.md`,
//! "Finding a desync"). [`World::state_hash`] says that two worlds differ; this says in which piece, and then in which
//! field.
//!
//! [`World::state_parts`] hashes the pieces `state_hash` covers one by one, in the same order, each with the same
//! hasher. [`World::first_difference`] takes the first piece whose hash differs and compares the pieces' `Debug`
//! text line by line (units one at a time) to name the first field that is not equal. It is a debugging aid: it
//! formats whole pieces, so it is slow, and it is never called in a normal game.

use std::fmt::Debug;
use std::hash::{Hash, Hasher};

use super::{Fnv, World};

/// The first difference between two worlds: the piece of state that differs and, below it, the first field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The piece, named as in [`World::state_parts`] (`units`, `players`, `terrain` and so on).
    pub part: &'static str,
    /// Where in the piece: for a unit its id and slot, then the nesting of the differing field.
    pub path: String,
    /// The differing field on the first world's side and on the second's, as `Debug` shows them.
    pub left: String,
    pub right: String,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.part)?;
        if !self.path.is_empty() {
            write!(f, " > {}", self.path)?;
        }
        write!(f, ": {} vs {}", self.left, self.right)
    }
}

fn fnv<T: Hash + ?Sized>(v: &T) -> u64 {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    v.hash(&mut h);
    h.finish()
}

fn pretty<T: Debug + ?Sized>(v: &T) -> String {
    format!("{v:#?}")
}

impl World {
    /// The pieces of state [`World::state_hash`] covers, each with its own hash: the same pieces, in the same order.
    pub fn state_parts(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("tick", fnv(&self.tick)),
            ("players", fnv(&self.players)),
            ("rng", fnv(&self.rng)),
            ("units", fnv(&self.units)),
            ("ylist", fnv(&self.ylist)),
            ("missiles", fnv(&self.missiles)),
            ("runes", fnv(&self.runes)),
            ("vision.countdown", fnv(&self.vision.countdown)),
            ("slots", fnv(&self.slots)),
            ("stale", fnv(&self.stale)),
            ("terrain", fnv(&self.terrain)),
            ("ai", fnv(&self.ai)),
            ("army", fnv(&self.army)),
            ("victory", fnv(&self.victory)),
            ("selections", fnv(&self.selections)),
            ("speed", fnv(&self.speed)),
            ("paused", fnv(&self.paused)),
            ("cheats.flags", fnv(&self.cheats.flags)),
            ("vision.seen", fnv(&self.vision.seen)),
        ]
    }

    /// The `Debug` text of one piece named in [`World::state_parts`].
    fn part_text(&self, part: &str) -> String {
        match part {
            "tick" => pretty(&self.tick),
            "players" => pretty(&self.players),
            "rng" => pretty(&self.rng),
            "ylist" => pretty(&self.ylist),
            "missiles" => pretty(&self.missiles),
            "runes" => pretty(&self.runes),
            "vision.countdown" => pretty(&self.vision.countdown),
            "slots" => pretty(&self.slots),
            "stale" => pretty(&self.stale),
            "terrain" => pretty(&self.terrain),
            "ai" => pretty(&self.ai),
            "army" => pretty(&self.army),
            "victory" => pretty(&self.victory),
            "selections" => pretty(&self.selections),
            "speed" => pretty(&self.speed),
            "paused" => pretty(&self.paused),
            "cheats.flags" => pretty(&self.cheats.flags),
            "vision.seen" => pretty(&self.vision.seen),
            _ => String::new(),
        }
    }

    /// The first piece, and in it the first field, in which this world differs from `other`; `None` if every piece
    /// the state hash covers is equal. Pieces are checked in the state hash's order, so the answer is stable.
    pub fn first_difference(&self, other: &World) -> Option<Difference> {
        let (a, b) = (self.state_parts(), other.state_parts());
        let &(part, _) = a.iter().zip(&b).find(|(x, y)| x.1 != y.1).map(|(x, _)| x)?;
        if part == "units" {
            return Some(units_difference(self, other));
        }
        let (left, right) = (self.part_text(part), other.part_text(part));
        Some(line_difference(part, "", &left, &right))
    }
}

/// The first differing unit: a missing one, or the first field of the first unit that is not equal.
fn units_difference(a: &World, b: &World) -> Difference {
    for (x, y) in a.units.iter().zip(&b.units) {
        if x != y {
            let at = format!("unit {:?} (slot {})", x.id, x.slot);
            let mut d = line_difference("units", &at, &pretty(x), &pretty(y));
            if x.id != y.id || x.slot != y.slot {
                d.path = format!("unit {:?} (slot {}) vs {:?} (slot {})", x.id, x.slot, y.id, y.slot);
            }
            return d;
        }
    }
    let (n, m) = (a.units.len(), b.units.len());
    Difference {
        part: "units",
        path: "count".into(),
        left: n.to_string(),
        right: m.to_string(),
    }
}

/// The first line in which two pretty `Debug` texts differ, with the lines that open the nesting around it.
fn line_difference(part: &'static str, at: &str, left: &str, right: &str) -> Difference {
    let (l, r): (Vec<&str>, Vec<&str>) = (left.lines().collect(), right.lines().collect());
    let i = (0..l.len().max(r.len())).find(|&i| l.get(i) != r.get(i)).unwrap_or(0);
    let indent = |s: &str| s.len() - s.trim_start().len();
    // The lines above the differing one that open the structures around it, outermost last while walking up.
    let mut path = Vec::new();
    let mut level = l.get(i).or(r.get(i)).map_or(0, |s| indent(s));
    for j in (0..i).rev() {
        if level == 0 {
            break;
        }
        let s = l.get(j).copied().unwrap_or("");
        if indent(s) < level && !s.trim().is_empty() {
            level = indent(s);
            let name = s
                .trim()
                .trim_end_matches(['{', '[', '('])
                .trim()
                .trim_end_matches(':')
                .trim();
            if !name.is_empty() {
                path.push(name.to_string());
            }
        }
    }
    path.reverse();
    if !at.is_empty() {
        path.insert(0, at.to_string());
    }
    let show = |v: &Vec<&str>| v.get(i).map_or("(end)", |s| s.trim()).trim_end_matches(',').to_string();
    Difference {
        part,
        path: path.join(" > "),
        left: show(&l),
        right: show(&r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_difference_names_the_field_and_its_nesting() {
        let a = "Unit {\n    id: 3,\n    order: Move {\n        to: (1, 2),\n    },\n}";
        let b = "Unit {\n    id: 3,\n    order: Move {\n        to: (1, 9),\n    },\n}";
        let d = line_difference("units", "unit 3", a, b);
        assert_eq!(d.path, "unit 3 > Unit > order: Move");
        assert_eq!((d.left.as_str(), d.right.as_str()), ("to: (1, 2)", "to: (1, 9)"));
    }

    #[test]
    fn a_missing_line_reads_as_the_end() {
        let d = line_difference("ylist", "", "[\n    1,\n]", "[\n    1,\n    2,\n]");
        assert_eq!((d.left.as_str(), d.right.as_str()), ("]", "2"));
    }
}
