use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_teams::{TeamDefinition, TeamStore};

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose team list\n  opengoose team show code-review\n  opengoose --json team list"
)]
pub enum TeamAction {
    /// List all team definitions
    #[command(after_help = "Examples:\n  opengoose team list\n  opengoose --json team list")]
    List,
    /// Show a team's full YAML
    #[command(after_help = "Example:\n  opengoose team show code-review")]
    Show {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Add a team from a YAML file
    #[command(after_help = "Example:\n  opengoose team add ./teams/custom.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the team already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a team
    #[command(after_help = "Example:\n  opengoose team remove code-review")]
    Remove {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Install bundled default teams
    #[command(after_help = "Examples:\n  opengoose team init\n  opengoose team init --force")]
    Init {
        /// Overwrite existing teams
        #[arg(long)]
        force: bool,
    },
}

pub fn execute(action: TeamAction, output: CliOutput) -> Result<()> {
    match action {
        TeamAction::List => cmd_list(output),
        TeamAction::Show { name } => cmd_show(&name, output),
        TeamAction::Add { path, force } => cmd_add(&path, force, output),
        TeamAction::Remove { name } => cmd_remove(&name, output),
        TeamAction::Init { force } => cmd_init(force, output),
    }
}

fn cmd_list(output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
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
        .collect::<Result<Vec<_>, _>>()?;

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

fn cmd_show(name: &str, output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
    let team = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.show",
            "team": team,
        }))?;
    } else {
        let yaml = team.to_yaml()?;
        print!("{yaml}");
    }

    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool, output: CliOutput) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let team = TeamDefinition::from_yaml(&content)?;
    let name = team.title.clone();

    let store = TeamStore::new()?;
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

fn cmd_remove(name: &str, output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "team.remove",
            "team": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed team `{name}`.");
    }

    Ok(())
}

fn cmd_init(force: bool, output: CliOutput) -> Result<()> {
    let store = TeamStore::new()?;
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
