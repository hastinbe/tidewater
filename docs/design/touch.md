# Touch input

Touch is **our own addition**: the original game has no touch input. The rule behind all of it is that touch only
*translates*: fingers become the mouse and keyboard events the game already understands, so no rule of the game
changes, Enhanced stays the default, and with no touch input every frame, sound and oracle trace is exactly what it
was. This document covers phases T0 (the input layer) and T1 (desktop emulation); the iOS shell
(T2, [ios.md](ios.md)) turns touch on from the first frame; the Android shell (T2b, [android.md](android.md)) does the same.

## Where it lives

| Module | Job |
|---|---|
| `input` | `Touch { id, phase, pos }` (window points) and `WindowGeom`, which now also carries the safe area, the density and whether the fit is fractional. |
| `gesture` | The state machine: fingers in, mouse frames and view intents out. No clock, no engine. |
| `chrome` | The on-screen controls: layout in the pillarbox, hit tests, the overlay picture. |
| `touchemu` | The mouse as fingers for `--touch-emulation`. |
| `touch` | The core's glue (`Core::touch`): runs the above in the frame's input step. |
| `display` | `Letterbox::fit_area`: the fractional "sharp bilinear" fit inside the safe area. |
| `mouse` | `tap_snap`: the tap tolerance. |
| `shell/sdl.rs`, `shell/main.rs` | SDL finger events, the safe area, the overlay texture, the sharp-bilinear pass, the soft keyboard. |

## Gestures

| Fingers | Gesture | Becomes |
|---|---|---|
| 1 | **Tap**: lift within 450 ms, moved under 3 mm | Left press, then release a frame later (right in right-click mode) |
| 1 | **Drag**: moves past 3 mm | Left button held from where the finger went down: box select, minimap drag, a placement drag |
| 1 | **Long press**: 450 ms, moved under 3 mm | Right click where the finger is, and a pulse ring where it fired |
| 2 | Move together | **Pan** of the map view (lists scroll as a wheel outside a game) |
| 2 | Spread or pinch | **Zoom** of the map view, from the original's view up to 4 times that |
| 3 | **Tap**: all up within 450 ms, none moved | F10, the game menu |

**One finger decides late.** A finger down does nothing until it moves past the slop, lifts or reaches 450 ms, so a
second finger can still turn it into a pan or pinch without a stray click. A second finger while one is undecided
cancels it silently; while a drag holds the left button it releases it where the finger is. When one of several
fingers lifts, the rest are ignored until all are up. A cancelled finger releases what it held and nothing more.

**Mouse frames.** The core's mouse model drops a release that arrives in the same frame as its press, so the machine
emits one frame at a time (`Gesture::pop`) and the core applies one per game frame: press, then release, then any
pointer move after a press. The pointer exists only while a click or drag is in progress; a lifted finger leaves no
cursor to scroll the map's edge.

**Sizes in millimetres.** Slop, pan and pinch thresholds, the tap tolerance and the chrome's buttons are millimetres,
turned into points with the display's density. The shell passes the best density it knows (`WindowGeom::ppi`: 140
points to the inch on a desktop, 160 on a phone or tablet, or `--touch-dpi`).

## Tap tolerance

A finger is wider than a unit. A tap on the map that lands on no unit's selection box, but within **6 mm** of one,
selects the nearest unit the local player can pick (`mouse::tap_snap`). The tap is moved to that unit's centre and the
ordinary release rules (`mouse::release`) run on it unchanged, so selection rules, double-click and modifiers all
behave as for a mouse. This is client-only: it changes which point a Select is sent for, nothing in the sim. A tap on a
box, a drag or a mouse click never snaps.

## The chrome

A touchscreen has no Shift, Ctrl or Alt, and no pause key. The chrome puts them in the black bars beside the picture
(in the safe area), and a keyboard button for typing:

| Control | Does |
|---|---|
| **S**, **C**, **A** (Shift, Ctrl, Alt) | A one-shot latch: the key is held for the next click, then let go a frame after that click's release (the rules that read Alt when the button comes up need it held through the release). |
| **R** | Right-click mode: taps are right clicks until it is switched off. |
| **II** (pause) | Pauses or resumes, as the pause key does. In a game only. |
| **K** (keyboard) | Opens or closes the system's soft keyboard (`SDL_StartTextInput`). |

