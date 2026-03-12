//! Bidirectional conversion between `AgentProfile` and Goose `Recipe`.
//!
//! This allows OpenGoose profiles to be used as Goose recipes and vice versa,
//! enabling interoperability with the broader Goose ecosystem (sub-recipes,
//! Summon extension, recipe sharing).

mod conversion;
mod extensions;
#[cfg(test)]
mod tests;

pub use conversion::{profile_to_recipe, recipe_to_profile, settings_to_retry_config};
pub use extensions::{config_to_ext_ref, ext_ref_to_config};
