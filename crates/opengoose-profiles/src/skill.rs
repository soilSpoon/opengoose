use serde::{Deserialize, Serialize};

use crate::error::{ProfileError, ProfileResult};
use crate::profile::ExtensionRef;

/// A skill package — a named bundle of extensions that can be referenced by profiles.
///
/// Skills are stored as YAML files under `~/.opengoose/skills/` and referenced
/// in profiles via the `skills` field. When a profile is loaded, each skill
/// name is resolved to its `ExtensionRef` list and merged into the profile's
/// effective extension set (duplicates are deduplicated by extension name).
///
/// # Example YAML
/// ```yaml
/// name: git-tools
/// description: "Git-related tools for repository inspection"
/// version: "1.0.0"
/// extensions:
///   - name: git-log
///     type: stdio
///     cmd: git
///     args: ["log", "--oneline", "-20"]
///   - name: git-diff
///     type: stdio
///     cmd: git
///     args: ["diff", "--stat"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionRef>,
}

impl Skill {
    /// Parse a skill from a YAML string.
    pub fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        let skill: Self = serde_yaml::from_str(yaml)?;
        skill.validate()?;
        Ok(skill)
    }

    /// Serialize to a YAML string.
    pub fn to_yaml(&self) -> ProfileResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Validate required fields.
    pub fn validate(&self) -> ProfileResult<()> {
        if self.name.trim().is_empty() {
            return Err(ProfileError::ValidationFailed(
                "skill name is required".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(ProfileError::ValidationFailed(
                "skill version is required".into(),
            ));
        }
        Ok(())
    }
}

impl opengoose_types::YamlDefinition for Skill {
    type Error = ProfileError;

    fn title(&self) -> &str {
        &self.name
    }

    fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        Skill::from_yaml(yaml)
    }

    fn to_yaml(&self) -> ProfileResult<String> {
        Skill::to_yaml(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_yaml() {
        let yaml = r#"
name: git-tools
description: "Git tools"
version: "1.0.0"
extensions:
  - name: git-log
    type: stdio
    cmd: git
    args:
      - log
      - "--oneline"
      - "-20"
"#;
        let skill = Skill::from_yaml(yaml).unwrap();
        assert_eq!(skill.name, "git-tools");
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(skill.extensions.len(), 1);
        assert_eq!(skill.extensions[0].name, "git-log");

        let serialized = skill.to_yaml().unwrap();
        let reparsed = Skill::from_yaml(&serialized).unwrap();
        assert_eq!(reparsed.name, skill.name);
        assert_eq!(reparsed.extensions.len(), 1);
    }

    #[test]
    fn validation_rejects_empty_name() {
        let yaml = r#"name: "  "
version: "1.0.0""#;
        let err = Skill::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("skill name is required"));
    }

    #[test]
    fn validation_rejects_empty_version() {
        let yaml = r#"name: my-skill
version: "  ""#;
        let err = Skill::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("skill version is required"));
    }

    #[test]
    fn skill_without_extensions_is_valid() {
        let yaml = r#"name: empty-skill
version: "1.0.0""#;
        let skill = Skill::from_yaml(yaml).unwrap();
        assert!(skill.extensions.is_empty());
    }
}
