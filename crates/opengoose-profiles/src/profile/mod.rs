mod types;

#[cfg(test)]
mod tests;

pub use types::{ExtensionRef, ParameterRef, ProfileSettings, ProviderFallback, SubRecipeRef};

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::error::{ProfileError, ProfileResult};

/// The current profile schema version written by this crate.
pub const CURRENT_VERSION: &str = "1.0.0";

/// Known extension types (aligned with Goose Recipe extension types).
const VALID_EXT_TYPES: &[&str] = &[
    "builtin",
    "stdio",
    "streamable_http",
    "platform",
    "inline_python",
];

/// Valid parameter requirement values.
const VALID_REQUIREMENTS: &[&str] = &["required", "optional"];

/// Valid parameter input type values.
const VALID_INPUT_TYPES: &[&str] = &["string", "number", "boolean", "array", "object"];

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

    /// Parse from YAML string, applying version migrations then validating.
    pub fn from_yaml(yaml: &str) -> ProfileResult<Self> {
        let raw: serde_yaml::Value = serde_yaml::from_str(yaml)?;
        let migrated = Self::migrate_value(raw);
        let profile: Self = serde_yaml::from_value(migrated)?;
        profile.validate()?;
        Ok(profile)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> ProfileResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Validate required fields and setting consistency.
    pub fn validate(&self) -> ProfileResult<()> {
        fn err(msg: impl Into<String>) -> ProfileError {
            opengoose_types::YamlStoreError::ValidationFailed(msg.into()).into()
        }

        if self.title.trim().is_empty() {
            return Err(err("title is required"));
        }

        if self.version.trim().is_empty() {
            return Err(err("version is required"));
        }

        // Settings validation
        if let Some(settings) = &self.settings {
            if let Some(temp) = settings.temperature {
                if !(0.0..=2.0).contains(&temp) {
                    return Err(err(format!(
                        "temperature must be between 0.0 and 2.0, got {temp}"
                    )));
                }
            }

            if let Some(model) = &settings.goose_model {
                if model.trim().is_empty() {
                    return Err(err("goose_model must not be empty"));
                }
            }

            for fb in &settings.provider_fallbacks {
                if fb.goose_provider.trim().is_empty() {
                    return Err(err(
                        "provider_fallbacks entry must have a non-empty goose_provider",
                    ));
                }
                if let Some(model) = &fb.goose_model {
                    if model.trim().is_empty() {
                        return Err(err("provider_fallbacks goose_model must not be empty"));
                    }
                }
            }
        }

        // Extension validation
        let mut ext_names: HashSet<&str> = HashSet::new();
        for ext in &self.extensions {
            if ext.name.trim().is_empty() {
                return Err(err("extension name must not be empty"));
            }
            if !ext_names.insert(ext.name.as_str()) {
                return Err(err(format!("duplicate extension name: {}", ext.name)));
            }
            if !VALID_EXT_TYPES.contains(&ext.ext_type.as_str()) {
                return Err(err(format!(
                    "unknown extension type '{}'; valid types are: {}",
                    ext.ext_type,
                    VALID_EXT_TYPES.join(", ")
                )));
            }
            match ext.ext_type.as_str() {
                "stdio" if ext.cmd.is_none() => {
                    return Err(err(format!(
                        "extension '{}' of type 'stdio' requires a 'cmd' field",
                        ext.name
                    )));
                }
                "streamable_http" if ext.uri.is_none() => {
                    return Err(err(format!(
                        "extension '{}' of type 'streamable_http' requires a 'uri' field",
                        ext.name
                    )));
                }
                "inline_python" if ext.code.is_none() => {
                    return Err(err(format!(
                        "extension '{}' of type 'inline_python' requires a 'code' field",
                        ext.name
                    )));
                }
                _ => {}
            }
        }

        // Parameter validation
        if let Some(params) = &self.parameters {
            let mut param_keys: HashSet<&str> = HashSet::new();
            for param in params {
                if param.key.trim().is_empty() {
                    return Err(err("parameter key must not be empty"));
                }
                if !param_keys.insert(param.key.as_str()) {
                    return Err(err(format!("duplicate parameter key: {}", param.key)));
                }
                if !VALID_REQUIREMENTS.contains(&param.requirement.as_str()) {
                    return Err(err(format!(
                        "parameter '{}' has invalid requirement '{}'; valid values: {}",
                        param.key,
                        param.requirement,
                        VALID_REQUIREMENTS.join(", ")
                    )));
                }
                if !VALID_INPUT_TYPES.contains(&param.input_type.as_str()) {
                    return Err(err(format!(
                        "parameter '{}' has invalid input_type '{}'; valid types: {}",
                        param.key,
                        param.input_type,
                        VALID_INPUT_TYPES.join(", ")
                    )));
                }
            }
        }

        Ok(())
    }

    /// Apply version-based migrations to a raw YAML value before parsing.
    ///
    /// This is the extension point for future schema changes. Each migration
    /// should detect the source version and transform the raw YAML to match
    /// the current schema before deserialization.
    ///
    /// Supported migrations:
    /// - **pre-1.0.0 / missing version**: inserts `version: "1.0.0"`.
    fn migrate_value(mut value: serde_yaml::Value) -> serde_yaml::Value {
        let has_version = value.get("version").is_some();

        if !has_version {
            // Pre-1.0.0: version field was absent — backfill it.
            if let Some(map) = value.as_mapping_mut() {
                map.insert(
                    serde_yaml::Value::String("version".to_string()),
                    serde_yaml::Value::String(CURRENT_VERSION.to_string()),
                );
            }
        }

        // Future migrations:
        // match version.as_str() {
        //     v if v < "2.0.0" => { /* transform for v2 */ }
        //     _ => {}
        // }

        value
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
