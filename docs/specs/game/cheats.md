# Game: cheat codes

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 28 (cheats as typed
  text; gap 11)"); the cheat phrases as widely published in public cheat lists, each confirmed to match the program's
  check values; the developer words from the research notes; `Rez\network.tbl` for the notices. Not yet confirmed
  against the running game.
- **Changes:** 2026-10-16, round 83 (traces, `ERRATA-round83.md`, reviewed by a spec-review session, 2026-10-16): the
  all-spells step's scope, players controlled by a person only, is traced (AI3,
  `xhuman2-xhuman02-cheat-scope-1600`): a computer player's caster still lacks the spells its owner didn't know. The
  mana top-up at a spell command applies to every caster.

## Behaviour

Cheats are phrases typed as a chat line during a game, or given as arguments when the program starts. Each phrase
turns a **cheat flag** on or off, or does something once. This spec covers how lines are recognised, what happens
when they are, and the immediate effects. Where a cheat's lasting effect is described in another spec, that spec is
linked.

### Entering a cheat during a game

A chat line is typed as in `ui/messages.md` and sent with Enter. Before it is sent as chat, it is checked as a cheat
unless the game is a custom or multiplayer game whose lobby options disable cheats; with cheats disabled the line is
always chat.

1. The line is compared with the codes in the order of the table under "Data" (first match wins).
2. **Match:** the line isn't sent as chat. The cheat flags as changed by the code are sent as a **cheat command**
   through the normal command path (in multiplayer, to every player through the lockstep exchange). Everything below
   happens when the command is carried out, on every machine.
3. **No match:** the line is chat (multiplayer) or is dropped (single player).

### Matching

- **Phrase codes** are matched by a 32-bit check value, not by text. Letter case doesn't matter, and a space and an
  underscore count as the same character. The formula is under "Data". Any line with the same check value works as
  that code, so many lines besides the published phrases trigger cheats (for example, the 3-letter code that matches
  "day" is also matched by "are").
- Four phrase codes are only recognised in single-player games (marked in the table).
- **Word codes** are matched as whole words, ignoring letter case.
- **Mission select** is a prefix and a number (below).

### Carrying out a cheat command

When a cheat command arrives:

1. **Cheat used.** If it came from the local player and the new flags aren't all off, the mission counts as one in
   which the local player cheated (rank entry 39, `game/campaign-flow.md`).
2. **Cheats disabled.** In a multiplayer game whose options disable cheats, the command is ignored, after one draw
   from the simulation generator is taken and thrown away (`core/timing-and-rng.md`).
3. Otherwise the cheat flags become the new flags, and then the steps of "Effects when the flags change" run.

### Effects when the flags change

In this order:

