use std::sync::Arc;

use anyhow::Result;

use opengoose_persistence::Database;
use opengoose_types::EventBus;

pub(super) async fn run(run_id: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let event_bus = EventBus::new(256);

    println!("Resuming run '{run_id}'...");

    let result = opengoose_teams::resume_headless(run_id, db, event_bus).await?;

    println!("\n--- Result ---");
    println!("{result}");

    Ok(())
}
