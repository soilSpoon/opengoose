use std::sync::Arc;

use dashmap::DashMap;
use tracing::{info, warn};
use uuid::Uuid;

use opengoose_persistence::{Database, OrchestrationStore, SessionStore, WorkflowRunStore};
use opengoose_profiles::ProfileStore;
use opengoose_teams::{OrchestrationContext, TeamOrchestrator, TeamStore};
use opengoose_types::{AppEventKind, EventBus, SessionKey};
use crate::workflow_runner::WorkflowRunner;

/// Parsed command from user input.
enum Command<'a> {
    Resume,
    ListWorkflows,
    RunWorkflow { name: &'a str, input: &'a str },
}

fn parse_command(text: &str) -> Option<Command<'_>> {
    let trimmed = text.trim();
    if trimmed == "!resume" {
        return Some(Command::Resume);
    }
    if trimmed == "!workflows" {
        return Some(Command::ListWorkflows);
    }
    if let Some(rest) = trimmed.strip_prefix("!workflow ") {
        let rest = rest.trim();
        if let Some((name, input)) = rest.split_once(' ') {
            return Some(Command::RunWorkflow {
                name: name.trim(),
                input: input.trim(),
            });
        }
        // Missing input — caller handles the usage error
    }
    None
}

/// Platform-agnostic core engine.
///
/// Owns session management, team management, and orchestration logic.
/// Adapters (Discord, Slack, CLI, Web) interact with this engine —
/// it knows nothing about any specific platform.
pub struct Engine {
    event_bus: EventBus,
    /// Per-session active team. Key = SessionKey, Value = team name.
    active_teams: DashMap<SessionKey, String>,
    /// Shared database for all persistence.
    db: Arc<Database>,
    /// Workflow runner for antfarm-style multi-step workflows.
    workflow_runner: WorkflowRunner,
    /// Cached team store (initialized once at startup).
    team_store: Option<TeamStore>,
    /// Cached profile store (initialized once at startup, used by agent execution).
    #[allow(dead_code)]
    profile_store: Option<ProfileStore>,
}

impl Engine {
    pub fn new(event_bus: EventBus, db: Database) -> Self {
        let db = Arc::new(db);
        let sessions = SessionStore::new(db.clone());

        // Suspend any incomplete orchestration runs from previous crash
        let orch_store = OrchestrationStore::new(db.clone());
        if let Err(e) = orch_store.suspend_incomplete() {
            warn!(%e, "failed to suspend incomplete team runs on startup");
        }

        // Suspend any incomplete workflow runs from previous crash
        let wf_run_store = WorkflowRunStore::new(db.clone());
        if let Err(e) = wf_run_store.suspend_incomplete() {
            warn!(%e, "failed to suspend incomplete workflow runs on startup");
        }

        // Initialize workflow runner and load bundled workflows
        let mut workflow_runner = WorkflowRunner::new(event_bus.clone(), db.clone());
        if let Err(e) = workflow_runner.load_bundled() {
            warn!(%e, "failed to load bundled workflows");
        }

        // Restore active teams from database
        let active_teams = DashMap::new();
        match sessions.load_all_active_teams() {
            Ok(teams) => {
                for (key, team) in teams {
                    info!(%key, team = %team, "restored active team from db");
                    active_teams.insert(key, team);
                }
            }
            Err(e) => {
                warn!(%e, "failed to restore active teams from db");
            }
        }

        // Cache stores (failures are non-fatal — methods gracefully degrade)
        let team_store = match TeamStore::new() {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(%e, "failed to initialize team store");
                None
            }
        };
        let profile_store = match ProfileStore::new() {
            Ok(s) => Some(s),
            Err(e) => {
                warn!(%e, "failed to initialize profile store");
                None
            }
        };

