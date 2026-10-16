# Interface: unit and event sounds

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:**
  - static study of the 2.02 program (research notes under `research/notes/`);
  - the default unit data (`Rez\unitdata.dat`), the sound options dialog (`Rez\snd_dlg.bin`) and the message table
    `Rez\network.tbl`;
  - the type list in the public PUD specification text.

  Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-12 errata: the rescue and capture triggers are now known (`game/rescue.md`); the
  open question on them is closed.
  2026-10-12, round-15 errata: the impact effects' explosion sound plays at the effect's first update, not at its
  creation; a table of the sounds each missile impact makes in all.
  2026-10-12, round-16 errata: the error sound's one trigger (a failed building placement click) and the placement
  sound; both are plain sounds.
- **Changes:** 2026-10-12, round-24 errata: the impact effects' sound plays at the effect's start step, one cycle after
  its first update.
- **Changes:** 2026-10-12: the option-labels open question is closed by `ui/game-menu.md` ("Sound dialog"): the
  dialog file's labels and the game's own names for the stored bits agree with the labels given here.
- **Changes:** 2026-10-16 (round 76, B-038 build/train consolidation, `ERRATA-buildtrain-consolidation.md`, reviewed;
  traced with a break on the alert): "Work complete" also plays for the tanker of a finished oil platform, and a
  building upgrade plays it only for halls (keep, stronghold, castle, fortress), not towers. New open question on a
  builder that dies for lack of room.
- **Changes:** 2026-10-16 (B-040 build and train audit, `ERRATA-audit-buildtrain.md`, errata 6 and 10; static study;
  reviewed by a spec-review session, 2026-10-16): two readings that differ from the stated rules are held as open
  questions ("Construction sound on every 12th pass", "Unit-ready event position"); no rule change.
- **Changes:** 2026-10-16 (round 78, B-038 combat consolidation, `ERRATA-combat-consolidation.md`, reviewed): the
  open question "Hammer and breath splash" is closed; `combat/damage.md` ("Gryphon hammer and dragon breath") already
  states it: `misc\explode` at the missile's tile on each splash that finds a free effect slot. "Other explosions"
  now points there. No rule change.
- **Changes:** 2026-10-16 (round 79 traces, `ERRATA-round79.md`, reviewed by a spec-review session, 2026-10-16;
  `human06-build-stats-1800` with breaks): the construction sound is asked for on every pass whose count is a multiple
  of 12, and on every pass with the cheat on; the unit-ready and work-complete event location is the building's
  top-left tile. Both audit open questions answered.
- **Changes:** 2026-10-16 (B-040 combat audit, `ERRATA-audit-combat.md`, erratum 6, static study; reviewed by a
  spec-review session, 2026-10-16): the attack sound's and the building explosion's variant draws are made in the
  simulation update, when the sound is made pending, whether or not it is heard. No change to which file plays.

## Behaviour

This spec says which sound file plays for each unit and game event, when it plays, how one of several variants is
picked, and how the sound options and the screen position change what is heard. It doesn't cover music, menu
sounds or how sound files are mixed.

### Basics

- **No effect on the game.** Choosing and playing sounds never changes the simulation, with one exception: clicking
  the same critter 10 times in single player makes it explode ("Critters" below).
- **Variant draws.** When an event has several variant files, one is picked with a draw from the
  **setup-shuffle generator** (`core/timing-and-rng.md`), never the simulation generator. With r the draw (0 to
  32767) and k variants, the pick is variant number `floor(r / 256) mod k`, counting from 0 in the listed order. So
  r = 1000 with k = 6 picks the 4th file. Events with one file make no draw.
- **Two sound sets.** At game start the game loads either the **human set** or the **orc set**. The choice comes
  from the local player's race setting, not from the race of the unit that makes the sound. The two sets differ only
  for the voices and buildings marked "human/orc" in the tables below. So a human player who controls a grunt hears
  the human voice files for it.
