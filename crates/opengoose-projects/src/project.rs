use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ProjectError, ProjectResult};

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
            let expanded = expand_tilde(cwd);
            return expanded;
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

/// Expand a leading `~` to the home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

/// Runtime project context — fully resolved paths and loaded file contents.
///
/// Created from a `ProjectDefinition` by [`ProjectContext::from_definition`].
/// Passed through the orchestration stack to inject goal/context into every
/// agent session without re-reading files on each agent invocation.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// Project title.
    pub title: String,
    /// High-level goal for the project (empty string if not set).
    pub goal: String,
    /// Resolved working directory.
    pub cwd: PathBuf,
    /// Loaded context file contents: `(label, content)` pairs.
    ///
    /// Each entry is injected as a named system prompt extension.
    pub context_entries: Vec<(String, String)>,
    /// Name of the default team (if configured).
    pub default_team: Option<String>,
}

impl ProjectContext {
    /// Build a `ProjectContext` from a `ProjectDefinition`, loading context
    /// files from disk.
    ///
    /// Non-existent context files are silently skipped with a warning rather
    /// than failing — this prevents a missing README from blocking all agents.
    pub fn from_definition(def: &ProjectDefinition, store_dir: Option<&Path>) -> Self {
        let cwd = def.resolve_cwd(store_dir);
        let goal = def.goal.clone().unwrap_or_default();

        let mut context_entries = Vec::new();
        for file_path in &def.context_files {
            let resolved = if std::path::Path::new(file_path).is_absolute() {
                PathBuf::from(file_path)
            } else {
                cwd.join(file_path)
            };

            match std::fs::read_to_string(&resolved) {
                Ok(content) => {
                    let label = resolved
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| file_path.clone());
                    context_entries.push((label, content));
                }
                Err(e) => {
                    tracing::warn!(
                        path = %resolved.display(),
                        error = %e,
                        project = %def.title,
                        "skipping context file (not readable)"
                    );
                }
            }
        }

