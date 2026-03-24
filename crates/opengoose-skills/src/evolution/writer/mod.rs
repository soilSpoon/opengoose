// writer — Skill file writing, refinement, and effectiveness tracking

mod effectiveness;
mod refine;

pub use effectiveness::{extract_name_from_content, update_effectiveness_versioned};
pub use refine::refine_skill;

use crate::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
use chrono::Utc;
use std::path::Path;

// ---------------------------------------------------------------------------
// WriteSkillParams — groups stamp context to keep argument counts under limit
// ---------------------------------------------------------------------------

/// Parameters for writing a skill from stamp context.
pub struct WriteSkillParams<'a> {
    pub stamp_id: i64,
    pub work_item_id: i64,
    pub dimension: &'a str,
    pub score: f32,
    pub evolver_work_item_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// write_skill_to_rig_scope — write SKILL.md + metadata.json
// ---------------------------------------------------------------------------

/// Write a new learned skill to the rig scope.
/// `base_dir` is the filesystem root (typically `~/.opengoose` parent, i.e. home dir).
/// The skill is written to `{base_dir}/.opengoose/rigs/{rig_id}/skills/learned/{name}/`.
pub fn write_skill_to_rig_scope(
    base_dir: &Path,
    rig_id: &str,
    skill_content: &str,
    params: WriteSkillParams<'_>,
) -> anyhow::Result<String> {
    let WriteSkillParams {
        stamp_id,
        work_item_id,
        dimension,
        score,
        evolver_work_item_id,
    } = params;
    let name = extract_name_from_content(skill_content)
        .ok_or_else(|| anyhow::anyhow!("cannot extract name from skill content"))?;

    let skill_dir = base_dir.join(format!(".opengoose/rigs/{rig_id}/skills/learned/{name}"));
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), skill_content)?;

    let metadata = SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id,
            work_item_id,
            dimension: dimension.to_string(),
            score,
        },
        generated_at: Utc::now().to_rfc3339(),
        evolver_work_item_id,
        last_included_at: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        },
        skill_version: 1,
    };
    std::fs::write(
        skill_dir.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    Ok(name)
}

// ---------------------------------------------------------------------------
// update_existing_skill — overwrite with new content, bump version
// ---------------------------------------------------------------------------

/// Update an existing learned skill with new content.
/// Resets effectiveness tracking and bumps skill_version.
pub fn update_existing_skill(
    skill_dir: &Path,
    new_content: &str,
    stamp_id: i64,
    work_item_id: i64,
    dimension: &str,
    score: f32,
    evolver_work_item_id: Option<i64>,
) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    let prev_version = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|c| serde_json::from_str::<SkillMetadata>(&c).ok())
        .map(|m| m.skill_version)
        .unwrap_or(1);

    std::fs::write(skill_dir.join("SKILL.md"), new_content)?;

    let metadata = SkillMetadata {
        generated_from: GeneratedFrom {
            stamp_id,
            work_item_id,
            dimension: dimension.to_string(),
            score,
        },
        generated_at: Utc::now().to_rfc3339(),
        evolver_work_item_id,
        last_included_at: None,
        effectiveness: Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        },
        skill_version: prev_version + 1,
    };
    std::fs::write(&meta_path, serde_json::to_string_pretty(&metadata)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_skill_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let base_dir = tmp.path();
        let content = "---\nname: my-skill\ndescription: Use when testing\n---\n# Body\n";

        let name = write_skill_to_rig_scope(
            base_dir,
            "rig-1",
            content,
            WriteSkillParams {
                stamp_id: 1,
                work_item_id: 2,
                dimension: "Quality",
                score: 0.2,
                evolver_work_item_id: None,
            },
        )
        .unwrap();
        assert_eq!(name, "my-skill");

        let skill_dir = base_dir.join(".opengoose/rigs/rig-1/skills/learned/my-skill");
        assert!(skill_dir.join("SKILL.md").exists());
        assert!(skill_dir.join("metadata.json").exists());
    }

    #[test]
    fn update_existing_skill_overwrites_content() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let original = "---\nname: my-skill\ndescription: Use when original\n---\nOld body\n";
        std::fs::write(skill_dir.join("SKILL.md"), original).unwrap();

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 2,
                subsequent_scores: vec![0.3, 0.4],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let new_content = "---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
        update_existing_skill(&skill_dir, new_content, 5, 42, "Quality", 0.15, Some(100)).unwrap();

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(written.contains("New body"));

        let updated_meta: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated_meta.generated_from.stamp_id, 5);
        assert!(updated_meta.effectiveness.subsequent_scores.is_empty());
        assert_eq!(updated_meta.skill_version, 2);
    }
}
