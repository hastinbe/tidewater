//! A thin safe wrapper over the parts of SDL 3 the shell uses (`sdl3-sys`, the C API as published in SDL's headers;
//! <https://wiki.libsdl.org/SDL3/>): the window, a renderer with one streaming texture, the system's events (including
//! files dropped on the window), the system's folder and file dialogs, the mouse modes and an audio stream fed by a
//! callback. Every `unsafe` block of the shell is here; each owner frees what it
//! made when it is dropped.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_int, c_void};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Result, anyhow};
use sdl3_sys::everything as sys;

/// SDL's message for the last error.
fn error(what: &str) -> anyhow::Error {
    // SAFETY: SDL_GetError returns a valid, NUL-terminated string (empty when there is no error).
    let msg = unsafe { CStr::from_ptr(sys::SDL_GetError()) };
    anyhow!("{what}: {}", msg.to_string_lossy())
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).expect("no NUL bytes left")
}

/// SDL itself, from `SDL_Init` to `SDL_Quit`.
pub struct Sdl(());

impl Sdl {
    /// Starts SDL's video and events, and its audio if `audio`. `sample_frames`: the audio device's buffer asked
    /// for, in sample frames (a hint; the device may choose another).
    pub fn init(audio: bool, sample_frames: u32) -> Result<Sdl> {
        let frames = cstring(&sample_frames.to_string());
        // SAFETY: the hint's name is SDL's own constant and the value a valid C string; SDL copies both.
        unsafe { sys::SDL_SetHint(sys::SDL_HINT_AUDIO_DEVICE_SAMPLE_FRAMES, frames.as_ptr()) };
        let mut flags = sys::SDL_INIT_VIDEO;
        if audio {
            flags |= sys::SDL_INIT_AUDIO;
        }
        // SAFETY: called once, from the main thread, before any other SDL call but the hint.
        if unsafe { sys::SDL_Init(flags) } {
            Ok(Sdl(()))
        } else {
            Err(error("SDL_Init"))
        }
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        // SAFETY: every window, renderer and stream borrows the `Sdl` and is gone by now.
        unsafe { sys::SDL_Quit() };
    }
}

/// How the window opens.
#[derive(Clone, Copy, Default)]
pub struct WindowOpts {
    /// One window pixel per physical pixel (a display with two pixels per point gets twice the pixels).
    pub high_density: bool,
    pub resizable: bool,
    pub always_on_top: bool,
    /// Doesn't take the keyboard from what the user is doing.
    pub not_focusable: bool,
}

/// A window.
pub struct Window<'s> {
    raw: *mut sys::SDL_Window,
    _sdl: PhantomData<&'s Sdl>,
}

impl<'s> Window<'s> {
    /// A window of `w` × `h` points.
    pub fn new(_sdl: &'s Sdl, title: &str, w: u32, h: u32, o: WindowOpts) -> Result<Self> {
        let mut flags = sys::SDL_WindowFlags(0);
        for (on, f) in [
            (o.high_density, sys::SDL_WINDOW_HIGH_PIXEL_DENSITY),
            (o.resizable, sys::SDL_WINDOW_RESIZABLE),
            (o.always_on_top, sys::SDL_WINDOW_ALWAYS_ON_TOP),
            (o.not_focusable, sys::SDL_WINDOW_NOT_FOCUSABLE),
        ] {
            if on {
                flags |= f;
            }
        }
        let title = cstring(title);
        // SAFETY: a valid title string; SDL copies it.
        let raw = unsafe { sys::SDL_CreateWindow(title.as_ptr(), w as c_int, h as c_int, flags) };
        if raw.is_null() {
            return Err(error("SDL_CreateWindow"));
        }
        Ok(Window { raw, _sdl: PhantomData })
    }

    /// The size in physical pixels.
    pub fn size_in_pixels(&self) -> (u32, u32) {
        let (mut w, mut h) = (0, 0);
        // SAFETY: a live window and two valid out pointers.
        unsafe { sys::SDL_GetWindowSizeInPixels(self.raw, &mut w, &mut h) };
        (w.max(0) as u32, h.max(0) as u32)
    }