        Self {
            title: def.title.clone(),
            goal,
            cwd,
            context_entries,
            default_team: def.default_team.clone(),
        }
    }

    /// Build the system prompt extension text for this project.
    ///
    /// Returns a multi-line block that can be passed to
    /// `AgentRunner::extend_system_prompt("project_context", ...)`.
    pub fn system_prompt_extension(&self) -> String {
        let mut parts = Vec::new();

        parts.push(format!("## Project: {}", self.title));

        if !self.goal.is_empty() {
            parts.push(format!("\n**Goal:** {}", self.goal));
        }

        parts.push(format!("\n**Working directory:** {}", self.cwd.display()));

        if !self.context_entries.is_empty() {
            parts.push("\n**Project context:**".to_string());
            for (label, content) in &self.context_entries {
                let truncated = if content.len() > 4096 {
                    format!("{}... [truncated]", &content[..4096])
                } else {
                    content.clone()
                };
                parts.push(format!("\n### {label}\n{truncated}"));
            }
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_yaml() -> &'static str {
        r#"
version: "1.0.0"
title: "my-project"
"#
    }

    #[test]
    fn round_trip_minimal() {
        let project = ProjectDefinition::from_yaml(minimal_yaml()).unwrap();
        assert_eq!(project.name(), "my-project");
        assert!(project.goal.is_none());
        assert!(project.cwd.is_none());
        assert!(project.context_files.is_empty());
        assert!(project.default_team.is_none());

        let serialized = project.to_yaml().unwrap();
        let reparsed = ProjectDefinition::from_yaml(&serialized).unwrap();
        assert_eq!(reparsed.title, project.title);
    }

    #[test]
    fn round_trip_full() {
        let yaml = r#"
version: "1.0.0"
title: "opengoose"
goal: "Build the best multi-agent orchestrator"
cwd: "/workspace/opengoose"
context_files:
  - README.md
  - docs/architecture.md
default_team: code-review
description: "Main OpenGoose development project"
settings:
  max_turns: 20
  message_retention_days: 30
"#;
        let project = ProjectDefinition::from_yaml(yaml).unwrap();
        assert_eq!(
            project.goal.as_deref(),
            Some("Build the best multi-agent orchestrator")
        );
        assert_eq!(project.cwd.as_deref(), Some("/workspace/opengoose"));
        assert_eq!(
            project.context_files,
            vec!["README.md", "docs/architecture.md"]
        );
        assert_eq!(project.default_team.as_deref(), Some("code-review"));
        let settings = project.settings.as_ref().unwrap();
        assert_eq!(settings.max_turns, Some(20));
        assert_eq!(settings.message_retention_days, Some(30));

        // Round-trip
        let serialized = project.to_yaml().unwrap();
        let reparsed = ProjectDefinition::from_yaml(&serialized).unwrap();
        assert_eq!(reparsed.title, project.title);
        assert_eq!(reparsed.goal, project.goal);
        assert_eq!(reparsed.context_files, project.context_files);
    }

    #[test]
    fn validation_rejects_empty_title() {
        let yaml = r#"
version: "1.0.0"
title: "   "
"#;
        let err = ProjectDefinition::from_yaml(yaml).unwrap_err();
        assert!(err.to_string().contains("title is required"));
    }

    #[test]
    fn resolve_cwd_explicit() {
        let yaml = r#"
version: "1.0.0"
title: "p"
cwd: "/tmp/myproject"
"#;
        let project = ProjectDefinition::from_yaml(yaml).unwrap();
        assert_eq!(project.resolve_cwd(None), PathBuf::from("/tmp/myproject"));
    }

    #[test]
    fn resolve_cwd_from_store_dir() {
        let yaml = r#"
version: "1.0.0"
title: "p"
"#;
        let project = ProjectDefinition::from_yaml(yaml).unwrap();
        let store_dir = PathBuf::from("/store/projects");
        assert_eq!(project.resolve_cwd(Some(&store_dir)), store_dir);
    }

    #[test]
    fn resolve_cwd_fallback_to_process_cwd() {
        let yaml = r#"
version: "1.0.0"
title: "p"
"#;
        let project = ProjectDefinition::from_yaml(yaml).unwrap();
        let resolved = project.resolve_cwd(None);
        // Should be a valid path (not panicked)
        assert!(resolved.is_absolute() || resolved.as_os_str().len() > 0);
    }

    #[test]
    fn project_context_from_definition_no_files() {
        let yaml = r#"
version: "1.0.0"
title: "test"
goal: "Test goal"
cwd: "/tmp"
"#;
        let def = ProjectDefinition::from_yaml(yaml).unwrap();
        let ctx = ProjectContext::from_definition(&def, None);
        assert_eq!(ctx.title, "test");
        assert_eq!(ctx.goal, "Test goal");
        assert_eq!(ctx.cwd, PathBuf::from("/tmp"));
        assert!(ctx.context_entries.is_empty());
    }

    #[test]
    fn system_prompt_extension_includes_goal_and_cwd() {
        let def = ProjectDefinition {
            version: "1.0.0".into(),
            title: "demo".into(),
            goal: Some("Ship v2".into()),
            cwd: Some("/workspace".into()),
            context_files: vec![],
            default_team: None,
            description: None,
            settings: None,
        };
        let ctx = ProjectContext::from_definition(&def, None);
        let ext = ctx.system_prompt_extension();
        assert!(ext.contains("demo"));
        assert!(ext.contains("Ship v2"));
        assert!(ext.contains("/workspace"));
    }

    #[test]
    fn system_prompt_extension_no_goal() {
        let def = ProjectDefinition {
            version: "1.0.0".into(),
            title: "minimal".into(),
            goal: None,
            cwd: Some("/tmp".into()),
            context_files: vec![],
            default_team: None,
            description: None,
            settings: None,
        };
        let ctx = ProjectContext::from_definition(&def, None);
        let ext = ctx.system_prompt_extension();
        assert!(ext.contains("minimal"));
        assert!(!ext.contains("Goal:"));
    }

    #[test]
    fn project_context_skips_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let def = ProjectDefinition {
            version: "1.0.0".into(),
            title: "proj".into(),
            goal: None,
            cwd: Some(tmp.path().to_string_lossy().into()),
            context_files: vec!["nonexistent.md".into()],
            default_team: None,
            description: None,
            settings: None,
        };
        // Should not panic; missing file is skipped.
        let ctx = ProjectContext::from_definition(&def, None);
        assert!(ctx.context_entries.is_empty());
    }

    #[test]
    fn project_context_loads_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx_file = tmp.path().join("notes.md");
        std::fs::write(&ctx_file, "# Notes\nImportant context here.").unwrap();

        let def = ProjectDefinition {
            version: "1.0.0".into(),
            title: "proj".into(),
            goal: Some("Build it".into()),
            cwd: Some(tmp.path().to_string_lossy().into()),
            context_files: vec!["notes.md".into()],
            default_team: None,
            description: None,
            settings: None,
        };
        let ctx = ProjectContext::from_definition(&def, None);
        assert_eq!(ctx.context_entries.len(), 1);
        assert_eq!(ctx.context_entries[0].0, "notes");
        assert!(ctx.context_entries[0].1.contains("Important context"));

        let ext = ctx.system_prompt_extension();
        assert!(ext.contains("notes"));
        assert!(ext.contains("Important context"));
    }
}
