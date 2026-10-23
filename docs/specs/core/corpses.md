# Core: corpses, wrecks and ruins

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`) and the animation data in
  `Rez\script.bin` (decoded with the instruction set of `core/animation-scripts.md`). Not yet confirmed against the
  running game.
- **Changes:** 2026-10-12, round-35 errata: bodies and ruins keep the dead unit's mana byte (oracle traces, static study
  for non-zero values).
- **Changes:** 2026-10-12, round-36 errata: the static dead-mage vector is replaced by a traced one (no rule change).
- **Changes:** 2026-10-12, round-37 errata: the dead-mage vector names the folder its trace is in.
- **Changes:** 2026-10-12, round-41 errata: an oil patch under a new oil platform gets the used mark and is killed,
  leaving no ruin (`orc09-naval` n = 268, slot released at n = 781).
- **Changes:** 2026-10-12, round-43 errata: a ship wreck shows frame 0 from the moment its order becomes "remove";
  bodies and ruins keep their last frame; `orc09-naval` vectors.
- **Changes:** 2026-10-13, round-54 errata: a water ruin is chosen by the type's naval movement class (or the shore
  building flag), not the layer; every ruin shows frame 0 in the cycle it is made; `human12-25000` vector.
- **Changes:** 2026-10-22, round 96 (oracle traces, `ERRATA-round96-traces.md`; reviewed by a spec-review session,
  2026-10-22): a dead body keeps the dead unit's target until a load, which clears it (`game/save-load.md`, SL28,
  SL29).
- **Changes:** 2026-10-22, round 104 (oracle sweep, `ERRATA-round104-sweep.md` SW5 and SW8; reviewed by a spec-review
  session, 2026-10-22): "The used mark": a building created during play marks the dying units it overlaps or touches
  (SW8a to SW8e); every unit with the mark is hidden from everyone (`vision/fog.md`).
- **Changes:** 2026-10-22, round 106 (oracle traces, `ERRATA-round106-sweep.md` SW8; reviewed by a spec-review
  session, 2026-10-22, see its "Review"): a site that only touches a ruin, and one with a free tile between it and a
  dead body, mark it, and a body marked so gives raise dead no skeleton: all three traced (SW8f to SW8h, replacing the
  two static vectors). A used-mark unit applies no sight at a fog refresh (`vision/fog.md`, SW12a). No rule changed.

## Behaviour

When a unit or building dies it first plays its own **death animation** (animation 1 of its type). When that
animation signals "step done" (`core/animation-scripts.md`), the dead unit is turned into a **remains** unit in the
same unit slot (`core/unit-slots.md`), or removed. The remains then decay through a fixed series of stages, driven by
their own animation, and are removed at the end.

### What a dead unit becomes

| Dead unit | Becomes |
| --- | --- |
| A unit whose type has a corpse kind (table below) | A **dead body** (PUD type 0x69) with that kind |
| A unit with no corpse kind | Nothing: removed |
| A unit whose type has flag bit 10 (labelled "transport" in the PUD specification) standing on shore water (below) at its death step done | Nothing: removed. In the default unit data only the two transports (types 0x1C and 0x1D) have this flag; other ships on shore water leave a wreck |
| A unit carrying the **used** mark (below), for example a polymorph victim | A dead body as above, which **keeps** the used mark |
| A building that dies normally | A **ruin**, whose type depends on the building's width (table below) |
| An oil platform that still has oil | Nothing: removed, no ruin. The oil patch was already created when it was killed (below) |
| A building carrying the used mark | Nothing: removed |

**Corpse kinds** by PUD unit type:

| Kind | Meaning | Unit types |
| --- | --- | --- |
| 1 | Human body | footman, peasant, knight, archer, paladin, attack peasant, ranger, Alleria, Turalyon, Danath |
| 2 | Orc body | grunt, peon, ogre, axe thrower, ogre-mage, attack peon, berserker, Grom Hellscream, Khorgath Bladefist |
| 3 | Ship wreck | tankers, transports, destroyers, battleships and juggernaughts, submarines and turtles (types 0x1A to 0x22 and 0x24 to 0x27) |
| none | No remains | ballista, catapult, mage, death knight, dwarven demolition squad, goblin sapper, Teron Gorefiend, Kurdan and Sky'ree, Dentarg, Khadgar, Deathwing, flying machine, zeppelin, gryphon rider, dragon, eye of Kilrogg, and every type from 0x30 to 0x39 (Cho'gall, Lothar, Gul'dan, Uther, Zuljin, skeleton, daemon, critter and the unused numbers) |

**Shore water** means that the tile at the unit's position (its tile coordinates, the same tile `movement/occupancy.md`
uses for the unit) has the shore-water flag (0x02) in the tile flags of `movement/occupancy.md`.

**The used mark** is one state mark that several things set:

- a unit inside a transport, a building or a mine carries it (it is the "inside something" mark);
- polymorph sets it on its victim just before killing it;
- raise dead sets it on each body it raises a skeleton from;
- an oil platform with oil left gets it when it is killed;
- an oil patch gets it when an oil platform is placed on it, just before it is killed (`economy/build-train.md`), so
  it leaves no ruin: its order becomes "remove" in the same cycle and its slot is released 513 cycles later;
- a transport that dies on shore water gets it at its death step done, at the same moment it is removed. Because
  it is removed at once, the mark has no further effect there;
- a **dying** unit (in its death animation, a body or a ruin; not already dead, removed or marked) gets it when a
  **building is created during play** whose area overlaps or touches the dying unit's area. A dead body (type 0x69)
  counts one tile larger on every side. "A building" is a construction site or any other created unit of types 0x3A to
  0x68 except the oil patch (0x5D) and type 0x64; nothing is marked while a saved game is being loaded. The marked
  unit keeps its death animation and timers, and a body marked this way can't be raised. Traced for 3 × 3 ruins
  overlapped by new 3 × 3 sites (SW8a to SW8d), a 2 × 2 ruin that a 2 × 2 site only touches (SW8f), a body with one
  free tile between it and a new 2 × 2 site (the body's wider area, SW8g) and raise dead skipping that body (SW8h); the
  other building types are static study only. A ruin two tiles from a new site isn't marked (traced: `sweep-mp-gold-rush-bne-o2`
  n = 5550, SW8e).

Every unit with the used mark is hidden from everyone (`vision/fog.md`, "Unit visibility (phase 5)", rule 1), so from
the cycle it gets the mark a dying unit drops out of every player's view.

**Passengers.** When a transport is killed, every unit inside it (carrying the used mark and recorded as being in that
transport) is killed at the same moment, at the kill and not at the death step done, whether or not the transport is
on shore water. What a passenger leaves afterwards is an open question (below).

**Units with the explode flag.** A unit whose type has flag bit 2 ("explode when killed" in the PUD specification) is
killed the ordinary way. The flag only adds a cosmetic explosion effect (effect kind 0x17) at the moment of the kill,
centred 8 px above the unit's centre. It deals no damage, makes no draw, and the effect is removed at its first step;
it only takes an effect slot for that time. In the default unit data only the ballista and the catapult (types 0x04
and 0x05) have this flag. Demolition squads and sappers don't have it, and their blast belongs to their own attack, not
to their death. Type 0x29 gets the same cosmetic effect. Both rules apply to a passenger exactly as to any other unit,
so an exploding passenger dies like the others and only adds its effect. One difference concerns the transport
itself: the passengers are killed only when the dying transport has neither the explode flag nor type 0x29. In the
default data no transport has either, so passengers always die with their transport. Type 0x29 can't be a passenger
in the default data: it is an air unit, and only land units board transports (`orders/transports.md`).

For remains it means "can't be raised". It doesn't change decay: a used body keeps its stage timers, shows the same
frames, turns neutral and is removed exactly like any other body. A building that dies with the mark leaves no ruin;
a unit that dies with it still leaves a body, which carries the mark (so a polymorph victim's body can't be raised).

**Oil platform killed with oil left:** at the moment it is killed (not when its death animation ends), an oil patch
(type 0x5D) is created for the neutral player (15) at the platform's position, and the patch's oil is set to the
platform's remaining oil. The platform gets the used mark, so it leaves no ruin.

**Ruin type** by the building's width in tiles: width 1 → type 0x6A, 2 → 0x6B, 3 → 0x6C, 4 → 0x6D (width 0 would give
0x6A). A ruin is a **water ruin** (kind 5) when the building's type has the naval movement class (class 2) in the unit
data or the shore building flag (bit 16); otherwise it is a **land ruin** (kind 4). The layer the building stands on
doesn't matter: the oil platform has the naval movement class, so a platform that leaves a ruin (one with no oil
left) leaves a water ruin (traced: `human12-25000`, vectors below).

When a unit becomes a dead body:

- its sprite facing is kept, except for the ogre and the ogre-mage, whose facing is remapped: N, NE and E become SW;
  SE and S become W; SW becomes NE; W and NW become SE (facings 0–7 become 5, 5, 5, 7, 7, 1, 3, 3);
- it is deselected;
- its target is kept (`core/unit-slots.md`, "Death, corpses and release"): the body goes on reporting it, even after
  that slot is released (`oracle/trace-format.md`, unit `target`), until the body's own slot is released or a game is
  loaded: after a load every dead body has no target (`game/save-load.md`, SL28, SL29; a ship wreck the same as a
  human or orc body);
- it starts the decay animation for its kind.

A ruin is marked complete, its facing is set to 0, it is deselected and it starts the decay animation for its kind.

Neither change touches the mana byte or its counter: the body or ruin keeps the value the unit had at the end of its
death animation. The timers pass skips dying units and remains, so a dead caster's mana doesn't grow.

### Random draws and creation numbers

- Turning a dead unit into a body, or a dead building into a ruin, makes **no draws** and takes **no new creation
  number**: the remains are the same unit in the same slot (`core/unit-slots.md`), with a new type.
- Removing remains or a dead unit makes no draws.
- The **oil patch** from a killed oil platform is a newly created unit. It takes a new slot and the next creation
  number, and it makes the creation draws of a building (`core/timing-and-rng.md`): first the stored 0 to 5 value,
  then the extra first wait, **2 draws**, at the moment the platform is killed, in the phase where the kill happens.

### When the first stage runs

- **Bodies and wrecks:** the decay animation starts and its first instruction runs **in the same cycle** as the
  death animation's "step done". So the first frame and the first wait begin in that cycle.
- **Ruins:** the decay animation is started with an ordinary start (`core/animation-scripts.md`), so its first
  instruction runs on the **next** cycle. Every ruin, land or water, shows **frame 0** in the cycle it is made; its
  first stage's frame comes with that first instruction (traced: `human12-25000`, vectors below).

### Decay stages

All remains types share one animation table: the animation number is the kind. Times are cycles after the remains'
first stage runs (see above). "Neutral" means the remains change owner to the neutral player (15) at that point. At
the last time the animation signals "step done", and the remains' order becomes **"remove"** (they are moved to the
neutral owner if they aren't already).

**Slot release:** the "remove" order releases the slot (`core/unit-slots.md`) at the unit's next "step done". Every
unit type's "remove" animation (animation 0) is: halt, halt, step done. Each halt holds for 256 cycles
(`core/animation-scripts.md`), and the animation starts with an ordinary start. So the slot is released **513
cycles** after the order becomes "remove": 1 cycle to reach the first halt, then 256 + 256. The same 513 cycles apply
to a dead unit that leaves no remains, counted from its death animation's "step done". The "Remove order at" column
below is the time the order changes; the slot is released 513 cycles later.

| Kind | Stages (time: frame shown) | Neutral at | Remove order at |
| --- | --- | --- | --- |
| 1 Human body | 0: frame 0; 200: frame 10; 400: frame 15; 600: frame 20; 800: frame 25 | 200 | 800 |
| 2 Orc body | 0: frame 5; 200: frame 10; 400: frame 15; 600: frame 20; 800: frame 25 | 200 | 800 |
| 3 Ship wreck | 0: frame 30; 200: frame 0 (shown from the "remove" order on; see below) | 100 | 200 |
| 4 Land ruin | 0: frame 0; 200: frame 1 | 200 | 400 |
| 5 Water ruin | 0: frame 2; 200: frame 3 | 100 | 400 |

The waits are plain waits: haste and slow don't change them. Human and orc bodies share their later frames (the
bones stages).

**A ship wreck's last frame.** When a ship wreck's order becomes "remove", its frame becomes **0** in the same cycle and
stays 0 until the slot is released. Bodies keep frame 25 and ruins keep their last frame (1 or 3). The cause: the
wreck's decay animation is number 3, the same number as the move animation. At its last "step done" the unit is treated
as switching back to its own animation (`core/animation-scripts.md`, "Starting an animation", the last row), so the
remains' animation restarts and its first instruction (frame 0) runs at once, before the order becomes "remove". The
body and ruin animations (1, 2, 4 and 5) don't take this path.

## Data

| Item | Value |
| --- | --- |
| Dead body type | 0x69 |
| Ruin types | 0x6A to 0x6D, by building width 1 to 4 |
| Remains animation numbers | 1 to 5 (the kind) |
| Slot release after the "remove" order | 513 cycles |
| Draws when remains are made | 0 |
| Draws when an oil patch is made from a killed platform | 2 (stored 0 to 5 value, then first wait) |
| Ogre facing remap | 0→5, 1→5, 2→5, 3→7, 4→7, 5→1, 6→3, 7→3 |

## Test vectors

Cycle numbers count updates of the unit: update 1 is its first update after it is killed.

| Input | Expected |
| --- | --- |
| Footman killed | Death animation: frame 45 (update 1), frame 50 (4), frame 55 (7), step done at update 107. Becomes a human body in the same update 107 (frame 0); neutral and frame 10 at 307; frames 15, 20 at 507, 707; frame 25 and "remove" order at 907; slot released at 1420 (907 + 513) |
| Same footman, draws | turning it into a body and every decay stage make no draws, and it keeps its creation number |
| Grunt killed | Same timing with frame 5 for the fresh body |
| Ogre facing E killed | Body facing SW (5) |
| Destroyer killed on open water | Death animation (frame 5, 50 cycles; frame 10, 50 cycles), then a wreck in the same cycle as its step done: frame 30, neutral after 100, "remove" order after 200, slot released after 713 |
| Transport killed while its tile has the shore-water flag | No wreck; "remove" order at its death step done; slot released 513 cycles later |
| Destroyer killed while its tile has the shore-water flag | A wreck, exactly as on open water (the destroyer type doesn't have flag bit 10) |
| Transport carrying 2 footmen killed on open water | Both footmen are killed at the same moment as the transport; the transport leaves a wreck |
| Mage killed, death step done at cycle D | No body; "remove" order at D; slot released at D + 513 |
| Farm (2 × 2) destroyed, death step done at cycle D | Ruin type 0x6B, land ruin: frame 0 at D + 1; frame 1 and neutral at D + 201; "remove" order at D + 401; slot released at D + 914 |
| Oil platform destroyed with 5000 oil left | An oil patch with 5000 oil appears at once, taking the next creation number; 2 draws are made at the kill; the platform plays its death animation and leaves no ruin |
| `orc09-naval-1500` (oracle trace), n = 794: wreck 1506 reaches its last stage | Order 0, animation 0, frame 0, wait 1 (frame 30 until n = 793) |
| Same trace, n = 894 (wreck 1497) and n = 1255 (wreck 1511) | Order 0, frame 0 |
| `orc02-assault-3000` (oracle trace): the 12 bodies and the ruin that reach their last stage | Bodies frame 25, the ruin frame 1 |
| Shipyard (shore building, 3 × 3) destroyed | Ruin type 0x6C, water ruin: frame 2 (first stage the cycle after the death step done); neutral after 100; frame 3 after 200; "remove" order after 400 |
| `human12-25000` (oracle trace): oil platform 1530 (type 0x57, owner 0) at (33, 27) collapses at n = 12948 | n = 12948: ruin type 0x6C, order 1, animation 5 (water ruin), frame 0, wait 1; n = 12949: frame 2, wait 100 |
| Polymorphed footman | Its death animation plays, then a human body with the used mark; it decays on the normal timetable and can't be raised |
| Body raised by raise dead | Gets the used mark; keeps decaying and is removed on its normal timetable |
| A slowed footman dies | The body's decay timing is unchanged (plain waits) |
| `orc02-assault-3000` (oracle trace), slot 1588 (type 0x40), destroyed | Ruin type 0x6B at n = 843, mana 0 |
| 30 conversions to bodies and ruins in the `orc02-assault`, `orc02-victory` and `human06-attack-wave` traces | Mana 0 before and after every one |
| `research/oracle-round36/xhuman10-r36a-700.jsonl.gz` (oracle trace): player 1's mage 1481 (type 0x0A, no corpse kind) has 89 mana; it is killed and is in order 1 from n = 197 to n = 232 | Mana 89 throughout; no gain at n = 200. From n = 233 to n = 700 the slot shows owner 15, order 0, animation 0 and mana 89 |
| SW8a: `sweep-mp-gold-rush-bne-o2` (oracle sweep trace, Enhanced, Gold Rush BNE through the multiplayer launch with two computer players; kept outside the repository, reproducible with `oracle/sweep.py capture`), n = 10478: gold mine 1549 at (12, 43) collapsed at n = 10198 and is a 3 × 3 ruin (type 0x6C, order 1, animation 4, wait 121), hidden-from set 246. Peon 1509 (owner 3) starts a 3 × 3 site, 1567 (type 0x3D), at (11, 43) | Ruin 1549: the used mark, so hidden-from set **255** from n = 10478 on until it is removed at n = 10599; its animation, waits and removal time don't change |
| SW8b: same trace, n = 12973: ruin 1569 (type 0x6C) at (9, 18); site 1568 (type 0x4C, owner 0) at (11, 17) shares column 11 with it | Ruin 1569: hidden-from set 255 from n = 12973 |
| SW8c: same trace, n = 18377: ruin 1533 at (18, 49); site 1498 (type 0x53, owner 3) at (17, 49) | Ruin 1533: 255 from n = 18377 |
| SW8d: same trace, n = 19225: ruin 1592 at (18, 9); site 1533 (type 0x3C, owner 0) at (17, 11) | Ruin 1592: 255 from n = 19225 |
| SW8e: same trace, n = 5550: a 2 × 2 site (type 0x3B) at (7, 47) is started; ruin 1547 (type 0x6C, 3 × 3) at (6, 43), hidden-from set 246, ends at row 45, so row 46 lies between them | Ruin 1547 isn't marked: hidden-from set still 246 |
| SW8f: `mp-r106-sw58-mp-r106-sw5-ruin-polymorph-fixed-game-goals-vision-5400` (oracle trace, Enhanced, generated map `R106-sw58.pud`, `EXTRAS=game,goals,vision`): farm ruin 1589 (type 0x6B, 2 × 2) at (16, 44) from n = 471, neutral from n = 672, hidden-from set 254 from n = 701. At n = 735 the local player's farm site 1583 (type 0x3A) appears at (18, 44), touching the ruin with no tile between | Ruin 1589: hidden-from set **255** from n = 735 (touching counts); removed (order 0) at n = 872 on its normal timetable |
| SW8g: `mp-r106-sw8b-mp-r106-sw8-bodies-fixed-game-goals-vision-1500` (oracle trace, Enhanced, generated map `R106-sw8b.pud`, orc local player, `EXTRAS=game,goals,vision`): footman bodies (type 0x69) 1594 at (36, 30) from n = 339 and 1593 at (36, 40) from n = 390, hidden-from set 252. At n = 521 a pig farm site 1588 (type 0x3B, 2 × 2) appears at (38, 30), one free tile (column 37) from 1594; at n = 529 site 1587 at (39, 40), two free tiles from 1593 | Body 1594: hidden-from set **255** from n = 521 (a body counts one tile larger). Body 1593: not marked (252; neutral from n = 590, 254 from n = 601) |
| SW8h: same trace: death knight 1599 (mana 109) casts raise dead at (36, 35) (command at n = 700); both bodies are at squared distance 25 | n = 983: one skeleton (1586, type 0x37) at (36, 40) from body 1593, which gets the used mark (hidden-from set 255); mana 109 → 59. Body 1594 gives none, although the mana left (59) would pay for a second (`spells/spells.md`, raise dead's scan) |

## Open questions

- What a passenger killed with its transport leaves. It isn't on the map while inside, and whether its death
  animation runs there hasn't been traced. Default: it follows the unit rule (a body with the used mark, at its
  recorded position), which only matters if raise dead or drawing could see it.
- Whether a used body (a polymorph victim's, a raised one, or a passenger's) is drawn. The mark is the one that
  keeps passengers out of sight, so it may be hidden. Default: drawn like any body, since drawing has no game-state
  effect.
- Whether the oil patch creation can fail its placement check while the dying platform still stands on the tile.
  Default: it succeeds, and an oil patch always appears.
- Confirm the 513-cycle slot release in the running game, for example by counting how long a destroyed unit's slot
  stays taken (a unit created 400 cycles after a body's last stage should not reuse its slot).
- What the remains do in the game while they exist (blocking, targeting by spells such as raise dead) is covered by
  the occupancy and spell specs, not here.