    /// Physical pixels per point.
    pub fn pixel_density(&self) -> f32 {
        // SAFETY: a live window.
        let d = unsafe { sys::SDL_GetWindowPixelDensity(self.raw) };
        if d > 0.0 { d } else { 1.0 }
    }

    /// The window's display: its size in points and its physical pixels per point.
    pub fn display(&self) -> Option<((u32, u32), f32)> {
        // SAFETY: a live window; the mode SDL returns stays valid until the display changes, and is read at once.
        unsafe {
            let id = sys::SDL_GetDisplayForWindow(self.raw);
            let mode = sys::SDL_GetDesktopDisplayMode(id);
            mode.as_ref()
                .map(|m| ((m.w.max(0) as u32, m.h.max(0) as u32), m.pixel_density.max(1.0)))
        }
    }

    /// Resizes to `w` × `h` points.
    pub fn set_size(&self, w: u32, h: u32) {
        // SAFETY: a live window.
        unsafe { sys::SDL_SetWindowSize(self.raw, w as c_int, h as c_int) };
    }

    pub fn centre(&self) {
        // SAFETY: a live window.
        unsafe { sys::SDL_SetWindowPosition(self.raw, sys::SDL_WINDOWPOS_CENTERED, sys::SDL_WINDOWPOS_CENTERED) };
    }

    /// The smallest size the user can make it, in points.
    pub fn set_min_size(&self, w: u32, h: u32) {
        // SAFETY: a live window.
        unsafe { sys::SDL_SetWindowMinimumSize(self.raw, w as c_int, h as c_int) };
    }

    pub fn set_title(&self, title: &str) {
        let title = cstring(title);
        // SAFETY: a live window and a valid string; SDL copies it.
        unsafe { sys::SDL_SetWindowTitle(self.raw, title.as_ptr()) };
    }

    /// Whether the window has the keyboard.
    pub fn focused(&self) -> bool {
        // SAFETY: a live window.
        let flags = unsafe { sys::SDL_GetWindowFlags(self.raw) };
        flags.0 & sys::SDL_WINDOW_INPUT_FOCUS.0 != 0
    }

    /// The system's pointer in the window's points, `None` when it is over another window.
    pub fn mouse(&self) -> Option<(f32, f32)> {
        // SAFETY: plain queries of SDL's mouse state, with valid out pointers.
        unsafe {
            if sys::SDL_GetMouseFocus() != self.raw {
                return None;
            }
            let (mut x, mut y) = (0.0, 0.0);
            sys::SDL_GetMouseState(&mut x, &mut y);
            Some((x, y))
        }
    }

    /// Relative mode: the system's pointer hidden and held still; the mouse's motion still comes in.
    pub fn set_relative_mouse(&self, on: bool) {
        // SAFETY: a live window.
        unsafe { sys::SDL_SetWindowRelativeMouseMode(self.raw, on) };
    }

    /// Confines the system's pointer to the window.
    pub fn set_mouse_grab(&self, on: bool) {
        // SAFETY: a live window.
        unsafe { sys::SDL_SetWindowMouseGrab(self.raw, on) };
    }

    /// Puts the system's pointer at a point of the window, in points.
    pub fn warp_mouse(&self, x: f32, y: f32) {
        // SAFETY: a live window.
        unsafe { sys::SDL_WarpMouseInWindow(self.raw, x, y) };
    }

    /// Typed text comes in as [`Event::Text`].
    pub fn start_text_input(&self) {
        // SAFETY: a live window.
        unsafe { sys::SDL_StartTextInput(self.raw) };
    }

    /// Opens the system's dialog for choosing a folder, over this window. The answer comes later (not before this
    /// call returns), through [`take_dialog_answers`].
    pub fn show_folder_dialog(&self) {
        // SAFETY: a live window; the callback is a plain function that uses no userdata (null).
        unsafe {
            sys::SDL_ShowOpenFolderDialog(
                Some(dialog_done),
                std::ptr::null_mut(),
                self.raw,
                std::ptr::null(),
                false,
            )
        };
    }

