use anyhow::Result;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_teams::TeamStore;

pub(super) fn run(name: &str, store: &TeamStore, output: CliOutput) -> Result<()> {
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.remove",
            "team": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed team `{name}`.");
    }

    Ok(())
}
