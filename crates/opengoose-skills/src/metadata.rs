// metadata.rs — Skill metadata types and frontmatter parsing
//
// Single source of truth for:
//   - SkillFrontmatter (name + description extracted from YAML frontmatter)
//   - SkillMetadata / GeneratedFrom / Effectiveness (persisted in metadata.json)
//   - parse_frontmatter() — unified frontmatter parser
//   - read_metadata() / write_metadata() — metadata.json I/O

use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// SkillFrontmatter — extracted from YAML frontmatter
// ---------------------------------------------------------------------------

pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Metadata types
// ---------------------------------------------------------------------------

fn default_version() -> u32 {
    1
}

/// Skill metadata stored alongside SKILL.md files.
///
/// # Examples
///
/// ```
/// use opengoose_skills::metadata::SkillMetadata;
///
/// let json = r#"{
///     "generated_from": {"stamp_id": 1, "work_item_id": 1, "dimension": "Quality", "score": 0.2},
///     "generated_at": "2026-03-20T00:00:00Z",
///     "evolver_work_item_id": null,
///     "last_included_at": null,
///     "effectiveness": {"injected_count": 0, "subsequent_scores": []},
///     "skill_version": 1
/// }"#;
/// let meta: SkillMetadata = serde_json::from_str(json).unwrap();
/// assert_eq!(meta.skill_version, 1);
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub generated_from: GeneratedFrom,
    pub generated_at: String,
    pub evolver_work_item_id: Option<i64>,
    pub last_included_at: Option<String>,
    pub effectiveness: Effectiveness,
    #[serde(default = "default_version")]
    pub skill_version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GeneratedFrom {
    pub stamp_id: i64,
    pub work_item_id: i64,
    pub dimension: String,
    pub score: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Effectiveness {
    pub injected_count: u32,
    pub subsequent_scores: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Parse YAML frontmatter (--- delimited) and extract name and description.
/// Returns None if frontmatter is missing or either field is absent.
pub fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];

    let mut name = None;
    let mut description = None;
    for line in frontmatter.lines() {
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').to_string());
        }
        if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').to_string());
        }
    }

    Some(SkillFrontmatter {
        name: name?,
        description: description?,
    })
}

// ---------------------------------------------------------------------------
// Effectiveness analysis
// ---------------------------------------------------------------------------

/// Determine if a skill is effective based on subsequent scores.
/// Returns None if not enough data (< 3 scores).
/// Returns Some(true) if average improved by 0.2+ over generation score.
/// Returns Some(false) if no improvement.
pub fn is_effective(meta: &SkillMetadata) -> Option<bool> {
    let scores = &meta.effectiveness.subsequent_scores;
    if scores.len() < 3 {
        return None;
    }
    let avg: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let improvement = avg - meta.generated_from.score;
    Some(improvement >= 0.2)
}

// ---------------------------------------------------------------------------
// metadata.json I/O
// ---------------------------------------------------------------------------

/// Read metadata.json from a skill directory. Returns None if missing or invalid.
pub fn read_metadata(skill_dir: &Path) -> Option<SkillMetadata> {
    let meta_path = skill_dir.join("metadata.json");
    let content = match std::fs::read_to_string(&meta_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::debug!(path = %meta_path.display(), "failed to read metadata.json: {e}");
            return None;
        }
    };
    match serde_json::from_str(&content) {
        Ok(meta) => Some(meta),
        Err(e) => {
            tracing::debug!(path = %meta_path.display(), "failed to parse metadata.json: {e}");
            None
        }
    }
}

