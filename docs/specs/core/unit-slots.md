# Core: unit slots, creation order and slot reuse

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`) and the unit data
  `Rez\unitdata.dat` in the game data. Not yet confirmed against the running game, except the creation hit points
  (oracle traces, round 31).
- **Changes:** 2026-10-11, round-8 errata: only flag-bit-10 units (the transports), not all ships, get no corpse on
  shore water.
- **Changes:** 2026-10-12, round-20 errata: the three permanent effects are never freed, so effects made during play
  take slot 3 or higher.
- **Changes:** 2026-10-12: the open question on what the three permanent effects show is closed by
  `ui/map-drawing.md`: slot 0 is the light building fire, slot 1 the heavy building fire and slot 2 the ground marker.
- **Changes:** 2026-10-12, round-31 errata: a new unit's hit points are its type's value, or 1 if that is 0; the AI's
  first-goal rule runs during creation, before the extra first-wait draw.
- **Changes:** 2026-10-12, round-33 errata: a new unit's facing is 0 (only mobile units then draw one) and its mana
  byte starts at its type's "magic" value.
- **Changes:** 2026-10-12, round-35 errata: a new unit starts hidden from every player until the next visibility pass,
  so no target search finds it before then; bodies and ruins keep the mana byte (open question narrowed).
- **Changes:** 2026-10-12, round-39 errata: no kill, by damage or outright, changes a unit's HP; `human12-demolition`
  vector.
- **Changes:** 2026-10-12, round-40 errata: a killed building makes a kind-0x17 explosion at its centre, 8 px up, in the
  lowest free effect slot (not walls or a building with the used mark); `human5` and `xhuman2` vectors.
- **Changes:** 2026-10-13, round-50 errata: a dying unit keeps the target recorded when it dies, which may be none.
- **Changes:** 2026-10-21 (B-040 game audit erratum 1 and round-90 traces; reviewed by a spec-review session,
  2026-10-21): step 6's goal step also runs when a unit that isn't held is converted (Enhanced); a load renumbers
  every unit's creation number (`game/save-load.md`).
- **Changes:** 2026-10-21, round 92 (oracle traces, `ERRATA-round92-traces.md`; reviewed by a spec-review session,
  2026-10-21): after a load the creation counter goes on from the unit-slot count plus 26 (1626 in Enhanced, traced).
- **Changes:** 2026-10-22, round 96 (oracle traces, `ERRATA-round96-traces.md`; reviewed by a spec-review session,
  2026-10-22): a load clears a dead body's target (`game/save-load.md`, SL28 to SL30).
- **Changes:** 2026-10-22, round 102 (oracle trace and static study, `ERRATA-round102-traces.md` sections 3 and 4;
  reviewed by a spec-review session, 2026-10-22): "Creating a unit", step 4 corrected: creation aligns every unit to
  the 32-pixel tile grid only, so ships and flyers placed by a map keep odd columns and rows (traced, FL1; the
  64-pixel alignment for ships and flyers is withdrawn). In Enhanced every kill tidies every player's control groups;
  the release clears the creation number and the local interface's references; when P drops on a release (static
  study).

## Behaviour

Every unit, building, corpse and ruin lives in one fixed-size **unit table**. Missiles and visual effects live in a
second, smaller **effect table**. The update order of each cycle, and so the order of random draws (see
`timing-and-rng.md`), follows slot order in these tables. A reimplementation that wants identical results must assign
slots exactly as described here.

### Table sizes

| Mode | Unit slots | Effect slots |
| --- | --- | --- |
| Classic | 600 | 200 |
| Enhanced | 1600 | 400 |

### Owner lists

Every unit slot belongs to exactly one owner list at all times.

- **Owners:** there are 16 owner values. 0 to 7 are the players. 15 is neutral. 14 is the **free list**, which holds
  every unused slot. 8 to 13 are not used.
- **List order:** a slot joining a list always goes to the **front**. Each list is therefore ordered from most
  recently added to least recently added.
- **Counts:** the game keeps a count for each list.

### Starting a map

1. Every slot is marked unused and given owner 14.
2. The slots are added to the free list in slot order 0, 1, 2, … , N − 1. Because each goes to the front, the free
   list starts with slot N − 1 and ends with slot 0.
3. The effect table is cleared. Three permanent effect entries are created at once, so they take effect slots 0, 1
   and 2. None of the three is ever freed: two loop forever, and the third is only ever replaced in place. So effects
   made during play always take slot 3 or higher. Slots 0 and 1 are the light and heavy building fires, and slot 2 is
   the ground marker (`ui/map-drawing.md`).
4. The map's units are created in the order the map file lists them (see "Creating a unit").
   - Units of an owner that is not playing are skipped.
   - Start-location markers are normally not created (they set the starting view instead).
   - Each unit's type is first converted to its owner's race.

### Creating a unit

1. **Per-player cap:**
   - Let P be the number of players (owners 0 to 7) with at least one slot in their list. The cap is N if P ≤ 1, and
     otherwise `floor(N / P)`.
   - A request for owner o (other than neutral, 15) fails if o's list already holds at least the cap.
   - Neutral units are never capped.
2. **Free slot:** the request fails if the free list is empty. Otherwise the game takes the slot at the **front** of
   the free list and moves it to the front of the owner's list. The count changes and the cap is recalculated
   (step 1).
3. **Reset:** the slot's contents are cleared, except its list link and its fixed slot identity. The unit then gets
   the next **creation number**, a counter that starts at 1 when the map starts and goes up by 1 per created unit.
   The counter isn't reused. A loaded game renumbers its units: each one's creation number becomes its slot number
   plus one (traced, round 90; `game/save-load.md`), and the counter goes on from the unit-slot count plus 26: 1626 in
   Enhanced (traced, round 92, `game/save-load.md` SL7; the Classic value 626 is a gap-fill). The new unit's hit points are set to its type's hit points from the unit data, or to 1 if that
   value is 0 (in the default data: types 0x22, 0x30, 0x36, 0x5D oil patch, 0x5E, 0x5F and 0x64). A unit under
   construction starts from this value too; `economy/build-train.md` then sets its hit points as it builds.
   Its facing is 0, and its mana byte is its type's "magic" value from the unit data (in the default data 1 for
   types 0x0A, 0x0B, 0x15, 0x17, 0x18 and 0x33, and 0 for every other type, the paladin and ogre-mage included).
   The decay and caster rules of `spells/spells.md` then apply: a unit with a decay rate created during play gets
   255, and a caster that isn't a building gets 85. The unit starts **hidden from every player**, computers included
   (the hidden-from set of `vision/fog.md`); the next visibility pass (phase 5) recomputes that, so until then no
   target search counts it as a candidate. Only mobile units then draw a facing (step 5); buildings and
   other units that aren't mobile keep facing 0, and nothing in normal play changes it. So in the default data a
   unit that can't cast has mana 0 unless it is a summoned unit, a berserker with regeneration
   (`spells/spells.md`) or a building under construction or working on a job (`economy/build-train.md`).
4. **Placement:**
   - The position is aligned to the tile grid, 32 pixels, for **every** unit, ships and flyers included: creation
     never moves a unit to an even tile, so a ship or flyer that a map places at an odd column or odd row stays there
     (traced: FL1). The one exception is a map whose unit data gives **type 0** the sea-unit flag (bit 3 of the PUD
     unit flags): creation then aligns every unit it creates, whatever its type, to 64 pixels (an even tile), because
     the choice is made from type 0's flags, not the new unit's (static study; the default data and every shipped map
     leave the flag clear; FL2). Units created during play stand where their caller puts them: trained units and an
     eye of Kilrogg on a tile of the ring search of `movement/placement.md`, whose air and sea tests take only even
     tiles; a critter from polymorph, a skeleton from raise dead and an oil patch from a killed platform on their
     source's tile. None of these is a ship or flyer on an odd tile, so nothing created during play changes.
   - If the placement check fails, the slot goes back to the free list (see "Death, corpses and release"), and creation fails
     with the "can't place" message.
   - A failed cap or empty free list shows the "too many units" message instead.
5. **Random draws:** these happen as listed in `timing-and-rng.md` (facing for mobile units, a stored 0 to 5 value for
   buildings, then the extra first wait for all).
6. **First goal:** after the unit's first order is set and before the extra first-wait draw, the unit's AI goal is
   cleared and the first-goal rule of `ai/attack-ai.md` ("Unit goals") runs. This applies to every created unit,
   the map's units included. Its only draws (a land unit's post, off-map case) therefore come before the wait draw.
   The same goal step runs again, without the other steps, when a unit that isn't held is converted to another type
   in Enhanced (`economy/tech-tree.md`, "Converting units"; `ai/attack-ai.md`, "When a unit is converted").

On a fresh map, therefore:

- The first unit created takes slot N − 1, the second N − 2, and so on.
- The **update order** each cycle, which is ascending slot order, is the **reverse of creation order**.

### Updating

Each cycle the unit table is walked from slot 0 to slot N − 1, and unused slots are skipped. Then the effect table is
walked from slot 0 upward in the same way (see `timing-and-rng.md` for what each update does).

### Death, corpses and release

When a unit is killed, it keeps its slot. The game:

- clears the unit off the occupancy marks;
- releases any tree it had reserved;
- kills any units inside it (transports, and buildings holding units);
- sets its order to "dying", which plays its death animation;
- right after that, under **Enhanced** rules, tidies the control groups of every player
  (`ui/selecting-and-commanding.md`, "Control groups", the tidy at every kill; static study). Classic rules have no
  control groups;
- keeps the target recorded at that moment, which may be none (a killer that forgot its target dies with no target;
  `combat/damage.md`, "Applying damage"). The dead unit, and the body it becomes, keep it until the slot is released;
  only a load clears it, and only on a body (type 0x69): a dying unit keeps it through a load (`game/save-load.md`,
  "What a saved game keeps", SL28 to SL30);
- leaves its HP as it is: no kill, whether by damage or outright, changes a unit's HP (`combat/damage.md`). This
  covers the killing blow, a demolisher killed after its blast, a passenger killed with its transport, a builder
  with no exit tile and every other outright kill. The HP shows unchanged through the death animation;
- for a **building**, right after its death sound (`ui/sounds.md`, "A building killed"), creates a cosmetic
  **explosion** (kind 0x17) in the lowest free effect slot, with only kind and position set (`spells/spells.md`,
  "Cosmetic effects"). Its position is the building's centre in pixels (its top-left tile × 32 plus half its size
  in pixels), moved 8 px up. Wall pieces make none, and neither does a building with the used mark
  (`core/corpses.md`; from the static study, not traced). If no effect slot is free, no explosion is made. When a
  missile kills the building, the explosion comes before the missile's own impact effect, which takes the next free
  slot (`combat/damage.md`, "Impact effects").

When the death animation reaches its step-done signal, one of three things happens:

| Unit | What happens |
| --- | --- |
| Mobile unit whose type has a corpse | It becomes a **corpse** unit in the same slot, and the corpse animation plays. A unit whose type has flag bit 10 (the transports in the default data) and that stands on shore water at its death step done gets no corpse; other ships leave one (`core/corpses.md` has the full rule). |
| Building | Its footprint marks are cleared, and it becomes a **ruin** unit sized by the building's width, in the same slot |
| Anything else, or a corpse or ruin whose time is over | Its order becomes "remove", and it is moved to the neutral owner (15) |

The "remove" order runs at the unit's next step-done signal. It **releases the slot**:

1. The unit is unlinked from its owner list. The owner's count goes down, and if it reaches 0 for a player, P goes
   down by 1. A slot released by the "remove" order belongs to the neutral player (15) by then, so its release never
   changes P: the count and P changed when it was handed to the neutral player ("Changing owner"). Only a slot
   released by a failed creation (step 4 of "Creating a unit") still has a player as owner.
2. The slot is marked unused, with owner 14, and goes to the **front** of the free list.
3. Its creation number becomes 0 (static study). No rule reads the creation number of an unused slot, so this has no
   effect in play; it is recorded for completeness.

Before that, the unit is taken out of the local selection and the local remembered groups (interface only,
`ui/selecting-and-commanding.md`).

The released slot is therefore the **first one reused**. The free list works last in, first out.

### Changing owner

A unit that changes owner leaves its old list and goes to the front of the new owner's list. The counts and P are
updated, but **the cap is not checked**. A player can go over the cap by converting units, for example with a spell.

### Saved games

When a saved game loads, the owner lists are rebuilt from scratch by walking slots 0 to N − 1 and adding each to the
front of its owner's list, as at map start. After a load, every list (including the free list) is in descending slot
order, whatever order it had before saving. Any later behaviour that depends on list order, such as slot reuse, can
differ between a game that was saved and reloaded and one that wasn't.

### Effect slots

Creating an effect takes the **lowest-numbered** unused effect slot, found by scanning from slot 0. If none is free,
no effect is created. A released effect slot is simply marked unused.

## Data

| Constant | Value |
| --- | --- |
| Unit slots | 600 classic, 1600 enhanced |
| Effect slots | 200 classic, 400 enhanced |
| Owner values | 0 to 7 players, 14 free list, 15 neutral |
| Per-player cap | N when P ≤ 1, else `floor(N / P)`, where P counts players 0 to 7 with at least one slot |
| First creation number | 1 (per map) |
| First creation number after a load | unit-slot count + 26 (1626 in Enhanced; traced) |
| Permanent effects at map start | 3, in effect slots 0 to 2 |

## Test vectors

| Case | Expected |
| --- | --- |
| Classic, fresh map, the map lists units A, B, C (in that order) | A in slot 599, B in 598, C in 597; each cycle C updates first, then B, then A |
| Then B is killed and has no corpse; after "remove" runs, a new unit D is created | D takes slot 598 |
| Classic, players 0 and 1 each have units, neutral has gold mines | cap 300 per player; neutral uncapped |
| Player 0 has 300 units in that game and trains another | creation fails ("too many units") |
| Player 1 loses its last unit (its list becomes empty) | P drops to 1; cap becomes 600 |
| Enhanced, 4 players with units | cap 400 |
| Classic, 7 players with units | cap `floor(600 / 7)` = 85 |
| Unit converted to a player already at the cap | conversion succeeds; that player now exceeds the cap |
| Two missiles created, the first is removed, then a third is created | the third reuses the first's effect slot (the lowest free slot) |
| Game saved and reloaded, then two units die and two are created | the new units take the two released slots, most recently released first |
| Human04 setup (oracle trace), the three oil patches (type 0x5D) in slots 1572, 1597 and 1599 | hit points 1 each |
| Human04 setup (oracle trace), the gold mine (type 0x5C) in slot 1559 | hit points 25500, the type's value (its trips, 700, come from the map) |
| Every building in the Human04, Human01, human12, orc8 and xorc7 traces, at every record | Facing 0 |
| Every unit in those traces whose type isn't a building, can't cast and has "magic" 0 | Mana 0 at every record |
| Human04 at setup: the 16 critters (type 0x39), peons, footmen and grunts | Facings drawn (the critters show facings 1 to 7); mana 0 |
| `xhuman4` (oracle trace): slots 1435, 1489, 1506 and 1516 have an enemy in reach from the start | No acquisition in the first start-up cycle (n = 1: order 2); every unit is still hidden from every player there |
| `human12-demolition` (oracle trace), n = 239: demolition squad 1430 (type 0x0E) with 40 HP reaches (29, 100) and blasts | 1430: order 1, animation 1, HP **40** at n = 239 and until its slot is released |
| A transport carrying a unit with 60 HP is killed (static study only) | The passenger is killed at the same moment and keeps 60 HP |
| `human5-10000` (oracle trace), n = 1595: barracks 1529 (type 0x3C, 3 × 3 at (46, 92)) destroyed by an axe from 1534; effect slots 0 to 2 in use, the axe in slot 4 | Effect slot 3: kind 0x17 at (1520, 2984) ((1472 + 48, 2944 + 48 − 8)), destination (0, 0), target null, shooter null, damage 0 |
| `xhuman2-10000` (oracle trace), n = 514: guard tower 1546 (type 0x60, 2 × 2 at (56, 69)) destroyed by a catapult rock from 1556 in effect slot 4; slots 0 to 2 in use | Effect slot 3: kind 0x17 at (1824, 2232) ((1792 + 32, 2208 + 32 − 8)), zero fields; the rock's impact effect (kind 0x15) in slot 5 |
| A wall piece destroyed (static study only) | No explosion |
| FL1: `mp-r102-flyers-fixed-60` (oracle trace, Enhanced, generated map `R102-flyers.pud`): gryphon riders at (31, 7) and (30, 21), a flying machine at (33, 9), a dragon at (37, 13), a zeppelin at (39, 15), destroyers at (21, 55) and (27, 59), a transport at (41, 57), all placed by the map | Setup record and every record to n = 60: each unit on exactly its map tile, pixels = tile × 32 (gryphon at (31, 7): (992, 224); destroyer at (27, 59): (864, 1888)); no column or row is made even |
| FL2: a custom map whose unit data sets the sea-unit flag on type 0 places a footman at (5, 7) and a gryphon rider at (9, 3) (static study only) | Footman at (4, 6), gryphon rider at (8, 2): every created unit is aligned to 64 pixels |
| A unit in slot 1400 is removed by the "remove" order (static study) | The slot is unused with owner 14 at the front of the free list, and its creation number reads 0; P is unchanged by the release |

## Open questions

- **Mana byte after a type change.** Not checked. Default: a type change sets it from the new type as at creation.
  Bodies and ruins keep the dead unit's mana byte (`core/corpses.md`).
- How many cycles a corpse and a ruin last, and the decay stages between them (bones, crater and so on).
- The exact rule for which ruin type a building becomes (it depends on the building's width, and on water for
  shore buildings).
- Whether a player with only corpses left in their list still counts towards P (it appears so, since corpses keep their
  owner until removed).
