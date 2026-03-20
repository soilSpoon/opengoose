// validator.rs — SKILL.md format validation

use crate::metadata::parse_frontmatter;

/// Validate SKILL.md content format.
/// Checks: frontmatter present, name is 1-64 chars lowercase+hyphens,
/// description starts with "Use when".
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

    // Use shared parse_frontmatter to extract fields, but we need individual field
    // validation errors, so we do the field extraction inline here.
    let name = frontmatter
        .lines()
        .find_map(|l| {
            l.strip_prefix("name:")
                .map(|v| v.trim().trim_matches('"').to_string())
        })
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

    // Suppress unused import warning — parse_frontmatter is available for callers
    let _ = parse_frontmatter;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
}
