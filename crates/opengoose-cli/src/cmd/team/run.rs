use std::sync::Arc;

use crate::error::CliResult;

use opengoose_persistence::Database;
use opengoose_teams::{HeadlessConfig, run_headless};
use opengoose_types::EventBus;

pub(super) async fn run(team_name: &str, input: &str, model: Option<String>) -> CliResult<()> {
    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);

    println!("Running team '{team_name}'...");

    let (run_id, result) = run_headless(HeadlessConfig {
        team_name: team_name.to_string(),
        input: input.to_string(),
        db,
        event_bus,
        selected_model: model,
        project: None,
    })
    .await?;

    println!("\n--- Result ---");
    println!("{result}");
    println!("\nRun ID: {run_id}");

    Ok(())
}
