# AI: computer player scripts

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the AI script file
  `Rez\ai.bin` in the game data (decoded with the format below), the unit flags in the game's unit data and the AI
  list and `AIPL` section in the public PUD specification text. Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-10 errata: units left out of the army counts are held units (map-placed, `UNIT` data
  value with a non-zero low byte), not rescuable ones; slots 0x09 to 0x12 filled in.
- **Changes:** 2026-10-13, round-60 errata: a player counts as allied with itself, so condition 7 never counts its
  own workers (open question closed; the rule is unchanged); `human11-30000` vector.
- **Changes:** 2026-10-16, round 82 (AI consolidation, `ERRATA-ai-consolidation.md`, reviewed by a spec-review session,
  2026-10-16): the model of the whole computer player is in `ai/computer-player.md`. Remarks that the other AI parts are
  "a later spec" now point at their specs; static study fills in that there are exactly four opcodes and eight
  conditions and that slots 0x1F and 0x21 have no reader. No rule change.
- **Changes:** 2026-10-21, round 94 (oracle traces, `ERRATA-round94-traces.md`; reviewed by a spec-review session,
  2026-10-21, see its "Review"): a new game runs the scripts' openings before the map's units are placed, so an
  opening's "wait until" is tested with every count at 0 and, when it passes in the second start-up cycle, the next
  instructions run at trace record 4 (static study; traced, AS-V1, AS-V2). The "Script 0" vector is corrected (AS-V3)
  and the open question on the opening's counts is closed.
- **Changes:** 2026-10-22, round 106 (save/load sweep, `ERRATA-round106-sweep.md` SL1; reviewed by a spec-review
  session, 2026-10-22, see its "Review"): after a load the land, sea and air army counts that conditions 4 to 6 read
  are the saved counts plus the loaded units counted again, as the training group counts are, so they double (traced
  for the land and sea armies, `game/save-load.md` SL34 to SL37; the air army by the same rule, a labelled gap-fill,
  untraced).

## Behaviour

Each player has an **AI script**: a small program, taken from the AI script file, that runs once per cycle for every
computer-controlled player. A script does not move units or train anything itself. It only writes numbers into a
per-player set of **slots** (wanted unit counts, how far down the build list to go, attack settings) and waits on
simple conditions. The rest of the computer player reads those slots: `ai/computer-player.md` is the model of the
whole, and its "Information flow" lists each slot's readers.

### Which script a player runs

- The map's `AIPL` section gives one byte per player (PUD specification, section 13). For players 0 to 7 that byte is
  the player's **script number**.
- The AI script file starts with a **header** of 128 little-endian 16-bit words. Word *n* is the file offset of
  script *n*'s **record**. The shipped file uses script numbers 0 to 0x52, matching the PUD specification's AI list
  (0 land attack, 1 passive, 0x19 sea attack, 0x1A air attack, the campaign scripts and the expansion scripts 0x20 to
  0x52).
- A script record is: a 16-bit file offset of the player's **build list**, a 16-bit file offset of a **second list**
  (both used by the rest of the AI), then the script's instructions, starting 4 bytes into the record.

All offsets are from the start of the file.

### Per-player state

For each of players 0 to 7 the game keeps:

| Item | Meaning |
| --- | --- |
| Wait counter | Cycles left before the script runs again; 0 means "run now" |
| Position | File offset of the next instruction |
| Slots | Bytes the script writes, numbered as in the file (table below) |
| Build-list limit | Slot 0x22. Starts at 0xFF when the script starts |
| List offsets | The two list offsets from the script record |

### Game start

At the start of a new game (not when a saved game is loaded, which restores this state):

1. All players' state is cleared to zero.
2. For each player 0 to 7 in turn, **whatever its controller** (human, computer or nobody): the wait counter is 0,
   the build-list limit is 0xFF, the two list offsets are taken from the script record, the position is the first
   instruction, and the script is **run** (below) until it pauses.

This happens **before the map's units are placed**. The game start first sets every count the conditions read
("Conditions": the live unit counts, workers and buildings included) to 0 for the new game, then runs the openings,
then places the map's units, which counts them (static study, round 94; the timing below is traced: AS-V1, AS-V2).

So every player's script has already run its opening instructions before the first cycle. For non-computer players
the script then never runs again.

**An opening that ends in a "wait until"** tests it with no units at all: conditions 0, 1, 2 and 7 are false,
condition 3 is false unless slot 0x13 is 0, and conditions 4 to 6 are true only when their product is 0. Every
shipped campaign opening that ends in a "wait until" fails it there (static study of the game's script file and maps,
round 94). As always the wait counter becomes 1 ("Running a script"). The first start-up cycle takes the counter to
0; the second start-up cycle tests the condition again, now with the map's units counted. When it is true there, the
counter is 1 again, the first normal cycle takes it to 0, and the instructions after it run in the second normal
cycle: trace record n = 4, counting the setup record as 0 and the two start-up cycles as 1 and 2
(`core/timing-and-rng.md`, "Scheduler"; traced, round 94: AS-V1, AS-V2). A slot an opening sets before the "wait
until" (the build-list limit, for example) therefore holds its opening value through records 0 to 3.