    /// Opens the system's dialog for choosing one file, over this window. `filters` are (name, patterns) pairs, the
    /// patterns file extensions without dots separated by `;` (`"iso;bin;cue"`), or `"*"` for any file. The answer
    /// comes later, through [`take_dialog_answers`].
    pub fn show_file_dialog(&self, filters: &[(&str, &str)]) {
        // SDL may read the filters until the dialog answers, so they are leaked: a few bytes per dialog opened.
        let list: Vec<sys::SDL_DialogFileFilter> = filters
            .iter()
            .map(|(name, pattern)| sys::SDL_DialogFileFilter {
                name: cstring(name).into_raw(),
                pattern: cstring(pattern).into_raw(),
            })
            .collect();
        let list: &'static [sys::SDL_DialogFileFilter] = Box::leak(list.into_boxed_slice());
        // SAFETY: a live window; `list` lives for the rest of the program and its strings are valid C strings; the
        // callback uses no userdata (null).
        unsafe {
            sys::SDL_ShowOpenFileDialog(
                Some(dialog_done),
                std::ptr::null_mut(),
                self.raw,
                list.as_ptr(),
                list.len() as c_int,
                std::ptr::null(),
                false,
            )
        };
    }
}

/// The answers of the dialogs, in the order they came: the path chosen, `None` for a cancelled or failed dialog. SDL
/// calls [`dialog_done`] from the main thread or another one, so they wait here for the frame to take them.
static DIALOG_ANSWERS: Mutex<Vec<Option<PathBuf>>> = Mutex::new(Vec::new());

/// SDL's dialog callback: `filelist` is NULL on an error, else a NULL-terminated list of the chosen paths (empty if
/// the dialog was cancelled).
unsafe extern "C" fn dialog_done(_userdata: *mut c_void, filelist: *const *const std::ffi::c_char, _filter: c_int) {
    let path = if filelist.is_null() {
        None
    } else {
        // SAFETY: a non-NULL list holds NULL-terminated UTF-8 strings and ends with a NULL pointer, valid during the
        // callback; only the first is read, after checking it is not the terminator.
        unsafe {
            let first = *filelist;
            (!first.is_null()).then(|| PathBuf::from(CStr::from_ptr(first).to_string_lossy().into_owned()))
        }
    };
    if let Ok(mut answers) = DIALOG_ANSWERS.lock() {
        answers.push(path);
    }
}

/// The dialogs' answers since the last call.
pub fn take_dialog_answers() -> Vec<Option<PathBuf>> {
    DIALOG_ANSWERS
        .lock()
        .map(|mut a| std::mem::take(&mut *a))
        .unwrap_or_default()
}

impl Drop for Window<'_> {
    fn drop(&mut self) {
        // SAFETY: the renderer borrowed the window and is gone by now.
        unsafe { sys::SDL_DestroyWindow(self.raw) };
    }
}

/// Shows or hides the system's pointer.
pub fn show_cursor(on: bool) {
    // SAFETY: plain calls, after SDL_Init.
    unsafe {
        if on {
            sys::SDL_ShowCursor();
        } else {
            sys::SDL_HideCursor();
        }
    }
}

/// A renderer for a window with one streaming RGBA texture, scaled with the nearest texel.
pub struct Renderer<'w> {
    raw: *mut sys::SDL_Renderer,
    texture: *mut sys::SDL_Texture,
    size: (u32, u32),
    _window: PhantomData<&'w Window<'w>>,
}

