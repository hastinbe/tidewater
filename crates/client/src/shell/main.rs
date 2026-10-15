//! Tidewater: the client core (`tidewater_client::core`) in an SDL 3 window, the program's only shell
//! (`docs/design/client-core.md`, "Platform trait"). It loads the user's installation and runs the startup sequence, a
//! map given on the command line, a `--goto` screen or a scripted scene (`script`).
//!
//! The 640 × 480 screen is drawn at a whole number of physical pixels per screen pixel, black-bordered in a larger
//! window, and the map view shows one map pixel per screen pixel, as the original does (`display`). The mouse wheel
//! zooms only with the `--free-zoom` development option. The controls are in the core (`game`).
//!
//! The shell does what the design asks of one: the system's key, button, wheel and focus events into the core's input;
//! [`Core::advance`] once a frame; the composed frame into one streaming texture, drawn at the window's whole-number
//! scale with black around it (`display::Letterbox`), with the core's title and system-pointer wish; the core's mixer
//! on the sound device through an SDL audio stream (SDL converts the 22,050 Hz stereo to the device's rate); the
//! desktop's storage (`host::launch`); and the end on `Core::exit` after the settings are written. The mouse lock
//! (`display`, "Mouse lock") uses SDL's relative mode on macOS and its mouse grab elsewhere.
//!
//! A scripted run (`--script`) also records each frame (`script::record`) and, with a window, compares the window's
//! own pixels at each shot (`script::compare_shot`); `--headless` runs it without SDL at all (`host::run_headless`).

mod sdl;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use tidewater_client::args::Args;
use tidewater_client::audio::SAMPLE_RATE;
use tidewater_client::core::{Core, Platform};
use tidewater_client::display::{self, Letterbox};
use tidewater_client::host::{self, FrameStats, LockedPointer};
use tidewater_client::input::{Button, Buttons, Key, KeyEvent, Wheel, WindowGeom};
use tidewater_client::script;

/// The audio device's buffer asked for, in sample frames at the device's rate: about 10.7 ms at 48 kHz.
const DEVICE_FRAMES: u32 = 512;

fn main() -> Result<()> {
    let l = host::launch(Args::parse())?;
    if l.headless {
        host::run_headless(l.core);
        return Ok(());
    }
    run(l)
}

/// Whether the mouse lock holds the system's pointer still (macOS can't confine it to the window) rather than
/// confining it.
const HOLD_STILL: bool = cfg!(target_os = "macos");

