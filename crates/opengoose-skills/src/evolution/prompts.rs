// prompts.rs — LLM prompt construction for skill evolution

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

pub struct UpdatePromptParams<'a> {
    pub skill_name: &'a str,
    pub existing_content: &'a str,
    pub dimension: &'a str,
    pub score: f32,
    pub comment: Option<&'a str>,
    pub work_item_title: &'a str,
    pub work_item_id: i64,
    pub log_summary: &'a str,
}

pub fn build_update_prompt(params: &UpdatePromptParams<'_>) -> String {
    let UpdatePromptParams {
        skill_name,
        existing_content,
        dimension,
        score,
        comment,
        work_item_title,
        work_item_id,
        log_summary,
    } = params;
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
// build_sweep_prompt — batch re-evaluation of dormant/archived skills
// ---------------------------------------------------------------------------

pub fn build_sweep_prompt(
    dormant_skills: &[(String, String, String, Option<String>)],
    recent_failures: &[String],
) -> String {
    let mut prompt = String::from(
        "You are reviewing dormant skills against recent failures.\n\
         For each skill, decide:\n\
         - RESTORE:{name} — if a recent failure could have been prevented by this skill\n\
         - REFINE:{name} — if the skill is relevant but needs updating (output updated SKILL.md after)\n\
         - KEEP:{name} — leave dormant, might be useful later\n\
         - DELETE:{name} — skill is too generic or obsolete, safe to remove\n\n",
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

// ---------------------------------------------------------------------------
// summarize_for_prompt — truncate content to max_chars (most recent)
// ---------------------------------------------------------------------------

pub fn summarize_for_prompt(content: &str, max_chars: usize) -> String {
    if content.len() <= max_chars {
        return content.to_string();
    }
    // Take the last max_chars characters (most recent context)
    content[content.len() - max_chars..].to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_basic() {
        let prompt = build_evolve_prompt("Quality", 0.2, Some("no tests"), "Fix auth", 42, "", &[]);
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
        let prompt = build_update_prompt(&UpdatePromptParams {
            skill_name: "validate-paths",
            existing_content,
            dimension: "Quality",
            score: 0.1,
            comment: Some("path traversal"),
            work_item_title: "Fix file reader",
            work_item_id: 55,
            log_summary: "log excerpt...",
        });
        assert!(prompt.contains("validate-paths"));
        assert!(prompt.contains("Always check paths"));
        assert!(prompt.contains("path traversal"));
        assert!(prompt.contains("UPDATE this skill"));
    }

    #[test]
    fn summarize_truncates_long_content() {
        let content = "a".repeat(10000);
        let summary = summarize_for_prompt(&content, 4000);
        assert_eq!(summary.len(), 4000);
    }

    #[test]
    fn build_sweep_prompt_includes_skills_and_failures() {
        let dormant_skills = vec![(
            "validate-paths".to_string(),
            "Use when reading files".to_string(),
            "Always check paths exist.".to_string(),
            None::<String>,
        )];
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
}
