use crate::error::CliResult;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_teams::TeamStore;

pub(super) fn run(name: &str, store: &TeamStore, output: CliOutput) -> CliResult<()> {
    let team = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.show",
            "team": team,
        }))?;
    } else {
        print!("{}", team.to_yaml()?);
    }

    Ok(())
}
