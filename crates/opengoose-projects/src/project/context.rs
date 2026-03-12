use std::path::{Path, PathBuf};

use super::definition::ProjectDefinition;

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
