# Economy: tech tree, research and upgrades

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the default upgrade data
  `upgrades.dat`, and the public PUD specification text (unit types in appendix A, upgrades in appendix B, the ALOW
  and UGRD sections). Not yet confirmed against the running game.

## Behaviour

This spec says which building makes, researches or becomes what, what a job needs when the command **arrives**, and
what finishing or cancelling a research job does. The job mechanics shared with training (one job at a time,
progress steps, timing, refunds of trained units and building upgrades) are in `economy/build-train.md`. What the
buttons show, and so what a player can normally ask for, is in `ui/command-buttons.md`, "Conditions".

Unit types and upgrade numbers are those of the PUD specification (appendices A and B), written in decimal. "Has X"
has the meaning of `ui/command-buttons.md`: the player has at least one **completed** X, either race's version.

### Two layers of checks

A job is checked twice:

1. **The button** (`ui/command-buttons.md`) decides what the local player can click. It reads the map's ALOW words,
   the "being researched" bits and the "known" bits.
2. **The command**, when it is executed on every machine, runs the smaller set of checks below. It **doesn't** read
   ALOW, and for research it doesn't check whether the upgrade or spell is already known or being researched.

Anything a button hides but the command check allows can still happen if such a command arrives (an old or modified
client, or the duplicate case in "Research twice" below). A reimplementation that must stay in step with the original
uses the command checks below, not the button rules.

### Starting a job (when the command arrives)

The command names a building, a job kind (train, research a spell, upgrade, or upgrade the building) and a unit type
or upgrade number. The checks run in this order; any failure ends it **silently** unless a message is named:

