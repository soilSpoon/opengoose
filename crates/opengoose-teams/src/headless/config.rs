use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use opengoose_persistence::Database;
use opengoose_projects::ProjectContext;
use opengoose_types::{EventBus, Platform, SessionKey};

use crate::context::OrchestrationContext;
use crate::store::TeamStore;
use crate::team::TeamDefinition;

/// Configuration for running a team workflow headlessly (no TUI, no gateway).
pub struct HeadlessConfig {
    pub team_name: String,
    pub input: String,
    pub db: Arc<Database>,
    pub event_bus: EventBus,
    pub selected_model: Option<String>,
    pub project: Option<Arc<ProjectContext>>,
}

impl HeadlessConfig {
    /// Create a minimal config with no model override or project.
    pub fn new(
        team_name: impl Into<String>,
        input: impl Into<String>,
        db: Arc<Database>,
        event_bus: EventBus,
    ) -> Self {
        Self {
            team_name: team_name.into(),
            input: input.into(),
            db,
            event_bus,
            selected_model: None,
            project: None,
        }
    }
}

pub(crate) fn load_team(team_name: &str) -> Result<TeamDefinition> {
    let team_store = TeamStore::new()?;
    Ok(team_store.get(team_name)?)
}

pub(crate) fn create_headless_context(
    input: &str,
    db: Arc<Database>,
    event_bus: EventBus,
    selected_model: Option<&str>,
) -> Result<(String, OrchestrationContext)> {
    let team_run_id = Uuid::new_v4().to_string();
    let session_key = SessionKey::new(Platform::Custom("cli".into()), "headless", &team_run_id);
    let ctx = OrchestrationContext::new(team_run_id.clone(), session_key, db, event_bus);

    // Ensure session exists for FK constraints
    ctx.sessions()
        .append_user_message(&ctx.session_key, input, Some("cli"))?;
    if let Some(selected_model) = selected_model {
        ctx.sessions()
            .set_selected_model(&ctx.session_key, Some(selected_model))?;
    }

    Ok((team_run_id, ctx))
}
