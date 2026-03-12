use crate::error::CliResult;
use serde_json::json;

use crate::cmd::output::{format_table, CliOutput};
use opengoose_profiles::ProfileStore;

pub(super) fn run(output: CliOutput) -> CliResult<()> {
    let store = ProfileStore::new()?;
    let names = store.list()?;

    if names.is_empty() {
        if output.is_json() {
            output.print_json(&json!({
                "ok": true,
                "command": "profile.list",
                "profiles": [],
            }))?;
        } else {
            println!("No profiles found. Use `opengoose profile init` to install defaults.");
        }
        return Ok(());
    }

    let profiles = names
        .iter()
        .map(|name| store.get(name).map(|profile| (name.clone(), profile)))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if output.is_json() {
        let profiles_json = profiles
            .iter()
            .map(|(name, profile)| {
                json!({
                    "name": name,
                    "description": profile.description,
                })
            })
            .collect::<Vec<_>>();
        output.print_json(&json!({
            "ok": true,
            "command": "profile.list",
            "profiles": profiles_json,
        }))?;
        return Ok(());
    }

    println!("{}", output.heading("Profiles"));
    let rows = profiles
        .iter()
        .map(|(name, profile)| {
            vec![
                name.clone(),
                profile
                    .description
                    .clone()
                    .unwrap_or_else(|| "(no description)".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    print!("{}", format_table(&["PROFILE", "DESCRIPTION"], &rows));

    Ok(())
}
