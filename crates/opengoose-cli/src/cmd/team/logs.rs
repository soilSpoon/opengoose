use std::sync::Arc;

use anyhow::{Result, anyhow};

use super::render::{preview_text, work_status_icon};
use opengoose_persistence::{Database, OrchestrationStore, WorkItemStore};

pub(super) fn run(run_id: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let orch_store = OrchestrationStore::new(db.clone());

    let run = orch_store
        .get_run(run_id)?
        .ok_or_else(|| anyhow!("run '{}' not found", run_id))?;

    println!(
        "Logs for run: {} (team: {}, workflow: {})",
        run.team_run_id, run.team_name, run.workflow
    );
    println!("Status: {}", run.status.as_str());
    println!();

    let work_store = WorkItemStore::new(db);
    let items = work_store.list_for_run(run_id, None)?;

    if items.is_empty() {
        println!("(no work items recorded)");
        return Ok(());
    }

    for item in &items {
        let agent = item.assigned_to.as_deref().unwrap_or("-");

        println!(
            "[{}] {} {} (agent: {}, step: {})",
            item.updated_at,
            work_status_icon(&item.status),
            item.title,
            agent,
            item.workflow_step
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into())
        );

        if let Some(ref input) = item.input {
            println!("  Input: {}", preview_text(input, 300));
        }
        if let Some(ref output) = item.output {
            println!("  Output: {}", preview_text(output, 300));
        }
        if let Some(ref error) = item.error {
            println!("  Error: {error}");
        }
        println!();
    }

    Ok(())
}
