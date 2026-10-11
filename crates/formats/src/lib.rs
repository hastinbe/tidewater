//! Readers for the data formats of a user-supplied *Warcraft II: Battle.net Edition* installation.
//!
//! Every format detail is cited in the module that uses it. Nothing here is derived from the original program's code.

pub mod alow;
pub mod grp;
pub mod install;
pub mod mpq;
mod mpq_codec;
pub mod pcx;
pub mod pud;
pub mod sprites;
pub mod tileset;
pub mod udta;
pub mod ugrd;
