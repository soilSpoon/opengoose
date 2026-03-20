// Skills Load — thin re-export layer + backward-compat wrapper.
//
// All logic lives in opengoose-skills::{loader, lifecycle, catalog, metadata}.

pub use opengoose_skills::lifecycle::{determine_lifecycle, Lifecycle};
pub use opengoose_skills::loader::{
    extract_body, load_dormant_and_archived, load_skills, update_inclusion_tracking, LoadedSkill,
    SkillScope,
};
pub use opengoose_skills::metadata::{is_effective, read_metadata};

/// Backward-compat wrapper: load skills using home_dir as base_dir.
/// Callers in the binary crate pass (rig_id, project_dir) without a base_dir.
pub fn load_skills_for(rig_id: Option<&str>, project_dir: Option<&std::path::Path>) -> Vec<LoadedSkill> {
    let base_dir = crate::home_dir();
    load_skills(&base_dir, rig_id, project_dir)
}
