use crate::error::CliResult;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_teams::TeamStore;

pub(super) fn run(force: bool, store: &TeamStore, output: CliOutput) -> CliResult<()> {
    let count = store.install_defaults(force)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.init",
            "installed": count,
            "force": force,
        }))?;
    } else if count == 0 {
        println!("All default teams already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default team(s).");
    }

    Ok(())
}