/// Write metadata.json to a skill directory.
pub fn write_metadata(skill_dir: &Path, meta: &SkillMetadata) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    std::fs::write(meta_path, serde_json::to_string_pretty(meta)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_metadata() -> impl Strategy<Value = SkillMetadata> {
        (any::<u32>(), "\\w{1,20}", 0i64..1000, 0.0f32..5.0).prop_map(
            |(version, dim, stamp_id, score)| SkillMetadata {
                generated_from: GeneratedFrom {
                    stamp_id,
                    work_item_id: 1,
                    dimension: dim,
                    score,
                },
                generated_at: "2026-01-01T00:00:00Z".to_string(),
                evolver_work_item_id: None,
                last_included_at: None,
                effectiveness: Effectiveness {
                    injected_count: 0,
                    subsequent_scores: vec![],
                },
                skill_version: version,
            },
        )
    }

    proptest! {
        #[test]
        fn prop_skill_metadata_json_roundtrip(meta in arb_metadata()) {
            let json = serde_json::to_string(&meta).expect("serialize");
            let back: SkillMetadata = serde_json::from_str(&json).expect("deserialize");
            prop_assert_eq!(meta.generated_from.stamp_id, back.generated_from.stamp_id);
            prop_assert_eq!(meta.generated_from.dimension, back.generated_from.dimension);
            prop_assert_eq!(meta.skill_version, back.skill_version);
            prop_assert!((meta.generated_from.score - back.generated_from.score).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn parse_frontmatter_valid() {
        let content = "---\nname: my-skill\ndescription: Use when testing\n---\n# Body\n";
        let fm = parse_frontmatter(content).expect("parse_frontmatter_valid should succeed");
        assert_eq!(fm.name, "my-skill");
        assert_eq!(fm.description, "Use when testing");
    }

    #[test]
    fn parse_frontmatter_missing_frontmatter() {
        let content = "# No frontmatter here";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn parse_frontmatter_missing_name() {
        let content = "---\ndescription: Use when testing\n---\n# Body\n";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn parse_frontmatter_missing_description() {
        let content = "---\nname: my-skill\n---\n# Body\n";
        assert!(parse_frontmatter(content).is_none());
    }

    #[test]
    fn skill_metadata_serde_roundtrip() {
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 5,
                work_item_id: 42,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: "2026-03-19T10:00:00Z".into(),
            evolver_work_item_id: Some(100),
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.5, 0.6],
            },
            skill_version: 1,
        };
        let json = serde_json::to_string(&meta).expect("JSON serialization should succeed");
        let parsed: SkillMetadata = serde_json::from_str(&json).expect("test JSON should parse");
        assert_eq!(parsed.generated_from.stamp_id, 5);
        assert_eq!(parsed.evolver_work_item_id, Some(100));
        assert_eq!(parsed.effectiveness.subsequent_scores, vec![0.5, 0.6]);
        assert_eq!(parsed.skill_version, 1);
    }

    #[test]
    fn is_effective_not_enough_data() {
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: "2026-03-20T08:00:00Z".into(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.5],
            },
            skill_version: 1,
        };
        assert_eq!(is_effective(&meta), None);
    }

    #[test]
    fn is_effective_improved() {
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: "2026-03-20T08:00:00Z".into(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.5, 0.6, 0.7],
            },
            skill_version: 1,
        };
        assert_eq!(is_effective(&meta), Some(true)); // avg 0.6 - 0.2 = 0.4 >= 0.2
    }

    #[test]
    fn is_effective_not_improved() {
        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Quality".into(),
                score: 0.2,
            },
            generated_at: "2026-03-20T08:00:00Z".into(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![0.2, 0.3, 0.25],
            },
            skill_version: 1,
        };
        assert_eq!(is_effective(&meta), Some(false)); // avg 0.25 - 0.2 = 0.05 < 0.2
    }

    #[test]
    fn read_write_metadata_roundtrip() {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).expect("directory creation should succeed");

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 7,
                work_item_id: 99,
                dimension: "Autonomy".into(),
                score: 0.35,
            },
            generated_at: "2026-03-20T08:00:00Z".into(),
            evolver_work_item_id: None,
            last_included_at: Some("2026-03-20T09:00:00Z".into()),
            effectiveness: Effectiveness {
                injected_count: 3,
                subsequent_scores: vec![0.4, 0.5, 0.6],
            },
            skill_version: 2,
        };

        write_metadata(&skill_dir, &meta).expect("write_metadata should succeed");
        let loaded = read_metadata(&skill_dir).expect("read_metadata should succeed");

        assert_eq!(loaded.generated_from.stamp_id, 7);
        assert_eq!(loaded.generated_from.dimension, "Autonomy");
        assert_eq!(loaded.evolver_work_item_id, None);
        assert_eq!(
            loaded.last_included_at.as_deref(),
            Some("2026-03-20T09:00:00Z")
        );
        assert_eq!(loaded.effectiveness.injected_count, 3);
        assert_eq!(loaded.skill_version, 2);
    }
}
