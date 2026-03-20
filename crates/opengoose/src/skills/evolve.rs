// Skill Evolution — thin re-export layer.
// All logic lives in opengoose-skills::evolution.
// read_conversation_log() and build_active_versions_json() stay here
// because they depend on opengoose_rig and crate::skills::load respectively.

pub use opengoose_skills::evolution::parser::{
    parse_evolve_response, parse_sweep_response, EvolveAction, SweepDecision,
};
pub use opengoose_skills::evolution::prompts::{
    build_evolve_prompt, build_sweep_prompt, build_update_prompt, summarize_for_prompt,
    UpdatePromptParams,
};
pub use opengoose_skills::evolution::validator::validate_skill_output;
pub use opengoose_skills::evolution::writer::{
    refine_skill, update_effectiveness_versioned, update_existing_skill, write_skill_to_rig_scope,
};
pub use opengoose_skills::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};

// ---------------------------------------------------------------------------
// read_conversation_log — depends on opengoose_rig (stays in binary crate)
// ---------------------------------------------------------------------------

pub fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_for_prompt(&content, 4000))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// build_active_versions_json — depends on crate::skills::load (stays here)
// TODO: move to opengoose-skills once LoadedSkill/SkillScope are moved in Task 4
// ---------------------------------------------------------------------------

pub fn build_active_versions_json(skills: &[crate::skills::load::LoadedSkill]) -> String {
    let mut map = std::collections::HashMap::new();
    for skill in skills {
        if skill.scope == crate::skills::load::SkillScope::Learned
            && let Some(meta) = crate::skills::load::read_metadata(&skill.path)
        {
            map.insert(skill.name.clone(), meta.skill_version);
        }
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}