1. The building isn't already working, and it is completed.
2. The job kind is one of the four.
3. **The kind's own check:**
   - **Train:** the type is a unit type (0–57) and is made by this building type ("Who makes what"). A keep or castle
     counts as a town hall, a stronghold or fortress as a great hall. Then, **only while requirements are
     enforced** (below), the type's requirement must hold ("Training requirements").
   - **Research a spell** and **upgrade:** the number is an upgrade number (0–51) and this building type is the one
     that researches it ("Who researches what"). Nothing else.
   - **Upgrade the building:** the target is a building type (58–104) that this building type becomes ("Building
     upgrades"). Then, only while requirements are enforced, its requirement must hold.
4. **Level check**, for the upgrade kind with upgrade numbers 0–23 only: the player's level in that upgrade's line
   must equal the level the upgrade raises from: **0** for the first upgrade of the pair (even number), **1** for
   the second (odd number). The lines are fixed by upgrade number, as in the table below; the "group" field of the
   upgrade data isn't used here.
5. **Affordability:** gold, lumber and oil (`economy/build-train.md`). A failure shows its message, to the owner
   only if the owner is the local player.
6. The cost is paid, and the job starts:
   - **train:** the owner's "units in training" count goes up by 1;
   - **upgrade:** the upgrade data's "affects" bits are set in the owner's **upgrades being researched** word;
   - **research a spell:** the "affects" bits are set in the owner's **spells being researched** word;
   - every kind: the building becomes working, with a job of **2 × time** progress steps.

Level lines (for the level check):

| Upgrade numbers | Line |
| --- | --- |
| 0–3 (swords, axes) | weapons |
| 4–7 (arrows, spears) | missiles |
| 8–11 (shields) | shields |
| 12–15 (ship cannons) | ship cannons |
| 16–19 (ship armour) | ship armour |
| 20–23 (catapult, ballista) | siege |

The ranger and berserker upgrades (24–31) have **no** level check and no check that ranger (berserker) training is
done. The church and altar spells have no check that the paladin (ogre-mage) upgrade is known.

### Who makes what

| Building | Trains |
| --- | --- |
| Human barracks (60) | footman (0), ballista (4), knight (6), archer (8), paladin (12), ranger (18) |
| Orc barracks (61) | grunt (1), catapult (5), ogre (7), axethrower (9), ogre-mage (13), berserker (19) |
| Town hall (74), keep (88), castle (90) | peasant (2) |
| Great hall (75), stronghold (89), fortress (91) | peon (3) |
| Mage tower (80) | mage (10) |
| Temple of the damned (81) | death knight (11) |
| Gnomish inventor (68) | dwarven demolition squad (14), gnomish flying machine (40) |
| Goblin alchemist (69) | goblin sappers (15), goblin zeppelin (41) |
| Human shipyard (72) | tanker (26), transport (28), elven destroyer (30), battleship (32), gnomish submarine (38) |
| Orc shipyard (73) | tanker (27), transport (29), troll destroyer (31), juggernaught (33), giant turtle (39) |
| Gryphon aviary (70) | gryphon rider (42) |
| Dragon roost (71) | dragon (43) |

No building trains any other unit type (the heroes, Deathwing, the eye of Kilrogg, skeletons, daemons, critters and
the unused types). The match is exact: a human barracks can't train a grunt.

### Training requirements

Checked only while requirements are enforced. "Paladin upgrade known" is spell bit 20 in the owner's **spells known**
word; "ranger training done" is the owner's ranger-training level (line "ranger training" in the upgrade data) above
0. The human and orc versions of a unit have the same requirement.

| Unit | Requirement |
| --- | --- |
| Peasant, footman, mage, tanker, destroyer, gryphon rider (and orc versions) | none |
| Ballista (catapult) | has blacksmith and has lumber mill |
| Knight (ogre) | has blacksmith, has stables, paladin upgrade **not** known |
| Paladin (ogre-mage) | has blacksmith, has stables, paladin upgrade known |
| Archer (axethrower) | has lumber mill, ranger training **not** done |
| Ranger (berserker) | has lumber mill, ranger training done |
| Demolition squad (sappers), submarine (turtle) | has inventor |
| Transport, battleship (juggernaught) | has foundry |
| Flying machine (zeppelin) | has lumber mill and has inventor |

These are the button rules of `ui/command-buttons.md` without the ALOW bit and without "ready". So while requirements
are enforced, a knight can't be trained once the paladin upgrade is known, nor an archer once ranger training is done.

### Building upgrades

| From | Becomes | Requirement (only while enforced) |
| --- | --- | --- |
| Town hall (74) | keep (88) | has barracks |
| Great hall (75) | stronghold (89) | has barracks |
| Keep (88) | castle (90) | has stables, has lumber mill, has blacksmith |
| Stronghold (89) | fortress (91) | has stables (ogre mound), has lumber mill, has blacksmith |
| Human scout tower (64) | human guard tower (96) or human cannon tower (98) | guard: has lumber mill; cannon: has blacksmith |
| Orc scout tower (65) | orc guard tower (97) or orc cannon tower (99) | as for the human tower |

No other building becomes anything. Time, cost and the hit-point change are in `economy/build-train.md`.

### When requirements are enforced

One game-wide setting decides whether the training and building-upgrade requirements are checked when a command
arrives. It is set when the game starts:

- **Multiplayer, Enhanced mode:** enforced.
- **Multiplayer, Classic mode:** enforced unless some participant runs an older client (its version number in the
  game's join data is below 4); then not enforced for anyone.
- **Single player:** enforced in both modes.

A reimplementation counts every one of its own clients as a current client, so its multiplayer games are always
enforced, unless the open question shows that the version test matters for some mix of clients.

The button rules apply in every case. With enforcement off, only the producer match limits training.

### Who researches what

| Upgrade numbers | Researched at |
| --- | --- |
| 0, 1 (swords), 8, 9 (human shields), 22, 23 (ballista) | human blacksmith (82) |
| 2, 3 (axes), 10, 11 (orc shields), 20, 21 (catapult) | orc blacksmith (83) |
| 4, 5 (arrows), 24–27 (ranger training, longbow, ranger scouting, marksmanship) | elven lumber mill (76) |
| 6, 7 (spears), 28–31 (berserker training, lighter axes, berserker scouting, regeneration) | troll lumber mill (77) |
| 12, 13 (human ship cannons), 16, 17 (human ship armour) | human foundry (78) |
| 14, 15 (orc ship cannons), 18, 19 (orc ship armour) | orc foundry (79) |
| 33 (paladins), 34–36 (holy vision, healing, exorcism) | church (62) |
| 32 (ogre-mages), 43, 44 (eye of Kilrogg, bloodlust), 50 (runes) | altar of storms (63) |
| 37–42 (flame shield, fireball, slow, invisibility, polymorph, blizzard) | mage tower (80) |
| 45–49, 51 (raise dead, death coil, whirlwind, haste, unholy armour, death and decay) | temple of the damned (81) |

The buttons research upgrades 0–31 with the **upgrade** kind, and 32–51 (the paladin and ogre-mage upgrades and all
spells) with the **spell** kind.

### Finishing or cancelling research

When an upgrade or spell job ends, finished or cancelled, the game looks at the job's "affects" bits in the matching
"being researched" word (upgrades or spells):

- **If none of those bits is set, nothing happens at all:** no level, no spell, no conversion, no notice, and on a
  cancel **no refund**.
- Otherwise the bits are cleared, and then:

| Job | Finished | Cancelled |
| --- | --- | --- |
| Upgrade | The owner's level in the upgrade's line (its "group" in the upgrade data) goes up by 1. If the "affects" value is exactly bit 16 (ranger or berserker training), the owner's archers become rangers and axethrowers become berserkers | Full refund |
| Spell | The "affects" bits are set in the owner's **spells known** word. If the value is exactly bit 20 (paladin or ogre-mage upgrade), see below | Full refund |

**Paladin or ogre-mage upgrade finished** (either race's version does all of this):

1. Holy vision (spell bit 0) becomes known, if the owner is **allowed to research** it (ALOW).
2. The owner's knights become paladins.
3. The eye of Kilrogg (spell bit 10) becomes known, if the owner is allowed to research it.
4. The owner's ogres become ogre-mages.

In Enhanced mode, a finished upgrade or spell (one that passed the bit test) is followed by a completion alert at the
building, for the local player only (sound and event position; a later spec).

The refund is the job's full cost, gold, lumber and oil, as the upgrade data gives it.

### Converting units

"The owner's X become Y" goes over all of that player's units:

- A unit of type X that isn't dead, removed or inside something changes type in place. It keeps its position,
  orders and other state. Its hit points go up by (Y's maximum − X's maximum), each maximum counted as at least 1.
  Its mana is set to Y's "magic" value in the unit data, or to **85** if Y can cast spells (unit flag bit 17), in
  which case its mana counter also starts at 40 (mana regeneration is in a later spec).
- A **barracks** (either race) that is working on a job for type X now works on type Y. The progress is kept, and Y
  is created when the job finishes. Its cost was already paid as X; a cancel then refunds Y's cost.

Because both conversions run for either race's upgrade, a player who somehow owns both races' units converts both.

### Research twice

Neither the command check nor the start checks the "being researched" word. So two jobs for the same bits can run at
once, in two buildings of the same type. The first to end clears the bits, and the other then finds them clear and
does nothing when it ends, even on a cancel.

**Arrow 2 is expected to reach this in normal play.** The evidence:

- the default upgrade data gives arrow 2 the "affects" bit **0**, the same as arrow 1 (anyone can check this in
  `upgrades.dat`, and in the data table below);
- the level-1 arrow button tests the fixed bit **1** for "being researched" (`ui/command-buttons.md`, "Upgrade
  bits"), so it still shows on a second lumber mill while the first researches arrow 2;
- neither the command check nor the job start reads the "being researched" word (static study).

So the player can start arrow 2 in a second elven lumber mill and pay again. **This hasn't been observed in the
running game yet** (open questions). Outcomes, with mill A started first and mill B second, both at arrow level 1:

| Sequence | Result |
| --- | --- |
| A finishes, then B finishes | level 2 after A; B does nothing and refunds nothing |
| A finishes, then B is cancelled | level 2; B is **not** refunded, so its payment is lost |
| B is cancelled, then A finishes | B is refunded in full; A then does nothing: level stays 1 and A's payment is lost |
| A is cancelled, then B is cancelled | A is refunded in full; B isn't refunded |
| A is cancelled, then B finishes | A is refunded in full; B does nothing; level stays 1 |
| A is cancelled while B runs, then a third mill C starts arrow 2 | C sets the bit again; whichever of B and C ends first counts normally (finish: level 2; cancel: refund), and the other is then a no-op |

For spells and the other upgrades the buttons hide the second job, so only a command from elsewhere does this.

### Upgrade lines and their effects

The upgrade data's "group" names the line whose level a finished upgrade raises. The game keeps 11 lines per player,
groups 0–10, each counting 0, 1, 2 and so on. Group 5 is kept but no default upgrade uses it. The paladin and
ogre-mage upgrades have group 20, which is never read because they finish as spells.

| Group | Line | Effect |
| --- | --- | --- |
| 0 | missiles (arrows, spears) | `combat/damage.md` (damage bonus) |
| 1 | weapons (swords, axes) | `combat/damage.md` |
| 2 | shields | `combat/damage.md` (armour bonus) |
| 3 | ship cannons | `combat/damage.md` |
| 4 | ship armour | `combat/damage.md` |
| 5 | unused | — |
| 6 | siege (catapult, ballista) | `combat/damage.md` |
| 7 | ranger (berserker) training | conversion above; training requirements; buttons |
| 8 | longbow (lighter axes) | +1 range, `combat/damage.md` |
| 9 | scouting | sight 9, `vision/fog.md` |
| 10 | marksmanship (regeneration) | `combat/damage.md` (marksmanship); `core/timing-and-rng.md` (berserker regeneration) |

A map's UGRD can change an upgrade's group or "affects" value. The level check at the start always uses the fixed
lines above, and the buttons use fixed bits (`ui/command-buttons.md`), while finishing uses the data. A group above
10 on the upgrade path has no defined effect (open questions).

### Per-player words from the map (ALOW)

Each player has six 32-bit words, loaded from the map's ALOW section or from defaults:

| Word | Used by |
| --- | --- |
| Units and buildings allowed | buttons only |
| Spells known (the PUD "spells you start with") | casting, buttons, the knight and paladin requirements; grows as spells finish |
| Spells allowed to research | buttons; the holy vision and eye of Kilrogg grants above |
| Spells being researched | set at the start of a spell job, cleared when it ends |
| Upgrades allowed | buttons only |
| Upgrades being researched | set at the start of an upgrade job, cleared when it ends |

## Data

### Default upgrade data

From `upgrades.dat`. A map's UGRD section replaces every field of all 52 entries when its "use default data" word
is **0**; when the word is not 0, the section is skipped and these defaults stay. In some multiplayer game types the
section is skipped whatever the word says (open questions). Time is in the data's units; the
job is 2 × time progress steps (`economy/build-train.md`). Costs are stored at full value. "Bit" is the single bit
set in the "affects" value.

| No. | Upgrade | Time | Gold | Lumber | Oil | Group | Bit |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | sword 1 | 200 | 800 | 0 | 0 | 1 | 2 |
| 1 | sword 2 | 250 | 2400 | 0 | 0 | 1 | 3 |
| 2 | axe 1 | 200 | 500 | 100 | 0 | 1 | 2 |
| 3 | axe 2 | 250 | 1500 | 300 | 0 | 1 | 3 |
| 4 | arrow 1 | 200 | 300 | 300 | 0 | 0 | 0 |
| 5 | arrow 2 | 250 | 900 | 500 | 0 | 0 | **0** |
| 6 | spear 1 | 200 | 300 | 300 | 0 | 0 | 0 |
| 7 | spear 2 | 250 | 900 | 500 | 0 | 0 | 1 |
| 8, 10 | human, orc shield 1 | 200 | 300 | 300 | 0 | 2 | 4 |
| 9, 11 | human, orc shield 2 | 250 | 900 | 500 | 0 | 2 | 5 |
| 12, 14 | human, orc ship cannon 1 | 200 | 700 | 100 | 1000 | 3 | 6 |
| 13, 15 | human, orc ship cannon 2 | 250 | 2000 | 250 | 3000 | 3 | 7 |
| 16, 18 | human, orc ship armour 1 | 200 | 500 | 500 | 0 | 4 | 8 |
| 17, 19 | human, orc ship armour 2 | 250 | 1500 | 900 | 0 | 4 | 9 |
| 20, 22 | catapult, ballista 1 | 250 | 1500 | 0 | 0 | 6 | 12 |
| 21, 23 | catapult, ballista 2 | 250 | 4000 | 0 | 0 | 6 | 13 |
| 24, 28 | ranger, berserker training | 250 | 1500 | 0 | 0 | 7 | 16 |
| 25, 29 | longbow, lighter axes | 250 | 2000 | 0 | 0 | 8 | 17 |
| 26, 30 | ranger, berserker scouting | 250 | 1500 | 0 | 0 | 9 | 18 |
| 27 | marksmanship | 250 | 2500 | 0 | 0 | 10 | 19 |
| 31 | regeneration | 250 | 3000 | 0 | 0 | 10 | 19 |
| 32, 33 | ogre-mages, paladins | 250 | 1000 | 0 | 0 | 20 | 20 |

Spells (lumber and oil 0; the group equals the bit and isn't used):

| No. | Spell | Time | Gold | Bit |
| --- | --- | --- | --- | --- |
| 34 | holy vision | 0 | 0 | 0 |
| 35 | healing | 200 | 1000 | 1 |
| 36 | exorcism | 200 | 2000 | 3 |
| 37 | flame shield | 100 | 1000 | 4 |
| 38 | fireball | 0 | 0 | 5 |
| 39 | slow | 100 | 500 | 6 |
| 40 | invisibility | 200 | 2500 | 7 |
| 41 | polymorph | 200 | 2000 | 8 |
| 42 | blizzard | 200 | 2000 | 9 |
| 43 | eye of Kilrogg | 0 | 0 | 10 |
| 44 | bloodlust | 100 | 1000 | 11 |
| 45 | raise dead | 100 | 1500 | 13 |
| 46 | death coil | 100 | 0 | 14 |
| 47 | whirlwind | 150 | 1500 | 15 |
| 48 | haste | 100 | 500 | 16 |
| 49 | unholy armour | 200 | 2500 | 17 |
| 50 | runes | 150 | 1000 | 18 |
| 51 | death and decay | 200 | 2000 | 19 |

Spell bits 2 and 12 have no spell.

### Constants

| Constant | Value |
| --- | --- |
| Research job length | 2 × upgrade time progress steps |
| Upgrade numbers with a level check | 0–23 |
| Level lines kept per player | 11 (groups 0–10) |
| Paladin / ogre-mage upgrade bit (spells word) | 20 |
| Ranger / berserker training bit (upgrades word) | 16 |
| Mana of a converted caster | 85, counter 40 |
| Client version below which Classic multiplayer turns enforcement off | 4 |

## Test vectors

Default upgrade data unless stated.

| Case | Expected |
| --- | --- |
| Human barracks, command "train grunt" | rejected silently (wrong producer) |
| Keep, command "train peasant", enough gold | starts; 1 more unit in training |
| Castle, command "upgrade to keep" | rejected (the castle isn't a town hall) |
| Human blacksmith, command "upgrade sword 2" (no. 1), sword level 0 | rejected silently (level must be 1) |
| Same, sword level 1, 2400 gold | starts; 2400 gold paid; upgrades-being-researched bit 3 set; 500 steps |
| Same, sword level 2 | rejected silently |
| Elven lumber mill, command "upgrade longbow" (no. 25), ranger training not done | starts (no prerequisite check at the command) |
| Mage tower, command "research blizzard" while blizzard is already known | starts and is paid in full; when it finishes, blizzard is still known and nothing else changes |
| Enforcement on, barracks, paladin upgrade known, command "train knight" | rejected silently |
| Enforcement off, same | starts; a knight is trained |
| Enforcement on, no stables, keep, command "upgrade to castle" | rejected silently |
| Enforcement on, scout tower, has blacksmith, no lumber mill: "become guard tower" / "become cannon tower" | rejected / starts |
| Arrow 2 started in mill A (level 1), then in mill B | both start; arrow cost paid twice |
| A finishes, then B finishes | level 2 after A; B does nothing and refunds nothing |
| A running, B cancelled | B refunded 900 gold, 500 lumber; when A finishes, nothing happens; level stays 1 |
| A finishes, then B cancelled | level 2; B gets nothing back (900 gold, 500 lumber lost) |
| A cancelled, then B cancelled | A refunded 900 gold, 500 lumber; B gets nothing back |
| A cancelled, then B finishes | A refunded; B does nothing; level stays 1 |
| A cancelled while B runs, then mill C starts arrow 2 and finishes before B | C gives level 2; B then does nothing |
| Single player, Classic, barracks, paladin upgrade known, command "train knight" | rejected silently (enforced) |
| Ranger training finishes; owner has 2 archers and a human barracks 40 % through an archer | both archers become rangers, each gaining (ranger max HP − archer max HP); the barracks job becomes a ranger and keeps its progress |
| Paladin upgrade finishes; holy vision allowed to research; eye not allowed | holy vision known; eye not known; knights become paladins with mana 85; ogres (if any) become ogre-mages |
| Same upgrade finishes but its bit was already cleared by a duplicate job | no conversion and no spell |
| Cancel a spell job whose bit is set | full refund of the spell's cost |
| Holy vision research (time 0) | job of 0 steps; ends on its first run, 13 cycles after it takes over (`economy/build-train.md`) |

## Open questions

- What the client version number in the multiplayer join data counts, and whether any real mix of clients turns
  enforcement off.
- Which multiplayer game types skip the map's UGRD section regardless of its "use default data" word. A
  reimplementation reads the section whenever the word is 0 until this is settled.
- Confirm "Research twice" in the running game: two elven lumber mills, arrow level 1; start arrow 2 in both, check
  that both are charged, then cancel one and check the refund and the final level. This decides whether a
  reimplementation must reproduce a cost the player can lose.
- Whether units inside a transport or building are converted by the ranger and paladin upgrades. The program skips
  units with any of three state marks (dead, removed, inside something); which mark covers passengers and workers in
  a mine wasn't confirmed.
- The exact completion alert in Enhanced mode (sound number and how the event position is used).
- What an upgrade with a group above 10 does when it finishes on the upgrade path (only possible with a custom UGRD
  or a forged command).
- What a job does when its kind and number don't match the buttons' use, for example an upgrade number below 32 sent
  as a spell job. The command check allows it; the effect would follow the rules above for the kind given.
- Whether the map's "spells being researched" and "upgrades being researched" words are loaded for ordinary maps or
  only for saved games (the PUD specification says "only good for save file").
