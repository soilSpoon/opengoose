use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_projects::ProjectStore;

pub(super) fn run(
    path: &PathBuf,
    force: bool,
    store: &ProjectStore,
    output: CliOutput,
) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let project = store.add_from_path(path, force)?;
    let name = project.title.clone();

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "project.add",
            "project": name,
            "path": path,
            "force": force,
        }))?;
    } else {
        println!("Added project `{name}`.");
    }

    Ok(())
}
