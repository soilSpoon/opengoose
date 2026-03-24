// opengoose-skills — Skill loading, evolution, metadata, management
//
// No dependency on board, rig, or goose.
// All public functions take base_dir: &Path for filesystem root.

pub mod error;
pub use error::*;

pub mod catalog;
pub mod evolution;
pub mod lifecycle;
pub mod loader;
pub mod manage;
pub mod metadata;
pub mod source;
#[cfg(test)]
pub(crate) mod test_utils;
