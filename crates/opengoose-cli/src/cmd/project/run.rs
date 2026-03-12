use std::sync::Arc;

use anyhow::Result;

use opengoose_persistence::Database;
use opengoose_projects::{ProjectContext, ProjectStore};
use opengoose_teams::run_headless_with_project;
use opengoose_types::EventBus;

pub(super) async fn run(
    project_name: &str,
    input: &str,
    team_override: Option<&str>,
    store: &ProjectStore,
) -> Result<()> {
    let project_def = store.get(project_name)?;

    let team_name = team_override
        .or(project_def.default_team.as_deref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "project '{}' has no default_team configured; specify one with --team",
                project_name
            )
        })?
        .to_string();

    let store_dir = store.dir().to_path_buf();
    let project_ctx = Arc::new(ProjectContext::from_definition(
        &project_def,
        Some(&store_dir),
    ));

    println!(
        "Running project '{}' with team '{team_name}' (cwd: {})...",
        project_ctx.title,
        project_ctx.cwd.display()
    );

    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);
    let (run_id, result) =
        run_headless_with_project(&team_name, input, db, event_bus, project_ctx).await?;

    println!("\n--- Result ---");
    println!("{result}");
    println!("\nRun ID: {run_id}");

    Ok(())
}
