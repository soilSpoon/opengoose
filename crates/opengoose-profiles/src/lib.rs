mod defaults;
mod error;
mod goose_bridge;
mod profile;
mod store;
pub mod workspace;

pub use defaults::all_defaults;
pub use error::{ProfileError, ProfileResult};
pub use goose_bridge::register_profiles_path;
pub use profile::{AgentProfile, ExtensionRef, ParameterRef, ProfileSettings, SubRecipeRef};
pub use store::ProfileStore;
