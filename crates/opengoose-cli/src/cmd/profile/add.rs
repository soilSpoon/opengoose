use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_profiles::{AgentProfile, ProfileStore};

pub(super) fn run(path: &PathBuf, force: bool, output: CliOutput) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let profile = AgentProfile::from_yaml(&content)?;
    let name = profile.title.clone();

    let store = ProfileStore::new()?;
    store.save(&profile, force)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.add",
            "profile": name,
            "path": path,
            "force": force,
        }))?;
    } else {
        println!("Added profile `{name}`.");
    }

    Ok(())
}
