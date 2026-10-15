//! The miniquad trial shell (`docs/design/client-core.md`, phase 3): the client core in a miniquad window. It does
//! what the design's "Platform trait" asks of a shell, as far as miniquad allows: the key, button and wheel events
//! into the core's input; [`Core::advance`] once a frame; the composed frame uploaded into one texture each frame and
//! drawn with the nearest texel at the whole-number scale with black around it (`display::Letterbox`); the core's
//! mixer on the sound device through `cpal` (`sound`); the desktop's storage (`host::launch`); and the end on
//! `Core::exit` after the settings are written. The mouse lock uses miniquad's cursor grab, which on macOS holds the
//! system's pointer still and hands over the mouse's motion.
//!
//! What miniquad can't do, and what the shell does instead: no focus events (the window counts as focused; a
//! minimised window releases the keys), no title change after the start, no monitor size (without `--ui-scale` the
//! window opens at 640 × 480 points, two pixels per screen pixel on a display with two pixels per point), no minimum
//! window size, no pointer warp when the lock ends, no always-on-top window, and the wheel's mouse steps and
//! touchpad pixels come as one number (taken as tenths of a step).
//!
//! A scripted run (`--script`) also records each frame (`script::record`) and, with a window, compares the window's
//! own pixels (read back from OpenGL's back buffer before it is shown) at each shot (`script::compare_shot`);
//! `--headless` runs it without a window (`host::run_headless`).

mod sound;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use miniquad::{
    Bindings, BufferLayout, BufferSource, BufferType, BufferUsage, EventHandler, FilterMode, KeyCode, KeyMods,
    MipmapFilterMode, MouseButton, PassAction, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource,
    TextureId, UniformBlockLayout, VertexAttribute, VertexFormat, conf, window,
};
use tidewater_client::args::Args;
use tidewater_client::core::{Core, Platform};
use tidewater_client::display::{self, Letterbox};
use tidewater_client::host::{self, FrameStats, LockedPointer};
use tidewater_client::input::{Button, Buttons, Key, KeyEvent, Wheel, WindowGeom};
use tidewater_client::script;

fn main() -> Result<()> {
    let l = host::launch(Args::parse())?;
    if l.headless {
        host::run_headless(l.core);
        return Ok(());
    }
    let scripted = l.core.script.is_some();
    let (w, h) = display::window_size(l.ui_scale.unwrap_or(1));
    let conf = conf::Conf {
        window_title: l.title,
        window_width: w as i32,
        window_height: h as i32,
        // A scripted window has one pixel per point whatever the display, so the screen fills it exactly.
        high_dpi: !scripted,
        window_resizable: !scripted,
        ..Default::default()
    };
    let core = l.core;
    miniquad::start(conf, move || Box::new(Stage::new(core)));
    Ok(())
}

const VERTEX: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec2 in_uv;
varying lowp vec2 uv;
void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    uv = in_uv;
}
"#;

const FRAGMENT: &str = r#"#version 100
varying lowp vec2 uv;
uniform sampler2D tex;
void main() {
    gl_FragColor = texture2D(tex, uv);
}
"#;

/// The shell's state between miniquad's calls.
struct Stage {
    core: Core,
    gfx: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    bindings: Bindings,
    texture: TextureId,
    scripted: bool,
    first: bool,
    last: Instant,
    /// A key event waiting for the text it types (`char_event` comes after `key_down_event`).
    pending_key: Option<KeyEvent>,
    /// The system's pointer in physical pixels, `None` outside the window.
    mouse: Option<(f32, f32)>,
    /// The mouse's motion since the last frame while the lock holds the pointer, in points.
    motion: (f32, f32),
    locked: LockedPointer,
    grabbed: bool,
    lock_enabled: bool,
    cursor_shown: bool,
    /// The program has ended; miniquad may still call in until its loop stops.
    done: bool,
    stats: FrameStats,
    audio: Option<(sound::Device, Arc<sound::Stats>)>,
}