impl<'w> Renderer<'w> {
    /// A renderer with a `size` texture; `vsync`: present waits for the display.
    pub fn new(window: &'w Window<'w>, size: (u32, u32), vsync: bool) -> Result<Self> {
        // SAFETY: a live window; a null name picks SDL's default renderer.
        let raw = unsafe { sys::SDL_CreateRenderer(window.raw, std::ptr::null()) };
        if raw.is_null() {
            return Err(error("SDL_CreateRenderer"));
        }
        // SAFETY: a live renderer.
        let texture = unsafe {
            sys::SDL_SetRenderVSync(raw, if vsync { 1 } else { 0 });
            sys::SDL_CreateTexture(
                raw,
                sys::SDL_PIXELFORMAT_RGBA32,
                sys::SDL_TEXTUREACCESS_STREAMING,
                size.0 as c_int,
                size.1 as c_int,
            )
        };
        if texture.is_null() {
            let e = error("SDL_CreateTexture");
            // SAFETY: the renderer made above, not used again.
            unsafe { sys::SDL_DestroyRenderer(raw) };
            return Err(e);
        }
        // SAFETY: a live texture.
        unsafe { sys::SDL_SetTextureScaleMode(texture, sys::SDL_SCALEMODE_NEAREST) };
        Ok(Renderer {
            raw,
            texture,
            size,
            _window: PhantomData,
        })
    }

    /// The window's drawable size in pixels.
    pub fn output_size(&self) -> (u32, u32) {
        let (mut w, mut h) = (0, 0);
        // SAFETY: a live renderer and valid out pointers.
        unsafe { sys::SDL_GetCurrentRenderOutputSize(self.raw, &mut w, &mut h) };
        (w.max(0) as u32, h.max(0) as u32)
    }

    /// Clears the window to black and draws `rgba` (the texture's size) over `dst` (x, y, width, height in pixels).
    pub fn draw(&mut self, rgba: &[u8], dst: (u32, u32, u32, u32)) {
        let pitch = self.size.0 as usize * 4;
        assert_eq!(
            rgba.len(),
            pitch * self.size.1 as usize,
            "the picture is the texture's size"
        );
        let dst = sys::SDL_FRect {
            x: dst.0 as f32,
            y: dst.1 as f32,
            w: dst.2 as f32,
            h: dst.3 as f32,
        };
        // SAFETY: a live renderer and texture; `rgba` holds the texture's whole size at `pitch` bytes a row.
        unsafe {
            sys::SDL_UpdateTexture(self.texture, std::ptr::null(), rgba.as_ptr().cast(), pitch as c_int);
            sys::SDL_SetRenderDrawColor(self.raw, 0, 0, 0, 255);
            sys::SDL_RenderClear(self.raw);
            sys::SDL_RenderTexture(self.raw, self.texture, std::ptr::null(), &dst);
        }
    }

    /// What was drawn since the last present, as the window will show it: width, height and RGBA bytes.
    pub fn read_pixels(&mut self) -> Option<(usize, usize, Vec<u8>)> {
        // SAFETY: a live renderer; the surfaces SDL returns are ours to free, and their pixels are `pitch` bytes a
        // row for `h` rows.
        unsafe {
            let read = sys::SDL_RenderReadPixels(self.raw, std::ptr::null());
            if read.is_null() {
                return None;
            }
            let rgba = sys::SDL_ConvertSurface(read, sys::SDL_PIXELFORMAT_RGBA32);
            sys::SDL_DestroySurface(read);
            let s = rgba.as_ref()?;
            let (w, h, pitch) = (s.w.max(0) as usize, s.h.max(0) as usize, s.pitch.max(0) as usize);
            let mut out = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                let row = std::slice::from_raw_parts(s.pixels.cast::<u8>().add(y * pitch), w * 4);
                out.extend_from_slice(row);
            }
            sys::SDL_DestroySurface(rgba);
            Some((w, h, out))
        }
    }

    /// Shows what was drawn.
    pub fn present(&mut self) {
        // SAFETY: a live renderer.
        unsafe { sys::SDL_RenderPresent(self.raw) };
    }
}

impl Drop for Renderer<'_> {
    fn drop(&mut self) {
        // SAFETY: made by `new`, not used again; the texture goes before its renderer.
        unsafe {
            sys::SDL_DestroyTexture(self.texture);
            sys::SDL_DestroyRenderer(self.raw);
        }
    }
}