fn run(l: host::Launch) -> Result<()> {
    let host::Launch {
        mut core,
        title,
        ui_scale,
        ..
    } = l;
    let scripted = core.script.is_some();
    let sdl = sdl::Sdl::init(!scripted, DEVICE_FRAMES)?;
    let points = ui_scale.unwrap_or(1);
    let (w, h) = display::window_size(points);
    // A scripted window has one pixel per point whatever the display, so the screen fills it exactly; it doesn't
    // take the keyboard (its input is the scene's) and stays on top, as the Bevy shell's does.
    let window = sdl::Window::new(
        &sdl,
        &title,
        w,
        h,
        sdl::WindowOpts {
            high_density: !scripted,
            resizable: !scripted,
            always_on_top: scripted,
            not_focusable: scripted,
        },
    )?;
    if !scripted {
        let d = window.pixel_density();
        let min = |len: u32| (len as f32 / d).ceil() as u32;
        window.set_min_size(min(display::SCREEN.0), min(display::SCREEN.1));
        // Without `--ui-scale`: the largest whole scale that fits the monitor, centred on it.
        if ui_scale.is_none()
            && let Some(((mw, mh), md)) = window.display()
        {
            let physical = ((mw as f32 * md) as u32, (mh as f32 * md) as u32);
            let (pw, ph) = display::window_size(display::monitor_scale(physical, md as f64));
            window.set_size(min(pw), min(ph));
        }
        window.centre();
    }
    let mut renderer = sdl::Renderer::new(&window, display::SCREEN, true)?;
    window.start_text_input();
    // A scripted run mixes its sound itself (`script::record`) and plays none.
    let audio_stats = Arc::new(sdl::AudioStats::default());
    let audio = if scripted {
        None
    } else {
        let output = core.audio.output();
        match sdl::AudioOut::open(&sdl, SAMPLE_RATE, move |buf| output.mix(buf), audio_stats.clone()) {
            Ok(a) => Some(a),
            Err(e) => {
                eprintln!("no sound: {e}");
                None
            }
        }
    };
    if let Some(a) = &audio
        && std::env::var_os("TIDEWATER_FRAME_STATS").is_some()
    {
        eprintln!("audio device (rate, channels, sample frames): {:?}", a.device_format());
    }

    let mut shell = Shell {
        window: &window,
        locked: LockedPointer::default(),
        grabbed: false,
        motion: (0.0, 0.0),
        lock_enabled: !core.opts.no_mouse_lock,
        cursor_shown: true,
    };
    let mut stats = FrameStats::default();
    let mut first = true;
    let mut last = Instant::now();
    loop {
        let now = Instant::now();
        let elapsed = match (scripted, first) {
            (true, _) => host::scripted_elapsed(first),
            (false, true) => Duration::ZERO,
            (false, false) => now - last,
        };
        let interval = now - last;
        last = now;
        first = false;
        let quit = shell.take_events(&mut core);
        core.advance(elapsed, &mut shell);
        let exiting = std::mem::take(&mut core.exit) || quit;
        if let Some(t) = core.title.take() {
            window.set_title(&t);
        }
        if shell.cursor_shown != core.cursor_visible {
            shell.cursor_shown = core.cursor_visible;
            sdl::show_cursor(core.cursor_visible);
        }
        if exiting {
            core.store_settings();
        }
        let size = renderer.output_size();
        let lb = Letterbox::fit(size);
        renderer.draw(&core.frame.rgba, lb.screen());
        let mut finished = false;
        if let Some(run) = core.script.as_mut() {
            let shown = (lb.scale == 1).then_some(&core.frame);
            let asked = script::record(run, shown, &core.audio.output(), exiting);
            for name in asked.screenshots {
                let shot = renderer.read_pixels();
                let shot = shot.as_ref().map(|(w, h, px)| (*w, *h, px.as_slice()));
                script::compare_shot(run, name, shot);
            }
            finished = asked.finished;
        }
        stats.frame(interval, now.elapsed());
        renderer.present();
        if exiting || finished {
            break;
        }
    }
    if audio.is_some() && std::env::var_os("TIDEWATER_FRAME_STATS").is_some() {
        print_audio_stats(&audio_stats);
    }
    drop(audio);
    Ok(())
}

fn print_audio_stats(s: &sdl::AudioStats) {
    use std::sync::atomic::Ordering::Relaxed;
    let calls = s.calls.load(Relaxed);
    let span = s.last_ns.load(Relaxed).saturating_sub(s.first_ns.load(Relaxed)) as f64 / 1e6;
    eprintln!(
        "audio: {calls} callbacks, {} sample frames at 22,050 Hz, mean period {:.2} ms, longest gap {:.2} ms",
        s.frames.load(Relaxed),
        if calls > 1 { span / (calls - 1) as f64 } else { 0.0 },
        s.max_gap_ns.load(Relaxed) as f64 / 1e6
    );
}

/// The window as the core sees it, and the mouse lock's state.
struct Shell<'a> {
    window: &'a sdl::Window<'a>,
    locked: LockedPointer,
    /// The lock holds the system's pointer.
    grabbed: bool,
    /// The mouse's motion since the last frame, in points.
    motion: (f32, f32),
    lock_enabled: bool,
    cursor_shown: bool,
}

impl Shell<'_> {
    /// Hands the window's events to the core's input, as they came; the text a key typed goes with its key event.
    /// Returns whether the user asked to quit.
    fn take_events(&mut self, core: &mut Core) -> bool {
        let mut quit = false;
        let mut pending: Option<KeyEvent> = None;
        self.motion = (0.0, 0.0);
        let input = &mut core.input;
        while let Some(e) = sdl::poll() {
            if let sdl::Event::Text(t) = &e {
                match pending.as_mut() {
                    Some(k) if k.pressed => k.text.get_or_insert_with(String::new).push_str(t),
                    _ => {}
                }
                continue;
            }
            if let Some(k) = pending.take() {
                input.key(k);
            }
            match e {
                sdl::Event::Quit => quit = true,
                sdl::Event::FocusLost => input.focus_lost(),
                sdl::Event::Key { scancode, down } => {
                    pending = Some(KeyEvent {
                        key: key_of(scancode),
                        pressed: down,
                        text: None,
                    })
                }
                sdl::Event::Button { button, down } => input.button(button_of(button), down),
                sdl::Event::Wheel { y } => input.wheel(Wheel { y, lines: true }),
                sdl::Event::Motion { dx, dy } => {
                    self.motion.0 += dx;
                    self.motion.1 += dy;
                }
                sdl::Event::Text(_) | sdl::Event::Other => {}
            }
        }
        if let Some(k) = pending {
            input.key(k);
        }
        quit
    }
}

