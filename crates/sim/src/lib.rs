//! The game simulation: deterministic, headless and free of engine types.
//!
//! State is integer-only and every container iterates in a fixed order, so the same commands from the same start
//! produce the same [`World::state_hash`] on every machine. That is what lockstep multiplayer and replays rely on.
//!
//! Engine behaviour comes only from reviewed specs (`docs/specs/`). Until a spec covers an area, the code here is a
//! placeholder of our own design and says so; constants that a spec will replace live in [`Rules`].

pub mod path;
pub mod rng;
pub mod terrain;
pub mod timing;
mod world;

pub use rng::Rng;
pub use terrain::{Terrain, Tile};
pub use world::{Command, Layer, Order, Player, Rules, Unit, UnitId, World, WorldError};

/// Map pixels per tile.
pub const TILE_PX: i32 = 32;