1. **Music.** If the disco flag changed, the game's music track changes (`ui/music.md`, "Disco cheat").
2. **Reveal.** If the reveal-map flag changed, the minimap and fog drawing are refreshed.
3. **All spells** (while on). For every player controlled by a person: every spell and research is known and
   allowed, and its record of research under way is cleared (what this does to research already running in a
   building isn't traced). This repeats every time the flags change while it stays on. Computer players aren't
   covered (round 83; traced: `xhuman2-xhuman02-cheat-scope-1600`, a computer death knight that knows only
   death coil casts nothing else with the cheat on). The cheat's other spell effect, the mana top-up at each spell
   command (`spells/spells.md`, "Giving the order"), isn't limited this way: it applies to a computer caster's
   commands too (the same trace: its mana reads 255 at each death-coil command).
4. **All upgrades** (while on). For every player controlled by a person: weapon, armour and other levelled upgrades
   (7 groups) are set to level 2, and the 4 one-step upgrades to level 1. This also repeats.
5. **Resources** (one-shot; the flag clears at once). Every one of the 16 players gets 10000 gold, 5000 lumber and
   5000 oil.
6. **Oil** (one-shot). Every one of the 16 players gets 5000 oil.
7. Then **the first** of these that applies, which ends the steps:
   - **Notice A** (one-shot): during a game, a fixed notice of four characters.
   - **Notice B** (one-shot): during a game, `network.tbl` entry 72 as a notice.
   - **Notice C** (one-shot): during a game, a short fixed notice.
   - **Finale:** in single player only, the game ends and a campaign ending (video, then the finale dialog) starts
     at once: the *Beyond the Dark Portal* ending if the map is a *Beyond the Dark Portal* campaign map and the game
     isn't custom, otherwise the *Tides of Darkness* ending (`game/campaign-flow.md`, "Endings"). It isn't a
     victory. No notice. In multiplayer nothing happens.
8. **Notice.** If none of step 7 applied, and a game is running (not while the map is being set up): `network.tbl`
   entry 81 if the new flags, read as an unsigned 32-bit number, are less than the old ones, else entry 80. In
   multiplayer the sender's name comes first, then a space, then the entry. Notices last 7 s (`ui/messages.md`).

So turning a cheat off normally shows entry 81 and turning one on shows entry 80. The one-shot resource cheats leave
the flags as they were, so they show entry 80.

### Mission select

If the mission-select flag is on, a line that isn't any other code is checked as mission select. It doesn't work in
an Enhanced-rules multiplayer game.

- The line must start with one of 4 prefixes (letter case ignored); the number after it is read in decimal (leading
  spaces allowed, anything after the digits ignored).
- If the prefix matches, the line is used up (not sent as chat) even when the number is out of range.
- With n from 1 to the maximum: the campaign entry becomes 2n − 2 + race + (28 for *Beyond the Dark Portal*), the
  local race and campaign map follow from it, the game is no longer custom, and the current game ends and that
  mission starts, with its act screen and briefing (`game/campaign-flow.md`).

Mission select also sends a cheat command with the flags unchanged. The mission-select flag is on, so the command
always counts as cheating (but see "Open questions").

### Word codes

Developer words, matched exactly (letter case ignored):

| Word | Effect |
| --- | --- |
| `allowsync` | Toggles a network sync-check setting |
| `title` | Toggles "skip the startup videos" (`game/campaign-flow.md`) |
| `noglues` | Toggles "skip the act screen, briefing and score screens" (`game/campaign-flow.md`) |
| `showpath` | Toggles a debug flag; when it is turned on during a single-player game, fog of war is also turned off (`vision/fog.md`, the fog-off cheat) |
| `fastdemo` | Toggles a demo-speed setting |
| `netprof` | Toggles a network profiling setting |
| `showinfo` | Toggles a debug display; shows a fixed notice right away; not a cheat command |
| `showmemory` | Toggles a debug display; shows a fixed notice right away; not a cheat command |
| `nopalette` | Toggles the colour-cycling preference (`ui/palette-effects.md`); shows a fixed notice right away; not a cheat command |

The first six are recognised like phrase codes: the line becomes a cheat command, so the notice of step 8 follows.
The settings they toggle without a flag (sync check, demo speed, profiling) change only on the typing player's machine,
at once. The last three change their setting at once, show their notice and are neither chat nor a command.

### Command-line arguments

When the program starts, each argument (quotes group words) is checked against the phrase codes and the word codes. A
match changes the flags at once, with no notices and no music change. Other arguments are program options.

### Flags across games

When a game starts, every flag is cleared except mission select, "skip the startup videos" and "skip the act screen,
briefing and score screens". During attract-demo playback the reveal-map flag is set. The flags aren't saved with a
game. So a cheat typed in one mission doesn't carry into the next, but those three do.

### Lasting effects

| Flag | Effect | Spec |
| --- | --- | --- |
| Reveal map | Nothing hidden; whole map and minimap shown | `vision/fog.md`, `ui/map-drawing.md`, `ui/minimap.md` |
| Build speed | Faster building and training | `economy/build-train.md` |
| Invulnerability and damage | Units of human-controlled players deal fixed high damage; computer-controlled units deal none | `combat/damage.md` |
| All spells | Casting skips the mana test; mana set to 255 | `spells/spells.md`, `ui/command-buttons.md` |
| Lumber | Each chop finishes the tree at once (counter straight to 50) | `economy/harvest.md` |
| Instant victory, instant defeat | Victory or defeat at the next check | `game/victory.md` |
| No victory | Single-player victory checks are frozen | `game/victory.md` |
| Disco | Disco music | `ui/music.md` |

## Data

### Phrase codes

In checking order. "SP" marks codes recognised only in single-player games.

| Code (public phrase) | Check value | Flag | Kind |
| --- | --- | --- | --- |
| tigerlily | 0x0909CE59 | mission select | toggle |
| glittering prizes | 0x11079E4E | resources | one-shot |
| valdez | 0x0601A5AE | oil | one-shot |
| every little thing she does | 0x1B091606 | all spells | toggle |
| deck me out | 0x0B09B983 | all upgrades | toggle |
| on screen | 0x0900C02A | reveal map | toggle |
| make it so | 0x0A088C5B | build speed | toggle |
| it is a good day to die | 0x1708F242 | invulnerability and damage | toggle |
| hatchet | 0x07076FA0 | lumber | toggle |
| (no published phrase; "day" matches) | 0x030007BD | notice A | one-shot |
| (no published phrase; "183" matches, no letters-only line does) | 0x030003C3 | notice B | one-shot |
| ucla | 0x04001AA1 | notice C | one-shot |
| unite the clans (SP) | 0x0F0DE3CB | instant victory | toggle |
| you pitiful worm (SP) | 0x10063796 | instant defeat | toggle |
| there can be only one (SP) | 0x150FF32E | finale | toggle |
| never a winner (SP) | 0x0E0B5902 | no victory | toggle |
| disco | 0x05007893 | disco | toggle |

### Check value

For a line of L characters c1 … cL (bytes): map a space to an underscore and A–Z to a–z. Start with h = 0; for each
character, h = (h × 4) XOR c, and then, if h is 2^20 or more, h = (h mod 2^20) XOR floor(h / 2^12). The check value
is (L mod 256) × 2^24 + h. (h always stays below 2^20, so the two parts never overlap.)

### Mission-select prefixes

In checking order.

| Prefix | Race | Highest n | Campaign |
| --- | --- | --- | --- |
| `human` | human (0) | 14 | *Tides of Darkness* |
| `orc` | orc (1) | 14 | *Tides of Darkness* |
| `xhuman` | human (0) | 12 | *Beyond the Dark Portal* |
| `xorc` | orc (1) | 12 | *Beyond the Dark Portal* |

### Amounts

| Cheat | Amount |
| --- | --- |
| Resources | +10000 gold, +5000 lumber, +5000 oil, each of 16 players |
| Oil | +5000 oil, each of 16 players |
| All upgrades | 7 groups to level 2, 4 groups to level 1 |
| Notices | `network.tbl` 80 (on), 81 (off), 72 (notice B); 7 s |

## Test vectors

| Situation | Expected |
| --- | --- |
| Check value of "on screen" | 0x0900C02A |
| Check value of "ON_SCREEN" | 0x0900C02A (same code) |
| Check value of "are" | 0x030007BD: notice A, like "day" |
| Single player, type "glittering prizes" | Every player +10000 gold, +5000 lumber, +5000 oil; notice entry 80; the mission counts as cheated (the command carries the resource flag, so it is never all off) |
| Single player, type "on screen" twice | First: map revealed, entry 80. Second: reveal off, entry 81 |
| Multiplayer, "unite the clans" | Not a cheat in multiplayer: sent as chat |
| Multiplayer with cheats allowed, player 2 types "make it so" | Every machine turns build speed on; notice: player 2's name, a space, entry 80 |
| Multiplayer with cheats disabled, "make it so" | Sent as chat |
| "tigerlily" then "orc5" in a single-player game | Entry 80 for tigerlily; then orc mission 5 of *Tides of Darkness* (entry 9) starts with its act screen and briefing |
| "tigerlily" then "xhuman13" | Line used up (not chat); a cheat command with the flags unchanged is sent, so entry 80 shows; no mission change |
| "xorc 3" with mission select on | *Beyond the Dark Portal* orc mission 3: entry 33 |
| "human3" with mission select off | No code: dropped in single player |
| "there can be only one" in a *Beyond the Dark Portal* campaign mission | The *Beyond the Dark Portal* finale starts; no notice |
| "disco" while music plays in a custom orc game | Track 25 after a fade of all sound (`ui/music.md`) |
| "on screen", then the next mission starts | Reveal is off again |
| Program started with the argument `title` | Startup videos skipped; the flag stays on across games |
| "nopalette" during a game | Colour cycling preference toggled; a fixed notice; nothing sent |

## Open questions

- **Notice texts.** Notices A and C and the three developer-word notices are fixed texts in the program, not table
  entries. A reimplementation needs its own text or none. Default: show a short notice of your own.
- **Collisions.** Every line with a matching check value acts as a code, which also swallows chat lines in
  multiplayer. Whether to copy this (full fidelity) or match only the phrases is a design choice. Default: copy it
  (it's what the original does).
- **Empty line after a developer word.** In multiplayer, the three words that aren't cheat commands leave an empty
  line, which may then go out as an empty chat line. Not traced. Default: send nothing.
- **Command-line words.** How arguments are split (quotes, tabs) is only partly traced.
- **Cheat-used flag on mission select.** Mission select sends a cheat command, but the game is ending; whether that
  command is carried out before the mission changes hasn't been traced.
- **Running game.** Not checked in the running game.
