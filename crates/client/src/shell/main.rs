//! The `tidewater` program: the core in an SDL 3 window. The shell itself is `app.rs` (the loop, input, mouse lock and
//! sound device) over `sdl.rs` (our thin safe wrapper over `sdl3-sys`, every `unsafe` block of the program). They sit
//! in modules of their own so the iOS app's static library (`crates/ios`) and the Android app's `libmain.so`
//! (`crates/android`) build the same two files: one code path on the desktop and on the phone.

mod app;
mod sdl;

fn main() -> anyhow::Result<()> {
    app::run_program()
}