        Self {
            event_bus,
            active_teams,
            db,
            workflow_runner,
            team_store,
            profile_store,
        }
    }

    // ── Team management ──────────────────────────────────────────────

    pub fn set_active_team(&self, session_key: &SessionKey, team_name: String) {
        info!(%session_key, team = %team_name, "activating team");
        self.active_teams
            .insert(session_key.clone(), team_name.clone());
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.set_active_team(session_key, Some(&team_name)) {
            warn!(%e, "failed to persist active team");
        }
        self.event_bus.emit(AppEventKind::TeamActivated {
            session_key: session_key.clone(),
            team_name,
        });
    }

    pub fn clear_active_team(&self, session_key: &SessionKey) {
        info!(%session_key, "deactivating team");
        self.active_teams.remove(session_key);
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.set_active_team(session_key, None) {
            warn!(%e, "failed to persist team deactivation");
        }
        self.event_bus.emit(AppEventKind::TeamDeactivated {
            session_key: session_key.clone(),
        });
    }

    pub fn active_team_for(&self, session_key: &SessionKey) -> Option<String> {
        self.active_teams.get(session_key).map(|v| v.clone())
    }

    pub fn team_exists(&self, name: &str) -> bool {
        match &self.team_store {
            Some(store) => store.get(name).is_ok(),
            None => false,
        }
    }

    pub fn list_teams(&self) -> Vec<String> {
        match &self.team_store {
            Some(store) => store.list().unwrap_or_default(),
            None => Default::default(),
        }
    }

    // ── Workflow management ─────────────────────────────────────────

    pub fn list_workflows(&self) -> Vec<&str> {
        self.workflow_runner.list_workflows()
    }

    pub fn workflow_exists(&self, name: &str) -> bool {
        self.workflow_runner.list_workflows().contains(&name)
    }

    // ── Session / history management ─────────────────────────────────

    pub fn record_user_message(&self, key: &SessionKey, content: &str, author: Option<&str>) {
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.append_user_message(key, content, author) {
            warn!(%e, "failed to persist user message");
        }
    }

    pub fn record_assistant_message(&self, key: &SessionKey, content: &str) {
        let sessions = SessionStore::new(self.db.clone());
        if let Err(e) = sessions.append_assistant_message(key, content) {
            warn!(%e, "failed to persist assistant message");
        }
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    pub fn sessions(&self) -> SessionStore {
        SessionStore::new(self.db.clone())
    }

    /// Record an assistant message and emit a ResponseSent event.
    fn send_response(&self, session_key: &SessionKey, msg: &str) {
        self.record_assistant_message(session_key, msg);
        self.event_bus.emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: msg.to_string(),
        });
    }

    // ── Message processing ───────────────────────────────────────────

    pub async fn process_message(
        &self,
        session_key: &SessionKey,
        author: Option<&str>,
        text: &str,
    ) -> anyhow::Result<Option<String>> {
        self.event_bus.emit(AppEventKind::MessageReceived {
            session_key: session_key.clone(),
            author: author.unwrap_or("unknown").to_string(),
            content: text.to_string(),
        });

        self.record_user_message(session_key, text, author);

        // Dispatch recognized commands
        if let Some(cmd) = parse_command(text) {
            return match cmd {
                Command::Resume => self.handle_resume(session_key).await.map(Some),
                Command::ListWorkflows => {
                    let names = self.list_workflows();
                    let msg = if names.is_empty() {
                        "No workflows available.".to_string()
                    } else {
                        format!("Available workflows: {}", names.join(", "))
                    };
                    self.send_response(session_key, &msg);
                    Ok(Some(msg))
                }
                Command::RunWorkflow { name, input } => {
                    self.run_workflow(session_key, name, input).await.map(Some)
                }
            };
        }

        // Handle "!workflow" with missing input (parse_command returns None)
        if text.trim().starts_with("!workflow ") {
            let msg = "Usage: !workflow <name> <input>".to_string();
            self.send_response(session_key, &msg);
            return Ok(Some(msg));
        }

        let team_name = match self.active_team_for(session_key) {
            Some(name) => name,
            None => return Ok(None),
        };

        let response = self
            .run_team_orchestration(session_key, &team_name, text)
            .await?;

        Ok(Some(response))
    }

    async fn run_team_orchestration(
        &self,
        session_key: &SessionKey,
        team_name: &str,
        input: &str,
    ) -> anyhow::Result<String> {
        let team = self
            .team_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("team store not available"))?
            .get(team_name)
            .map_err(|e| anyhow::anyhow!("team load error: {e}"))?;

        // Orchestrator takes ownership of ProfileStore, so create a fresh one
        let profile_store =
            ProfileStore::new().map_err(|e| anyhow::anyhow!("profile store error: {e}"))?;

        let team_run_id = Uuid::new_v4().to_string();
        let ctx = OrchestrationContext::new(
            team_run_id,
            session_key.clone(),
            self.db.clone(),
            self.event_bus.clone(),
        );

        let orchestrator = TeamOrchestrator::new(team, profile_store);
        let response = orchestrator.execute(input, &ctx).await?;

        self.send_response(session_key, &response);

        Ok(response)
    }

    async fn run_workflow(
        &self,
        session_key: &SessionKey,
        workflow_name: &str,
        input: &str,
    ) -> anyhow::Result<String> {
        if !self.workflow_exists(workflow_name) {
            let available = self.list_workflows();
            let msg = format!(
                "Unknown workflow '{}'. Available: {}",
                workflow_name,
                if available.is_empty() {
                    "none".to_string()
                } else {
                    available.join(", ")
                }
            );
            self.send_response(session_key, &msg);
            return Ok(msg);
        }

        let run_id = Uuid::new_v4().to_string();
        let session_id = session_key.to_stable_id();

        // The execute_step callback sends the prompt through a simple echo
        // for now. In a full integration, this would route through the
        // platform's LLM backend (e.g., Goose session).
        let response = self
            .workflow_runner
            .run(
                workflow_name,
                input.to_string(),
                &run_id,
                Some(&session_id),
                |ctx| async move {
                    // TODO: Route through the platform's LLM backend.
                    // For now, return a placeholder indicating the step was reached.
                    Ok(format!(
                        "[{}] Agent '{}' executed step '{}' with prompt: {}",
                        ctx.step_id, ctx.agent_name, ctx.step_name, ctx.user_prompt
                    ))
                },
            )
            .await?;

        self.send_response(session_key, &response);

        Ok(response)
    }

    async fn handle_resume(&self, session_key: &SessionKey) -> anyhow::Result<String> {
        let session_id = session_key.to_stable_id();

        // Check for suspended team orchestrations first
        let orch_store = OrchestrationStore::new(self.db.clone());
        let suspended = orch_store.find_suspended(&session_id)?;

        if suspended.is_empty() {
            // Check for suspended workflow runs
            let wf_store = WorkflowRunStore::new(self.db.clone());
            let wf_suspended = wf_store.find_suspended(&session_id)?;

            if let Some(wf_run) = wf_suspended.first() {
                return self
                    .handle_workflow_resume(session_key, wf_run)
                    .await;
            }

            let msg = "No suspended runs to resume.".to_string();
            self.send_response(session_key, &msg);
            return Ok(msg);
        }

        let run = &suspended[0];

        info!(
            team_run_id = %run.team_run_id,
            team = %run.team_name,
            step = run.current_step,
            "resuming suspended orchestration"
        );

        let team = self
            .team_store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("team store not available"))?
            .get(&run.team_name)
            .map_err(|e| anyhow::anyhow!("team load error: {e}"))?;

        // Orchestrator takes ownership of ProfileStore, so create a fresh one
        let profile_store =
            ProfileStore::new().map_err(|e| anyhow::anyhow!("profile store error: {e}"))?;

        let ctx = OrchestrationContext::new(
            run.team_run_id.clone(),
            session_key.clone(),
            self.db.clone(),
            self.event_bus.clone(),
        );

        let work_items = ctx.work_items().list_for_run(&run.team_run_id, None)?;
        let parent_id = work_items
            .iter()
            .find(|wi| wi.parent_id.is_none())
            .map(|wi| wi.id.clone())
            .ok_or_else(|| anyhow::anyhow!("no parent work item found for run"))?;

        let orchestrator = TeamOrchestrator::new(team, profile_store);
        let response = orchestrator.resume(&ctx, &parent_id).await?;

        self.send_response(session_key, &response);

        Ok(response)
    }

    async fn handle_workflow_resume(
        &self,
        session_key: &SessionKey,
        wf_run: &opengoose_persistence::WorkflowRunRow,
    ) -> anyhow::Result<String> {
        let session_id = session_key.to_stable_id();

        info!(
            run_id = %wf_run.run_id,
            workflow = %wf_run.workflow_name,
            step = wf_run.current_step,
            "resuming suspended workflow run"
        );

        let response = self
            .workflow_runner
            .resume_and_run(
                &wf_run.run_id,
                &wf_run.workflow_name,
                Some(&session_id),
                |ctx| async move {
                    // TODO: Route through the platform's LLM backend.
                    Ok(format!(
                        "[{}] Agent '{}' executed step '{}' with prompt: {}",
                        ctx.step_id, ctx.agent_name, ctx.step_name, ctx.user_prompt
                    ))
                },
            )
            .await?;

        self.send_response(session_key, &response);

        Ok(response)
    }
}
