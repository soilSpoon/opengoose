use std::sync::Arc;

use crate::error::CliResult;

use opengoose_persistence::Database;
use opengoose_types::EventBus;

pub(super) async fn run(team_name: &str, input: &str, model: Option<String>) -> CliResult<()> {
    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);

    println!("Running team '{team_name}'...");

    let (run_id, result) =
        opengoose_teams::run_headless_with_model(team_name, input, db, event_bus, model).await?;

    println!("\n--- Result ---");
    println!("{result}");
    println!("\nRun ID: {run_id}");

    Ok(())
}
