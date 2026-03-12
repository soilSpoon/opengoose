use crate::error::CliResult;
use serde_json::json;

use crate::cmd::output::{format_table, CliOutput};
use opengoose_teams::TeamStore;

pub(super) fn run(store: &TeamStore, output: CliOutput) -> CliResult<()> {
    let names = store.list()?;

    if names.is_empty() {
        if output.is_json() {
            output.print_json(&json!({
                "ok": true,
                "command": "team.list",
                "teams": [],
            }))?;
        } else {
            println!("No teams found. Use `opengoose team init` to install defaults.");
        }
        return Ok(());
    }

    let teams = names
        .iter()
        .map(|name| store.get(name).map(|team| (name.clone(), team)))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if output.is_json() {
        let teams_json = teams
            .iter()
            .map(|(name, team)| {
                json!({
                    "name": name,
                    "description": team.description,
                    "workflow": format!("{:?}", team.workflow).to_lowercase(),
                    "agent_count": team.agents.len(),
                })
            })
            .collect::<Vec<_>>();
        output.print_json(&json!({
            "ok": true,
            "command": "team.list",
            "teams": teams_json,
        }))?;
        return Ok(());
    }

    println!("{}", output.heading("Teams"));
    let rows = teams
        .iter()
        .map(|(name, team)| {
            vec![
                name.clone(),
                format!("{:?}", team.workflow).to_lowercase(),
                team.agents.len().to_string(),
                team.description
                    .clone()
                    .unwrap_or_else(|| "(no description)".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    print!(
        "{}",
        format_table(&["TEAM", "WORKFLOW", "AGENTS", "DESCRIPTION"], &rows)
    );

    Ok(())
}
