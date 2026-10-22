# Design: optional improvements ("extras")

**Bottom line.** Tidewater plays and looks like the original unless you switch an extra on. Every extra is **off by
default**; with all of them off the frames, sounds, saved games, replays and oracle traces are byte-for-byte what they
were without the framework. Extras that only change the client live in the settings file and on the command line.
Extras that would change the simulation live in `Rules::extras`, travel with the game, and are never read from local
settings in a game with other players. Enhanced rules stay the target; Classic stays an opt-in for multiplayer.

## The options

The table is generated from the one registry in `crates/client/src/extras.rs` (`REGISTRY`); a test fails if this copy
drifts from it. **Scope** C is client-only. **C+L** is client-only but gives an edge, so in a game with other players
the session (the lobby, later) sets it for everyone instead of your own setting.

| Option | Flag | Values | Scope | Default | What it does |
|---|---|---|---|---|---|
| `health-bars` | `--extra-health-bars VALUE` | off, hovered, selected, always | C+L | off | Hit point bars over units: only those under the pointer, only selected ones, or all |
| `idle-worker` | `--extra-idle-worker` | on, off | C | off | An idle-worker button at the top of the screen and the F6 key: select your next idle worker |
| `game-clock` | `--extra-game-clock VALUE` | off, clock, clock-apm | C | off | The game clock over the map view, with or without your actions per minute |
| `group-bar` | `--extra-group-bar` | on, off | C | off | A strip over the map view's top left listing your control groups with their sizes: click one to recall it |
| `smart-attack-move` | `--extra-smart-attack-move` | on, off | C+L | off | A right-click on open ground with only fighting units selected attack-moves there instead of moving |
| `volume-voices` | `--extra-volume-voices VALUE` | 0 to 100 | C | 100 | Unit voices, in percent of the sound volume (100 leaves them as the game sets them) |
| `volume-effects` | `--extra-volume-effects VALUE` | 0 to 100 | C | 100 | Sound effects, in percent of the sound volume |
| `volume-music` | `--extra-volume-music VALUE` | 0 to 100 | C | 100 | Music, in percent of the music volume |
| `scaling` | `--extra-scaling VALUE` | integer, sharp | C | integer | How the picture fits the window: only whole-number scales, or any size drawn sharp bilinear (touch platforms always use sharp) |

- **Default** is the value when nothing says otherwise. For a volume, 100 means "leave the sound as the game's own
  dialog sets it", so the original sound dialog keeps its look and meaning.
- **Settings file.** The `[extras]` section of `settings.txt` lists every option as `name=value`, with a comment line
  per option. A missing section, an unknown name or a bad value leaves the default.
- **Command line.** `--extra-NAME` (with a value, except for `idle-worker`, which is a switch) sets an option for this
  run; `--no-extra-NAME` puts it back to its default. The last of a pair wins. The command line is never written back
  to the settings file. `tidewater --help` lists them under "Optional improvements".

## What each one does

- **Health bars** (`health-bars`). A three-row bar over a unit's selection box, as wide as the box, filled in the
  information panel's health colours (green from 75 percent, yellow from 50, red below; the panel's empty colour behind).
  `hovered` shows the bar of the unit under the pointer, `selected` those of selected units, `always` every visible unit.
  Drawn in the map view (`view.rs`) above units and effects, under the fog. Rules in `overlay.rs`.
- **Idle worker** (`idle-worker`). A button "Idle N (F6)" in the resource bar, right of the food item, and the F6 key
  (the original leaves F6 unbound). Either selects the next idle worker (a peasant or peon of yours with no order),
  going through them in creation order and wrapping, and centres the view on it. It sends the normal `Select`
  command, as a click would. Rules in `idle.rs`.
- **Game clock** (`game-clock`). The game's own clock (`M:SS`, `H:MM:SS` from the first hour) in the map view's top
  right corner; `clock-apm` adds your actions per minute (every command you give, selecting included, over the clock),
  shown from the first full second. Rules in `overlay.rs`.
