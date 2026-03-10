use crate::skill::Skill;

/// All bundled default skills.
pub fn all_default_skills() -> Vec<Skill> {
    let yamls: &[&str] = &[
        include_str!("../skills/git-tools.yaml"),
        include_str!("../skills/web-search.yaml"),
        include_str!("../skills/file-manager.yaml"),
    ];

    yamls
        .iter()
        .map(|y| Skill::from_yaml(y).expect("bundled skill YAML is valid"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_skills_parse() {
        let defaults = all_default_skills();
        assert_eq!(defaults.len(), 3);
        let names: Vec<&str> = defaults.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"git-tools"));
        assert!(names.contains(&"web-search"));
        assert!(names.contains(&"file-manager"));
    }

    #[test]
    fn git_tools_has_extensions() {
        let defaults = all_default_skills();
        let git = defaults.iter().find(|s| s.name == "git-tools").unwrap();
        assert!(!git.extensions.is_empty());
    }
}
