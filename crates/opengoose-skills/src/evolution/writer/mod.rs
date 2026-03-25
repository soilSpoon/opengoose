// writer — Skill file writing, refinement, and effectiveness tracking

mod effectiveness;
mod refine;

pub use effectiveness::{extract_name_from_content, update_effectiveness_versioned};
pub use refine::refine_skill;

use crate::metadata::{Effectiveness, GeneratedFrom, SkillMetadata};
use chrono::Utc;
use std::path::Path;

// ---------------------------------------------------------------------------
// Pure functions — testable without filesystem I/O
// ---------------------------------------------------------------------------

/// Extract skill name from YAML frontmatter content.
/// Returns an error if the content has no valid frontmatter or no `name:` field.
pub(crate) fn parse_skill_name(content: &str) -> anyhow::Result<String> {
    extract_name_from_content(content)
        .ok_or_else(|| anyhow::anyhow!("no name found in skill content"))
}

/// Compute the next skill version: increments existing, defaults to 1 when None.
pub(crate) fn compute_version_bump(existing_version: Option<u32>) -> u32 {
    existing_version.map_or(1, |v| v + 1)
}

/// Pure metadata builder — constructs `SkillMetadata` without any I/O.
pub(crate) fn build_skill_metadata(
    stamp_id: i64,
    work_item_id: i64,
    dimension: &str,
    score: f32,
    version: u32,
    evolver_work_item_id: Option<i64>,
) -> SkillMetadata {
    SkillMetadata {
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
            subsequent_scores: Vec::new(),
        },
        skill_version: version,
    }
}

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
    let name = parse_skill_name(skill_content)?;

    let skill_dir = base_dir.join(format!(".opengoose/rigs/{rig_id}/skills/learned/{name}"));
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), skill_content)?;

    let metadata = build_skill_metadata(
        stamp_id,
        work_item_id,
        dimension,
        score,
        1,
        evolver_work_item_id,
    );
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
    let prev_version = match std::fs::read_to_string(&meta_path) {
        Ok(c) => match serde_json::from_str::<SkillMetadata>(&c) {
            Ok(meta) => Some(meta.skill_version),
            Err(e) => {
                tracing::debug!(path = %meta_path.display(), "failed to parse metadata.json: {e}");
                None
            }
        },
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            tracing::debug!(path = %meta_path.display(), "failed to read metadata.json: {e}");
            None
        }
        Err(_) => None,
    };

    let new_version = compute_version_bump(prev_version);

    std::fs::write(skill_dir.join("SKILL.md"), new_content)?;

    let metadata = build_skill_metadata(
        stamp_id,
        work_item_id,
        dimension,
        score,
        new_version,
        evolver_work_item_id,
    );
    std::fs::write(&meta_path, serde_json::to_string_pretty(&metadata)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Pure function tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_skill_name_extracts_from_frontmatter() {
        let content = "---\nname: auto-commit\ndescription: foo\n---\nbody";
        assert_eq!(
            parse_skill_name(content).expect("should parse"),
            "auto-commit"
        );
    }

    #[test]
    fn parse_skill_name_fails_on_missing_name() {
        let content = "---\ndescription: foo\n---\nbody";
        assert!(parse_skill_name(content).is_err());
    }

    #[test]
    fn parse_skill_name_fails_on_no_frontmatter() {
        let content = "# No frontmatter here";
        assert!(parse_skill_name(content).is_err());
    }

    #[test]
    fn compute_version_bump_increments() {
        assert_eq!(compute_version_bump(Some(3)), 4);
    }

    #[test]
    fn compute_version_bump_defaults_to_one_when_none() {
        assert_eq!(compute_version_bump(None), 1);
    }

    #[test]
    fn build_skill_metadata_sets_all_fields() {
        let meta = build_skill_metadata(42, 10, "quality", 4.5, 2, None);
        assert_eq!(meta.skill_version, 2);
        assert_eq!(meta.generated_from.stamp_id, 42);
        assert_eq!(meta.generated_from.work_item_id, 10);
        assert_eq!(meta.generated_from.dimension, "quality");
        assert_eq!(meta.generated_from.score, 4.5);
        assert!(meta.evolver_work_item_id.is_none());
        assert!(meta.last_included_at.is_none());
        assert_eq!(meta.effectiveness.injected_count, 0);
        assert!(meta.effectiveness.subsequent_scores.is_empty());
    }

    #[test]
    fn build_skill_metadata_with_evolver_work_item() {
        let meta = build_skill_metadata(1, 2, "autonomy", 0.8, 5, Some(99));
        assert_eq!(meta.evolver_work_item_id, Some(99));
        assert_eq!(meta.skill_version, 5);
    }

    #[test]
    fn parse_skill_name_empty_content_fails() {
        assert!(parse_skill_name("").is_err());
    }

    #[test]
    fn parse_skill_name_whitespace_only_fails() {
        assert!(parse_skill_name("   \n\n  ").is_err());
    }

    // -----------------------------------------------------------------------
    // I/O integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn write_skill_creates_files() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
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
        .expect("file write should succeed");
        assert_eq!(name, "my-skill");

        let skill_dir = base_dir.join(".opengoose/rigs/rig-1/skills/learned/my-skill");
        assert!(skill_dir.join("SKILL.md").exists());
        assert!(skill_dir.join("metadata.json").exists());
    }

    #[test]
    fn write_skill_to_rig_scope_empty_content_fails() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let result = write_skill_to_rig_scope(
            tmp.path(),
            "rig-1",
            "",
            WriteSkillParams {
                stamp_id: 1,
                work_item_id: 2,
                dimension: "Quality",
                score: 0.2,
                evolver_work_item_id: None,
            },
        );
        assert!(
            result.is_err(),
            "empty content should fail to parse skill name"
        );
    }

    #[test]
    fn write_skill_to_rig_scope_no_name_in_frontmatter_fails() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let content = "---\ndescription: Use when testing\n---\nbody";
        let result = write_skill_to_rig_scope(
            tmp.path(),
            "rig-1",
            content,
            WriteSkillParams {
                stamp_id: 1,
                work_item_id: 2,
                dimension: "Quality",
                score: 0.2,
                evolver_work_item_id: None,
            },
        );
        assert!(result.is_err(), "content without name field should fail");
    }

    #[test]
    fn update_existing_skill_missing_metadata_still_succeeds() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");
        // No metadata.json exists — update should still work (defaults to version 1)
        let new_content = "---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
        update_existing_skill(&skill_dir, new_content, 5, 42, "Quality", 0.15, None)
            .expect("should succeed even without prior metadata");

        let meta: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("test file read should succeed"),
        )
        .expect("JSON parse should succeed");
        assert_eq!(
            meta.skill_version, 1,
            "should default to version 1 when no prior metadata"
        );
    }

    #[test]
    fn update_existing_skill_overwrites_content() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");

        let original = "---\nname: my-skill\ndescription: Use when original\n---\nOld body\n";
        std::fs::write(skill_dir.join("SKILL.md"), original)
            .expect("test fixture write should succeed");

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
            serde_json::to_string_pretty(&meta).expect("JSON serialization should succeed"),
        )
        .expect("file write should succeed");

        let new_content = "---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
        update_existing_skill(&skill_dir, new_content, 5, 42, "Quality", 0.15, Some(100))
            .expect("update_existing_skill should succeed");

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md"))
            .expect("test file read should succeed");
        assert!(written.contains("New body"));

        let updated_meta: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("test file read should succeed"),
        )
        .expect("JSON parse should succeed");
        assert_eq!(updated_meta.generated_from.stamp_id, 5);
        assert!(updated_meta.effectiveness.subsequent_scores.is_empty());
        assert_eq!(updated_meta.skill_version, 2);
    }
}