impl Platform for Shell<'_> {
    fn window(&self) -> Option<WindowGeom> {
        Some(WindowGeom {
            size: self.window.size_in_pixels(),
            factor: self.window.pixel_density(),
        })
    }

    fn pointer(&mut self, keys: &Buttons<Key>, game_pointer: bool) -> (Option<(f32, f32)>, bool) {
        let w = self.window;
        let focused = w.focused();
        let want = display::lock_wanted(display::LockInput {
            enabled: self.lock_enabled,
            in_game: game_pointer,
            focused,
            held_free: keys.any_pressed([Key::SuperLeft, Key::SuperRight]),
        });
        if want != self.grabbed {
            self.grabbed = want;
            if HOLD_STILL {
                w.set_relative_mouse(want);
            } else {
                w.set_mouse_grab(want);
            }
        }
        if HOLD_STILL {
            let lb = Letterbox::fit(w.size_in_pixels());
            if let Some((x, y)) = self.locked.update(want, w.mouse(), self.motion, &lb, w.pixel_density()) {
                w.warp_mouse(x, y);
            }
        }
        (self.locked.0.or(w.mouse()), focused)
    }
}

/// A key by its place: SDL's scancodes are the USB HID usage numbers (keyboard page).
fn key_of(code: i32) -> Key {
    match code {
        4..=29 => Key::Letter((b'A' + (code - 4) as u8) as char),
        30..=38 => Key::Digit((code - 29) as u8),
        39 => Key::Digit(0),
        40 => Key::Enter,
        41 => Key::Escape,
        42 => Key::Backspace,
        44 => Key::Space,
        45 => Key::Minus,
        46 => Key::Equal,
        57 => Key::CapsLock,
        58..=69 => Key::F((code - 57) as u8),
        70 => Key::PrintScreen,
        74 => Key::Home,
        75 => Key::PageUp,
        77 => Key::End,
        78 => Key::PageDown,
        79 => Key::Right,
        80 => Key::Left,
        81 => Key::Down,
        82 => Key::Up,
        86 => Key::NumpadSubtract,
        87 => Key::NumpadAdd,
        88 => Key::NumpadEnter,
        89..=97 => Key::Numpad((code - 88) as u8),
        98 => Key::Numpad(0),
        104..=106 => Key::F((code - 91) as u8),
        224 => Key::ControlLeft,
        225 => Key::ShiftLeft,
        226 => Key::AltLeft,
        227 => Key::SuperLeft,
        228 => Key::ControlRight,
        229 => Key::ShiftRight,
        230 => Key::AltRight,
        231 => Key::SuperRight,
        0 => Key::Other(0),
        _ => host::other_key(&sdl::scancode_name(code)),
    }
}

fn button_of(b: u8) -> Button {
    match b {
        1 => Button::Left,
        2 => Button::Middle,
        3 => Button::Right,
        4 => Button::Other(0),
        5 => Button::Other(1),
        n => Button::Other(n as u16 - 4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_by_their_usb_place() {
        assert_eq!(key_of(4), Key::Letter('A'));
        assert_eq!(key_of(29), Key::Letter('Z'));
        assert_eq!(key_of(30), Key::Digit(1));
        assert_eq!(key_of(39), Key::Digit(0));
        assert_eq!(key_of(58), Key::F(1));
        assert_eq!(key_of(67), Key::F(10));
        assert_eq!(key_of(104), Key::F(13));
        assert_eq!(key_of(89), Key::Numpad(1));
        assert_eq!(key_of(98), Key::Numpad(0));
        assert_eq!(key_of(227), Key::SuperLeft);
    }

    #[test]
    fn mouse_buttons() {
        assert_eq!(button_of(1), Button::Left);
        assert_eq!(button_of(3), Button::Right);
        assert_eq!(button_of(6), Button::Other(2));
    }
}
