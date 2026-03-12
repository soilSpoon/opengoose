mod recipe;
#[cfg(test)]
mod tests;
mod types;
mod validation;

use crate::error::{TeamError, TeamResult};

pub use types::{
    CommunicationMode, FanOutConfig, MergeStrategy, OrchestrationPattern, RouterConfig,
    RouterStrategy, TeamAgent, TeamDefinition,
};

impl TeamDefinition {
    /// Team name (the title).
    pub fn name(&self) -> &str {
        &self.title
    }

    /// File-safe name: lowercase, spaces replaced with hyphens.
    pub fn file_name(&self) -> String {
        format!("{}.yaml", self.title.to_lowercase().replace(' ', "-"))
    }

    /// Parse from YAML string.
    pub fn from_yaml(yaml: &str) -> TeamResult<Self> {
        let team: Self = serde_yaml::from_str(yaml)?;
        team.validate()?;
        Ok(team)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> TeamResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}

impl opengoose_types::YamlDefinition for TeamDefinition {
    type Error = TeamError;

    fn title(&self) -> &str {
        &self.title
    }

    fn from_yaml(yaml: &str) -> TeamResult<Self> {
        TeamDefinition::from_yaml(yaml)
    }

    fn to_yaml(&self) -> TeamResult<String> {
        TeamDefinition::to_yaml(self)
    }
}