impl Stage {
    fn new(core: Core) -> Stage {
        let mut gfx = window::new_rendering_backend();
        let texture = gfx.new_texture_from_rgba8(display::SCREEN.0 as u16, display::SCREEN.1 as u16, &core.frame.rgba);
        gfx.texture_set_filter(texture, FilterMode::Nearest, MipmapFilterMode::None);
        // One quad over the viewport; the picture's first row at the top.
        #[rustfmt::skip]
        let vertices: [f32; 16] = [
            -1.0, -1.0, 0.0, 1.0,
             1.0, -1.0, 1.0, 1.0,
             1.0,  1.0, 1.0, 0.0,
            -1.0,  1.0, 0.0, 0.0,
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vb = gfx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );
        let ib = gfx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );
        let shader = gfx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: VERTEX,
                    fragment: FRAGMENT,
                },
                ShaderMeta {
                    images: vec!["tex".to_string()],
                    uniforms: UniformBlockLayout { uniforms: vec![] },
                },
            )
            .expect("the shader compiles");
        let pipeline = gfx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
            ],
            shader,
            PipelineParams::default(),
        );
        let bindings = Bindings {
            vertex_buffers: vec![vb],
            index_buffer: ib,
            images: vec![texture],
        };
        let scripted = core.script.is_some();
        // A scripted run mixes its sound itself (`script::record`) and plays none.
        let audio = if scripted {
            None
        } else {
            let stats = Arc::new(sound::Stats::default());
            match sound::open(core.audio.output(), stats.clone()) {
                Ok(d) => Some((d, stats)),
                Err(e) => {
                    eprintln!("no sound: {e}");
                    None
                }
            }
        };
        if let Some((d, _)) = &audio
            && std::env::var_os("TIDEWATER_FRAME_STATS").is_some()
        {
            eprintln!(
                "audio device: {} Hz, {} channels, buffer asked {:?}, reported {:?} frames",
                d.rate,
                d.channels,
                d.asked,
                d.buffer_size()
            );
        }
        let lock_enabled = !core.opts.no_mouse_lock;
        Stage {
            core,
            gfx,
            pipeline,
            bindings,
            texture,
            scripted,
            first: true,
            last: Instant::now(),
            pending_key: None,
            mouse: None,
            motion: (0.0, 0.0),
            locked: LockedPointer::default(),
            grabbed: false,
            lock_enabled,
            cursor_shown: true,
            done: false,
            stats: FrameStats::default(),
            audio,
        }
    }

    fn flush_key(&mut self) {
        if let Some(k) = self.pending_key.take() {
            self.core.input.key(k);
        }
    }

    /// The program ends: the settings are written (a normal exit) and a scripted run's fingerprints so far.
    fn end(&mut self) {
        if self.done {
            return;
        }
        self.done = true;
        self.core.store_settings();
        if let Some(run) = self.core.script.as_mut() {
            script::record(run, Some(&self.core.frame), &self.core.audio.output(), true);
        }
        self.end_quietly();
    }
}

/// The window's back buffer as drawn so far: width, height and RGBA bytes, top row first.
fn read_back() -> (usize, usize, Vec<u8>) {
    let (w, h) = window::screen_size();
    let (w, h) = (w as usize, h as usize);
    let mut px = vec![0u8; w * h * 4];
    read_pixels(w, h, &mut px);
    // OpenGL's rows run bottom to top.
    let row = w * 4;
    let mut flipped = Vec::with_capacity(px.len());
    for y in (0..h).rev() {
        flipped.extend_from_slice(&px[y * row..(y + 1) * row]);
    }
    (w, h, flipped)
}

/// Reads the bound framebuffer (the window's back buffer) into `px`, bottom row first.
#[allow(unsafe_code)]
fn read_pixels(w: usize, h: usize, px: &mut [u8]) {
    assert_eq!(px.len(), w * h * 4);
    // SAFETY: called on miniquad's GL thread during `draw`, with a buffer of the size asked for (4-byte rows need no
    // pack alignment).
    unsafe {
        miniquad::gl::glReadPixels(
            0,
            0,
            w as i32,
            h as i32,
            miniquad::gl::GL_RGBA,
            miniquad::gl::GL_UNSIGNED_BYTE,
            px.as_mut_ptr().cast(),
        );
    }
}

fn print_audio_stats(s: &sound::Stats, rate: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    let calls = s.calls.load(Relaxed);
    let span = s.last_ns.load(Relaxed).saturating_sub(s.first_ns.load(Relaxed)) as f64 / 1e6;
    eprintln!(
        "audio: {calls} callbacks, {} device frames at {rate} Hz, mean period {:.2} ms, longest gap {:.2} ms",
        s.frames.load(Relaxed),
        if calls > 1 { span / (calls - 1) as f64 } else { 0.0 },
        s.max_gap_ns.load(Relaxed) as f64 / 1e6
    );
}

/// The window as the core sees it during a frame.
struct Window<'a> {
    mouse: Option<(f32, f32)>,
    motion: (f32, f32),
    locked: &'a mut LockedPointer,
    grabbed: &'a mut bool,
    lock_enabled: bool,
}

