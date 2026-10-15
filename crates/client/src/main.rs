//! Tidewater client: the client core (`tidewater_client::core`) in the Bevy shell (`shell`). Loads the user's
//! installation, runs the startup sequence, a map given on the command line, or a scripted scene (`script`).
//!
//! The 640 × 480 screen is drawn at a whole number of physical pixels per screen pixel, black-bordered in a larger
//! window, and the map view shows one map pixel per screen pixel, as the original does (`display`). The mouse wheel
//! zooms only with the `--free-zoom` development option. The controls are in `game`.

use anyhow::Result;
use clap::Parser;
use tidewater_client::{args::Args, host, shell};

fn main() -> Result<()> {
    let l = host::launch(Args::parse())?;
    shell::run(l.core, l.title, l.ui_scale, l.headless);
    Ok(())
}
