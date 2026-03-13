use std::sync::Arc;

use crate::error::{CliError, CliResult};

use super::render::{preview_text, work_status_icon};
use opengoose_persistence::{Database, OrchestrationStore, ProllyBeadsStore, WorkItemStore};

pub(super) fn run(run_id: Option<&str>) -> CliResult<()> {
    let db = Arc::new(Database::open()?);
    let orch_store = OrchestrationStore::new(db);

    match run_id {
        Some(id) => {
            let run = orch_store
                .get_run(id)?
                .ok_or_else(|| CliError::Validation(format!("run '{}' not found", id)))?;

            println!("Run: {}", run.team_run_id);
            println!("Team: {}", run.team_name);
            println!("Workflow: {}", run.workflow);
            println!("Status: {}", run.status.as_str());
            println!("Progress: {}/{}", run.current_step, run.total_steps);
            println!("Created: {}", run.created_at);
            println!("Updated: {}", run.updated_at);

            if let Some(ref result) = run.result {
                println!("Result: {}", preview_text(result, 200));
            }

            let work_store = WorkItemStore::new(Arc::new(ProllyBeadsStore::in_memory()));
            let items = work_store.list_for_run(id, None);

            if !items.is_empty() {
                println!("\nWork Items:");
                for item in &items {
                    let indent = if item.parent_hash_id.is_some() {
                        "    "
                    } else {
                        "  "
                    };
                    let agent = item.assigned_to.as_deref().unwrap_or("-");
                    println!(
                        "{indent}{} {} [{}] (step: {})",
                        work_status_icon(&item.status),
                        item.title,
                        agent,
                        item.workflow_step
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".into())
                    );
                }
            }
        }
        None => {
            let runs = orch_store.list_runs(None, 20)?;

            if runs.is_empty() {
                println!("No team runs found.");
                return Ok(());
            }

            println!(
                "{:<38} {:<16} {:<10} {:<10} UPDATED",
                "RUN ID", "TEAM", "WORKFLOW", "STATUS"
            );
            for run in &runs {
                println!(
                    "{:<38} {:<16} {:<10} {:<10} {}",
                    run.team_run_id,
                    run.team_name,
                    run.workflow,
                    run.status.as_str(),
                    run.updated_at,
                );
            }
        }
    }

    Ok(())
}