- **Files.** Every sound is a `.wav` file. File names below omit the extension and give the folder under `Gamesfx\`,
  for example `human\hwhat1` is `Gamesfx\human\hwhat1.wav`. The one exception is `Sfx\Button.wav`, written
  "Button".

### Sound options

The sound options dialog has these settings that matter here. They are kept with the player's preferences, as
bits of one preferences word (Data, "Option bits in the preferences"). Each event tests one bit, or none:

| Option (bit) | Events that test it |
| --- | --- |
| Digital volume | Every sound below. At 0 nothing plays |
| Unit speech (0x01) | Unit ready, work complete, under attack |
| Acknowledgements (0x02) | Selecting a unit that isn't a building (including critters and the critter explosion sound), acknowledging an order |
| Building sounds (0x04) | Selecting a building (including the large-fire and under-construction sounds), the construction sound |
| Subtitles (0x40000) | Shows a message line for some sounds ("Subtitles" below) |

Every other sound in this spec (deaths, attacks, explosions, missiles, spells, docking, rescue and capture, and the
interface sounds) tests no option bit: it plays whenever the volume and the position rules allow.

Which dialog label belongs to which bit is inferred from the order of the dialog's controls. The bit each event tests
is certain; the labels are an open question.

### How a sound is played

There are two kinds of sound:

- **Plain sounds** play at the full level wherever the event happens: selection, acknowledgement, ready, work
  complete, under attack.
- **Placed sounds** belong to a unit or to a tile: deaths, attacks, explosions, missile impacts, spells, the
  construction sound, the dock sound.

A placed sound **on a unit** is silent when:

- the unit is inside another unit (a passenger, or a worker inside a building or a mine);
- the unit is **hidden from** the local player: the local player is in the unit's hidden-from set (`vision/fog.md`,
  "Unit visibility"), as it stood after the most recent phase 5. So sounds of units the local player can see play,
  and sounds of units in the fog don't.

A placed sound **at a tile** is silent when the local player's "seen" bit for that tile says "not seen"
(`vision/fog.md`). In the demo playback mode this test is skipped. A tile outside the map skips the test too.

Then the position is compared with the main view. Let the view's top-left tile be (vx, vy), and let w and h be its
width and height in tiles. The position (x, y) is **on screen** when `vx ≤ x ≤ vx + w` and `vy ≤ y ≤ vy + h`. Both
ends are included, so one extra column and row count as on screen.

- **On screen:** the sound plays at the normal level.
- **Off screen:** every placed sound listed in this spec plays at the **quiet level**, except the chop sounds and
  the error sound, which are silent off screen. (The error sound is in fact always a plain sound, so this never
  applies to it.)

**Levels.** With V the digital volume setting, the normal level is `floor(V × 99 / 100)` and the quiet level is
`floor(V × 24 / 100)`. Each level selects one step of the game's volume table. The stereo position is always the
centre.

### Subtitles

Five kinds of sound have a subtitle, a line in the message area. The text is a `Rez\network.tbl` entry (numbered
from 1):

| Sound | Entry |
| --- | --- |
| Every unit-ready file (the files named `...ready`, `...redy`), but not the gryphon rider's ready sound, which is `griffon\grwhat` | 75 |
| Work complete (`human\hwrkdone`, `orc\owrkdone`, `peasant\pswrkdon`) | 76 |
| Under attack, unit (`human\hhelp1`, `orc\ohelp1`) | 77 |
| Under attack, building (`human\hhelp2`, `orc\ohelp2`) | 78 |
| Building destroyed (`misc\bldexpl1` to `bldexpl3`) | 79 |

The subtitle is shown when the event reaches the play step and any of these holds:

- the subtitles option is on;
- the digital volume is 0;
- the event's own option (unit speech, for these events) is off.

A placed sound that was silenced by the fog, by being inside a unit, or by being off screen while not audible there
shows no subtitle.

### Selecting units

**When.** The selection sound is asked for by the selecting actions themselves, with the selection's **first unit**
(the first filled place of the selection list):

- **Classic rules:** only when exactly one unit is selected.
- **Enhanced rules:** also when several are selected.

**The rule.** The game remembers the last unit that made a selection sound, a **click count** for it, a separate
**critter count**, and a **selection cooldown**. For the selected unit U:

1. **Owner.** If U belongs neither to the local player nor to the neutral player (player 15), nothing plays. The
   remembered unit, the click count and the critter count are cleared.
2. **Cooldown.** If U is the remembered unit and the cooldown is still running, nothing happens. That click isn't
   counted.
3. **Start.** The cooldown is set to 10 ticks. If U isn't the remembered unit, U is remembered and both counts are
   set to 0.
4. **Pick the sound** by the first case that applies:
   - U is a completed building that shows the **large fire** (hit points below half of its maximum, half rounded
     down): `misc\burning`.
   - U isn't completed (a building under construction): `misc\constrct`. The cooldown becomes 30 ticks and the
     construction-sound timer ("Construction" below) becomes 150 ticks.
   - U is one of the buildings in Data, "Building selection sounds": that file.
   - U is any other type with a number above the critter's (0x39), meaning all other buildings and the map objects:
     `Button`.
   - U is a critter: see "Critters".
   - Otherwise U is a unit. The click count goes up by 1, and n = click count − 3 (0 if that is negative). With L the
     type's annoyance limit (Data, "Unit sounds"):
     - n = 0: one of the type's **selected** files (a draw if there are several);
     - 1 ≤ n ≤ L: the type's n-th **annoyed** file, with no draw. The cooldown becomes 30 ticks;
     - n > L: the click count goes back to 0, and one of the **selected** files plays (a draw if there are several).

     A type whose selected column says "none" plays nothing.
5. **Play** it as a plain sound: a building's sound under the building sounds option, any other type's under the
   acknowledgements option.

So with the cooldown expired each time, a footman's clicks give: three selected sounds, then annoyed 1 to 7 (clicks
4 to 10), then a selected sound that restarts the count (click 11), then three more selected sounds before annoyed 1
again.

### Critters

A critter selected under the rule above (step 4) does this instead of the unit case:

1. The critter count goes up by 1.
2. **On the 10th:** the critter count goes back to 0.
   - In single player (exactly one human), in this order:
     1. The critter is **killed** by the ordinary kill (`core/corpses.md`), with everything that kill does. The kill
        plays the critter's **death** sound like any other death: the terrain death file of the table below, as a
        placed sound on the critter, audible off screen.
     2. The selection code itself then creates the cosmetic explosion effect (effect kind 0x17), the same effect that
        the kill adds for a unit with the explode flag (`core/corpses.md`). The critter type doesn't have that flag,
        so the kill doesn't make it; the selection code does. It makes no draws and deals no damage.
     3. `misc\explode` plays as a plain sound, under the acknowledgements option.

     So both sounds play, the death file first. This all happens when the click is handled, not during a cycle's
     phases; see the open questions.
   - In multiplayer (more than one human) nothing is killed, and the critter's selected sound plays.
3. **Otherwise** the unit case runs with an annoyance limit of 1: the click count goes up by 1, n = 0 or n > 1 gives
   the selected sound, and n = 1 gives the annoyed sound.

The critter's files depend on the map's terrain (tileset):

| Tileset | Selected and acknowledge | Annoyed | Death |
| --- | --- | --- | --- |
| Forest | `misc\sheep` | `misc\shpisd1` | `misc\sheep` |
| Winter | `misc\seal` | `misc\sealpisd` | `misc\sealpisd` |
| Wasteland | `misc\pig` | `misc\pigpissd` | `misc\pigpissd` |
| Swamp | `misc\warthog` | `misc\wartpisd` | `misc\wartpisd` |

Note the forest exception: a dying sheep plays `misc\sheep`, while the other critters play their annoyed file.

So in single player, with the cooldown expired each time, a sheep gives: selected on clicks 1 to 3, annoyed on click 4,
selected on clicks 5 to 8, annoyed on click 9, and it explodes on click 10.

### Acknowledging orders

When the local player gives an order, the acknowledgement plays for the selection's first unit. The Eye of Kilrogg
spell, which runs at once without a target, does the same.

1. The remembered unit of "Selecting units" is forgotten, so the next click on any unit starts a new click count.
2. If the unit belongs to the local player, its type's **acknowledge** sound plays (a draw if there are several), as
   a plain sound under the acknowledgements option. Types with "none" play nothing.

### Ready, work complete and under attack

These plain sounds play under the unit speech option, for units and buildings of the local player only.

- **Unit ready.** When training finishes and the new unit is placed, the trained type's **ready** sound plays (Data,
  "Unit sounds"), and the training building's **top-left tile** (not the new unit's tile) becomes the latest event
  location (traced with a break: `human06-build-stats-1800`, n = 643, vectors below). If the unit can't be created,
  there is no sound and a status message is shown instead.
- **Work complete.** It plays in these cases:
  - **Construction finished:** for each unit inside the finished building: each builder that comes out, and the
    tanker of a new oil platform, which goes straight into it (traced: `economy/build-train.md`, V53). In all rule
    sets. It plays as the unit is let out, before it is placed; whether a builder that then finds no room and dies
    also gets it is open (below).
  - **Research or an upgrade finished:** for the building. In Enhanced rules only.
  - **A building upgrade finished:** for the building, only when it becomes a keep, stronghold, castle or fortress.
    A scout tower becoming a guard or cannon tower plays nothing (traced: `economy/build-train.md`, V51). In
    Enhanced rules only.

  When construction or a building upgrade finishes, the building's top-left tile becomes the latest event location
  (traced with a break: `human06-build-stats-1800`, vectors below).

  The file is `peasant\pswrkdon` if the unit is a peasant (type 0x02). Otherwise it is `human\hwrkdone` (orc set:
  `orc\owrkdone`). So a peon that finishes a building plays the race-set file, not a peasant file.
- **Under attack.** When an attack hits a unit or building of the local player, takes hit points, and doesn't kill
  it. Hits that unholy armour blocks don't count. Each unit has a **quiet counter**. The sound plays only if all of these hold:
  - the game isn't in demo playback;
  - the victim isn't a critter-flagged type;
  - the global **help timer** is 0;
  - the victim's quiet counter is 0.

  Then:
  1. Every unit referenced from a tile of the 23 × 23 square centred on the victim gets its quiet counter set to 200.
     This includes units referenced from either the ground or the air layer, and the square is clipped to the map.
  2. The help timer is set to 60 ticks.
  3. `human\hhelp1` (orc set: `orc\ohelp1`) plays for a unit, `human\hhelp2` (orc set: `orc\ohelp2`) for a building.
  4. Its position becomes the latest event location.

  Quiet counters go down by 1 on every second tick, so 200 lasts 400 ticks.

### Timers

The selection cooldown, the construction-sound timer and the help timer each go down by 1 per **interface tick**
and stop at 0. The tick rate is an open question.

### Attacks

- **Units.** Each time a unit turns to face its target to strike, it gets its type's **attack** sound as its
  pending sound (Data, "Unit sounds"). The sword types pick one of `misc\sword1` to `sword3` with a draw at that
  moment. The animation's play-sound instruction (`core/animation-scripts.md`) plays the pending sound, as a placed
  sound on the unit, and clears it. The sound is audible off screen. These draws happen for every attacking unit,
  seen or not: the variant is drawn in the simulation update, when the sound is made pending, and held with it, so a
  pending sound that is cleared (a command given later in the same update) or never heard has still made its draw,
  as for the chop sound (`economy/harvest.md`, "Chop sound") (static study; combat audit erratum 6).
- **Workers chopping.** Each chop picks one of `misc\tree1` to `tree4` with a draw (`economy/harvest.md`). It is
  played in the same way, but it is silent off screen.
- **Towers.** When a tower fires, guard towers (types 0x60, 0x61) play `misc\bowfire` and cannon towers (0x62, 0x63)
  play `misc\fireball`. Both are placed sounds on the tower, audible off screen.

### Deaths

**A unit killed** (not while the map is being set up) plays a placed death sound on itself, audible off screen. The
first rule that matches its type and flags picks it:

1. The air flag, the explode flag, or flag bit 25 (in the default data: dwarven demolition squad and goblin sappers):
   `misc\explode`.
2. Type 0x39 (critter): the death file of the terrain table in "Critters".
3. Type 0x37 (skeleton): `misc\Skeleton Death`.
4. The critter flag (bit 4): `orc\odead`. In the default data every type with this flag has already matched rule 1,
   2 or 3.
5. The sea flag (bit 3): `ships\shipsink`.
6. Otherwise an even type number plays `human\hdead` and an odd one plays `orc\odead`.

**A building killed** plays one of `misc\bldexpl1` to `bldexpl3` (a draw) on itself, audible off screen, with
subtitle 79. The variant is drawn at the kill, in the simulation update, when the sound is made pending, whether or
not it is then heard (static study; combat audit erratum 6). A building that has the used mark (`core/corpses.md`) or
is a wall piece (types 0x67, 0x68) makes no sound and no draw.

### Missiles, explosions and other effects

**Missile impacts.** When a missile reaches its impact, the sound for its kind plays at the missile's tile, audible
off screen. "If damaging" means the sound plays only when the missile carries damage (stored damage not 0).

| Missile kind | Sound |
| --- | --- |
| 0x00 lightning, 0x01 gryphon hammer, 0x02 dragon breath, 0x05 blizzard, 0x1B demon fire | `misc\firehit`, if damaging |
| 0x0E ballista bolt | `misc\explode`, if damaging |
| 0x0F arrow, 0x10 axe | `misc\bowhit`, if damaging |
| 0x07 big cannon, 0x11 submarine missile, 0x12 turtle missile | `misc\explode` |
| 0x15 ballista impact, 0x1A cannon tower explosion | `misc\explode`, at the effect's start step (`spells/spells.md`, "When a new effect first runs"), with no damage condition (`combat/damage.md`, "Impact effects") |
| All other kinds | none |

The impact effects' sound plays at the effect's tile, audible off screen, at its start step, one cycle after its first
update: the cycle after the impact, or the one after that when the effect's slot comes before the missile's
(`spells/spells.md`, "When a new effect first runs"). Kind 0x17, the effect left by submarine and turtle missiles, makes
no sound of its own. So each impact sounds like this:

| Missile | Impact sound | Effect sound | In all |
| --- | --- | --- | --- |
| Catapult rock | none | `misc\explode` | 1 |
| Ballista bolt | `misc\explode` if damaging | `misc\explode` | 1 or 2 |
| Big cannon | `misc\explode` | `misc\explode` | 2 |
| Small cannon | none | `misc\explode` | 1 |
| Submarine and turtle missiles | `misc\explode` | none | 1 |

**Other explosions.** `misc\explode` also plays at a tile, audible off screen, when:

- a demolition unit blows up (dwarven demolition squad, goblin sappers);
- a rune is set off;
- a flame shield's bolt arrives and makes its flames;
- a flying hammer or dragon breath splashes with a free effect slot, at the missile's tile (`combat/damage.md`,
  "Gryphon hammer and dragon breath").

**Construction.** The sound is asked for on every construction pass of a building under construction whose pass
counter is a multiple of 12 (`economy/build-train.md`, "The construction pass"): each step, the pass that shows a
damaged site full, and the completion pass. With the build-speed cheat on it is asked for on every pass, since every
pass is then a step (traced with a break on the request: `human06-build-stats-1800`, vectors below; the full-progress
pass of a damaged site is static study). When it is asked for and the construction-sound timer is 0,
`misc\constrct` plays on the building. It is a placed sound, audible off screen, under the building sounds option.
The timer is then set to 150 ticks. There is one timer for all buildings, so the sound repeats at most once per 150
ticks.

**Docking.** When a transport (flag bit 10) moves onto a shore-water tile, `misc\dock` plays on it. It is a placed
sound, audible off screen.

**Rescue and capture.** `misc\hcapture` (orc set: `misc\ocapture`) plays on a rescuable unit when it joins a
human-controlled player, only for the unit that was touched, not for the rest of its side that follows it.
`misc\hrescue` (orc set: `misc\orescue`) plays on a circle of power for each rescuable unit it rescues. Both are placed
sounds, audible off screen (`game/rescue.md`).

**Placing a building.** A click that places a building plays `misc\thunk`. A click on a site that fails the site
check plays `misc\error` and shows the reason (`ui/placing-buildings.md`). Both are plain sounds. Nothing else plays
the error sound.

### Spells

When a spell takes effect, its sound plays as a placed sound (on the unit or tile the spell acts on), audible off
screen. The Eye of Kilrogg sound is a plain sound.

| Spell | Sound |
| --- | --- |
| Bloodlust | `spells\blodlust` |
| Death and decay | `spells\decay` |
| Death coil | `spells\dethcoil` |
| Exorcism | `spells\exorcism` |
| Flame shield | `spells\flamshld` |
| Haste | `spells\haste` |
| Healing | `spells\heal` |
| Holy vision | `spells\holyvisn` |
| Blizzard | `spells\icestorm` |
| Invisibility | `spells\invisibl` |
| Polymorph | `spells\morph` |
| Slow | `spells\slow` |
| Unholy armour | `spells\unhlyarm` |
| Whirlwind | `spells\whrlwind` |
| Raise dead | `misc\Raise Dead` |
| Runes | `misc\Runes` |
| Eye of Kilrogg | `misc\Eye of Killrog` |

## Data

### Unit sounds (default unit data)

"h…/o…" means the human set plays the first file and the orc set the second. "a–b" is a range of numbered variants,
picked with a draw. L is the annoyance limit; the annoyed files are numbered 1 to L. Death sounds come from the rules
in "Deaths"; this table shows the result for the default flags.

| Type | Selected | Annoyed (L) | Acknowledge | Ready | Attack | Death |
| --- | --- | --- | --- | --- | --- | --- |
| 0x00 footman | human\hwhat1–6 / orc\owhat1–6 | hpissed1–7 / opissed1–7 (7) | human\hyessir1–4 / orc\oyessir1–4 | human\hready / orc\oready | sword | hdead |
| 0x01 grunt | as footman | as footman | as footman | as footman | sword | odead |
| 0x02 peasant | peasant\pswhat1–4 | pspissd1–7 (7) | peasant\psyessr1–4 | peasant\psready | misc\peonatak | hdead |
| 0x03 peon | as footman | as footman | as footman | peon\pnready | misc\peonatak | odead |
| 0x04 ballista | Button | none | misc\catyessr | human\hready / orc\oready | misc\catapult | explode |
| 0x05 catapult | as ballista | none | as ballista | as ballista | misc\catapult | explode |
| 0x06 knight | knight\knwhat1–4 | knpissd1–3 (3) | knight\knyessr1–4 | knight\knready | sword | hdead |
| 0x07 ogre | ogre\ogwhat1–4 | ogpissd1–5 (5) | ogre\ogyessr1–3 | ogre\ogready | misc\punch | odead |
| 0x08 archer | elves\ewhat1–4 | epissed1–3 (3) | elves\eyessir1–4 | elves\eready | misc\bowfire | hdead |
| 0x09 axethrower | troll\trwhat1–3 | trpissd1–3 (3) | troll\tryessr1–3 | troll\trready | misc\axe | odead |
| 0x0A mage | wizard\wzwhat1–3 | wzpissd1–3 (3) | wizard\wzyessr1–3 | wizard\wzready | misc\fireball | hdead |
| 0x0B death knight | deathknt\dkwhat1–2 | dkpissd1–3 (3) | deathknt\dkyessr1–3 | deathknt\dkready | spells\touchdrk | odead |
| 0x0C paladin | paladin\pkwhat1–4 | pkpissd1–3 (3) | paladin\pkyessr1–4 | paladin\pkready | sword | hdead |
| 0x0D ogre-mage | ogremage\omwhat1–4 | ompissd1–3 (3) | ogremage\omyessr1–3 | ogremage\omready | misc\punch | odead |
| 0x0E dwarven demolition squad | dwarf\dwhat1–2 | dwpissd1–3 (3) | dwarf\dwyessr1–5 | dwarf\dwready | sword | explode |
| 0x0F goblin sappers | goblin\gowhat1–4 | gopissd1–3 (3) | goblin\goyessr1–4 | goblin\goready | sword | explode |
| 0x10 attack peasant | as peasant | as peasant | as peasant | none | misc\peonatak | hdead |
| 0x11 attack peon | as footman | as footman | as footman | none | misc\peonatak | odead |
| 0x12 ranger | as archer | as archer | as archer | elves\eready | misc\bowfire | hdead |
| 0x13 berserker | as axethrower | as axethrower | as axethrower | troll\trready | misc\axe | odead |
| 0x14 Alleria | aleria\alwhat1–3 | alpissd1–3 (3) | aleria\alyessr1–3 | none | misc\bowfire | hdead |
| 0x15 Teron Gorefiend | teron\tewhat1–3 | tepissd1–3 (3) | teron\teyessr1–3 | none | spells\touchdrk | odead |
| 0x16 Kurdran and Sky'ree | kurdran\kuwhat1–3 | kupissd1–3 (3) | kurdran\kuyessr1–3 | none | none | explode |
| 0x17 Dentarg | dentarg\odwhat1–3 | odpissd1–3 (3) | dentarg\odyessr1–3 | none | misc\punch | odead |
| 0x18 Khadgar | khadgar\khwhat1–3 | khpissd1–3 (3) | khadgar\khyessr1–3 | none | misc\fireball | hdead |
| 0x19 Grom Hellscream | grom\grwhat1–3 | grpissd1–3 (3) | grom\gryessr1–3 | none | sword | odead |
| 0x1A human tanker | ships\hshpwht1–3 / oshpwht1–3 | hshppis1–3 / oshppis1–3 (3) | ships\foghorn | ships\hshpredy / oshpredy | misc\fireball | shipsink |
| 0x1B orc tanker | as human tanker | as human tanker | ships\hshpyes1–3 / oshpyes1–3 | as human tanker | misc\fireball | shipsink |
| 0x1C to 0x21 transports, destroyers, battleship, juggernaught | as orc tanker | as orc tanker | as orc tanker | as orc tanker | misc\fireball | shipsink |
| 0x22, 0x24, 0x25, 0x30, 0x36 (unused) | none | none | none | none | 0x22: misc\fireball; others none | by the rules |
| 0x23 Deathwing | deathwng\dewhat1–3 | depissd1–3 (3) | deathwng\deyessr1–3 | none | none | explode |
| 0x26 gnomish submarine | as orc tanker | as orc tanker | as orc tanker | as orc tanker | misc\fireball | shipsink |
| 0x27 giant turtle | human\hwhat1–6 / orc\owhat1–6 | ships\subpiss1–4 (4) | as orc tanker | as orc tanker | misc\fireball | shipsink |
| 0x28 gnomish flying machine | Button | gnome\gnpissd1–5 (5) | gnome\gnyessr1 | gnome\gnready | none | explode |
| 0x29 goblin zeppelin | Button | zeppelin\gbpissd1–2 (2) | zeppelin\gbyessr1 | zeppelin\gbready | none | explode |
| 0x2A gryphon rider | griffon\grwhat | none | griffon\griffon1–2 | griffon\grwhat | none | explode |
| 0x2B dragon | dragon\drwhat | none | dragon\dryessr1–2 | dragon\drready | none | explode |
| 0x2C Turalyon | turalyon\tuwhat1–3 | tupissd1–3 (3) | turalyon\tuyessr1–3 | none | sword | hdead |
| 0x2D eye of Kilrogg | Button | none | misc\Eye of Killrog Move | none | none | explode |
| 0x2E Danath | danath\dnwhat1–3 | dnpisd1–3 (3) | danath\dnyessr1–3 | none | sword | hdead |
| 0x2F Korgath Bladefist | kargath\kawhat1–3 | kapissd1–3 (3) | kargath\kayessr1–3 | none | sword | odead |
| 0x31 Cho'gall | as ogre-mage | as ogre-mage | as ogre-mage | none | misc\fireball | odead |
| 0x32 Lothar | as knight | as knight | as knight | none | sword | hdead |
| 0x33 Gul'dan | as death knight | as death knight | as death knight | none | misc\fireball | odead |
| 0x34 Uther Lightbringer | as paladin | as paladin | as paladin | none | sword | hdead |
| 0x35 Zuljin | as axethrower | as axethrower | as axethrower | none | misc\axe | odead |
| 0x37 skeleton | Button | none | misc\Skeleton Move | none | misc\punch | misc\Skeleton Death |
| 0x38 daemon | Button | none | Button | none | none | explode |
| 0x39 critter | by terrain ("Critters") | by terrain | the selected file | none | none | by terrain |

Notes on the table:

- "sword" is one of `misc\sword1` to `sword3`, with a draw. "explode" is `misc\explode`. "hdead" and "odead" are
  `human\hdead` and `orc\odead`, which are the same in both sets.
- The annoyed files of every type are in the same folder as its selected files, unless the table names another one.
- The giant turtle's selected sounds are the generic footman/grunt voices, and its annoyed sounds are the submarine
  files. The human tanker's acknowledgement is the foghorn. Both are taken from the data as they are.
- A ready sound plays only for trained units, so the "none" entries of hero types never matter in normal play.

### Building selection sounds

| Types | Sound |
| --- | --- |
| 0x5C gold mine | `bldg\mine` |
| 0x48, 0x49 shipyards | `bldg\shipbell` |
| 0x46 gryphon aviary, 0x47 dragon roost | `bldg\aviary` / `bldg\dragon` (by the sound set) |
| 0x4E, 0x4F foundries | `bldg\foundry` |
| 0x44 gnomish inventor, 0x45 goblin alchemist | `bldg\inventor` / `bldg\alchemst` (by the sound set) |
| 0x50 mage tower, 0x51 temple of the damned | `bldg\wzrdtowr` / `bldg\dthtower` (by the sound set) |
| 0x3A farm, 0x3B pig farm | `bldg\hfarm` / `bldg\ofarm` (by the sound set) |
| 0x52, 0x53 blacksmiths | `bldg\smith` |
| 0x3E church, 0x3F altar of storms | `bldg\hchant` / `bldg\ochant` (by the sound set) |
| 0x42 stables, 0x43 ogre mound | `bldg\stables` / `bldg\ogrecamp` (by the sound set) |
| 0x56, 0x57 oil platforms | `bldg\oilplat` |
| 0x54, 0x55 refineries | `bldg\oilrefin` |
| Any type with the wood-storage flag (bit 18; default data: 0x4C, 0x4D lumber mills) | `bldg\lumbmill` |

The rows are tested in this order, after the large-fire and under-construction cases. The variant of a race pair
follows the **sound set**, not the building's race. So a human player who selects a captured pig farm hears
`bldg\hfarm`.

### Option bits in the preferences

The four on/off options are stored as bits of one preferences word: unit speech 0x01, acknowledgements 0x02,
building sounds 0x04, subtitles 0x40000 (`ui/game-menu.md`, "Stored settings", lists the others).

## Test vectors

Unless stated otherwise: single player, local player human (human sound set), all options on, every click after the
selection cooldown has expired, and r is the setup-shuffle draw.

| Input | Expected |
| --- | --- |
| Select own footman once, r = 1000 | `human\hwhat4` (floor(1000 / 256) = 3, 3 mod 6 = 3); 1 draw; cooldown 10 |
| Select own footman 11 times | Clicks 1–3: one of hwhat1–6 (1 draw each); clicks 4–10: hpissed1–7 in order (no draws, cooldown 30 each); click 11: one of hwhat1–6 and the click count goes back to 0 |
| Select own knight 7 times | Clicks 1–3: knwhat; 4–6: knpissd1–3; 7: knwhat and the count goes back to 0 |
| Same footman clicked twice within 10 ticks | The second click plays nothing and isn't counted |
| Select own footman 3 times, give it an order, select it again | The order plays an acknowledgement and forgets the remembered unit, so the 4th selection is a selected sound, not hpissed1 |
| Select own footman, then own archer, then the footman 3 more times | The archer resets the count: the footman's next three are selected sounds |
| Select an enemy unit | Nothing; remembered unit and counts cleared |
| Select a neutral gold mine | `bldg\mine` (building sounds option) |
| Select own town hall | `Button` |
| Select own farm under construction | `misc\constrct`; cooldown 30; construction-sound timer 150 |
| Select own barracks with 300 of 800 hit points | `misc\burning` (300 < 400) |
| Select own barracks with 400 of 800 hit points | `Button` (not below half) |
| Orc local player selects own grunt; human local player selects a grunt it controls | `orc\owhat…` in the first case, `human\hwhat…` in the second |
| Sheep (forest), single player, clicked 10 times | Clicks 1–3: `misc\sheep`; 4: `misc\shpisd1`; 5–8: `misc\sheep`; 9: `misc\shpisd1`; 10: the sheep is killed (`misc\sheep` plays as its death sound), the explosion effect is created, then `misc\explode` plays |
| Same in multiplayer | Click 10: `misc\sheep`, nothing killed |
| Pig (wasteland) killed | `misc\pigpissd` |
| Sheep (forest) killed | `misc\sheep` |
| Skeleton killed | `misc\Skeleton Death` |
| Daemon killed | `misc\explode` (air flag) |
| Goblin sappers killed | `misc\explode` (flag bit 25) |
| Elven destroyer killed | `ships\shipsink` |
| Grunt killed (type 1, odd) | `orc\odead` |
| Barracks destroyed, r = 600 | `misc\bldexpl3` (floor(600 / 256) = 2, 2 mod 3 = 2); subtitle 79 shown only if subtitles are on |
| Wall piece completed or destroyed | No building explosion sound, no draw |
| Footman trained, own | `human\hready`, subtitle 75 if subtitles are on |
| Footman trained, own, unit speech option off | No sound; subtitle 75 shown |
| Gryphon rider trained, own | `griffon\grwhat`, no subtitle |
| Peasant leaves a finished farm | `peasant\pswrkdon`, subtitle 76 if subtitles are on |
| Peon leaves a finished farm, local player orc | `orc\owrkdone` |
| Upgrade finishes in Classic rules | No work-complete sound |
| Own footman hit for 5 (survives), timers at 0 | `human\hhelp1`; help timer 60; every unit in the 23 × 23 square gets quiet counter 200 |
| Second hit on any unit in that square 100 ticks later | Silent (help timer is 0, but the quiet counters are about 150) |
| Own footman killed by one hit | No under-attack sound; death sound `human\hdead` |
| Archer arrow with damage hits | `misc\bowhit` at the impact tile |
| Arrow impact on a tile the local player doesn't see | Silent |
| Footman attacks off screen, visible | sword sound at the quiet level: floor(V × 24 / 100) |
| Worker chops off screen | Silent; the draw still happens |
| View at (10, 10), 15 × 11 tiles; placed sound at (25, 21) | On screen (25 ≤ 10 + 15, 21 ≤ 10 + 11) |
| Same view; placed sound at (26, 21) | Off screen: quiet level, or silent for chop and error sounds |
| Death sound of a passenger killed with its transport | Silent (inside a unit) |
| Two buildings under construction, both visible, construction steps every 12 cycles | `misc\constrct` at most once per 150 ticks in total |
| Two grunts each make a strike decision in the same update, both off screen (static study; combat audit erratum 6) | Two setup-shuffle draws in that update, one per attack sound made pending, though neither is heard |
| `human06-build-stats-1800` (oracle trace, Enhanced), break on the construction-sound request: farm 1521 (T = 100), site created at n = 68 | 101 requests: pass counts 12 to 1200 (n = 79 to 1267, the 100 steps) and 1224 (n = 1279, the completion pass); none at 1212, which the counter jumps to at step 100 |
| Same, wall piece 1519 (created at n = 114) | 31 requests: the 30 steps (pass counts 12 to 360) and 384 at n = 485, its completion |
| Same, farm 1516 (created at n = 949); the build-speed cheat goes on after n = 1401 | Requests every 12 passes to n = 1392; from n = 1402 one every pass, at pass counts 465, 477, …, 1209 (63 passes, each a step: 176 HP at n = 1402 to 400 at n = 1464), then 1224 at n = 1465, the completion |
| Same, break on the latest-event-location call: barracks 1588 (top-left (88, 5)) finishes a footman, placed at (87, 5), n = 643 | The event location is (88, 5) |
| Same: completions at n = 485, 1279, 1465 and a keep upgrade at n = 1468 | Event locations (80, 9), (76, 5), (84, 2) and (82, 4): each building's top-left tile |

## Open questions

- **Work complete for a builder with no room.** When construction finishes, the sound is asked for each unit inside
  before it is placed, so static study says a builder that finds no free tile and dies still plays it. No trace has
  a builder with no room. Default: it plays. Trace request BT3 (`specs/ERRATA-buildtrain-consolidation.md`).
- **Tick rate.** The selection cooldown (10 or 30), the construction-sound timer (150), the help timer (60) and the
  quiet counters (200, going down every second tick) count interface ticks. Whether a tick is one game cycle or a
  fixed real-time interval isn't established. Default: one tick per game cycle.
- **Which selecting actions ask for the sound.** The selection sound is asked for by several selecting actions:
  clicking, dragging a box, recalling a numbered group, and others. Which of them do, and with which unit, isn't
  mapped yet. Default: every action that changes the local player's selection asks for it, with the first unit.
- **Sound file flags.** Each sound file carries flags that the mixer uses: whether a new start replaces one already
  playing, and whether the file is shared with an earlier entry. Their meaning for overlapping sounds, and how many
  sounds can play at once, isn't covered. Default: every play starts a new sound and overlaps freely.
- **A unit state bit silences placed sounds.** Besides being inside a unit, one more unit state bit silences placed
  sounds on that unit. Its meaning isn't identified. Default: ignore it.
- **When the critter explosion happens.** The kill on the 10th click is made by the selection code when the click is
  handled, outside the cycle's phases. Only single player is affected, so no other machine has to agree. Default:
  the kill is applied between two cycles, before the next cycle's phase 1, and it makes the same draws as any other
  kill would at that point.
- **3-D sound option.** The dialog's 3-D sound option isn't consulted by any of the paths above. Default: it changes
  nothing here.
- **Unused files.** `spells\iokilrog`, `spells\thunder`, `spells\spell` and `misc\fist` aren't referenced by any
  event found. Default: unused.
- **Spell sound placement.** Which unit or tile each spell's sound is placed on (caster or target) isn't recorded per
  spell. Default: the target unit or tile, or the caster for spells without one.
