// Shared test fixtures for opengoose-skills.
//
// Consolidates duplicated SkillMetadata and LoadedSkill construction
// patterns found across loader.rs, metadata.rs, effectiveness.rs tests.
//
// For environment isolation (HOME override), see test_utils.rs.

use crate::loader::{LoadedSkill, SkillScope};
use crate::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
use std::path::PathBuf;

/// Build a LoadedSkill with sensible test defaults.
pub fn make_loaded_skill(name: &str, scope: SkillScope) -> LoadedSkill {
    LoadedSkill {
        name: name.to_string(),
        description: format!("Test skill: {name}"),
        path: PathBuf::from(format!("/tmp/test-skills/{name}")),
        content: format!("---\nname: {name}\ndescription: Test skill: {name}\n---\nTest body"),
        scope,
    }
}

/// Build SkillMetadata with a given version and neutral defaults.
pub fn make_metadata(version: u32) -> SkillMetadata {
    SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id: 1,
            work_item_id: 1,
            dimension: "Quality".to_string(),
            score: 0.2,
        },
        generated_at: chrono::Utc::now().to_rfc3339(),
        evolver_work_item_id: None,
        last_included_at: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        },
        skill_version: version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_loaded_skill_has_expected_fields() {
        let skill = make_loaded_skill("code-review", SkillScope::Installed);
        assert_eq!(skill.name, "code-review");
        assert_eq!(skill.scope, SkillScope::Installed);
        assert!(skill.content.contains("code-review"));
    }

    #[test]
    fn make_metadata_sets_version() {
        let meta = make_metadata(3);
        assert_eq!(meta.skill_version, 3);
        assert_eq!(meta.effectiveness.injected_count, 0);
        assert!(meta.last_included_at.is_none());
    }
}