- **Control-group bar** (`group-bar`). A strip of small buttons over the map view's top left corner, starting 4 pixels in
  from the corner, one for each control group 0 to 9 that has units (`1:5` is group 1, five units; units no longer in the
  game are not counted). A click on a button sends what the group's number key sends (`mouse::GroupKeys::press`: select
  the group, and a second click within the double-press time centres the view). It reads the synced groups
  (`World::selections`), the lists the number keys recall. **Where it sits:** that corner holds none of the original
  interface. The message list starts 194 pixels down the screen, the notice line is at the bottom, and the game clock
  (when on) is at the top right; ten buttons are at most 400 pixels long, so they never reach the clock. Rules in
  `groupbar.rs`.
- **Smart attack-move** (`smart-attack-move`, C+L). A right-click on open ground (no unit under the pointer) with only
  fighting units selected sends the Attack button's command for the ground (`Command::Issue`, command 8, no target),
  which the sim already turns into an attack-move, instead of the right-click's move. "Fighting" means every selected
  unit is yours, can attack, and is not a worker, an oil tanker or a building; one peasant in the selection keeps the
  normal move. The simulation is not changed. It follows the session in a game with other players because it saves an
  order. Rules in `attackmove.rs`.
- **Separate volumes** (`volume-voices`, `volume-effects`, `volume-music`). Master trims on top of the game menu's
  digital and music volumes, applied in the mixer (`audio.rs`, `Trims`). Voices are unit speech and acknowledgements;
  effects are every other sound (a screen's sounds and narration included); music is the jukebox. At 100 the mixer
  does exactly what it did before (the gain is not touched). The original sound dialog is unchanged.
- **Fractional scaling** (`scaling`, C). `integer` (the default) draws the 640 × 480 screen at the largest whole number
  of window pixels that fits, as always. `sharp` fits it by a fraction of the window's safe area: the picture is scaled
  up by the next whole number with the nearest texel and then scaled linearly down, so pixels stay crisp. Touch
  platforms use `sharp` whatever the setting says (`docs/design/touch.md`). Rules in `display.rs` (`Letterbox::fit_area`).

## Rebindable keys

Not an option with a switch: the `[keys]` section of `settings.txt` rebinds the game's keys and the command buttons'
keys. With no `[keys]` section every key is the original's (`keymap.rs`: the key table `mouse::key_action`, the buttons'
hotkeys from the string table, F6 for the idle worker). The game writes the section back exactly as it found it, so a
mistake in it is never lost.

```
[keys]
pause=ctrl+u
button-move=j
slot-1=q
idle-worker=ctrl+i
```

- **The line.** `action=key`, one per action. A key is optional `ctrl+`, `alt+` and `shift+` then a letter, a digit,
  `f1` to `f12` (not `f10`), `minus`, or `equal` (`plus` and `numpad-add` mean the same). Escape, Enter, Space, the
  arrow keys and F10 can't be rebound.
- **Game keys** need `ctrl+` or `alt+` with a letter (a bare letter belongs to the command buttons). Names:
  `speed-up`, `speed-down`, `pause`, `centre-primary`, `group-set-0` to `group-set-9`, `group-recall-0` to
  `group-recall-9`, `group-show-0` to `group-show-9` (Alt+digit), `goto-view-1` to `goto-view-3`, `remember-view-1` to
  `remember-view-3`, `game-menu`, `options`, `help-menu`, `keyboard-help`, `sound-dialog`, `speed-dialog`,
  `preferences`, `tips`, `save-game`, `load-game`, `restart`, `quit`, `quit-dialog`, `toggle-music`, `toggle-sounds`,
  and `idle-worker` (the key of the `idle-worker` option; F6 with any modifiers by default).
- **Command buttons** take a bare letter or digit. `button-move`, `button-attack`, `button-patrol`, `button-stop`,
  `button-stand-ground`, `button-harvest`, `button-repair`, `button-return-goods`, `button-unload`,
  `button-attack-ground`, `button-demolish`, `button-build-basic` and `button-build-advanced` give the button with that
  command a key of its own (its original hotkey stops working). Every other button is reached by place with `slot-1` to
  `slot-9`, the 3 × 3 panel read left to right and top to bottom, whatever the button is (so `q w e / a s d / z x c`
  works as a grid). A slot key wins over a button's own letter.
