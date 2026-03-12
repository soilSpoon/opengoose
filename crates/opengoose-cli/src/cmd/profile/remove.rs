use anyhow::Result;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_profiles::ProfileStore;

pub(super) fn run(name: &str, output: CliOutput) -> Result<()> {
    let store = ProfileStore::new()?;
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.remove",
            "profile": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed profile `{name}`.");
    }

    Ok(())
}
