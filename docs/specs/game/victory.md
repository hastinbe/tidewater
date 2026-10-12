# Game: victory and defeat

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), and the unit type numbers
  and flags in the public PUD specification text. Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-12 errata: the rescued count goes up only when a rescuable unit reaches a circle of
  power (`game/rescue.md`); modes W and Z lower L's rescued count when they make a hero rescuable (flagged open
  question).
- **Changes:** 2026-10-12: the open question on whether a player is allied with itself is closed by `game/setup.md`:
  the alliance table starts with every player allied with itself.

## Behaviour

Victory and defeat are decided by a **check** that runs in phase 8 of a cycle (`core/timing-and-rng.md`), but not on
every cycle. Which rule the check applies depends on the kind of game:

| Game | Rule |
| --- | --- |
| Multiplayer | The multiplayer rule |
| Single player, campaign mission | The mission's rule (from the mission table below) |
| Single player, custom scenario | The standard rule with "destroy all" on |
| Demo playback | No rule; the demo ends after 1,400 cycles |

All rules are evaluated for the **local player**, written L below.

### When the check runs

- At game start a countdown is set to 1 (1,400 for demo playback).
- Each cycle in phase 8 the countdown is tested and then lowered by 1. If the value tested was 0, it is instead
  set to **200** (not lowered in that cycle) and the check runs.
- So after a check the countdown takes 200 cycles to come down from 200 to 0, and the next check runs on the cycle
  after that: the period is **201** cycles. The first check runs on the **second** cycle of the game (cycle 1 lowers
  1 to 0, cycle 2 finds 0), then at cycles 203, 404, 605, …
- The fog refresh countdown of `core/timing-and-rng.md` works differently: when it is found at 0 it is reloaded and
  lowered in the same cycle, so its reload value (100 or 101) is its period. The victory countdown's reload value,
  200, is one less than its period. `core/timing-and-rng.md` gives the same period.
- In a single-player game, a cheat that disables victory freezes the countdown (no checks at all while it is on).

**Outcome timing.** In single-player games, a check that finds victory or defeat does not end the game at once. It
replaces the rule with the outcome, and the outcome (the victory or defeat screen) happens at the **next** check, 201
cycles later. A later step of the same check can still replace the outcome (see the mission rules). In multiplayer
games the outcome happens within the check.

### Terms

- **Unit counts.** For each player the game keeps live counts, updated as units appear, die or change owner:
  - **units:** all non-building units (of any kind);
  - **buildings:** all buildings, complete or not;
  - **under construction:** buildings not yet complete;
  - **in training:** units being trained;
  - **per-type counts** of complete buildings and of units, by type group (a human type and its orc counterpart count
    together, for example "shipyards" counts human and orc shipyards).
- **Real units:** units minus flying machines and zeppelins, transports, and tankers.
- **Present:** a player is present when it has at least one real unit or at least one building.
- **Production buildings:** complete barracks, shipyards, town halls, great halls, keeps, strongholds, castles,
  fortresses, guard towers and cannon towers (both races). Scout towers don't count.
- **Gold:** the player's gold stock.
- **Rescuable units:** units on the map that have the rescuable mark, whoever owns them. Units of rescue players have
  it from the start; joining a human player (`game/rescue.md`) keeps it, and reaching a circle of power removes it.
  The game counts them, and separately counts the rescuable **heroes** (type flag bit 23).
- **Rescued count of a player:** an unsigned 16-bit value. It goes up by 1 for each of the player's rescuable units
  that reaches a circle of power (`game/rescue.md`). If the mission's required count has bit 0x1000 set, a hero adds
  a further 0x1000.
- **Dead (for a hero):** dying, dead or removed.
- **Finding a hero:** the unit slots are searched in slot order (`core/unit-slots.md`) for units of the named types,
  whoever owns them.

### The standard rule

The standard rule has two options: **destroy all** and **rescue** (with a required count). Evaluate in order:

1. **Cheats.** The instant-victory cheat gives victory; the instant-defeat cheat gives defeat.
2. If L has nothing **in training** and nothing **under construction**:
   - **L has no units:**
     - if L has no production building, **defeat**;
     - otherwise, if L has less than **400 gold**, **defeat**;
     - otherwise go to step 3.
   - **L has units, and destroy all is on:** if every other player (0 to 7, L excluded, allies included) has no
     buildings and no real units, **victory**. Otherwise **no result**. (Step 3 is skipped.)
   - **L has units, destroy all is off:** go to step 3.
3. **Rescue** (only when the rescue option is on; otherwise no result):
   - if L's rescued count is at least the required count, **victory**;
   - otherwise let **possible** = (L's rescued count without bit 0x1000) + (rescuable units left) + 0x1000 if a
     rescuable hero is left or L's rescued count already has bit 0x1000. If possible is less than the required
     count, **defeat**;
   - otherwise no result.

Consequences worth testing:

- Destroy-all victory is only seen while L has at least one unit and nothing in training or under construction. A
  player who wins while a farm is being built wins at a later check.
- Every other player counts, including allies and rescue players. Flying machines, zeppelins, transports and tankers
  left over don't prevent victory.
- With units in training or buildings under construction, L is never defeated by step 2.

### The multiplayer rule

1. **Cheats** as above (instant victory goes to step 6).
2. **L is out** if L is not present, or L is a human player who is no longer connected. Then L is **defeated**: in a
   network game L's slot is turned into "nobody", L is removed from everyone's shared vision, and all of L's living
   units are given order 32. Order 32 is the docked-transport wait of `orders/orders.md` (order table, row 32): its
   logic does nothing, so the units stand where they are. The defeat screen is shown.
3. Otherwise each player q (0 to 7) is checked the same way: q is **in** when it is present, and, if q is a human
   player, still connected (computer players that are present are always in). A player who is out and has no result
   yet is marked defeated and, in a network game, turned into "nobody" as above.
4. If any **computer** player is in, **no result**.
5. If exactly one human player is in (L), **victory**.
6. If several human players are in: every one of them must have the allied-victory setting on, and each pair of them
   must be allied both ways (the test also takes each player paired with itself, which game setup makes allied;
   `game/setup.md`). Otherwise **no result**. If they are, **victory** for all.
7. **Victory:** in a network game, if no player has a result yet, every used slot gets the "game ended" result.
   L's result becomes victory if it has none. The victory screen is shown.

### Mission rules

Each campaign mission has a **mode** and a **required count**, from a 52-entry table. The entry is
2(n − 1) for human mission n and 2(n − 1) + 1 for orc mission n, plus 28 for *Beyond the Dark Portal*: entries 0–27
are *Tides of Darkness* (14 missions per race) and 28–51 *Beyond the Dark Portal* (12 per race). The game computes the
entry this way when the mission-select cheat starts a mission; a normally started campaign map takes its entry from
the map's position in the campaign's map list, in the same interleaved order (human 1, orc 1, human 2, …). The mode
and required count apply only to single-player campaign maps; a custom scenario always gets "destroy all".

| Entries | Mode | Required |
| --- | --- | --- |
| 0, 1 | A | 0 |
| 2 | Rescue | 1 |
| 3 | Rescue | hero + 1 (0x1001) |
| 4, 5 | B | 0 |
| 6–10, 13, 19, 21, 23, 25, 27, 33, 42, 43 | Destroy all | 0 |
| 11, 16 | Rescue | hero + 1 (0x1001) |
| 12 | C | 0 |
| 14 | D | 0 |
| 15 | J | 0 |
| 17 | K | 0 |
| 18 | E (rescue) | 4 |
| 20 | F | 0 |
| 22 | G | 0 |
| 24 | H | 0 |
| 26 | I | 0 |
| 28 | U | 3 |
| 29 | L | 0 |
| 30 | V | 0 |
| 31 | M | 0 |
| 32 | W | 1 |
| 34 | X | 0 |
| 35, 39 | N | 0 |
| 36 | Y | 0 |
| 37 | O | 0 |
| 38 | Z | 2 |
| 40 | AA | 0 |
| 41 | P | 0 |
| 44 | AB | 0 |
| 45 | Q | 0 |
| 46 | AC | 0 |
| 47 | R | 0 |
| 48 | AD | 0 |
| 49 | S | 0 |
| 50 | AE | 0 |
| 51 | T | 0 |

The lettered modes are numbered in letter order from 0: A is 0, Z is 25, AA to AE are 26 to 30.

"Destroy all" and "Rescue" are the standard rule with that option. Every lettered mode first runs the standard rule
with **neither option** (so only the cheats and the defeat of step 2 apply), unless it says "destroy-all check", which
means the standard rule with destroy all on. Then it applies its own conditions, in the order listed; a later
condition replaces an earlier result. "Victory" and "defeat" below mean the outcome is set.

| Mode | Extra conditions |
| --- | --- |
| A | Victory when L has at least 1 barracks and at least 4 farms |
| B | Victory when L has at least 4 oil platforms |
| C | Victory when no other player has a refinery |
| D | Destroy-all check. If it gives victory, it counts only if L has a castle or fortress; otherwise the check goes on (no result) |
| E | None: the mission start switches it to the plain rescue rule (below) |
| F | Once any unit or building has changed owner to L (for example by rescue), the mission switches to destroy all |
| G | Victory when no other player has a transport, an oil platform or a shipyard |
| H | None: the mission start switches it to destroy all (below) |
| I | Victory when player 15 has no dark portal |
| J | Defeat when no player 0–7 has a runestone. Otherwise victory when no other player has a castle or fortress and L has a runestone |
| K | Victory when L has a castle or fortress and a shipyard, and L owns a complete orc shipyard and a complete fortress that each stand at least partly in the region of the map's circle of power |
| L | Defeat when no Grom Hellscream exists or he is dead. Victory when player 6 has no mages, death knights, Teron Gorefiend or Khadgar (one type group) and no buildings. (When Grom doesn't exist the victory condition is still tested afterwards and can replace the defeat) |
| M | Destroy-all check, then: defeat unless two units among Khorgath Bladefist and Dentarg are found and both alive |
| N | Destroy-all check, then: defeat unless Teron Gorefiend is found and alive (the first one found) |
| O | Defeat when neither player 2 nor L has a gryphon aviary or dragon roost. Victory when L has one and player 2 has no gryphon riders, dragons, Kurdan or Deathwing |
| P | Victory when L has at least 5 shipyards and no other player has a shipyard, tanker, destroyer, battleship or juggernaught, or submarine or turtle |
| Q | Victory when player 1 has no living daemon |
| R | If the standard part gave defeat, stop. Defeat when no mage owned by player 4 or L exists, or the first one found is dead. No result while player 6, player 0 or player 2 is present (in the multiplayer sense). Otherwise victory once that mage is no longer rescuable (it has been rescued) |
| S | Victory when player 3 has no buildings and no real units |
| T | If the standard part gave a result, stop. Defeat when neither player 15 nor L has a dark portal. Otherwise destroy-all check: its victory counts only if L has a dark portal; otherwise the check goes on |
| U | Defeat if any of Danath, Turalyon and Alleria found is dead, or fewer than 3 of them are found. Then the rescue test of step 3 with required 3 (which can replace that defeat with victory) |
| V | Defeat unless Danath is found and alive. Then destroy-all check |
| W | Defeat unless Turalyon is found and alive. When no player other than L and player 4 has a keep, stronghold, castle or fortress: Turalyon is made rescuable (if he isn't already) and the mission switches to the rescue rule (required 1). Making a hero rescuable this way (only a hero without the mark) also lowers L's rescued count by 1 (unsigned 16-bit, so 0 becomes 65535). The rescue test first runs at the next check |
| X | No result while any player other than L and player 7 is in (multiplayer sense). Then victory if L has a castle or fortress; otherwise the check goes on |
| Y | Victory when L has at least 3 shipyards and no other player has a shipyard |
| Z | Defeat unless two units among Turalyon and Danath are found and both alive. No result while player 4 is in. Then both are made rescuable and the mission switches to the rescue rule (required 2). Making a hero rescuable this way (only a hero without the mark) also lowers L's rescued count by 1 (unsigned 16-bit, so 0 becomes 65535). The rescue test first runs at the next check |
| AA | Defeat unless Alleria, Khadgar and Kurdan (three found) are all alive. Victory when no living Deathwing exists and no other player has a gryphon aviary or dragon roost |
| AB | Victory when player 5 has no castle or fortress and player 4 has no runestone |
| AC | Defeat unless Danath, Khadgar, Turalyon and Alleria (four found) are all alive. Then destroy-all check. Then victory when no other player has 400 gold or more |
| AD | No standard part. Defeat unless Khadgar, Turalyon and Alleria (three found) are all alive. Then destroy-all check |
| AE | Defeat unless Khadgar is found and alive. Victory when player 15 has no dark portal |

### Mission start

When a campaign mission starts, its mode may change the game once, before the first check. "Knows" means the bits
are added to the player's **spells known** word, with the spell bit numbers of `economy/tech-tree.md` (bit 20 is the
paladin or ogre-mage upgrade). "Become" is the unit conversion of `economy/tech-tree.md` ("Converting units"). The
**rescuable mark** is the per-unit mark that makes a unit count as rescuable (Terms); rescue-owned units have it
when the map loads.

| Mode | At mission start |
| --- | --- |
| D | Player 4's peasants (type 0x02) become attack peasants (type 0x10), and each converted unit loses the rescuable mark. L and player 4 stop being allied, both ways |
| E | Player 4's peasants become attack peasants and keep the rescuable mark. The mission switches to the plain rescue rule; the required count stays 4 |
| H | Every one of player 3's Lothars (type 0x32) that isn't dying or dead loses the rescuable mark. The mission switches to destroy all |
| M | L knows eye of Kilrogg and bloodlust (bits 10, 11) |
| Q | Player 1 knows healing, exorcism and the paladin upgrade (bits 1, 3, 20). Player 1's knights (0x06) become paladins (0x0C) |
| T | Every one of player 15's dark portals that isn't dying or dead gets the rescuable mark. L knows bits 10, 11 and 13 to 19 (eye of Kilrogg, bloodlust, raise dead, death coil, whirlwind, haste, unholy armour, runes, death and decay) |
| U | L knows healing and exorcism (bits 1, 3) |
| W, Z | L knows holy vision, healing and exorcism (bits 0, 1, 3) |
| AA | L knows bits 1, 3, 4, 5, 6, 7, 9 and 20 (healing, exorcism, flame shield, fireball, slow, invisibility, blizzard, the paladin upgrade). L's knights and player 3's knights become paladins |
| AB | Every runestone owned by players 0 to 7 has its hit points set to 400 |
| AC | L knows bits 0, 1 and 3 to 9 (holy vision, healing, exorcism, flame shield, fireball, slow, invisibility, polymorph, blizzard) |
| AD | L knows the same spells as in AC. Player 1's and player 0's knights become paladins |
| AE | L knows bits 0 to 9 (the AC spells and the unused bit 2) |

Every other mode, and "destroy all" and "rescue", change nothing at mission start. The knights-to-paladins
conversions keep the rescuable mark.

## Data

| Item | Value |
| --- | --- |
| Check interval | every 201 cycles; first on the second cycle |
| Demo playback end | after 1,400 cycles |
| Gold needed to survive without units | 400 |
| Hero bit in rescue counts | 0x1000 |
| PUD unit types named above | Grom Hellscream 0x19, Teron Gorefiend 0x15, Dentarg 0x17, Khorgath Bladefist 0x2F, Danath 0x2E, Turalyon 0x2C, Alleria 0x14, Khadgar 0x18, Kurdan and Sky'ree 0x16, Deathwing 0x23, Lothar 0x32, mage 0x0A, daemon 0x38, circle of power 0x64, dark portal 0x65, runestone 0x66 |

## Test vectors

| Input | Expected |
| --- | --- |
| Custom scenario. The last enemy building dies in cycle 500; L has 3 footmen, nothing in training or construction | Checks run at cycles 2, 203, 404, 605, …: victory is found at 605 and the victory screen appears at 806 |
| Same, but L is training a peasant until cycle 700 | Victory found at the first check after training ends (806), screen at 1007 |
| Custom scenario; enemies have only a zeppelin and a transport left | Victory (they don't count) |
| Custom scenario; an allied computer player still has units | No victory |
| L has no units, only a farm, nothing training | Defeat |
| L has no units, a town hall and 399 gold | Defeat |
| L has no units, a town hall and 400 gold | No result |
| L has no units, a guard tower and 500 gold | No result (towers count as production buildings) |
| Mission entry 18 (required 4). L has rescued 3, one rescuable unit is left | No result |
| Same, the last rescuable unit dies | possible = 3 < 4: defeat |
| Mission entry 3 (required 0x1001). L rescues a hero first | Rescued count 0x1001: victory |
| Mission entry 3. L rescues one ordinary unit; the rescuable hero dies; no rescuable units left | Rescued 1; possible = 1 < 0x1001: defeat |
| Mission entry 0. L has 1 barracks and 4 farms (complete) | Victory |
| Multiplayer, 2 humans and 1 computer; the computer is still present | No result |
| Multiplayer, 2 humans left, allied both ways, both with allied victory on | Victory for both |
| Multiplayer, L's last unit and building die | Defeat at that check; L's slot becomes nobody |
| Network game, humans L, B and C, all present, no alliances; B disconnects | At the next check B is out: defeated, B's slot becomes nobody, B leaves everyone's shared vision, and B's living units get order 32 and stand still. L and C are in and not allied: no result |
| Same, but C has no units and no buildings left | C is out and defeated too. Only L is in: victory for L. Because B and C already have results, no slot gets the "game ended" result |
| Human mission 8 of *Tides of Darkness* | Entry 14, mode D |
| Orc mission 1 of *Beyond the Dark Portal* | Entry 29, mode L |
| Entry 0 (mode A). L has 1 barracks and 3 complete farms, and a fourth farm under construction | No result (only complete farms count) |
| Same, the fourth farm completes in cycle 300 | Victory found at the check at cycle 404; the victory screen appears at 605 |
| Entry 36 (mode Y). L has 2 shipyards; no other player has one | No result |
| Same, L completes a third shipyard | Victory at the next check |
| Entry 14 (mode D) starts; player 4 has 5 peasants, all with the rescuable mark | Player 4 has 5 attack peasants and no rescuable units; L and player 4 are not allied either way |
| Entry 44 (mode AB) starts; player 4 has a runestone at 5000 HP | It has 400 HP |
| Entry 40 (mode AA) starts; L has 2 knights at full HP | They are paladins, with HP raised by the difference in maximum HP |

## Open questions

- **Flagged: modes W and Z.** Taken literally, the count drop gives victory at the next check when L's count was 0,
  and defeat if the hero reaches a circle of power first (with nothing else marked). The heroes are rescued like any
  other rescuable unit L owns, by reaching a circle of power (`game/rescue.md`). Whether the shipped missions (32
  for W, 38 for Z) really play this way, and whether the rescued counts are reset between missions (no reset was
  found at mission start), hasn't been confirmed in play. Default: the literal behaviour.
- The allied-victory setting's source (assumed: the diplomacy screen's allied-victory box).
- The exact result codes reported to the network ("game ended" when nobody has a result) are not specified.
- Whether passive and rescue players' units should block destroy-all victory in custom scenarios (they do in the
  program, by the rule above).
- The mission-start conversions name only the human peasant and knight types. Whether a map gives those players orc
  units that would need converting too is a data question. Default: only the named types change.
