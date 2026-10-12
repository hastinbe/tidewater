//! The game simulation: deterministic, headless and free of engine types.
//!
//! State is integer-only and every container iterates in a fixed order, so the same commands from the same start
//! produce the same [`World::state_hash`] on every machine. That is what lockstep multiplayer and replays rely on.
//!
//! Engine behaviour comes only from reviewed specs (`docs/specs/`). Until a spec covers an area, the code here is a
//! placeholder of our own design and says so; constants that a spec will replace live in [`Rules`].

pub mod acquire;
pub mod ai;
pub mod anim;
pub mod attack_ai;
mod battle;
mod build;
pub mod caster_ai;
pub mod combat;
pub mod corpses;
pub mod economy;
mod ground;
mod harvest;
mod movement;
mod orders;
pub mod path;
pub mod placement;
mod repair;
pub mod rescue;
pub mod rng;
pub mod site;
pub mod slots;
pub mod sound;
pub mod spells;
pub mod tech;
pub mod terrain;
pub mod timing;
pub mod training_ai;
mod transport;
pub mod victory;
pub mod vision;
pub mod worker_ai;
mod world;

pub use battle::Missile;
pub use rng::Rng;
pub use terrain::{Terrain, WallStage};
pub use world::{Command, Gather, Layer, Order, Player, Rules, Source, SpawnError, Unit, UnitId, World, WorldError};

/// Map pixels per tile.
pub const TILE_PX: i32 = 32;
