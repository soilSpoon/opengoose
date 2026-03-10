use crate::profile::AgentProfile;

/// All bundled default profiles.
pub fn all_defaults() -> Vec<AgentProfile> {
    let yamls: &[&str] = &[
        include_str!("../profiles/main.yaml"),
        include_str!("../profiles/architect.yaml"),
        include_str!("../profiles/researcher.yaml"),
        include_str!("../profiles/developer.yaml"),
        include_str!("../profiles/bug-triager.yaml"),
        include_str!("../profiles/security-auditor.yaml"),
        include_str!("../profiles/reviewer.yaml"),
        include_str!("../profiles/tester.yaml"),
        include_str!("../profiles/writer.yaml"),
    ];

    yamls
        .iter()
        .map(|y| AgentProfile::from_yaml(y).expect("bundled profile YAML is valid"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_profiles_parse() {
        let defaults = all_defaults();
        assert_eq!(defaults.len(), 9);
        let names: Vec<&str> = defaults.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"architect"));
        assert!(names.contains(&"main"));
        assert!(names.contains(&"bug-triager"));
        assert!(names.contains(&"researcher"));
        assert!(names.contains(&"developer"));
        assert!(names.contains(&"security-auditor"));
        assert!(names.contains(&"reviewer"));
        assert!(names.contains(&"tester"));
        assert!(names.contains(&"writer"));
    }
}
