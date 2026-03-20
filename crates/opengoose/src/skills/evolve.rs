// Skill Evolution — LLM-based skill generation
//
// Building blocks for the Evolver loop (Task 7):
// 1. parse_evolve_response() — parse LLM output (SKIP / UPDATE / Create)
// 2. validate_skill_output() — validate SKILL.md format
// 3. build_evolve_prompt() — construct the LLM prompt from stamp + work item context
// 4. read_conversation_log() — get conversation context for the prompt
// 5. write_skill_to_rig_scope() — write SKILL.md + metadata.json to rig's learned dir

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// EvolveAction — LLM response categories
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum EvolveAction {
    Create(String), // valid SKILL.md content
    Update(String), // existing skill name to update
    Skip,           // lesson is too generic
}

pub fn parse_evolve_response(response: &str) -> EvolveAction {
    let trimmed = response.trim();
    if trimmed == "SKIP" {
        return EvolveAction::Skip;
    }
    if let Some(name) = trimmed.strip_prefix("UPDATE:") {
        return EvolveAction::Update(name.trim().to_string());
    }
    EvolveAction::Create(trimmed.to_string())
}

// ---------------------------------------------------------------------------
// validate_skill_output — SKILL.md format validation
// ---------------------------------------------------------------------------

pub fn validate_skill_output(content: &str) -> anyhow::Result<()> {
    let content = content.trim();
    if !content.starts_with("---") {
        anyhow::bail!("missing YAML frontmatter (must start with ---)");
    }
    let rest = &content[3..];
    let end = rest
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("unclosed frontmatter"))?;
    let frontmatter = &rest[..end];

    // Check name
    let name = frontmatter
        .lines()
        .find_map(|l| l.strip_prefix("name:").map(|v| v.trim().trim_matches('"').to_string()))
        .ok_or_else(|| anyhow::anyhow!("missing name field"))?;
    if name.is_empty() || name.len() > 64 {
        anyhow::bail!("name must be 1-64 chars, got {}", name.len());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        anyhow::bail!("name must be lowercase + hyphens only: {name}");
    }

    // Check description
    let desc = frontmatter
        .lines()
        .find_map(|l| {
            l.strip_prefix("description:")
                .map(|v| v.trim().trim_matches('"').to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("missing description field"))?;
    if !desc.starts_with("Use when") {
        anyhow::bail!("description must start with 'Use when', got: {desc}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// build_evolve_prompt — construct the LLM prompt
// ---------------------------------------------------------------------------

pub fn build_evolve_prompt(
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    work_item_title: &str,
    work_item_id: i64,
    log_summary: &str,
    existing_skills: &[(String, String)], // (name, description)
) -> String {
    let mut prompt = format!(
        "Analyze this failed task and create a SKILL.md.\n\n\
         ## Stamp\n\
         dimension: {dimension}, score: {score:.1}, comment: '{}'\n\n\
         ## Work Item\n\
         #{work_item_id}: '{work_item_title}'\n\n",
        comment.unwrap_or("(none)"),
    );

    if !log_summary.is_empty() {
        prompt.push_str(&format!("## Conversation Log\n{log_summary}\n\n"));
    }

    if !existing_skills.is_empty() {
        prompt.push_str("## Existing Skills (check for duplicates)\n");
        for (name, desc) in existing_skills {
            prompt.push_str(&format!("- {name}: {desc}\n"));
        }
        prompt.push('\n');
    }

    prompt.push_str(
        "Generate a SKILL.md with YAML frontmatter (name, description) and markdown body.\n\
         Or output SKIP if the lesson is too generic.\n\
         Or output UPDATE:{name} if an existing skill should be updated instead.",
    );

    prompt
}

// ---------------------------------------------------------------------------
// build_update_prompt — construct the LLM prompt for skill refinement
// ---------------------------------------------------------------------------

pub fn build_update_prompt(
    skill_name: &str,
    existing_content: &str,
    dimension: &str,
    score: f32,
    comment: Option<&str>,
    work_item_title: &str,
    work_item_id: i64,
    log_summary: &str,
) -> String {
    format!(
        "UPDATE this skill based on a new failure.\n\n\
         ## Existing Skill: {skill_name}\n\
         ```\n{existing_content}\n```\n\n\
         ## New Failure\n\
         dimension: {dimension}, score: {score:.1}, comment: '{}'\n\n\
         ## Work Item\n\
         #{work_item_id}: '{work_item_title}'\n\n\
         ## Conversation Log\n{log_summary}\n\n\
         Rewrite the SKILL.md. Keep the same `name:` field. \
         Incorporate the new failure pattern into the existing rules. \
         Output the complete updated SKILL.md with YAML frontmatter.",
        comment.unwrap_or("(none)"),
    )
}

// ---------------------------------------------------------------------------
// read_conversation_log — get conversation context for the prompt
// ---------------------------------------------------------------------------

pub fn read_conversation_log(work_item_id: i64) -> String {
    let session_id = format!("task-{work_item_id}");
    opengoose_rig::conversation_log::read_log(&session_id)
        .map(|content| summarize_for_prompt(&content, 4000))
        .unwrap_or_default()
}

fn summarize_for_prompt(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        return content.to_string();
    }
    // Take the last max_chars characters (most recent context)
    content[content.len() - max_chars..].to_string()
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
// write_skill_to_rig_scope — write SKILL.md + metadata.json
// ---------------------------------------------------------------------------

pub fn write_skill_to_rig_scope(
    rig_id: &str,
    skill_content: &str,
    stamp_id: i64,
    work_item_id: i64,
    dimension: &str,
    score: f32,
    evolver_work_item_id: Option<i64>,
) -> anyhow::Result<String> {
    // Parse name from frontmatter
    let name = extract_name_from_content(skill_content)
        .ok_or_else(|| anyhow::anyhow!("cannot extract name from skill content"))?;

    // Write to ~/.opengoose/rigs/{rig_id}/skills/learned/{name}/
    let home = dirs::home_dir().unwrap_or_else(|| ".".into());
    let skill_dir = home.join(format!(".opengoose/rigs/{rig_id}/skills/learned/{name}"));
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(skill_dir.join("SKILL.md"), skill_content)?;

    // Write metadata.json
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

fn extract_name_from_content(content: &str) -> Option<String> {
    let content = content.trim();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("\n---")?;
    rest[..end]
        .lines()
        .find_map(|l| l.strip_prefix("name:").map(|v| v.trim().trim_matches('"').to_string()))
}

// ---------------------------------------------------------------------------
// Effectiveness tracking
// ---------------------------------------------------------------------------

/// Update a skill's effectiveness with a new subsequent score.
/// Called when a stamp arrives for the same rig+dimension after skill was injected.
pub fn update_effectiveness(skill_dir: &Path, new_score: f32) -> anyhow::Result<()> {
    let meta_path = skill_dir.join("metadata.json");
    let content = std::fs::read_to_string(&meta_path)?;
    let mut meta: SkillMetadata = serde_json::from_str(&content)?;
    meta.effectiveness.subsequent_scores.push(new_score);
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_response_skip() {
        assert_eq!(parse_evolve_response("SKIP"), EvolveAction::Skip);
    }

    #[test]
    fn parse_response_update() {
        assert_eq!(
            parse_evolve_response("UPDATE:existing-skill"),
            EvolveAction::Update("existing-skill".into())
        );
    }

    #[test]
    fn parse_response_create() {
        let content = "---\nname: test\ndescription: Use when testing\n---\n# Body\n";
        match parse_evolve_response(content) {
            EvolveAction::Create(c) => assert!(c.contains("test")),
            _ => panic!("expected Create"),
        }
    }

    #[test]
    fn validate_valid_output() {
        let content = "---\nname: test-skill\ndescription: Use when testing code\n---\n# Body\n";
        assert!(validate_skill_output(content).is_ok());
    }

    #[test]
    fn validate_missing_frontmatter() {
        assert!(validate_skill_output("# No frontmatter").is_err());
    }

    #[test]
    fn validate_bad_description() {
        let content = "---\nname: test\ndescription: This does things\n---\n";
        assert!(validate_skill_output(content).is_err());
    }

    #[test]
    fn validate_bad_name() {
        let content = "---\nname: Test_Skill\ndescription: Use when testing\n---\n";
        assert!(validate_skill_output(content).is_err());
    }

    #[test]
    fn build_prompt_basic() {
        let prompt =
            build_evolve_prompt("Quality", 0.2, Some("no tests"), "Fix auth", 42, "", &[]);
        assert!(prompt.contains("Quality"));
        assert!(prompt.contains("0.2"));
        assert!(prompt.contains("no tests"));
        assert!(prompt.contains("Fix auth"));
    }

    #[test]
    fn build_prompt_with_existing_skills() {
        let existing = vec![("skill-a".into(), "desc-a".into())];
        let prompt = build_evolve_prompt("Quality", 0.2, None, "task", 1, "", &existing);
        assert!(prompt.contains("skill-a"));
        assert!(prompt.contains("Existing Skills"));
    }

    #[test]
    fn build_update_prompt_includes_existing_skill() {
        let existing_content = "---\nname: validate-paths\ndescription: Use when reading files\n---\nAlways check paths.\n";
        let prompt = build_update_prompt(
            "validate-paths",
            existing_content,
            "Quality", 0.1, Some("path traversal"), "Fix file reader", 55, "log excerpt...",
        );
        assert!(prompt.contains("validate-paths"));
        assert!(prompt.contains("Always check paths"));
        assert!(prompt.contains("path traversal"));
        assert!(prompt.contains("UPDATE this skill"));
    }

    #[test]
    fn write_skill_creates_files() {
        let content = "---\nname: my-skill\ndescription: Use when testing\n---\n# Body\n";
        let name = extract_name_from_content(content).unwrap();
        assert_eq!(name, "my-skill");
    }

    #[test]
    fn metadata_roundtrip() {
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
                subsequent_scores: vec![],
            },
            skill_version: 1,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let parsed: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.generated_from.stamp_id, 5);
        assert_eq!(parsed.evolver_work_item_id, Some(100));
    }

    #[test]
    fn summarize_truncates_long_content() {
        let content = "a".repeat(10000);
        let summary = summarize_for_prompt(&content, 4000);
        assert_eq!(summary.len(), 4000);
    }

    #[test]
    fn update_effectiveness_adds_score() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("test-skill");
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

        update_effectiveness(&skill_dir, 0.7).unwrap();

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.7]);
        assert_eq!(updated.effectiveness.injected_count, 0); // unchanged
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
        update_existing_skill(&skill_dir, new_content, 5, 42, "Quality", 0.15, Some(100))
            .unwrap();

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
