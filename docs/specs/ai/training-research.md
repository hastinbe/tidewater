# AI: computer training, research and building upgrades

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the AI script file
  `Rez\ai.bin` in the game data, and the unit type, upgrade and AI lists in the public PUD specification text. Not
  yet confirmed against the running game.
- **Changes:** 2026-10-12, round-14 errata: the attacker handling before a think is the building forgetting its
  attacker (`orders/orders.md`, row 33).
- **Changes:** 2026-10-12, round-21 errata: only a research start and the game-start marks mark a research entry
  done, so an entry the player can never research blocks the requests of the entries after it.
- **Changes:** 2026-10-16, round 82 (AI consolidation, `ERRATA-ai-consolidation.md`, reviewed by a spec-review session,
  2026-10-16): the model of the whole computer player is in `ai/computer-player.md`; the chance draws are placed in
  phase 4, in the building's own update. No rule change.
- **Changes:** 2026-10-21, round 92 (oracle traces, `ERRATA-round92-traces.md`; reviewed by a spec-review session,
  2026-10-21): "Training buildings": after a load the footmen, archers, siege and knights counts are doubled (traced,
  `game/save-load.md` SL4 to SL6).
- **Changes:** 2026-10-21, round 93 (oracle traces, `ERRATA-round93-traces.md`; reviewed by a spec-review session,
  2026-10-21): a load clears every research request and makes the done marks again (traced, `game/save-load.md` SL9
  to SL12); after a load every group count from footmen to submarines is doubled (mages, gryphons and destroyers
  traced, SL13 to SL15; four groups a labelled gap-fill), the worker and tanker counts aren't.
- **Changes:** 2026-10-21, round 94 (oracle traces, `ERRATA-round94-traces.md`; reviewed by a spec-review session,
  2026-10-21): the flying machine, transport, battleship and submarine counts are traced doubling too
  (`game/save-load.md` SL21); no rule change.

## Behaviour

A computer player's buildings decide by themselves what to train, research or upgrade into. The decisions read the
player's script slots (`ai/ai-scripts.md`), its unit counts and its **research requests**. Worker and tanker jobs are
in `ai/worker-ai.md`; what the trained units then do is in `ai/attack-ai.md`. How the parts fit together is in
`ai/computer-player.md`.

