// effectiveness — Version-aware effectiveness tracking + name extraction

use crate::metadata::SkillMetadata;
use std::path::Path;

// ---------------------------------------------------------------------------
// version_matches — pure logic for version comparison
// ---------------------------------------------------------------------------

/// Check whether a skill's current version matches the active version recorded
/// in a JSON map of `{ skill_name: version }`.
/// Returns `true` when no version info is provided (legacy behavior).
fn version_matches(
    skill_name: &str,
    current_version: u32,
    active_versions_json: Option<&str>,
) -> bool {
    match active_versions_json {
        Some(json) => {
            let versions: std::collections::HashMap<String, u32> =
                serde_json::from_str(json).unwrap_or_default();
            versions.get(skill_name).copied() == Some(current_version)
        }
        None => true, // no version info = legacy behavior, always count
    }
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

    let skill_name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if version_matches(skill_name, meta.skill_version, active_versions_json) {
        meta.effectiveness.subsequent_scores.push(new_score);
        std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// extract_name_from_content — parse skill name from YAML frontmatter
// ---------------------------------------------------------------------------

pub fn extract_name_from_content(content: &str) -> Option<String> {
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
    use crate::metadata::{Effectiveness, GeneratedFrom};
    use chrono::Utc;

    #[test]
    fn version_matches_with_matching_version() {
        assert!(version_matches("my-skill", 2, Some(r#"{"my-skill": 2}"#)));
    }

    #[test]
    fn version_matches_with_old_version() {
        assert!(!version_matches("my-skill", 2, Some(r#"{"my-skill": 1}"#)));
    }

    #[test]
    fn version_matches_legacy_no_versions() {
        assert!(version_matches("my-skill", 5, None));
    }

    #[test]
    fn version_matches_skill_not_in_map() {
        assert!(!version_matches(
            "missing",
            1,
            Some(r#"{"other": 1}"#)
        ));
    }

    fn make_test_metadata(version: u32) -> SkillMetadata {
        SkillMetadata {
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
            skill_version: version,
        }
    }

    #[test]
    fn update_effectiveness_versioned_matching_version() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&make_test_metadata(2)).unwrap(),
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

        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&make_test_metadata(2)).unwrap(),
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

        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&make_test_metadata(1)).unwrap(),
        )
        .unwrap();

        update_effectiveness_versioned(&skill_dir, 0.7, None).unwrap();

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.7]);
    }

    #[test]
    fn extract_name_from_valid_frontmatter() {
        let content = "---\nname: my-skill\ndescription: Use when testing\n---\n# Body\n";
        assert_eq!(
            extract_name_from_content(content),
            Some("my-skill".to_string())
        );
    }

    #[test]
    fn extract_name_returns_none_without_frontmatter() {
        assert_eq!(extract_name_from_content("# No frontmatter"), None);
    }

    #[test]
    fn extract_name_returns_none_for_unclosed_frontmatter() {
        assert_eq!(
            extract_name_from_content("---\nname: test\nno closing"),
            None
        );
    }
}
