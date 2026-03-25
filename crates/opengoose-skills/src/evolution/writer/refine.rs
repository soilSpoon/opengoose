// refine — Sweep-mode skill refinement (no stamp context)

use crate::metadata::{Effectiveness, SkillMetadata};
use std::path::Path;

// ---------------------------------------------------------------------------
// refine_skill — sweep-mode refinement (no stamp context)
// ---------------------------------------------------------------------------

/// Refine a skill's content without stamp context (used by sweep mode).
/// Bumps version, resets effectiveness, preserves generated_from as-is.
pub fn refine_skill(skill_dir: &Path, new_content: &str) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    let prev = match std::fs::read_to_string(&meta_path) {
        Ok(c) => match serde_json::from_str::<SkillMetadata>(&c) {
            Ok(meta) => Some(meta),
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{Effectiveness, GeneratedFrom};
    use chrono::Utc;

    #[test]
    fn refine_skill_bumps_version_preserves_generated_from() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");

        let original = "---\nname: my-skill\ndescription: Use when original\n---\nOld body\n";
        std::fs::write(skill_dir.join("SKILL.md"), original)
            .expect("test fixture write should succeed");

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
            serde_json::to_string_pretty(&meta).expect("JSON serialization should succeed"),
        )
        .expect("file write should succeed");

        let new_content = "---\nname: my-skill\ndescription: Use when refined\n---\nNew body\n";
        refine_skill(&skill_dir, new_content).expect("refine_skill should succeed");

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md"))
            .expect("test file read should succeed");
        assert!(written.contains("New body"));

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json"))
                .expect("test file read should succeed"),
        )
        .expect("JSON parse should succeed");
        assert_eq!(updated.generated_from.stamp_id, 99);
        assert_eq!(updated.evolver_work_item_id, Some(200));
        assert_eq!(updated.skill_version, 4);
        assert!(updated.effectiveness.subsequent_scores.is_empty());
        assert!(updated.last_included_at.is_none());
    }
}