Type numbers are PUD unit types written with the human number; the race bit (the type's lowest bit) of the deciding
building is added to every unit or building it asks for, so an orc barracks asks for 0x01 where the text says 0x00.
Upgrade numbers are those of the PUD upgrade list (Appendix B). "Count" means the player's count of **complete**
units of that kind, not counting units under construction or in training; a building count covers both races'
types. The unit **group** counts in the table under "Training buildings" also leave out **held** units
(`ai/attack-ai.md`, "Held units"); the worker, tanker and building counts include them.

### When a building thinks

A building of a computer player runs its **think** from its resting order (order 33, `orders/orders.md`), each time
that order's logic runs, after the order's attacker handling (the building's attacker is forgotten,
`orders/orders.md`, row 33). Only buildings in order 33 think, so towers that can attack (order 14) never do; scout
towers (types 0x40, 0x41) are in order 33 and do.

The script's **second list** (`ai/ai-scripts.md`) holds one 16-bit **interval** per building type, starting with
type 0x3A. An interval of 0xFFFF means "never". Each building has a think counter, 0 when it is created. Each
time the order's logic runs, if the building is complete and its interval is not 0xFFFF:

1. let c be the counter's value; the counter goes up by 1;
2. if the interval is below c, the counter becomes 0 and the building thinks.

So a building thinks on every (interval + 2)-th run of its order logic: with interval 1, on runs 3, 6, 9, ….

A think does nothing if the building is not complete or is **busy** (training, researching or upgrading).

### Starting a job

Every decision below "starts" a job of one of four kinds: **train** a unit, **research a spell** (also used for the
paladin and ogre-mage upgrades), **research an upgrade**, or **upgrade the building** into another type. Starting
uses the same rules as a human player's command (`economy/build-train.md` and `economy/tech-tree.md`): it fails
silently, changing nothing, if the building is busy, the item is not allowed, its prerequisites are missing, or the
player can't pay. On success the cost is paid and the building becomes busy, so at most one job starts per think;
later attempts in the same think fail.

For an upgrade research, the start also fails unless the upgrade's current level is the one the item raises
(level 0 for the "1" upgrades, 1 for the "2" upgrades).

### Research requests and done marks

Each computer player has a **request** flag for each research code 0x80 to 0x9A, all clear at game start. A request
is set:

- by the worker job scan, when it reaches a build-list entry of 0x80 or more (`ai/worker-ai.md`, "Build list");
- by the **no-workers pass**: every 50 cycles (`ai/attack-ai.md`, "Timing", step 2), for each player 0 to 15 that is
  computer-controlled and has a worker count of 0, the player's build list is scanned from entry 0, below
  min(slot 0x22, 64) and up to an 0xFF entry; the first entry that is **not done** and is 0x80 or more gets its
  request set. Entries below 0x80 are passed over.

When a research or building upgrade **starts**, every build-list entry equal to its code (entries 0 to 63, stopping at
an 0xFF entry) is marked **done**, and the code's request is cleared. A request is never cleared otherwise during a
game. A load clears every request and makes the done marks again (`game/save-load.md`, "What a loaded game starts
afresh"; traced, round 93, SL9 to SL12).

Nothing else marks a research entry done, apart from the marks made at the start of a game for what the player already
has (made again after a load) and the load's marks for a keep or castle (`ai/worker-ai.md`, "Done marks"). So a research
still running at a save is never marked after the load, even when it finishes (`game/save-load.md` SL10). No scan,
timeout or missing building marks it. If the player never has a building that can research an entry, the entry stays not
done, and the requests of the entries after it are never set by either scan: both the worker job scan and the no-workers
pass stop at that entry.

### Research codes

Each code names one research item per race; the race comes from the player's `SIDE` value (1 is orc, anything else
human). Every item needs the code's request flag. Research items are tried in the table order of their building
(see each building below).

| Code | Human / orc item (upgrade number) | Researched at | Also needs |
| --- | --- | --- | --- |
| 0x80 | arrow 1 (0x04) / spear 1 (0x06) | lumber mill | level 0, not in progress |
| 0x81 | arrow 2 (0x05) / spear 2 (0x07) | lumber mill | level 1, not in progress |
| 0x82 | rangers (0x18) / berserkers (0x1C) | lumber mill | a keep or castle (either race) complete; not yet researched; not in progress |
| 0x83 | marksmanship (0x1B) / regeneration (0x1F) | lumber mill | rangers researched; not yet researched; not in progress |
| 0x84 | longbow (0x19) / lighter axes (0x1D) | lumber mill | rangers researched; not yet researched; not in progress |
| 0x85 | scouting (0x1A / 0x1E) | lumber mill | rangers researched; not yet researched; not in progress |
| 0x86 | sword 1 (0x00) / axe 1 (0x02) | blacksmith | level 0, not in progress |
| 0x87 | sword 2 (0x01) / axe 2 (0x03) | blacksmith | level 1, not in progress |
| 0x88 | shield 1 (0x08 / 0x0A) | blacksmith | level 0, not in progress |
| 0x89 | shield 2 (0x09 / 0x0B) | blacksmith | level 1, not in progress |
| 0x8A | ballista 1 (0x16) / catapult 1 (0x14) | blacksmith | level 0, not in progress |
| 0x8B | ballista 2 (0x17) / catapult 2 (0x15) | blacksmith | level 1, not in progress |
| 0x8C | ship cannon 1 (0x0C / 0x0E) | foundry | level 0, not in progress |
| 0x8D | ship cannon 2 (0x0D / 0x0F) | foundry | level 1, not in progress |
| 0x8E | ship armour 1 (0x10 / 0x12) | foundry | level 0, not in progress |
| 0x8F | ship armour 2 (0x11 / 0x13) | foundry | level 1, not in progress |
| 0x90 | paladins (0x21) / ogre-mages (0x20) | church / altar of storms | not yet known, not in progress |
| 0x91 | healing (0x23) / bloodlust (0x2C) | church / altar | paladins (ogre-mages) known; spell not yet known, not in progress |
| 0x92 | exorcism (0x24) / runes (0x32) | church / altar | paladins (ogre-mages) known; spell not yet known, not in progress |
| 0x93 | flame shield (0x25) / raise dead (0x2D) | mage tower / temple of the damned | spell not yet known, not in progress |
| 0x94 | slow (0x27) / haste (0x30) | mage tower / temple | spell not yet known, not in progress |
| 0x95 | invisibility (0x28) / whirlwind (0x2F) | mage tower / temple | spell not yet known, not in progress |
| 0x96 | polymorph (0x29) / unholy armour (0x31) | mage tower / temple | spell not yet known, not in progress |
| 0x97 | blizzard (0x2A) / death and decay (0x33) | mage tower / temple | spell not yet known, not in progress |
| 0x98 | town hall → keep | town hall / great hall | see "Halls" |
| 0x99 | keep → castle | keep / stronghold | see "Halls" |

"Level" is the player's current level of that upgrade line (0, 1 or 2). "In progress" means the player already has
that item being researched somewhere. Codes 0x80 to 0x8F are upgrade research; 0x90 to 0x97 are spell research.

Codes 0x9A and above are never acted on; codes above 0x9A would set the request of a different player (see Open
questions).

### Research buildings

Each research building, when it thinks, first checks a **hold-back** rule; if it applies, the think ends. Otherwise
it tries each of its codes in the table order below, starting each one whose conditions hold (only the first can
succeed, since the building is then busy). The mage tower then trains (below).

| Building (types) | Code order | Hold-back: the think ends if |
| --- | --- | --- |
| Lumber mill (0x4C, 0x4D) | 0x80, 0x81, 0x82, 0x83, 0x84, 0x85 | slot 0x15 ≠ 0 and the archer count is 0 |
| Blacksmith (0x52, 0x53) | 0x86, 0x88, 0x87, 0x89, 0x8A, 0x8B | slot 0x17 ≠ 0, knight count 0, slot 0x14 ≠ 0 and footman count 0 |
| Foundry (0x4E, 0x4F) | 0x8C, 0x8E, 0x8D, 0x8F | slot 0x19 ≠ 0 and the destroyer count is 0 |
| Church / altar (0x3E, 0x3F) | 0x90, 0x91, 0x92 | slot 0x17 ≠ 0 and the knight count is 0 |
| Mage tower / temple (0x50, 0x51) | 0x93, 0x94, 0x95, 0x96, 0x97 | a barracks is complete, slot 0x17 ≠ 0, knight count 0, slot 0x14 ≠ 0 and footman count 0 |

Note the blacksmith's order: the first sword and shield upgrades come before the second ones.

**Mage tower training.** After the hold-back check and **before** its research, if slot 0x1D ≠ 0 and the mage count
is 0, the mage tower starts training a mage (0x0A) and the think ends. After its research, if the mage count is below
slot 0x1D, it starts training a mage (which fails if a research has just started).

### Training buildings

The **unit counts** (all complete; held units are left out of the groups, but not of the worker and tanker
counts):

| Count | Unit types |
| --- | --- |
| footmen | 0x00, 0x01, 0x19, 0x2E, 0x2F |
| archers | 0x08, 0x09, 0x12 to 0x14 |
| siege | 0x04, 0x05 |
| knights | 0x06, 0x07, 0x0C, 0x0D, 0x17, 0x2C |
| mages | 0x0A, 0x0B, 0x15, 0x18 |
| gryphons | 0x16, 0x23, 0x2A, 0x2B |
| flying machines | 0x28, 0x29 |
| transports | 0x1C, 0x1D |
| destroyers | 0x1E, 0x1F |
| battleships | 0x20, 0x21 |
| submarines | 0x26, 0x27 |
| tankers | 0x1A, 0x1B |
| workers | as in `ai/ai-scripts.md` |

Heroes are counted in their groups, so a map hero reduces what is trained.

**After a load** every group count from footmen to submarines is the saved count plus the loaded units counted
again, so it is doubled when the saved count was right, and the surplus stays (`game/save-load.md`, "What a saved game
keeps"; traced for footmen, archers, siege and knights in round 92, SL4 to SL6, and for mages, gryphons and
destroyers in round 93, SL13 to SL15, and for flying machines, transports, battleships and submarines in round 94,
SL21). The rules below then compare the doubled counts: a computer player that has loaded trains fewer of
these groups than it would have. The worker and tanker counts aren't affected (traced, round 93), and neither are the
lumber mill, blacksmith and stables tests.

**Draws.** Every "chance" below is one simulation draw r (`core/timing-and-rng.md`), made in phase 4 during the
building's own update, and **only** when the condition written before it holds; conditions after it are not checked
unless the draw succeeds.

**Barracks (0x3C, 0x3D).** LM, BS and ST mean "the player has a complete lumber mill, blacksmith, stables" (either
race). The **archer choice** trains a ranger (0x12) if rangers are researched, else an archer (0x08). The **knight
choice** trains a paladin (0x0C) if the paladin upgrade is known, else a knight (0x06). The first rule that applies
decides, and the think ends there:

1. Slot 0x14 ≠ 0 and footmen = 0: footman (0x00).
2. Slot 0x17 ≠ 0, knights = 0, BS and ST: the knight choice.
3. **Archers now** if either:
   - slot 0x15 ≠ 0, archers = 0 and LM; or
   - archers < slot 0x15, then a 1-in-4 chance (r mod 4 = 1), then LM.
   If so: the archer choice.
4. Siege < slot 0x16, then a 1-in-8 chance (r mod 8 = 1), then BS and LM: ballista (0x04).
5. Knights < slot 0x17, BS and ST: the knight choice.
6. Footmen < slot 0x14: footman.
7. Slot 0x16 ≠ 0, siege = 0, BS and LM: ballista.
8. Archers < slot 0x15 and LM: the archer choice.
9. Siege < slot 0x16, BS and LM: ballista.
10. Otherwise nothing.

The draws of rules 3 and 4 happen in the same think when both conditions hold and the earlier draw fails.

**Shipyard (0x48, 0x49).** F = a complete foundry, I = a complete gnomish inventor or goblin alchemist (either race).
First rule that applies:

1. Slot 0x18 ≠ 0 and tankers = 0: tanker (0x1A).
2. Slot 0x19 ≠ 0 and destroyers = 0: destroyer (0x1E).
3. Slot 0x1A ≠ 0, transports = 0 and F: transport (0x1C).
4. Destroyers < slot 0x19, then a 1-in-4 chance (r mod 4 = 1): destroyer.
5. Battleships < slot 0x1B, then a 1-in-8 chance, then F: battleship (0x20).
6. Submarines < slot 0x1C, then a 1-in-8 chance, then I: submarine (0x26).
7. Transports < slot 0x1A, then a 1-in-8 chance, then F: transport.
8. Tankers < slot 0x18: tanker.
9. Destroyers < slot 0x19: destroyer.
10. Battleships < slot 0x1B and F: battleship.
11. Transports < slot 0x1A and F: transport.
12. Submarines < slot 0x1C and I: submarine.

**Gryphon aviary / dragon roost (0x46, 0x47).** Hold-back as the mage tower. Then, if gryphons < slot 0x20: gryphon
rider (0x2A).

**Gnomish inventor / goblin alchemist (0x44, 0x45).** Hold-back as the mage tower. Then, if flying machines < slot 0x1E
and LM: flying machine (0x28).

**Scout towers (0x40, 0x41).** Always try to upgrade into a guard tower (0x60). No draw is made. The
upgrade fails silently when it isn't allowed (it needs a lumber mill while requirements are enforced) or can't be
paid for.

### Halls

Town hall, great hall, keep, stronghold, castle and fortress (0x4A, 0x4B, 0x58 to 0x5B):

1. If slot 0x13 > the worker count: train a worker (0x02). The think ends.
2. A town hall or great hall: if code 0x98 is requested and a barracks is complete, upgrade into a keep (0x58). On
   success, the first 64 build-list entries are scanned up to the list's end; each entry 0x98 is marked done, and the
   request is cleared when at least one such entry is found. (Requests come from build-list entries, so one is
   normally found.)
3. A keep or stronghold: if code 0x99 is requested and a stables, a blacksmith and a lumber mill are complete, upgrade
   into a castle (0x5A). On success, entries 0x99 are marked done and the request cleared in the same way.

Castles and fortresses only train workers.

### Buildings with nothing to decide

Farms (0x3A, 0x3B), stables (0x42, 0x43), refineries (0x54, 0x55), oil platforms (0x56, 0x57) and types 0x5C to 0x66
have a think that does nothing. A type whose interval is 0xFFFF never thinks at all.

## Data

### Second-list intervals in the shipped scripts (selected)

| Script | Barracks | Church | Scout tower | Stables | Inventor | Aviary | Shipyard | Halls | Lumber mill, foundry, mage tower, blacksmith | Keeps, castles |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 (land attack) | 1 | 1 | 1 | 0xFFFF | 5000 | 1000 | 1 | 1 | 1 | 100 |
| 1 (passive) | 0xFFFF | 0xFFFF | 0xFFFF | 0xFFFF | 0xFFFF | 0xFFFF | 0xFFFF | 100 | 0xFFFF | 0xFFFF |
| 0x19 (sea attack) | 1 | 1 | 1 | 0xFFFF | 500 | 1000 | 1 | 1 | 1 | 100 |
| 0x1A (air attack) | 1 | 1 | 1 | 0xFFFF | 5000 | 1 | 1 | 1 | 1 | 100 |

The list holds one word per type from 0x3A. For types above 0x5B the game reads on into whatever follows in the
file; those types' thinks do nothing, so only the counter is affected.

### Request table

| Item | Value |
| --- | --- |
| Codes per player | 0x80 to 0x9A (27 flags) |
| Build-list scan for done marks | entries 0 to 63, stops at 0xFF |
| No-workers pass | every 50 cycles, players 0 to 15, computer-controlled, worker count 0 |

## Test vectors

| Input | Expected |
| --- | --- |
| Barracks, interval 1, complete, idle | Thinks on the 3rd, 6th, 9th … run of its order-33 logic |
| Barracks, interval 0xFFFF | Never thinks |
| Slots 0x14 = 2, 0x15 = 2; 0 footmen; LM present | Footman (rule 1), no draw |
| Slots 0x14 = 2, 0x15 = 2; 1 footman, 0 archers, LM | Archer (rule 3, first case), no draw |
| Slot 0x15 = 2; 1 footman (slot 0x14 = 1), 0 archers, no LM; slot 0x16 = 0 | Rule 3 first case fails (no LM); 0 < 2 so a draw is made; whatever it is, LM fails; rules 4 to 9 don't apply (no LM for rule 8): nothing, 1 draw |
| Same with rangers researched and LM | Ranger (0x12) |
| Slot 0x17 = 1, 0 knights, BS and ST; slot 0x14 = 2 with 2 footmen | Knight (rule 2); paladin if the paladin upgrade is known |
| Orc barracks, same | Ogre (0x07) or ogre-mage (0x0D) |
| Shipyard, slots 0x18 = 1, 0x19 = 2; 1 tanker, 0 destroyers | Destroyer (rule 2), no draw |
| Shipyard, slot 0x19 = 2, 1 destroyer, slot 0x1B = 1, 0 battleships, F, all other slots 0; draws 2 then 9 | Rule 4: 2 mod 4 ≠ 1; rule 5: 9 mod 8 = 1, F: battleship |
| Town hall, slot 0x13 = 5, 5 workers, code 0x98 requested, barracks complete, enough gold and lumber | Upgrade to keep starts; build-list entries 0x98 marked done; request cleared |
| Town hall, slot 0x13 = 6, 5 workers | Trains a peasant; the keep upgrade is not tried this think |
| Lumber mill, code 0x80 requested, arrow level 0 | Arrow 1 research starts; entries 0x80 marked done; request cleared |
| Lumber mill, codes 0x80 and 0x81 requested, arrow level 0 | Arrow 1 starts; 0x81 isn't started (level is 0, and the building is busy) |
| Lumber mill, slot 0x15 = 3, 0 archers | Hold-back: nothing researched |
| Lumber mill, code 0x82 requested, no keep or castle | Rangers not started |
| Church, code 0x91 requested, paladins not known | Healing not started |
| Mage tower, slot 0x1D = 2, 0 mages | Trains a mage before any research |
| Mage tower, slot 0x1D = 2, 1 mage, code 0x94 requested | Slow research starts; the mage training after it fails (busy) |
| Computer player with 0 workers, build list 0x4A (done), 0x3C (not done), 0x80 (not done) | No-workers pass sets the request for 0x80 (the barracks entry is passed over) |
| Scout tower of a computer player with guard-tower upgrade allowed and affordable | Upgrades into a guard tower at its first think |
| *Tides of Darkness* human mission 13, player 4 (script 0x15, build list 0x93, 0x90, 0x91, …) | Once the limit is 2 or more and 0x93 is done, every scan stops at 0x90 (the list has no building entries, so no altar is built). The requests for 0x91 onward are never set. This is the original game's behaviour |

## Open questions

- **Interval timing.** How often order 33's logic runs for an idle building depends on its still animation; the
  think period in cycles is (interval + 2) times that. Default: as derived from the animation scripts.
- **Codes above 0x9A.** The request flags are 27 per player in one table, so a build-list code above 0x9A would set or
  clear a flag of the next player. The shipped scripts use only 0x80 to 0x99. Default: ignore codes above 0x9A.
- **Hold-back of the mage tower, aviary and inventor** depends on a barracks being complete, unlike the blacksmith's
  identical rule. Default: implement literally.
- **Held units in counts.** A map-placed held hero or unit is not counted in its group, so the computer may train
  more of that group. Default: as described.
- **Spell research request without the building.** A request stays set until the research starts; nothing expires
  it. Default: as described.
- **Players 8 to 15 in the no-workers pass.** The pass looks at players 0 to 15, but script state exists only for
  players 0 to 7. Players 8 to 15 are never computer-controlled in normal maps. Default: only players 0 to 7.
