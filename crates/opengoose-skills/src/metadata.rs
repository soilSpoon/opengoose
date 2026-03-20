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
// metadata.json I/O
// ---------------------------------------------------------------------------

/// Read metadata.json from a skill directory. Returns None if missing or invalid.
pub fn read_metadata(skill_dir: &Path) -> Option<SkillMetadata> {
    let meta_path = skill_dir.join("metadata.json");
    let content = std::fs::read_to_string(meta_path).ok()?;
    serde_json::from_str(&content).ok()
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

    #[test]
    fn parse_frontmatter_valid() {
        let content = "---\nname: my-skill\ndescription: Use when testing\n---\n# Body\n";
        let fm = parse_frontmatter(content).unwrap();
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
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.generated_from.stamp_id, 5);
        assert_eq!(parsed.evolver_work_item_id, Some(100));
        assert_eq!(parsed.effectiveness.subsequent_scores, vec![0.5, 0.6]);
        assert_eq!(parsed.skill_version, 1);
    }

    #[test]
    fn read_write_metadata_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

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

        write_metadata(&skill_dir, &meta).unwrap();
        let loaded = read_metadata(&skill_dir).unwrap();

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
