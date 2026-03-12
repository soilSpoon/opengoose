use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_teams::{TeamDefinition, TeamStore};

pub(super) fn run(path: &PathBuf, force: bool, store: &TeamStore, output: CliOutput) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let team = TeamDefinition::from_yaml(&content)?;
    let name = team.title.clone();

    store.save(&team, force)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.add",
            "team": name,
            "path": path,
            "force": force,
        }))?;
    } else {
        println!("Added team `{name}`.");
    }

    Ok(())
}
