use anyhow::Result;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_profiles::ProfileStore;

pub(super) fn run(force: bool, output: CliOutput) -> Result<()> {
    let store = ProfileStore::new()?;
    let count = store.install_defaults(force)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.init",
            "installed": count,
            "force": force,
        }))?;
    } else if count == 0 {
        println!("All default profiles already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default profile(s).");
    }

    Ok(())
}