The latches, **R** and pause show in a game; the keyboard button always shows. The controls go in the left bar and
right bar, one column each, centred in the bar's height; if only one bar is wide enough they share it, and a window
with none gets a column over the picture's left edge. A control acts when its finger lifts inside it. The chrome is an
RGBA overlay of the window's size that the shell blends over the picture; it is never part of the frame (640 x 480; the game screen is never wider on a touch device, because the `widescreen`
option is ignored there: the pillarbox is where the chrome lives, see `client-core.md`, "Widescreen"), so frames and
snapshots do not depend on it.

## Scaling

`Letterbox::fit_area(size, safe, fractional)` fits the picture in the window's **safe area** (clear of notches,
rounded corners and the home bar). With `fractional` it uses any scale, down to a quarter, and the shell draws it
**sharp bilinear**: the picture is first scaled up by the next whole number with the nearest texel, then scaled down
to the exact size with a linear filter, so pixels stay crisp without shimmering at a fractional scale. A fit that is
a whole number is drawn directly. Touch platforms (iOS, Android) always fit fractionally. On a desktop the default is
whole numbers, as before; the client-only extra `scaling` (`--extra-scaling sharp`, values `integer` and `sharp`)
switches a desktop to the fractional fit.

## Zoom and pan of the map view

The view keeps its top-left corner on a whole tile (screen spec, "Layout"), so a pan moves it a tile at a time and
carries the remainder to the next frame. A pinch changes the view's zoom (`MapView::zoom`, map pixels per screen
pixel) between 1 (the original) and 0.25 (4 times magnification), snapping to 1 when close, and re-centres to keep
the map point between the fingers under them; the whole-tile rule leaves it off by up to half a tile at the highest
zoom. Pinch zoom is separate from `--free-zoom`, which stays a wheel-driven development option.

## Touch emulation (`--touch-emulation`)

The mouse acts as one finger, so every path above can be tried without a touchscreen, and the chrome shows.

| Mouse | Fingers |
|---|---|
| Left button down, drag, up | One finger |
| Alt held at the press | A second finger mirrored through the picture's centre (as in the iOS Simulator): drag outward to pinch, sideways to pan |
| Alt and Shift | The second finger moves beside the first: a two-finger pan |
| Alt and Ctrl | A third finger rests at the centre: let go for a three-finger tap |

The modifiers are read when the button goes down. The right and middle buttons and the wheel work as usual, Alt is
not passed to the game, and the mouse lock is off so dragging is not fought.

## Real touchscreens

Fingers on a screen the user touches directly arrive as SDL finger events (`SDL_EVENT_FINGER_*`, positions as a
fraction of the window, scaled to points). SDL's own mouse-from-touch and touch-from-mouse events are turned off
(`SDL_HINT_TOUCH_MOUSE_EVENTS` and `SDL_HINT_MOUSE_TOUCH_EVENTS` to 0) only when emulating or when a touchscreen is
attached, so a desktop mouse is unchanged. A trackpad's fingers (an *indirect* device) are not read as touches: they
are pointer input, and its scroll gestures keep working as the wheel. Touch mode ends when a real mouse moves with no
finger down.

## Scripted scenes

`script::Cmd::Finger(id, phase, x, y)` puts a finger event in at a screen pixel. The scenes `touch-select` (tap near a
peasant), `touch-order` (long press orders it), `touch-drag` (box select), `touch-pinch`, `touch-pan` and `touch-menu`
(three-finger tap) run on the default density and have their own snapshots (`tests/snapshots/touch-*.txt`); no other
snapshot changed. To look at the chrome and the fractional fit, run a scene in a larger window:

```
TIDEWATER_SCRIPT_WINDOW=1000x600 tidewater --script touch-order --script-out DIR --no-mouse-lock [--extra-scaling sharp]
```

It also writes `NAME-full.png`, the whole window with the chrome.

## Deviations from the original

All of touch is ours: the original has no touch, so there is nothing to be faithful to except the game's own rules,
which touch leaves alone. The choices to review are the 450 ms and 3 mm thresholds, the 6 mm tap tolerance, the chrome's
controls and layout, the pinch range (1 to 4 times, never zooming out past the original's view), and ignoring a
trackpad's fingers.
