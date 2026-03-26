// Skill bridge functions — thin wrappers over opengoose-skills that
// add cross-crate dependencies (opengoose-rig for conversation_log).

pub use opengoose_skills::evolution::parser::{
    EvolveAction, SweepDecision, parse_evolve_response, parse_sweep_response,
};
pub use opengoose_skills::evolution::prompts::{
    UpdatePromptParams, build_evolve_prompt, build_sweep_prompt, build_update_prompt,
    summarize_for_prompt,
};
pub use opengoose_skills::evolution::validator::validate_skill_output;
pub use opengoose_skills::evolution::writer::{
    WriteSkillParams, refine_skill, update_effectiveness_versioned, update_existing_skill,
    write_skill_to_rig_scope,
};
pub use opengoose_skills::lifecycle::{Lifecycle, determine_lifecycle};
pub use opengoose_skills::loader::{
    LoadedSkill, SkillScope, extract_body, load_dormant_and_archived, load_skills,
    update_inclusion_tracking,
};
pub use opengoose_skills::metadata::{SkillMetadata, is_effective, read_metadata};

/// Read and summarize a Worker's conversation log for a given work item.
/// Bridges opengoose-rig (conversation_log) with opengoose-skills (summarize).
pub fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_for_prompt(&content, 4000))
        .unwrap_or_default()
}

/// Load skills for a given rig and project directory.
pub fn load_skills_for(
    base_dir: &std::path::Path,
    rig_id: Option<&str>,
    project_dir: Option<&std::path::Path>,
) -> Vec<LoadedSkill> {
    load_skills(base_dir, rig_id, project_dir)
}
