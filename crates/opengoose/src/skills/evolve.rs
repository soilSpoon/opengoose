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
// SweepDecision — batch re-evaluation decisions
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum SweepDecision {
    Restore(String),
    Refine(String, String), // (name, new SKILL.md content)
    Keep(String),
    Delete(String),
}

pub fn parse_sweep_response(response: &str) -> Vec<SweepDecision> {
    let mut decisions = Vec::new();
    let lines: Vec<&str> = response.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if let Some(name) = line.strip_prefix("RESTORE:") {
            decisions.push(SweepDecision::Restore(name.trim().into()));
        } else if let Some(name) = line.strip_prefix("DELETE:") {
            decisions.push(SweepDecision::Delete(name.trim().into()));
        } else if let Some(name) = line.strip_prefix("KEEP:") {
            decisions.push(SweepDecision::Keep(name.trim().into()));
        } else if let Some(name) = line.strip_prefix("REFINE:") {
            // Collect remaining lines as SKILL.md content until next decision line
            let name = name.trim().to_string();
            let mut content = String::new();
            i += 1;
            while i < lines.len()
                && !lines[i].starts_with("RESTORE:")
                && !lines[i].starts_with("DELETE:")
                && !lines[i].starts_with("KEEP:")
                && !lines[i].starts_with("REFINE:")
            {
                content.push_str(lines[i]);
                content.push('\n');
                i += 1;
            }
            decisions.push(SweepDecision::Refine(name, content.trim().into()));
            continue; // don't increment i again
        }
        i += 1;
    }

    decisions
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