/// The system's events the shell uses.
pub enum Event {
    /// The user asked to quit (closed the window, or the system's quit).
    Quit,
    FocusLost,
    /// A key, by its place (SDL's scancode).
    Key {
        scancode: i32,
        down: bool,
    },
    /// Text typed by the last key.
    Text(String),
    /// A mouse button (SDL's number: 1 left, 2 middle, 3 right, 4 and 5 the side buttons).
    Button {
        button: u8,
        down: bool,
    },
    /// The wheel, in steps (fractions from a touchpad); positive away from the user.
    Wheel {
        y: f32,
    },
    /// The mouse moved, in points.
    Motion {
        dx: f32,
        dy: f32,
    },
    /// A file or folder dropped on the window.
    Drop(PathBuf),
    Other,
}

/// The next event waiting, if any.
pub fn poll() -> Option<Event> {
    let mut e = std::mem::MaybeUninit::<sys::SDL_Event>::uninit();
    // SAFETY: SDL fills the event when it returns true; each variant is read by the type SDL gave it.
    unsafe {
        if !sys::SDL_PollEvent(e.as_mut_ptr()) {
            return None;
        }
        let e = e.assume_init();
        let t = sys::SDL_EventType(e.r#type);
        Some(match t {
            sys::SDL_EVENT_QUIT | sys::SDL_EVENT_WINDOW_CLOSE_REQUESTED => Event::Quit,
            sys::SDL_EVENT_WINDOW_FOCUS_LOST => Event::FocusLost,
            sys::SDL_EVENT_KEY_DOWN | sys::SDL_EVENT_KEY_UP => Event::Key {
                scancode: e.key.scancode.0,
                down: e.key.down,
            },
            sys::SDL_EVENT_TEXT_INPUT if !e.text.text.is_null() => {
                Event::Text(CStr::from_ptr(e.text.text).to_string_lossy().into_owned())
            }
            sys::SDL_EVENT_MOUSE_BUTTON_DOWN | sys::SDL_EVENT_MOUSE_BUTTON_UP => Event::Button {
                button: e.button.button,
                down: e.button.down,
            },
            sys::SDL_EVENT_MOUSE_WHEEL => Event::Wheel { y: e.wheel.y },
            sys::SDL_EVENT_MOUSE_MOTION => Event::Motion {
                dx: e.motion.xrel,
                dy: e.motion.yrel,
            },
            sys::SDL_EVENT_DROP_FILE if !e.drop.data.is_null() => Event::Drop(PathBuf::from(
                CStr::from_ptr(e.drop.data).to_string_lossy().into_owned(),
            )),
            _ => Event::Other,
        })
    }
}

/// SDL's name for a key.
pub fn scancode_name(scancode: i32) -> String {
    // SAFETY: SDL returns a valid string (empty for a key without a name).
    unsafe { CStr::from_ptr(sys::SDL_GetScancodeName(sys::SDL_Scancode(scancode))) }
        .to_string_lossy()
        .into_owned()
}

/// What the audio callback has done, for measuring.
#[derive(Default)]
pub struct AudioStats {
    pub calls: AtomicU64,
    /// Sample frames handed over.
    pub frames: AtomicU64,
    /// Nanoseconds after `epoch` of the first and last call, and the longest gap between two calls.
    pub first_ns: AtomicU64,
    pub last_ns: AtomicU64,
    pub max_gap_ns: AtomicU64,
}

/// What the callback owns.
struct Feed<F> {
    fill: F,
    buf: Vec<i16>,
    stats: Arc<AudioStats>,
    epoch: Instant,
}

/// An audio stream to the default playback device, 16-bit stereo at a fixed rate (SDL converts to the device's):
/// SDL's audio thread calls `fill` for each piece the device needs, in interleaved samples.
pub struct AudioOut<'s, F> {
    stream: *mut sys::SDL_AudioStream,
    feed: *mut Feed<F>,
    _sdl: PhantomData<&'s Sdl>,
}

