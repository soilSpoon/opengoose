use crate::team::TeamDefinition;

/// All bundled default team definitions.
pub fn all_defaults() -> Vec<TeamDefinition> {
    let yamls: &[&str] = &[
        include_str!("../teams/feature-dev.yaml"),
        include_str!("../teams/code-review.yaml"),
        include_str!("../teams/security-audit.yaml"),
        include_str!("../teams/bug-triage.yaml"),
        include_str!("../teams/full-review.yaml"),
        include_str!("../teams/research-panel.yaml"),
        include_str!("../teams/smart-router.yaml"),
    ];

    yamls
        .iter()
        .map(|y| TeamDefinition::from_yaml(y).expect("bundled team YAML is valid"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_teams_parse() {
        let defaults = all_defaults();
        assert_eq!(defaults.len(), 7);
        let names: Vec<&str> = defaults.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"feature-dev"));
        assert!(names.contains(&"security-audit"));
        assert!(names.contains(&"bug-triage"));
        assert!(names.contains(&"full-review"));
        assert!(names.contains(&"research-panel"));
        assert!(names.contains(&"smart-router"));
    }
}
