// writer.rs — Skill file writing and effectiveness tracking

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
// refine_skill — sweep-mode refinement (no stamp context)
// ---------------------------------------------------------------------------

/// Refine a skill's content without stamp context (used by sweep mode).
/// Bumps version, resets effectiveness, preserves generated_from as-is.
pub fn refine_skill(skill_dir: &Path, new_content: &str) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    let prev = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|c| serde_json::from_str::<SkillMetadata>(&c).ok());

    let prev_version = prev.as_ref().map(|m| m.skill_version).unwrap_or(1);

    std::fs::write(skill_dir.join("SKILL.md"), new_content)?;

    if let Some(mut meta) = prev {
        meta.skill_version = prev_version + 1;
        meta.effectiveness = Effectiveness {
            injected_count: 0,
            subsequent_scores: vec![],
        };
        meta.last_included_at = None;
        std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// update_effectiveness_versioned — version-aware effectiveness update
// ---------------------------------------------------------------------------

/// Version-aware effectiveness update.
/// Only appends score if the stamp's active version for this skill matches the current version.
pub fn update_effectiveness_versioned(
    skill_dir: &Path,
    new_score: f32,
    active_versions_json: Option<&str>,
) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    let content = std::fs::read_to_string(&meta_path)?;
    let mut meta: SkillMetadata = serde_json::from_str(&content)?;

    // Extract skill name from directory name
    let skill_name = skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Check version match
    let version_matches = match active_versions_json {
        Some(json) => {
            let versions: std::collections::HashMap<String, u32> =
                serde_json::from_str(json).unwrap_or_default();
            versions.get(skill_name).copied() == Some(meta.skill_version)
        }
        None => true, // no version info = legacy behavior, always count
    };

    if version_matches {
        meta.effectiveness.subsequent_scores.push(new_score);
        std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// extract_name_from_content — private helper
// ---------------------------------------------------------------------------

fn extract_name_from_content(content: &str) -> Option<String> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    frontmatter.lines().find_map(|l| {
        l.strip_prefix("name:")
            .map(|v| v.trim().trim_matches('"').to_string())
    })
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

    #[test]
    fn refine_skill_bumps_version_preserves_generated_from() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let original = "---\nname: my-skill\ndescription: Use when original\n---\nOld body\n";
        std::fs::write(skill_dir.join("SKILL.md"), original).unwrap();

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 99,
                work_item_id: 50,
                dimension: "Quality".into(),
                score: 0.15,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: Some(200),
            last_included_at: Some(Utc::now().to_rfc3339()),
            effectiveness: Effectiveness {
                injected_count: 5,
                subsequent_scores: vec![0.4, 0.5],
            },
            skill_version: 3,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let new_content = "---\nname: my-skill\ndescription: Use when refined\n---\nNew body\n";
        refine_skill(&skill_dir, new_content).unwrap();

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(written.contains("New body"));

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.generated_from.stamp_id, 99);
        assert_eq!(updated.evolver_work_item_id, Some(200));
        assert_eq!(updated.skill_version, 4);
        assert!(updated.effectiveness.subsequent_scores.is_empty());
        assert!(updated.last_included_at.is_none());
    }

    #[test]
    fn update_effectiveness_versioned_matching_version() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

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
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 2,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let versions = r#"{"my-skill": 2}"#;
        update_effectiveness_versioned(&skill_dir, 0.8, Some(versions)).unwrap();

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.8]);
    }

    #[test]
    fn update_effectiveness_versioned_old_version_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

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
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 2,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let old_versions = r#"{"my-skill": 1}"#;
        update_effectiveness_versioned(&skill_dir, 0.8, Some(old_versions)).unwrap();

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert!(updated.effectiveness.subsequent_scores.is_empty());
    }

    #[test]
    fn update_effectiveness_versioned_no_versions_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

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
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        update_effectiveness_versioned(&skill_dir, 0.7, None).unwrap();

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.7]);
    }
}