impl Platform for Window<'_> {
    fn window(&self) -> Option<WindowGeom> {
        let (w, h) = window::screen_size();
        Some(WindowGeom {
            size: (w as u32, h as u32),
            factor: window::dpi_scale(),
        })
    }

    fn pointer(&mut self, keys: &Buttons<Key>, game_pointer: bool) -> (Option<(f32, f32)>, bool) {
        // miniquad reports no focus changes.
        let focused = true;
        let want = display::lock_wanted(display::LockInput {
            enabled: self.lock_enabled,
            in_game: game_pointer,
            focused,
            held_free: keys.any_pressed([Key::SuperLeft, Key::SuperRight]),
        });
        if want != *self.grabbed {
            *self.grabbed = want;
            window::set_cursor_grab(want);
        }
        let f = window::dpi_scale();
        let (w, h) = window::screen_size();
        let lb = Letterbox::fit((w as u32, h as u32));
        let mouse = self.mouse.map(|(x, y)| (x / f, y / f));
        // miniquad can't put the system's pointer back where ours was when the lock ends.
        let _ = self.locked.update(want, mouse, self.motion, &lb, f);
        (self.locked.0.or(mouse), focused)
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {}

    fn draw(&mut self) {
        if self.done {
            return;
        }
        let now = Instant::now();
        let elapsed = match (self.scripted, self.first) {
            (true, _) => host::scripted_elapsed(self.first),
            (false, true) => Duration::ZERO,
            (false, false) => now - self.last,
        };
        let interval = now - self.last;
        self.last = now;
        self.first = false;
        self.flush_key();
        let mut platform = Window {
            mouse: self.mouse,
            motion: std::mem::take(&mut self.motion),
            locked: &mut self.locked,
            grabbed: &mut self.grabbed,
            lock_enabled: self.lock_enabled,
        };
        self.core.advance(elapsed, &mut platform);
        let exiting = std::mem::take(&mut self.core.exit);
        // miniquad can't change the title after the start (`Core::title` is dropped).
        self.core.title = None;
        // The grab hides and shows the system's pointer on its own, in pairs, so the two don't interfere.
        if self.cursor_shown != self.core.cursor_visible {
            self.cursor_shown = self.core.cursor_visible;
            window::show_mouse(self.core.cursor_visible);
        }
        if exiting {
            self.core.store_settings();
        }
        self.gfx.texture_update(self.texture, &self.core.frame.rgba);
        let (w, h) = window::screen_size();
        let (w, h) = (w as u32, h as u32);
        let lb = Letterbox::fit((w, h));
        let (x, y, sw, sh) = lb.screen();
        self.gfx.begin_default_pass(PassAction::clear_color(0.0, 0.0, 0.0, 1.0));
        // OpenGL's viewport counts from the bottom.
        self.gfx
            .apply_viewport(x as i32, h.saturating_sub(y + sh) as i32, sw as i32, sh as i32);
        self.gfx.apply_pipeline(&self.pipeline);
        self.gfx.apply_bindings(&self.bindings);
        self.gfx.draw(0, 6, 1);
        self.gfx.end_render_pass();
        let mut finished = false;
        if let Some(run) = self.core.script.as_mut() {
            let shown = (lb.scale == 1).then_some(&self.core.frame);
            let asked = script::record(run, shown, &self.core.audio.output(), exiting);
            for name in asked.screenshots {
                let (sw, sh, px) = read_back();
                script::compare_shot(run, name, Some((sw, sh, &px)));
            }
            finished = asked.finished;
        }
        self.gfx.commit_frame();
        self.stats.frame(interval, now.elapsed());
        // The settings are already written on an exit; a finished scene ends without them, as in the Bevy shell.
        if exiting || finished {
            self.done = true;
            self.end_quietly();
        }
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        // miniquad's macOS backend puts the point a physical pixel up (it subtracts one from the flipped row).
        let y = if cfg!(target_os = "macos") { y + 1.0 } else { y };
        self.mouse = Some((x, y));
    }

    fn raw_mouse_motion(&mut self, dx: f32, dy: f32) {
        self.motion.0 += dx;
        self.motion.1 += dy;
    }

    fn mouse_leave_event(&mut self) {
        self.mouse = None;
    }

    fn mouse_wheel_event(&mut self, _x: f32, y: f32) {
        self.flush_key();
        self.core.input.wheel(Wheel {
            y: y / 10.0,
            lines: true,
        });
    }

    fn mouse_button_down_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.mouse_motion_event(x, y);
        self.flush_key();
        self.core.input.button(button_of(button), true);
    }

    fn mouse_button_up_event(&mut self, button: MouseButton, x: f32, y: f32) {
        self.mouse_motion_event(x, y);
        self.flush_key();
        self.core.input.button(button_of(button), false);
    }

    fn char_event(&mut self, character: char, _keymods: KeyMods, _repeat: bool) {
        if let Some(k) = self.pending_key.as_mut()
            && k.pressed
        {
            k.text.get_or_insert_with(String::new).push(character);
        }
    }

    fn key_down_event(&mut self, keycode: KeyCode, _keymods: KeyMods, _repeat: bool) {
        self.flush_key();
        self.pending_key = Some(KeyEvent {
            key: key_of(keycode),
            pressed: true,
            text: None,
        });
    }

    fn key_up_event(&mut self, keycode: KeyCode, _keymods: KeyMods) {
        self.flush_key();
        self.core.input.key(KeyEvent {
            key: key_of(keycode),
            pressed: false,
            text: None,
        });
    }

    fn window_minimized_event(&mut self) {
        self.flush_key();
        self.core.input.focus_lost();
    }

    fn quit_requested_event(&mut self) {
        // Closing the window is a normal exit.
        self.end();
    }
}

