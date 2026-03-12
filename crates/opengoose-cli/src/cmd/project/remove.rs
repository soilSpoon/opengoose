use anyhow::Result;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_projects::ProjectStore;

pub(super) fn run(name: &str, store: &ProjectStore, output: CliOutput) -> Result<()> {
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.remove",
            "project": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed project `{name}`.");
    }

    Ok(())
}
