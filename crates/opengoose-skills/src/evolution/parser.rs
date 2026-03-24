// parser.rs — LLM response parsing for skill evolution

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
    fn parse_sweep_response_extracts_decisions() {
        let response = "RESTORE:validate-paths\nDELETE:old-generic-skill\nKEEP:maybe-useful\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 3);
        assert_eq!(
            decisions[0],
            SweepDecision::Restore("validate-paths".into())
        );
        assert_eq!(
            decisions[1],
            SweepDecision::Delete("old-generic-skill".into())
        );
        assert_eq!(decisions[2], SweepDecision::Keep("maybe-useful".into()));
    }

    #[test]
    fn parse_sweep_response_refine_with_content() {
        let response =
            "REFINE:my-skill\n---\nname: my-skill\ndescription: Use when updated\n---\nNew body\n";
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

    #[test]
    fn parse_evolve_response_whitespace_only_is_create() {
        // Whitespace-only input trims to empty string, treated as Create("")
        match parse_evolve_response("   \n  ") {
            EvolveAction::Create(c) => assert!(c.is_empty()),
            other => panic!("expected Create, got {other:?}"),
        }
    }

    #[test]
    fn parse_evolve_response_skip_with_whitespace() {
        assert_eq!(parse_evolve_response("  SKIP  "), EvolveAction::Skip);
    }

    #[test]
    fn parse_evolve_response_update_with_whitespace() {
        assert_eq!(
            parse_evolve_response("UPDATE: skill-name "),
            EvolveAction::Update("skill-name".into())
        );
    }

    #[test]
    fn parse_sweep_response_unrecognized_lines_ignored() {
        let response = "some garbage\nDELETE:real-skill\nmore garbage\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0], SweepDecision::Delete("real-skill".into()));
    }

    #[test]
    fn parse_sweep_response_refine_with_empty_content() {
        let response = "REFINE:my-skill\nDELETE:other\n";
        let decisions = parse_sweep_response(response);
        assert_eq!(decisions.len(), 2);
        match &decisions[0] {
            SweepDecision::Refine(name, content) => {
                assert_eq!(name, "my-skill");
                assert!(content.is_empty(), "expected empty content for refine with no body");
            }
            other => panic!("expected Refine, got {other:?}"),
        }
    }
}
