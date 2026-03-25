use std::fmt;

/// Validated skill name. Non-empty, no `/`, no `..`, max 64 chars.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillName(String);

impl SkillName {
    pub fn new(name: impl Into<String>) -> Result<Self, String> {
        let s = name.into();
        if s.is_empty() {
            return Err("SkillName cannot be empty".into());
        }
        if s.len() > 64 {
            return Err(format!("SkillName exceeds 64 chars: {}", s.len()));
        }
        if s.contains('/') {
            return Err(format!("SkillName cannot contain '/': {s}"));
        }
        if s.contains("..") {
            return Err(format!("SkillName cannot contain '..': {s}"));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SkillName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for SkillName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names() {
        assert!(SkillName::new("auto-commit").is_ok());
        assert!(SkillName::new("my_skill_v2").is_ok());
        assert!(SkillName::new("a").is_ok());
    }

    #[test]
    fn empty_name_rejected() {
        let err = SkillName::new("").unwrap_err();
        assert!(err.contains("empty"), "error: {err}");
    }

    #[test]
    fn too_long_rejected() {
        let err = SkillName::new("a".repeat(65)).unwrap_err();
        assert!(err.contains("64"), "error: {err}");
    }

    #[test]
    fn slash_rejected() {
        let err = SkillName::new("foo/bar").unwrap_err();
        assert!(err.contains("/"), "error: {err}");
    }

    #[test]
    fn path_traversal_rejected() {
        let err = SkillName::new("..escape").unwrap_err();
        assert!(err.contains(".."), "error: {err}");
    }

    #[test]
    fn display_shows_inner() {
        let name = SkillName::new("test-skill").expect("valid name");
        assert_eq!(format!("{name}"), "test-skill");
    }
}