**A loaded game** runs no script at its start: every player's script state (wait counter, position, slots, the
build-list limit included, list offsets) is restored as saved, and the start-up cycles go on from it (traced for the
limit, round 94: AS-V1's loaded game).

### Each cycle

In phase 2 of each cycle (`core/timing-and-rng.md`), for each player 0 to 7 in ascending order, if the player is
**computer-controlled**:

- if its wait counter is above 0, the counter goes down by 1 and nothing else happens this cycle;
- otherwise the script is **run**.

Players 8 to 15 never run a script.

### Running a script

Running means: read the instruction at the position, move the position past its first byte (the opcode), carry it
out, and repeat **until the wait counter is not 0**. Instructions that don't set the wait counter don't pause, so any
number of them run in the same cycle.

| Opcode | Bytes | Instruction | Effect |
| --- | --- | --- | --- |
| 0 | 3: opcode, slot, value | **Set** | Slot *slot* ← *value*. No pause |
| 1 | 3: opcode, 16-bit offset | **Jump** | Position ← *offset* (from the start of the file). No pause |
| 2 | 5: opcode, 32-bit count | **Wait** | Wait counter ← *count*; position moves past the instruction. Pauses if *count* > 0 |
| 3 | 2: opcode, condition | **Wait until** | Tests the condition (below). True: the position moves past the instruction. False: the position goes back to this instruction's opcode. In both cases the wait counter ← 1, so the script pauses |

Multi-byte values are little-endian. These four are the only opcodes, and conditions 0 to 7 below the only
conditions; no other value is defined (static study, round 82; see Open questions).

**Timing that follows from this:**

- A wait of *N* (*N* ≥ 1) run in cycle *t* lets the script resume in cycle *t* + *N* + 1: the counter goes down in
  cycles *t* + 1 to *t* + *N*, and in cycle *t* + *N* + 1 it is 0 and the script runs. A wait of 0 doesn't pause.
- A **wait until** always costs 2 cycles. When its condition is true in cycle *t*, the script goes on in cycle
  *t* + 2. When false, the condition is tested again in cycle *t* + 2, and so on every 2 cycles until it is true.

### Conditions

*p* is the player running the script. "Counts" are the player's live unit counts (`game/victory.md`, "Unit counts"),
except that after a load the three army counts carry a surplus ("After a load", below).

| Condition | True when |
| --- | --- |
| 0 | *p* has at least one complete shipyard (either race) |
| 1 | *p* has at least one complete keep or stronghold |
| 2 | *p* has at least one complete castle or fortress |
| 3 | *p*'s **workers** ≥ slot 0x13 |
| 4 | *p*'s **land army** ≥ slot 0x0D × slot 0x0E |
| 5 | *p*'s **sea army** ≥ slot 0x0F × slot 0x10 |
| 6 | *p*'s **air army** ≥ slot 0x11 × slot 0x12 |
| 7 | Some player from 0 to 7 that *p* does not count as allied has at least one worker. *p* counts as allied with itself, so its own workers never count |

Comparisons are unsigned. The products are of two bytes, so up to 65025.

**Workers** are units of types 0x02, 0x03, 0x10 and 0x11 (peasant, peon and both attack versions). The count includes
held units (below).

**Army** counts are kept per player for every unit that exists, is complete and is **not held**. A **held** unit is
one placed by the map's `UNIT` section whose 16-bit data value has a non-zero low byte (the public PUD text calls this
field "0 passive, 1 active"; gold mines and oil patches use it for their amount and never get the mark). The mark is
set when the map is loaded. It is not the rescuable mark of rescue-owned units, which doesn't affect any count here.
The unit's type flags decide which army it is in (the PUD flag bits: 0 land, 1 air, 2 explodes, 3 sea, 4 critter, 8
peon, 19 can attack). Only units with at least one of bits 0 to 4 and with bit 19 count. Of those:

| Army | Rule, first match |
| --- | --- |
| Land | None of bits 1 (air), 3 (sea) and 8 (peon) |
| Sea | Bit 3 |
| Air | Bit 1 |

With the default unit data:

| Army | Unit types |
| --- | --- |
| Land | 0x00, 0x01, 0x04 to 0x15, 0x17 to 0x19, 0x2C, 0x2E, 0x2F, 0x31 to 0x35, 0x37 |
| Sea | 0x1E to 0x21 (destroyers, battleships, juggernaughts), 0x26, 0x27 (submarines, turtles) |
| Air | 0x16, 0x23, 0x2A, 0x2B, 0x38 (Kurdan and Sky'ree, Deathwing, gryphon riders, dragons, daemon) |

Workers (peon flag), tankers, transports, flying machines, zeppelins, critters, the eye of Kilrogg and all buildings
(including towers, which have none of bits 0 to 4) are in no army. The attack peasant and attack peon are in the land
army and also count as workers.

A held unit is taken out of the army counts when the map is loaded. The worker count still includes it.

**After a load** the three army counts behave like the training group counts (`game/save-load.md`, "What a saved
game keeps"). Each is restored as saved, and then the load counts every loaded unit again that the army count
includes (complete, not held, not dying), so it is the saved value plus those units: twice the saved value when the
saved count was right. Nothing corrects the surplus: the count rises and falls with the units from there, and a
further save and load adds the loaded units again. The worker count of condition 3 is worked out afresh and comes back
right. So a computer player that loads with a sea army of 6 counts 12, and a "wait until" on condition 5 can then be
true where the same game without the load would still wait.

- **Traced** for the sea army (condition 5: `game/save-load.md` SL34) and the land army (condition 4: SL35, SL36;
  SL37 needs both). Without the surplus each of these traces differs between 51 and 16838 cycles after the load; with it
  (and, for SL37's trace, `orders/orders.md` SW13a) each matches to the end of its three games, through two loads.
- **The air army** (condition 6) follows the same rule by default: a labelled gap-fill, untraced (no traced load had
  a computer player waiting on condition 6 with flyers alive).
- **What was traced is the surplus of units that a training group also counts** (`ai/training-research.md`,
  "Training buildings"). A unit in an army that no training group counts (a demolition squad or sapper, a hero, a
  skeleton, a daemon) is counted again by the same rule: a gap-fill, untraced.

### Slots

Scripts in the shipped file write slots 0x09 to 0x22. The specs that read each slot are named in the table.

| Slot | Use | Known from |
| --- | --- | --- |
| 0x09, 0x0A, 0x0B | Land, sea and air attack-wave triggers (`ai/attack-ai.md`, "Attack waves"). Scripts set 0x09 to 1 just after an army condition is met | Script patterns; the AI work every 50 cycles |
| 0x0C | Switches the ferry's landing-spot search (the attack AI). Set to 1 for every player when the game starts | The AI work every 50 cycles; game start |
| 0x0D, 0x0E | Land army size test: condition 4 compares with their product. Also the land wave's size and repeat count | Conditions; attack waves |
| 0x0F, 0x10 | Sea army size test (condition 5). Also the sea wave's size and repeat count | Conditions; attack waves |
| 0x11, 0x12 | Air army size test (condition 6). Also the air wave's size and repeat count | Conditions; attack waves |
| 0x13 | Wanted workers; also condition 3's threshold | Conditions, training |
| 0x14 | Wanted footmen / grunts | Training |
| 0x15 | Wanted archers / axe throwers (rangers / berserkers once upgraded) | Training |
| 0x16 | Wanted ballistas / catapults | Training |
| 0x17 | Wanted knights / ogres (paladins / ogre-mages once upgraded) | Training |
| 0x18 | Wanted tankers | Training |
| 0x19 | Wanted destroyers | Training |
| 0x1A | Wanted transports | Training |
| 0x1B | Wanted battleships / juggernaughts | Training |
| 0x1C | Wanted submarines / turtles | Training |
| 0x1D | Wanted mages / death knights | Training |
| 0x1E | Wanted flying machines / zeppelins | Training |
| 0x1F | Written by scripts; nothing reads it (static study, round 82) | Script patterns |
| 0x20 | Wanted gryphon riders / dragons | Training |
| 0x21 | Written by scripts; nothing reads it (static study, round 82) | Script patterns |
| 0x22 | **Build-list limit**: the computer's workers only work on build-list entries whose index (from 0) is below this value | Worker AI |

"Wanted" means the training logic compares the slot with the player's count of that unit group. The training rules
(order of preference, random choices, prerequisites) are in `ai/training-research.md`.

### Build list and second list

The build list is up to 64 bytes ending with 0xFF. Values below 0x80 are building types written with the human type
number; the worker AI uses the builder's race (the type's lowest bit) for the actual building. Values from 0x80 up
are research and building-upgrade codes (`ai/training-research.md`, "Research codes"). The second list holds one
think interval per building type (`ai/training-research.md`, "When a building thinks").

## Data

| Item | Value |
| --- | --- |
| Players with a script | 0 to 7 |
| Header | 128 × 16-bit offsets at the start of the file |
| Script record | build-list offset (16-bit), second-list offset (16-bit), instructions |
| Opcodes | 0 set (slot, value), 1 jump (16-bit offset), 2 wait (32-bit count), 3 wait until (condition byte) |
| Wait until cost | wait counter 1 (resumes 2 cycles later) |
| Build-list limit at script start | 0xFF |
| Conditions | 0 to 7 (table above) |

In the shipped file, conditions 1 to 7 all occur (condition 4 most often, about 330 times; condition 7 once) and
condition 0 never does. Wait counts range from 300 to 65535.

## Test vectors

| Input | Expected |
| --- | --- |
| Script 1 (passive) at game start | Runs: jump; sets slot 0x22, slots 0x09 to 0x20 except 0x13 and 0x18, and slot 0x21 to 0; slot 0x13 ← 2; slot 0x18 ← 1; wait 65535. Then, as a computer player, it doesn't run again until 65536 cycles later, where it jumps back to the same wait |
| **AS-V3.** Script 0 (land attack) at a new game, the map gives the computer player 1 peasant | The opening (before the map's units are placed) runs its sets, slot 0x22 ← 3, slot 0x13 ← 1, then "wait until 3": false (0 workers counted), wait counter 1. n = 1 (first start-up cycle): counter → 0. n = 2 (second start-up cycle): "wait until 3" true (1 worker), counter 1. n = 3: counter → 0. n = 4: slot 0x13 ← 9, slot 0x14 ← 3, slot 0x0D ← 3, "wait until 4" |
| Script 0 continuing, land army 2, slot 0x0D = 3, slot 0x0E = 1 | Condition 4 false; tested again every 2 cycles. In the first test with land army ≥ 3 it is true; 2 cycles later: slot 0x0D ← 1, slot 0x09 ← 1, wait 500; the next instruction runs 501 cycles after that |
| Wait 500 run in cycle 1000 | Script runs again in cycle 1501 |
| Wait 0 | No pause: the next instruction runs in the same cycle |
| Human player 0 with script 0 | Script runs at game start until its first pause, and never again |
| Computer player 9 (above 7) | No script |
| Condition 4 with slot 0x0D = 0 | Always true (product 0) |
| Land army counts: 2 footmen, 1 peasant, 1 guard tower, 1 held grunt, 1 flying machine | Land army 2; workers 1; sea 0; air 0 |
| Attack peon owned by *p* | Counts as a worker and in the land army |
| Condition 7, player 1 (computer, no workers of its own) allied with player 2 only; player 2 has peasants; no other player has workers | False |
| Same, but player 3 (not allied) gets a peon | True |
| Condition 7 for player *p* whose own workers are the only workers outside its allies (static study) | False |
| **AS-V1.** `xhuman9-xhuman9-runestone-saveload-aigroups-airesearch-game-400` (oracle trace, Enhanced, `airesearch` extra): computer player 6's opening sets the build-list limit to 29 and ends in "wait until 3" with slot 0x13 = 3; the map gives it more workers than that. Saved after n = 1704 and loaded | New game: limit 29 in records n = 0 to 3, 37 from n = 4 (the sets after the "wait until"). Loaded game: 37 in its setup record and every record after it (no opening runs) |
| **AS-V2.** `orc10-aigroups-airesearch-12` (oracle trace, Enhanced, no input): computer player 1's opening sets the limit to 0 and ends in "wait until 3" with slot 0x13 = 4; the map gives it 4 peasants | Limit 0 in records n = 0 to 3, 255 from n = 4 |
| `human11-30000` (oracle trace, Enhanced): player 4's script waits on condition 7; player 3 (passive, allied with player 4) has 6 peasants, player 1 has no peons and player 6 no workers | Never true: the script doesn't pass it, and at n = 22113 player 4's gold is still 6020 (not 5420) |

## Open questions

- Whether a script number above 127 (a map's `AIPL` byte) is refused somewhere else or reads past the header.
- Writing slots 0 to 8 or 0x23 and above would overwrite the wait counter, position or list offsets. No shipped
  script does this; what a reimplementation should do with such a script is open.
- **An opcode above 3 or a condition above 7** has no defined behaviour (static study, round 82). No shipped script
  has one. Default: treat a script file with one as invalid and refuse it.
- Whether anything clears the held mark after map load. None was found. Default: the mark stays for the unit's life.
- **The air army after a load** (round 106, SL1). Untraced: no traced load had a computer player waiting on condition
  6 with flyers alive. **Default:** doubled like the land and sea armies ("After a load").
- **Army units outside the training groups after a load** (round 106, SL1). The traced surplus is the units the
  training groups also count; whether a unit in an army but in no group (a demolition squad, a hero, a skeleton, a
  daemon) is counted again is untraced. **Default:** it is, like every unit the army count includes.
