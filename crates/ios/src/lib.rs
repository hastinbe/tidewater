//! The iOS app's static library: the client's SDL 3 shell, built for the phone.
//!
//! The shell is two files in `crates/client/src/shell/`: `app.rs` (the loop, input and sound) and `sdl.rs` (the safe
//! wrapper over SDL, the only `unsafe`). The desktop's `tidewater` binary and this library both include them, so the
//! desktop and the phone run one code path. The iOS-only parts (the entry point `tidewater_ios_main`, the document
//! picker, the lifecycle events) are inside them, behind `target_os = "ios"`. See `docs/design/ios.md`.
//!
//! On every other target the library is empty, so the workspace builds as before.

#[cfg(target_os = "ios")]
#[path = "../../client/src/shell/app.rs"]
mod app;
#[cfg(target_os = "ios")]
#[path = "../../client/src/shell/sdl.rs"]
mod sdl;
