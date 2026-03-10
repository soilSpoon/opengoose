use std::sync::Arc;

use anyhow::{Result, bail};
use uuid::Uuid;

use opengoose_persistence::Database;
use opengoose_profiles::ProfileStore;
use opengoose_types::{EventBus, Platform, SessionKey};

use crate::context::OrchestrationContext;
use crate::orchestrator::TeamOrchestrator;
use crate::store::TeamStore;

/// Run a team workflow headlessly (no TUI, no gateway).
///
/// Returns `(team_run_id, result)` on success.
pub async fn run_headless(
    team_name: &str,
    input: &str,
    db: Arc<Database>,
    event_bus: EventBus,
) -> Result<(String, String)> {
    let team_store = TeamStore::new()?;
    let team = team_store.get(team_name)?;

    let profile_store = ProfileStore::new()?;
    let team_run_id = Uuid::new_v4().to_string();
    let session_key = SessionKey::new(Platform::Custom("cli".into()), "headless", &team_run_id);

    let ctx = OrchestrationContext::new(team_run_id.clone(), session_key, db, event_bus);

    // Ensure session exists for FK constraints
    ctx.sessions()
        .append_user_message(&ctx.session_key, input, Some("cli"))?;

    let orchestrator = TeamOrchestrator::new(team, profile_store);
    let result = orchestrator.execute(input, &ctx).await?;

    Ok((team_run_id, result))
}

/// Resume a suspended team workflow headlessly.
///
/// Returns `(team_run_id, result)` on success.
pub async fn resume_headless(
    team_run_id: &str,
    db: Arc<Database>,
    event_bus: EventBus,
) -> Result<String> {
    let orch_store = opengoose_persistence::OrchestrationStore::new(db.clone());
    let run = orch_store
        .get_run(team_run_id)?
        .ok_or_else(|| anyhow::anyhow!("run '{}' not found", team_run_id))?;

    if run.status != opengoose_persistence::RunStatus::Suspended {
        bail!(
            "run '{}' is not suspended (status: {})",
            team_run_id,
            run.status.as_str()
        );
    }

    let team_store = TeamStore::new()?;
    let team = team_store.get(&run.team_name)?;
    let profile_store = ProfileStore::new()?;

    let session_key = SessionKey::from_stable_id(&run.session_key);
    let ctx =
        OrchestrationContext::new(team_run_id.to_string(), session_key, db.clone(), event_bus);

    // Find parent work item for this run
    let work_items = opengoose_persistence::WorkItemStore::new(db);
    let items = work_items.list_for_run(team_run_id, None)?;
    let parent = items
        .into_iter()
        .find(|w| w.parent_id.is_none())
        .ok_or_else(|| anyhow::anyhow!("no parent work item found for run '{}'", team_run_id))?;

    let orchestrator = TeamOrchestrator::new(team, profile_store);
    orchestrator.resume(&ctx, parent.id).await
}
