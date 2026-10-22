//! Network play: lockstep over TCP (`docs/design/network.md`, `docs/specs/net/lockstep.md`,
//! `docs/specs/net/leave-and-drop.md`).
//!
//! | Module | What it holds |
//! |---|---|
//! | [`proto`] | The wire messages (postcard), the version and the identity a handshake compares |
//! | [`tcp`] | Framed TCP on standard threads, the handshake that fills the game's slots, and the hub that relays |
//! | [`lockstep`] | The turn engine, [`NetSession`], the [`Session`](crate::session::Session) a network game plays through |
//! | [`room`] | The lobby as a room joiners enter: rows, races, chat, kick out, start |
//! | [`lobby`] | The lobby given on the command line (`--lobby`) and the game it makes |
//!
//! No async runtime: a reader thread and a writer thread per connection feed the game thread through channels, and the
//! game thread never blocks on the network once a game is running.

pub mod lobby;
pub mod lockstep;
pub mod proto;
pub mod room;
pub mod tcp;

pub use lobby::NetStart;
pub use lockstep::NetSession;