impl Stage {
    /// Ends after an exit the frame already wrote the settings and fingerprints for.
    fn end_quietly(&mut self) {
        if let Some((d, s)) = self.audio.take() {
            if std::env::var_os("TIDEWATER_FRAME_STATS").is_some() {
                print_audio_stats(&s, d.rate);
            }
            drop(d);
        }
        window::order_quit();
    }
}

/// A key by its place.
fn key_of(k: KeyCode) -> Key {
    use KeyCode as K;
    let n = k as u32;
    match k {
        K::A
        | K::B
        | K::C
        | K::D
        | K::E
        | K::F
        | K::G
        | K::H
        | K::I
        | K::J
        | K::K
        | K::L
        | K::M
        | K::N
        | K::O
        | K::P
        | K::Q
        | K::R
        | K::S
        | K::T
        | K::U
        | K::V
        | K::W
        | K::X
        | K::Y
        | K::Z => Key::Letter(char::from_u32(n).unwrap_or('?')),
        K::Key0 | K::Key1 | K::Key2 | K::Key3 | K::Key4 | K::Key5 | K::Key6 | K::Key7 | K::Key8 | K::Key9 => {
            Key::Digit((n - K::Key0 as u32) as u8)
        }
        K::Kp0 | K::Kp1 | K::Kp2 | K::Kp3 | K::Kp4 | K::Kp5 | K::Kp6 | K::Kp7 | K::Kp8 | K::Kp9 => {
            Key::Numpad((n - K::Kp0 as u32) as u8)
        }
        K::KpAdd => Key::NumpadAdd,
        K::KpSubtract => Key::NumpadSubtract,
        K::KpEnter => Key::NumpadEnter,
        K::F1
        | K::F2
        | K::F3
        | K::F4
        | K::F5
        | K::F6
        | K::F7
        | K::F8
        | K::F9
        | K::F10
        | K::F11
        | K::F12
        | K::F13
        | K::F14
        | K::F15 => Key::F((n - K::F1 as u32 + 1) as u8),
        K::Up => Key::Up,
        K::Down => Key::Down,
        K::Left => Key::Left,
        K::Right => Key::Right,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,
        K::Enter => Key::Enter,
        K::Escape => Key::Escape,
        K::Backspace => Key::Backspace,
        K::Space => Key::Space,
        K::Minus => Key::Minus,
        K::Equal => Key::Equal,
        K::PrintScreen => Key::PrintScreen,
        K::CapsLock => Key::CapsLock,
        K::LeftShift => Key::ShiftLeft,
        K::RightShift => Key::ShiftRight,
        K::LeftControl => Key::ControlLeft,
        K::RightControl => Key::ControlRight,
        K::LeftAlt => Key::AltLeft,
        K::RightAlt => Key::AltRight,
        K::LeftSuper => Key::SuperLeft,
        K::RightSuper => Key::SuperRight,
        K::Unknown => Key::Other(0),
        k => host::other_key(&format!("{k:?}")),
    }
}

fn button_of(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
        MouseButton::Unknown => Button::Other(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_by_their_place() {
        assert_eq!(key_of(KeyCode::A), Key::Letter('A'));
        assert_eq!(key_of(KeyCode::Z), Key::Letter('Z'));
        assert_eq!(key_of(KeyCode::Key0), Key::Digit(0));
        assert_eq!(key_of(KeyCode::Key9), Key::Digit(9));
        assert_eq!(key_of(KeyCode::Kp7), Key::Numpad(7));
        assert_eq!(key_of(KeyCode::F1), Key::F(1));
        assert_eq!(key_of(KeyCode::F15), Key::F(15));
        assert_eq!(key_of(KeyCode::LeftSuper), Key::SuperLeft);
    }
}
