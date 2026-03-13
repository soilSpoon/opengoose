use std::future::Future;
use std::sync::Arc;

use anyhow::{Result, bail};

use opengoose_persistence::Database;
use opengoose_types::{EventBus, SessionKey};

use crate::context::OrchestrationContext;
use crate::team::TeamDefinition;

use opengoose_profiles::ProfileStore;

use super::config::load_team;

/// Resume a suspended team workflow headlessly.
///
/// Returns the resumed workflow result as `String` on success.
pub async fn resume_headless(
    team_run_id: &str,
    db: Arc<Database>,
    event_bus: EventBus,
) -> Result<String> {
    resume_headless_with(
        team_run_id,
        db,
        event_bus,
        |team, profile_store, ctx, parent_hash_id| async move {
            let orchestrator = crate::orchestrator::TeamOrchestrator::new(team, profile_store);
            orchestrator.resume(&ctx, &parent_hash_id).await
        },
    )
    .await
}

pub(crate) async fn resume_headless_with<Resume, Fut>(
    team_run_id: &str,
    db: Arc<Database>,
    event_bus: EventBus,
    resume: Resume,
) -> Result<String>
where
    Resume: FnOnce(TeamDefinition, ProfileStore, OrchestrationContext, String) -> Fut,
    Fut: Future<Output = Result<String>>,
{
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

    let team = load_team(&run.team_name)?;
    let profile_store = ProfileStore::new()?;
    let session_key = SessionKey::from_stable_id(&run.session_key);
    let ctx =
        OrchestrationContext::new(team_run_id.to_string(), session_key, db.clone(), event_bus);
    let parent_hash_id = find_parent_work_item(&ctx, team_run_id)?;

    resume(team, profile_store, ctx, parent_hash_id).await
}

pub(crate) fn find_parent_work_item(
    ctx: &OrchestrationContext,
    team_run_id: &str,
) -> Result<String> {
    let items = ctx.work_items().list_for_run(team_run_id, None);
    let parent = items
        .into_iter()
        .find(|w| w.parent_hash_id.is_none())
        .ok_or_else(|| anyhow::anyhow!("no parent work item found for run '{}'", team_run_id))?;
    Ok(parent.hash_id)
}
