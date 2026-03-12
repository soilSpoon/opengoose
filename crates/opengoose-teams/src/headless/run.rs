use std::future::Future;

use anyhow::Result;

use opengoose_persistence::Database;
use opengoose_profiles::ProfileStore;
use opengoose_types::EventBus;

use crate::context::OrchestrationContext;
use crate::orchestrator::TeamOrchestrator;
use crate::team::TeamDefinition;

use super::config::{HeadlessConfig, create_headless_context, load_team};

/// Run a team workflow headlessly (no TUI, no gateway).
///
/// Returns `(team_run_id, result)` on success.
pub async fn run_headless(config: HeadlessConfig) -> Result<(String, String)> {
    let HeadlessConfig {
        team_name,
        input,
        db,
        event_bus,
        selected_model,
        project,
    } = config;
    let model_override = selected_model.clone();
    run_headless_with(
        &team_name,
        &input,
        db,
        event_bus,
        selected_model,
        move |team, profile_store, input, ctx| async move {
            let ctx = match project {
                Some(project) => ctx.with_project(project),
                None => ctx,
            };
            let orchestrator =
                TeamOrchestrator::new_with_model_override(team, profile_store, model_override);
            orchestrator.execute(&input, &ctx).await
        },
    )
    .await
}

pub(crate) async fn run_headless_with<Execute, Fut>(
    team_name: &str,
    input: &str,
    db: std::sync::Arc<Database>,
    event_bus: EventBus,
    selected_model: Option<String>,
    execute: Execute,
) -> Result<(String, String)>
where
    Execute: FnOnce(TeamDefinition, ProfileStore, String, OrchestrationContext) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let team = load_team(team_name)?;
    let profile_store = ProfileStore::new()?;
    let (team_run_id, ctx) =
        create_headless_context(input, db, event_bus, selected_model.as_deref())?;
    let result = execute(team, profile_store, input.to_string(), ctx).await?;
    Ok((team_run_id, result))
}
