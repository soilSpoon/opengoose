mod defaults;
mod error;
mod goose_bridge;
mod profile;
mod skill;
mod skill_defaults;
mod skill_store;
mod store;
pub mod workspace;

pub use defaults::all_defaults;
pub use error::{ProfileError, ProfileResult};
pub use goose_bridge::register_profiles_path;
pub use profile::{AgentProfile, ExtensionRef, ParameterRef, ProfileSettings, SubRecipeRef};
pub use skill::Skill;
pub use skill_defaults::all_default_skills;
pub use skill_store::SkillStore;
pub use store::ProfileStore;
