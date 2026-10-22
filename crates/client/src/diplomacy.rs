//! A minimal way to send the diplomacy command (`docs/specs/game/diplomacy.md`): a chat line that starts with
//! `/diplomacy`. The diplomacy screen ("CHOOSE YOUR ALLIES") is a later UI spec; this is only enough to send the
//! command and to test it.
//!
//! ```text
//! /diplomacy [allies=0,2] [vision=0,2] [victory=on|off]
//! ```
//!
//! A field left out keeps the player's current setting. Our own screen always sends the sender in its own vision set
//! and allied with itself (spec, "A player outside its own set"), and so does this line: the sender is added to both
//! lists whatever they say. A command that leaves the sender out can only come from another program.

use tidewater_sim::Command;

/// The word that starts the line.
pub const WORD: &str = "/diplomacy";

/// What the sender's settings are now, which a field left out keeps.
#[derive(Clone, Copy, Debug)]
pub struct Current {
    /// The alliance row, bit q for player q (only players 0 to 7 are sent).
    pub allies: u16,
    pub vision: u8,
    pub victory: bool,
}

/// Reads a typed line. `None` when it isn't a diplomacy line; `Some(Err)` with a short reason when it is one but
/// can't be read; otherwise the command for `player`.
pub fn parse(line: &str, player: u8, now: Current) -> Option<Result<Command, String>> {
    let mut words = line.split_whitespace();
    if !words.next().is_some_and(|w| w.eq_ignore_ascii_case(WORD)) {
        return None;
    }
    let (mut allies, mut vision, mut victory) = ((now.allies & 0xff) as u8, now.vision, now.victory);
    for w in words {
        let Some((key, value)) = w.split_once('=') else {
            return Some(Err(format!("expected name=value, got {w}")));
        };
        match key.to_ascii_lowercase().as_str() {
            "allies" => match players(value) {
                Some(set) => allies = set,
                None => return Some(Err(format!("players 0 to 7, comma-separated: {value}"))),
            },
            "vision" => match players(value) {
                Some(set) => vision = set,
                None => return Some(Err(format!("players 0 to 7, comma-separated: {value}"))),
            },
            "victory" => match value.to_ascii_lowercase().as_str() {
                "on" => victory = true,
                "off" => victory = false,
                _ => return Some(Err("victory=on or victory=off".into())),
            },
            _ => return Some(Err(format!("unknown field {key}"))),
        }
    }
    if player >= 8 {
        return Some(Err("only players 0 to 7 have diplomacy".into()));
    }
    // The sender stays in its own set and allied with itself.
    allies |= 1 << player;
    vision |= 1 << player;
    Some(Ok(Command::Diplomacy {
        player,
        alliances: std::array::from_fn(|q| (allies >> q) & 1),
        vision,
        allied_victory: victory,
    }))
}

/// A comma-separated list of players 0 to 7 (empty is the empty set).
fn players(s: &str) -> Option<u8> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .try_fold(0u8, |set, p| match p.parse::<u8>() {
            Ok(q) if q < 8 => Some(set | 1 << q),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Current = Current {
        allies: 0xff00 | 0b101,
        vision: 0b001,
        victory: false,
    };

    #[test]
    fn other_lines_are_not_ours() {
        assert_eq!(parse("hello", 0, NOW), None);
        assert_eq!(parse("/diplomacyx", 0, NOW), None);
    }

    #[test]
    fn fields_left_out_keep_the_current_settings() {
        let c = parse("/diplomacy victory=on", 0, NOW).unwrap().unwrap();
        assert_eq!(
            c,
            Command::Diplomacy {
                player: 0,
                alliances: [1, 0, 1, 0, 0, 0, 0, 0],
                vision: 0b001,
                allied_victory: true,
            }
        );
    }

    #[test]
    fn the_sender_always_stays_in_its_own_set_and_allied_with_itself() {
        let c = parse("/diplomacy allies=1 vision=1,3", 2, NOW).unwrap().unwrap();
        assert_eq!(
            c,
            Command::Diplomacy {
                player: 2,
                alliances: [0, 1, 1, 0, 0, 0, 0, 0],
                vision: 0b1010 | 0b0100,
                allied_victory: false,
            }
        );
    }

    #[test]
    fn bad_lines_say_why() {
        assert!(parse("/diplomacy allies=9", 0, NOW).unwrap().is_err());
        assert!(parse("/diplomacy victory=maybe", 0, NOW).unwrap().is_err());
        assert!(parse("/diplomacy colour=red", 0, NOW).unwrap().is_err());
        assert!(parse("/diplomacy", 9, NOW).unwrap().is_err());
    }
}
