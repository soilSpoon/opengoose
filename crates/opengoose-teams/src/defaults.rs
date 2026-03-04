use crate::team::TeamDefinition;

/// All bundled default team definitions.
pub fn all_defaults() -> Vec<TeamDefinition> {
    let yamls: &[&str] = &[
        include_str!("../teams/code-review.yaml"),
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
        assert_eq!(defaults.len(), 3);
        let names: Vec<&str> = defaults.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"code-review"));
        assert!(names.contains(&"research-panel"));
        assert!(names.contains(&"smart-router"));
    }
}
