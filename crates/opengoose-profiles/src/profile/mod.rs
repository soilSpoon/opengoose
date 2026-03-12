mod types;

#[cfg(test)]
mod tests;

pub use types::{ExtensionRef, ParameterRef, ProfileSettings, ProviderFallback, SubRecipeRef};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::error::{ProfileError, ProfileResult};

/// An agent profile — a YAML-serializable struct compatible with Goose's Recipe schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub version: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionRef>,
    /// Skill names to load extensions from (`~/.opengoose/skills/<name>.yaml`).
    ///
    /// Extensions contributed by skills are appended after the profile's own
    /// `extensions` list. Duplicates (by extension name) are skipped — the
    /// profile's own extensions always take precedence, and earlier skill
    /// entries win over later ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProfileSettings>,
    /// Activity pills displayed when loading the profile (maps to Recipe `activities`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activities: Option<Vec<String>>,
    /// JSON schema for structured output (maps to Recipe `response.json_schema`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<JsonValue>,
    /// Sub-recipes this profile can delegate to via Goose's Summon extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_recipes: Option<Vec<SubRecipeRef>>,
    /// Input parameters for parameterized execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<ParameterRef>>,
}

impl AgentProfile {
    /// Profile name (the title, lowercased).
    pub fn name(&self) -> &str {
        &self.title
    }

    /// Resolve the effective extension list for this profile.
    ///
    /// Merges the profile's own `extensions` with those contributed by all
    /// referenced `skills`. Deduplication is by extension name; the profile's
    /// own extensions always win, and skill extensions are appended in order.
    ///
    /// Returns a `ProfileError` if any referenced skill cannot be loaded.
    pub fn resolve_extensions(
        &self,
        skill_store: &crate::skill_store::SkillStore,
    ) -> crate::error::ProfileResult<Vec<ExtensionRef>> {
        if self.skills.is_empty() {
            return Ok(self.extensions.clone());
        }

        let mut seen: std::collections::HashSet<String> =
            self.extensions.iter().map(|e| e.name.clone()).collect();
        let mut result = self.extensions.clone();

        let skill_exts = skill_store.resolve_extensions(&self.skills)?;
        for ext in skill_exts {
            if seen.insert(ext.name.clone()) {
                result.push(ext);
            }
        }
        Ok(result)
    }

    /// File-safe name: lowercase, spaces replaced with hyphens.
    pub fn file_name(&self) -> String {
        format!("{}.yaml", self.title.to_lowercase().replace(' ', "-"))
    }

    /// Clone the profile, overriding the configured Goose model when provided.
    pub fn with_model_override(&self, goose_model: Option<&str>) -> Self {
        let Some(goose_model) = goose_model else {
            return self.clone();
        };

        let mut profile = self.clone();
        let settings = profile
            .settings
            .get_or_insert_with(ProfileSettings::default);
        settings.goose_model = Some(goose_model.to_string());
        profile
    }

    /// Parse from YAML string.
    pub fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        let profile: Self = serde_yaml::from_str(yaml)?;
        profile.validate()?;
        Ok(profile)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> ProfileResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Validate required fields.
    pub fn validate(&self) -> ProfileResult<()> {
        if self.title.trim().is_empty() {
            return Err(opengoose_types::YamlStoreError::ValidationFailed(
                "title is required".into(),
            )
            .into());
        }
        Ok(())
    }
}

impl opengoose_types::YamlDefinition for AgentProfile {
    type Error = ProfileError;

    fn title(&self) -> &str {
        &self.title
    }

    fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        AgentProfile::from_yaml(yaml)
    }

    fn to_yaml(&self) -> ProfileResult<String> {
        AgentProfile::to_yaml(self)
    }
}
