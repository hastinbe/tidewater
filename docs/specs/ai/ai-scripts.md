# AI: computer player scripts

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the AI script file
  `Rez\ai.bin` in the game data (decoded with the format below), the unit flags in the game's unit data and the AI
  list and `AIPL` section in the public PUD specification text. Not yet confirmed against the running game.

## Behaviour

Each player has an **AI script**: a small program, taken from the AI script file, that runs once per cycle for every
computer-controlled player. A script does not move units or train anything itself. It only writes numbers into a
per-player set of **slots** (wanted unit counts, how far down the build list to go, attack settings) and waits on
simple conditions. The rest of the computer player (training, building, attacking; not covered by this spec) reads
those slots.

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

So every player's script has already run its opening instructions before the first cycle. For non-computer players
the script then never runs again.

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

Multi-byte values are little-endian.

**Timing that follows from this:**

- A wait of *N* (*N* ≥ 1) run in cycle *t* lets the script resume in cycle *t* + *N* + 1: the counter goes down in
  cycles *t* + 1 to *t* + *N*, and in cycle *t* + *N* + 1 it is 0 and the script runs. A wait of 0 doesn't pause.
- A **wait until** always costs 2 cycles. When its condition is true in cycle *t*, the script goes on in cycle
  *t* + 2. When false, the condition is tested again in cycle *t* + 2, and so on every 2 cycles until it is true.

### Conditions

*p* is the player running the script. "Counts" are the player's live unit counts (`game/victory.md`, "Unit counts").

| Condition | True when |
| --- | --- |
| 0 | *p* has at least one complete shipyard (either race) |
| 1 | *p* has at least one complete keep or stronghold |
| 2 | *p* has at least one complete castle or fortress |
| 3 | *p*'s **workers** ≥ slot 0x13 |
| 4 | *p*'s **land army** ≥ slot 0x0D × slot 0x0E |
| 5 | *p*'s **sea army** ≥ slot 0x0F × slot 0x10 |
| 6 | *p*'s **air army** ≥ slot 0x11 × slot 0x12 |
| 7 | Some player from 0 to 7 that *p* does not count as allied has at least one worker |

Comparisons are unsigned. The products are of two bytes, so up to 65025.

**Workers** are units of types 0x02, 0x03, 0x10 and 0x11 (peasant, peon and both attack versions). The count includes
rescuable units.

**Army** counts are kept per player for every unit that exists, is complete and is **not rescuable**. The unit's type
flags decide which army it is in (the PUD flag bits: 0 land, 1 air, 2 explodes, 3 sea, 4 critter, 8 peon, 19 can
attack). Only units with at least one of bits 0 to 4 and with bit 19 count. Of those:

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

A unit is taken out of the army counts when it is made rescuable at map load. (That a rescued unit joins them
again is assumed, not traced.)

### Slots

Scripts in the shipped file write slots 0x09 to 0x22. What the rest of the AI does with them is not covered here;
this table records what is known so far.

| Slot | Use | Known from |
| --- | --- | --- |
| 0x09 to 0x0C | Attack control (not yet traced). Scripts set 0x09 to 1 just after an army condition is met | Script patterns |
| 0x0D, 0x0E | Land army size test: condition 4 compares with their product | Conditions |
| 0x0F, 0x10 | Sea army size test (condition 5) | Conditions |
| 0x11, 0x12 | Air army size test (condition 6) | Conditions |
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
| 0x1F | Not traced (likely demolition squads / sappers) | Script patterns |
| 0x20 | Wanted gryphon riders / dragons | Training |
| 0x21 | Not traced | Script patterns |
| 0x22 | **Build-list limit**: the computer's workers only work on build-list entries whose index (from 0) is below this value | Worker AI |

"Wanted" means the training logic compares the slot with the player's count of that unit group. The exact training
rules (order of preference, random choices, prerequisites) are for a later spec.

### Build list and second list

The build list is up to 64 bytes ending with 0xFF. Values below 0x80 are building types written with the human type
number; the worker AI uses the builder's race (the type's lowest bit) for the actual building. Values from 0x80 up
are not buildings (they look like upgrades and research); their meaning is for a later spec. The second list's
format is not yet known.

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
| Script 0 (land attack) at game start, player has 1 peasant counted | Runs its opening sets, slot 0x22 ← 3, slot 0x13 ← 1, then "wait until 3": true, wait counter 1. First cycle: counter → 0. Second cycle: slot 0x13 ← 9, slot 0x14 ← 3, slot 0x0D ← 3, "wait until 4" |
| Script 0 continuing, land army 2, slot 0x0D = 3, slot 0x0E = 1 | Condition 4 false; tested again every 2 cycles. In the first test with land army ≥ 3 it is true; 2 cycles later: slot 0x0D ← 1, slot 0x09 ← 1, wait 500; the next instruction runs 501 cycles after that |
| Wait 500 run in cycle 1000 | Script runs again in cycle 1501 |
| Wait 0 | No pause: the next instruction runs in the same cycle |
| Human player 0 with script 0 | Script runs at game start until its first pause, and never again |
| Computer player 9 (above 7) | No script |
| Condition 4 with slot 0x0D = 0 | Always true (product 0) |
| Land army counts: 2 footmen, 1 peasant, 1 guard tower, 1 rescuable grunt, 1 flying machine | Land army 2; workers 1; sea 0; air 0 |
| Attack peon owned by *p* | Counts as a worker and in the land army |
| Condition 7, player 1 (computer, no workers of its own) allied with player 2 only; player 2 has peasants; no other player has workers | False |
| Same, but player 3 (not allied) gets a peon | True |

## Open questions

- Whether a script number above 127 (a map's `AIPL` byte) is refused somewhere else or reads past the header.
- At game start the unit counts may still be zero when the scripts first run (the counts appear to be reset just
  before), so an opening "wait until 3" may be false at that moment and pass two cycles later instead. The vector for
  script 0 assumes the units are counted first; to check against the running game.
- Whether a player counts as allied with itself for condition 7 (it doesn't matter for the condition: a player that
  counts itself as allied skips itself, otherwise it tests its own workers). The evidence suggests the alliance
  table includes the player itself.
- Writing slots 0 to 8 or 0x23 and above would overwrite the wait counter, position or list offsets. No shipped
  script does this; what a reimplementation should do with such a script is open.
- The meaning of slots 0x09 to 0x0C, 0x1F and 0x21, the second list, and build-list values from 0x80 up.
- The training, building and attack logic that reads the slots, and the AI work run every 50 cycles in the same
  phase (starting with the first cycle), are not yet specified.
