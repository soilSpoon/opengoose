use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, ProjectResult};

use super::path::expand_tilde;

/// Per-project agent settings overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSettings {
    /// Override max turns for all agents in this project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Override message retention days for the project's sessions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_retention_days: Option<u32>,
}

impl ProjectSettings {
    pub fn is_empty(&self) -> bool {
        self.max_turns.is_none() && self.message_retention_days.is_none()
    }
}

/// A project definition — a YAML-serializable config that groups agents and
/// teams around a shared goal and working directory.
///
/// Projects provide:
/// - A **goal** that is injected into each agent's system prompt so every
///   agent understands the "why" behind every task.
/// - A per-project **`cwd`** so file operations by agents are isolated to the
///   project's directory rather than the process working directory.
/// - **Context files** whose contents are prepended to the agent system prompt
///   for project-specific background knowledge.
/// - A **default team** that `opengoose project run` uses when `--team` is
///   not specified.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDefinition {
    pub version: String,
    pub title: String,
    /// High-level goal injected into every agent's system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// Working directory for agents in this project.
    ///
    /// Supports `~` expansion. Defaults to the project file's parent directory
    /// when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Paths to context files injected into agent system prompts.
    ///
    /// Each entry is appended as a named prompt extension (key = file stem).
    /// Relative paths are resolved relative to `cwd` (or the project's store
    /// directory if `cwd` is not set).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_files: Vec<String>,
    /// Name of the default team to run with `opengoose project run`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_team: Option<String>,
    /// Description shown in `project list` / `project show`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Per-project settings overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectSettings>,
}

impl ProjectDefinition {
    /// Project name (the title).
    pub fn name(&self) -> &str {
        &self.title
    }

    /// File-safe name: lowercase, spaces replaced with hyphens.
    pub fn file_name(&self) -> String {
        format!("{}.yaml", self.title.to_lowercase().replace(' ', "-"))
    }

    /// Parse from YAML string.
    pub fn from_yaml(yaml: &str) -> ProjectResult<Self> {
        let project: Self = serde_yaml::from_str(yaml)?;
        project.validate()?;
        Ok(project)
    }

    /// Serialize to YAML string.
    pub fn to_yaml(&self) -> ProjectResult<String> {
        Ok(serde_yaml::to_string(self)?)
    }

    /// Validate required fields.
    pub fn validate(&self) -> ProjectResult<()> {
        if self.title.trim().is_empty() {
            return Err(opengoose_types::YamlStoreError::ValidationFailed(
                "title is required".into(),
            )
            .into());
        }
        Ok(())
    }

    /// Resolve the effective working directory for this project.
    ///
    /// Resolution order:
    /// 1. `cwd` field (with `~` expansion)
    /// 2. `store_dir` (the directory the project YAML lives in)
    /// 3. Current process working directory
    pub fn resolve_cwd(&self, store_dir: Option<&Path>) -> PathBuf {
        if let Some(cwd) = &self.cwd {
            return expand_tilde(cwd);
        }
        if let Some(dir) = store_dir {
            return dir.to_path_buf();
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    }
}

impl opengoose_types::YamlDefinition for ProjectDefinition {
    type Error = ProjectError;

    fn title(&self) -> &str {
        &self.title
    }

    fn from_yaml(yaml: &str) -> ProjectResult<Self> {
        ProjectDefinition::from_yaml(yaml)
    }

    fn to_yaml(&self) -> ProjectResult<String> {
        ProjectDefinition::to_yaml(self)
    }
}