pub fn summarize_for_prompt(content: &str, max_chars: usize) -> String {
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
    let home = crate::home_dir();
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

/// Build a prompt for batch re-evaluation of dormant/archived skills.
/// The LLM decides for each: RESTORE, REFINE, KEEP, or DELETE.
pub fn build_sweep_prompt(
    dormant_skills: &[(String, String, String, Option<String>)], // (name, desc, body, effectiveness)
    recent_failures: &[String],                                  // formatted failure summaries
) -> String {
    let mut prompt = String::from(
        "You are reviewing dormant skills against recent failures.\n\
         For each skill, decide:\n\
         - RESTORE:{name} — if a recent failure could have been prevented by this skill\n\
         - REFINE:{name} — if the skill is relevant but needs updating (output updated SKILL.md after)\n\
         - KEEP:{name} — leave dormant, might be useful later\n\
         - DELETE:{name} — skill is too generic or obsolete, safe to remove\n\n"
    );

    prompt.push_str("## Dormant/Archived Skills\n\n");
    for (name, desc, body, effectiveness) in dormant_skills {
        prompt.push_str(&format!("### {name}\n{desc}\n{body}\n"));
        if let Some(eff) = effectiveness {
            prompt.push_str(&format!("**Effectiveness:** {eff}\n"));
        }
        prompt.push('\n');
    }

    prompt.push_str("## Recent Failures (last 30 days)\n\n");
    for failure in recent_failures {
        prompt.push_str(&format!("- {failure}\n"));
    }

    prompt.push_str(
        "\nOutput one decision per skill, one per line.\n\
         For REFINE, output the line then the full updated SKILL.md on the next lines.\n",
    );

    prompt
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
    let skill_name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

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
// build_active_versions_json — map skill name → version for stamp versioning
// ---------------------------------------------------------------------------

/// Build a JSON map of active skill name → version from loaded skills.
pub fn build_active_versions_json(skills: &[crate::skills::load::LoadedSkill]) -> String {
    let mut map = std::collections::HashMap::new();
    for skill in skills {
        if skill.scope == crate::skills::load::SkillScope::Learned {
            if let Some(meta) = crate::skills::load::read_metadata(&skill.path) {
                map.insert(skill.name.clone(), meta.skill_version);
            }
        }
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
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

        // Version matches → score should be added
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

        // Old version → score should be ignored
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

        // No version info (legacy) → score should always count
        update_effectiveness_versioned(&skill_dir, 0.7, None).unwrap();

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.effectiveness.subsequent_scores, vec![0.7]);
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

    #[test]
    fn build_active_versions_json_works() {
        use crate::skills::load::{LoadedSkill, SkillScope};

        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-skill\ndescription: Use when test\n---\n",
        )
        .unwrap();

        let meta = SkillMetadata {
            generated_from: GeneratedFrom {
                stamp_id: 1,
                work_item_id: 1,
                dimension: "Q".into(),
                score: 0.2,
            },
            generated_at: Utc::now().to_rfc3339(),
            evolver_work_item_id: None,
            last_included_at: None,
            effectiveness: Effectiveness {
                injected_count: 0,
                subsequent_scores: vec![],
            },
            skill_version: 3,
        };
        std::fs::write(
            skill_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();

        let skills = vec![LoadedSkill {
            name: "test-skill".into(),
            description: "Use when test".into(),
            path: skill_dir,
            content: String::new(),
            scope: SkillScope::Learned,
        }];

        let json = build_active_versions_json(&skills);
        let parsed: std::collections::HashMap<String, u32> =
            serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.get("test-skill"), Some(&3));
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
                stamp_id: 99, work_item_id: 50,
                dimension: "Quality".into(), score: 0.15,
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
        ).unwrap();

        let new_content = "---\nname: my-skill\ndescription: Use when refined\n---\nNew body\n";
        refine_skill(&skill_dir, new_content).unwrap();

        let written = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(written.contains("New body"));

        let updated: SkillMetadata = serde_json::from_str(
            &std::fs::read_to_string(skill_dir.join("metadata.json")).unwrap()
        ).unwrap();
        // generated_from preserved
        assert_eq!(updated.generated_from.stamp_id, 99);
        assert_eq!(updated.evolver_work_item_id, Some(200));
        // version bumped, effectiveness reset
        assert_eq!(updated.skill_version, 4);
        assert!(updated.effectiveness.subsequent_scores.is_empty());
        assert!(updated.last_included_at.is_none());
    }

    #[test]
    fn build_sweep_prompt_includes_skills_and_failures() {
        let dormant_skills = vec![
            ("validate-paths".to_string(), "Use when reading files".to_string(), "Always check paths exist.".to_string(), None::<String>),
        ];
        let recent_failures = vec![
            "stamp #5: Quality 0.1 on 'File reader crashed on missing path'".to_string(),
            "stamp #8: Quality 0.2 on 'Path traversal vulnerability found'".to_string(),
        ];
        let prompt = build_sweep_prompt(&dormant_skills, &recent_failures);
        assert!(prompt.contains("validate-paths"));
        assert!(prompt.contains("File reader crashed"));
        assert!(prompt.contains("RESTORE"));
        assert!(prompt.contains("DELETE"));
    }

    #[test]
    fn sweep_prompt_includes_effectiveness() {
        let skills = vec![(
            "fix-auth".to_string(),
            "Use when auth fails".to_string(),
            "Check token expiry".to_string(),
            Some("8 injections, avg score 0.22, verdict: ineffective".to_string()),
        )];
        let failures = vec!["stamp #5: Quality 0.2 on 'missed validation'".to_string()];
        let prompt = build_sweep_prompt(&skills, &failures);
        assert!(prompt.contains("8 injections"));
        assert!(prompt.contains("ineffective"));
        assert!(prompt.contains("Effectiveness"));
    }

    #[test]
    fn parse_sweep_response_extracts_decisions() {
        let response = "RESTORE:validate-paths\nDELETE:old-generic-skill\nKEEP:maybe-useful\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0], SweepDecision::Restore("validate-paths".into()));
        assert_eq!(decisions[1], SweepDecision::Delete("old-generic-skill".into()));
        assert_eq!(decisions[2], SweepDecision::Keep("maybe-useful".into()));
    }

    #[test]
    fn parse_sweep_response_refine_with_content() {
        let response = "REFINE:my-skill\n---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 1);
        match &decisions[0] {
            SweepDecision::Refine(name, content) => {
                assert_eq!(name, "my-skill");
                assert!(content.contains("Use when updated"));
            }
            _ => panic!("expected Refine"),
        }
    }

    #[test]
    fn parse_sweep_response_mixed_with_refine() {
        let response = "RESTORE:skill-a\nREFINE:skill-b\n---\nname: skill-b\ndescription: Use when b\n---\nbody\nDELETE:skill-c\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 3);
        assert_eq!(decisions[0], SweepDecision::Restore("skill-a".into()));
        match &decisions[1] {
            SweepDecision::Refine(name, _) => assert_eq!(name, "skill-b"),
            _ => panic!("expected Refine"),
        }
        assert_eq!(decisions[2], SweepDecision::Delete("skill-c".into()));
    }

    #[test]
    fn parse_sweep_response_empty() {
        let decisions = parse_sweep_response("");
        assert!(decisions.is_empty());
    }
}
