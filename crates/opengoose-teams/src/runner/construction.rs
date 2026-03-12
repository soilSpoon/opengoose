use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use tracing::{debug, info};
use uuid::Uuid;

use goose::agents::Agent;
use goose::session::SessionType;
use opengoose_profiles::AgentProfile;
use opengoose_projects::ProjectContext;

use crate::recipe_bridge;

use super::AgentRunner;
use super::types::{AgentOutput, resolve_provider_chain};

impl AgentRunner {
    /// Create a Goose Agent configured from an `AgentProfile`.
    ///
    /// Generates a fresh random session ID on each call. Use
    /// `from_profile_keyed` when you want a stable, reusable session.
    pub async fn from_profile(profile: &AgentProfile) -> Result<Self> {
        Self::from_profile_keyed(profile, Uuid::new_v4().to_string()).await
    }

    /// Create a Goose Agent with an explicit session ID.
    ///
    /// When `session_id` is derived deterministically from user + agent context
    /// (e.g. `"{session_key}::{agent_name}"`), Goose reuses the same underlying
    /// session across invocations — preserving message history and (if
    /// `save_extension_state` was called previously) extension connections.
    pub async fn from_profile_keyed(profile: &AgentProfile, session_name: String) -> Result<Self> {
        Self::from_profile_keyed_with_project(profile, session_name, None).await
    }

    /// Create a Goose Agent with an explicit session ID and an optional project
    /// context.
    ///
    /// When a `ProjectContext` is provided:
    /// - The Goose session's working directory is set to `project.cwd` instead
    ///   of the process `cwd`.
    /// - The project goal, cwd, and context files are injected into the agent's
    ///   system prompt via `extend_system_prompt("project_context", ...)`.
    ///
    /// This allows the same profile to behave differently across projects,
    /// scoping file operations and knowledge to the project boundary.
    pub async fn from_profile_keyed_with_project(
        profile: &AgentProfile,
        session_name: String,
        project: Option<&ProjectContext>,
    ) -> Result<Self> {
        let agent = Arc::new(Agent::new());

        // Choose the working directory: project cwd > process cwd.
        let cwd = project
            .map(|p| p.cwd.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        // Create a real session in Goose's DB first. All subsequent Goose
        // calls (update_provider, add_extension, add_message, reply) require
        // the session row to exist due to FK constraints.
        let session = agent
            .config
            .session_manager
            .create_session(cwd.clone(), session_name, SessionType::Gateway)
            .await
            .map_err(|e| anyhow!("failed to create goose session: {e}"))?;
        let session_id = session.id;
        let settings = profile.settings.as_ref();
        let provider_chain = resolve_provider_chain(profile);

        // Inject project context (goal, cwd, context files) into the system
        // prompt *before* the profile instructions so the profile can refer to
        // project goals without needing to know about them at authoring time.
        // This is done via a named extension key so it does not overwrite the
        // base identity set by the profile.
        if let Some(ctx) = project {
            let ext = ctx.system_prompt_extension();
            if !ext.is_empty() {
                agent
                    .extend_system_prompt("project_context".to_string(), ext)
                    .await;
                info!(
                    project = %ctx.title,
                    profile = %profile.name(),
                    "injected project context into agent system prompt"
                );
            }
        }

        // Set system prompt.
        //
        // If the profile carries explicit instructions (inline/team agents),
        // use them directly.  Otherwise build a workspace-backed identity:
        // seed the workspace on first run, load context files, and inject them
        // as an additive extension so the agent can read/modify them at runtime.
        if let Some(instructions) = &profile.instructions {
            agent.override_system_prompt(instructions.clone()).await;
        } else if let Some(prompt) = &profile.prompt {
            agent.override_system_prompt(prompt.clone()).await;
        } else {
            use opengoose_profiles::workspace;

            if let Some(workspace_dir) = workspace::workspace_dir_for(&profile.title) {
                if let Err(e) = workspace::setup_workspace(&profile.title, &workspace_dir) {
                    tracing::warn!(%e, profile = %profile.title, "failed to set up workspace");
                }

                let base_identity = format!(
                    "You are {}, a helpful AI assistant.\n\
                    Your workspace directory is: {}\n\
                    You have access to personal context files in your workspace. \
                    Read and follow any instructions contained in those files.",
                    profile.title,
                    workspace_dir.display()
                );
                agent.override_system_prompt(base_identity).await;

                let context = workspace::load_workspace_context(&workspace_dir);
                if !context.is_empty() {
                    agent
                        .extend_system_prompt("workspace_context".to_string(), context)
                        .await;
                }
            } else {
                let base_identity = format!("You are {}, a helpful AI assistant.", profile.title);
                agent.override_system_prompt(base_identity).await;
            }
        }

        // Add extensions — reuse the shared conversion from recipe_bridge.
        for ext in &profile.extensions {
            let config = match recipe_bridge::ext_ref_to_config(ext) {
                Some(c) => c,
                None => {
                    debug!(
                        ext = %ext.name,
                        ext_type = %ext.ext_type,
                        "skipping extension (unsupported type or missing required fields)"
                    );
                    continue;
                }
            };
            if let Err(e) = agent.add_extension(config, &session_id).await {
                debug!(
                    ext = %ext.name,
                    error = %e,
                    "failed to add extension (non-fatal)"
                );
            }
        }

        info!(
            profile = %profile.name(),
            session_id = %session_id,
            "created agent runner"
        );

        let max_turns = settings.and_then(|s| s.max_turns).unwrap_or(10);
        let retry_config = settings.and_then(recipe_bridge::settings_to_retry_config);

        Ok(Self {
            agent,
            session_id,
            profile_name: profile.title.clone(),
            provider_chain,
            max_turns,
            retry_config,
            cwd,
        })
    }

    /// Create an agent runner from an inline system prompt (no profile file needed).
    pub async fn from_inline_prompt(system_prompt: &str, agent_name: &str) -> Result<Self> {
        let profile = AgentProfile {
            version: "1.0.0".to_string(),
            title: agent_name.to_string(),
            description: None,
            instructions: Some(system_prompt.to_string()),
            prompt: None,
            extensions: vec![],
            skills: vec![],
            settings: None,
            activities: None,
            response: None,
            sub_recipes: None,
            parameters: None,
        };
        Self::from_profile(&profile).await
    }

    /// Convenience: create from inline prompt and run in one call.
    pub async fn run_with_inline_prompt(
        system_prompt: &str,
        agent_name: &str,
        user_prompt: &str,
    ) -> Result<AgentOutput> {
        let runner = Self::from_inline_prompt(system_prompt, agent_name).await?;
        runner.run(user_prompt).await
    }
}