unsafe extern "C" fn feed_callback<F: FnMut(&mut [i16])>(
    userdata: *mut c_void,
    stream: *mut sys::SDL_AudioStream,
    additional: c_int,
    _total: c_int,
) {
    // SAFETY: `userdata` is the `Feed` made by `AudioOut::open`, used only by this callback until the stream is
    // destroyed (SDL calls it from one thread at a time).
    let feed = unsafe { &mut *userdata.cast::<Feed<F>>() };
    let now = feed.epoch.elapsed().as_nanos() as u64;
    let s = &feed.stats;
    if s.calls.fetch_add(1, Ordering::Relaxed) == 0 {
        s.first_ns.store(now, Ordering::Relaxed);
    } else {
        let gap = now.saturating_sub(s.last_ns.load(Ordering::Relaxed));
        s.max_gap_ns.fetch_max(gap, Ordering::Relaxed);
    }
    s.last_ns.store(now, Ordering::Relaxed);
    let samples = (additional.max(0) as usize / 2) & !1;
    if samples == 0 {
        return;
    }
    feed.buf.resize(samples, 0);
    (feed.fill)(&mut feed.buf);
    s.frames.fetch_add(samples as u64 / 2, Ordering::Relaxed);
    // SAFETY: a live stream and `samples` valid samples.
    unsafe { sys::SDL_PutAudioStreamData(stream, feed.buf.as_ptr().cast(), (samples * 2) as c_int) };
}

impl<'s, F: FnMut(&mut [i16]) + Send + 'static> AudioOut<'s, F> {
    /// Opens the default playback device at `rate` sample frames a second, stereo, and starts it.
    pub fn open(_sdl: &'s Sdl, rate: u32, fill: F, stats: Arc<AudioStats>) -> Result<Self> {
        let spec = sys::SDL_AudioSpec {
            format: sys::SDL_AUDIO_S16,
            channels: 2,
            freq: rate as c_int,
        };
        let feed = Box::into_raw(Box::new(Feed {
            fill,
            buf: Vec::new(),
            stats,
            epoch: Instant::now(),
        }));
        // SAFETY: a valid spec; `feed` lives until `Drop` destroys the stream, after which SDL no longer calls back.
        let stream = unsafe {
            sys::SDL_OpenAudioDeviceStream(
                sys::SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,
                &spec,
                Some(feed_callback::<F>),
                feed.cast(),
            )
        };
        if stream.is_null() {
            let e = error("SDL_OpenAudioDeviceStream");
            // SAFETY: SDL didn't take it.
            drop(unsafe { Box::from_raw(feed) });
            return Err(e);
        }
        // SAFETY: a live stream; its device starts paused.
        unsafe { sys::SDL_ResumeAudioStreamDevice(stream) };
        Ok(AudioOut {
            stream,
            feed,
            _sdl: PhantomData,
        })
    }

    /// The device's own format: its rate, channels and buffer in sample frames.
    pub fn device_format(&self) -> Option<(u32, u32, u32)> {
        let mut spec = sys::SDL_AudioSpec {
            format: sys::SDL_AUDIO_S16,
            channels: 0,
            freq: 0,
        };
        let mut frames = 0;
        // SAFETY: a live stream and valid out pointers.
        let ok = unsafe {
            let dev = sys::SDL_GetAudioStreamDevice(self.stream);
            sys::SDL_GetAudioDeviceFormat(dev, &mut spec, &mut frames)
        };
        ok.then_some((
            spec.freq.max(0) as u32,
            spec.channels.max(0) as u32,
            frames.max(0) as u32,
        ))
    }
}

impl<F> Drop for AudioOut<'_, F> {
    fn drop(&mut self) {
        // SAFETY: destroying the stream stops the callback first; then the feed is no longer shared.
        unsafe {
            sys::SDL_DestroyAudioStream(self.stream);
            drop(Box::from_raw(self.feed));
        }
    }
}
