//! The Android app's native library, `libmain.so`: the client's SDL 3 shell, built for the phone.
//!
//! The shell is two files in `crates/client/src/shell/`: `app.rs` (the loop, input and sound) and `sdl.rs` (the safe
//! wrapper over SDL, the only `unsafe`). The desktop's `tidewater` binary, the iOS library and this one all include
//! them, so every platform runs one code path. The Android-only parts (the entry point `SDL_main`, the picker's
//! callback, the lifecycle events) are inside them, behind `target_os = "android"`. SDL's Java activity (copied into
//! `platform/android` at build time) loads this library and calls `SDL_main`. See `docs/design/android.md`.
//!
//! On every other target the library is empty, so the workspace builds as before.

#[cfg(target_os = "android")]
#[path = "../../client/src/shell/app.rs"]
mod app;
#[cfg(target_os = "android")]
#[path = "../../client/src/shell/sdl.rs"]
mod sdl;
