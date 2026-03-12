use crate::error::CliResult;
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_projects::ProjectStore;

pub(super) fn run(name: &str, store: &ProjectStore, output: CliOutput) -> CliResult<()> {
    let project = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.show",
            "project": project,
        }))?;
    } else {
        let yaml = project.to_yaml()?;
        print!("{yaml}");
    }

    Ok(())
}