- **Conflicts.** A key on two actions, or a key an action you did not rebind still uses, is refused: the game prints
  `settings: [keys] pause=Ctrl+S: Ctrl+S is already used by toggle-sounds; the line is ignored` at start-up and the
  action keeps its original key. The first line wins; a second line for the same action is refused too. Rebinding an
  action frees its original keys, so two actions can swap. Two buttons that show at the same time with the same
  hotkey letter are not caught (which buttons show depends on the selection); the first in panel order acts.
- **Checks.** Rules and messages: `keymap.rs` tests. With no section the key map hands every key on unchanged (a test
  tries every key and modifier). The frame scene `extras-keys` rebinds Move, pause and the idle key.

## Sim options

`tidewater_sim::Extras` (`crates/sim/src/extras.rs`) is the set of options that change the simulation. It is empty for
now. It is a field of `Rules`, so it is:

- **hashed** with the world (`World::state_hash`), so two machines with different options desync at once;
- **saved** with the rest of the rules in a saved game;
- **carried in a replay's header** (`RulesTag::extras`), and playback starts the world from the header's value;
- to be **carried in the lobby's start message** (M3), with the session handing it to the game
  (`Session::sim_extras`).

A game never reads sim options from the local settings unless it is a new local game (`extras::local_sim_extras`).
A replay uses the recording's; a loaded game keeps the ones it was saved with.

**The layout today.** An empty struct writes no bytes (postcard), so adding it changed neither the saved game
(then version 14; it is version 15 since the round-92 load rules, for other reasons) nor the replay (version 1, unchanged) nor the state hash; the frame and oracle snapshots and
the whole-map snapshot are unchanged. **The first field changes the layout.** In that commit, bump
`savefile::VERSION` and `tidewater_replay::format::VERSION`, hash the field only when it is not at its default (as the
cheat flags are), and bless the save-file hashes in the frame snapshot.

## Multiplayer (C+L)

`extras::effective(local, session)` picks a game's values. A local session returns `None` from
`Session::lobby_client_extras` and the player's own settings are used. Any other session returns its own set, and
for the C+L options that set wins over the local settings. Until the lobby exists (M3) the only non-local session is a
replay, which hands over the defaults (off): a recording carries no health-bar choice of its own. The lobby will hand
over what its players agreed. Tests: `a_non_local_session_sets_the_c_plus_l_options_and_leaves_the_c_ones` and
`a_replay_follows_its_recording_and_a_local_game_the_player` in `extras.rs`.

## How to add an option

1. **Add a row** to `REGISTRY` in `extras.rs` (and an `Id` variant and the `COUNT`): name, kind, default (off), scope
   and a one-line description. The settings file, the flags and the help text follow.
2. **Add a typed accessor** on `ClientExtras` (like `health_bars()`), so the rest of the client never handles raw numbers.
3. **Read it where it acts** and keep the off path untouched: the code for the default must not run, or must be a
   no-op to the bit (see how `audio::trimmed` skips the multiply at unity). Put the rule in a small module with unit tests
   (like `overlay.rs`, `idle.rs`).
4. **Choose the scope.** If the option shows information the original hides, or lets one player act faster than another
   could, use C+L. Read it through `game::extras(c)`, not `c.extras`, so a session can override it.
5. **Test it with the option on**: unit tests for the rules, and a scripted scene whose `extras` field turns it on
   (`script.rs`, scene `extras`). A new scene is a new snapshot entry; the old scenes must not change.
6. **Document it**: copy `extras::doc_table()` into this file (a test checks), and add the flag to the README's
   "Optional improvements".
7. **If it touches the sim**, it is not a client option: add a field to `tidewater_sim::Extras` instead (and a registry
   row only for the local default), bump the save and replay versions, and make sure it is hashed, saved and in the
   replay header.

## Checks that keep the defaults honest

- The frame snapshot (`crates/client/tests/frames.rs`) runs its original scenes with every extra off and is never
  re-blessed for an extra; the scenes `extras`, `extras-bars`, `extras-groups`, `extras-attack` and `extras-keys` turn options on and have their own entries.
- `Trims::default()` leaves the mixed sound identical; the audio snapshot of the original scenes shows it.
- The oracle traces and the whole-map snapshot run on the sim, which an empty `Extras` does not change.
