use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_profiles::{AgentProfile, ProfileStore};

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose profile list\n  opengoose profile show developer\n  opengoose --json profile list"
)]
pub enum ProfileAction {
    /// List all agent profiles
    #[command(after_help = "Examples:\n  opengoose profile list\n  opengoose --json profile list")]
    List,
    /// Show a profile's full YAML
    #[command(after_help = "Example:\n  opengoose profile show developer")]
    Show {
        /// Profile name (e.g. researcher)
        name: String,
    },
    /// Add a profile from a YAML file
    #[command(after_help = "Example:\n  opengoose profile add ./profiles/custom.yaml --force")]
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the profile already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a profile
    #[command(after_help = "Example:\n  opengoose profile remove developer")]
    Remove {
        /// Profile name (e.g. researcher)
        name: String,
    },
    /// Install bundled default profiles
    #[command(after_help = "Examples:\n  opengoose profile init\n  opengoose profile init --force")]
    Init {
        /// Overwrite existing profiles
        #[arg(long)]
        force: bool,
    },
}

pub fn execute(action: ProfileAction, output: CliOutput) -> Result<()> {
    match action {
        ProfileAction::List => cmd_list(output),
        ProfileAction::Show { name } => cmd_show(&name, output),
        ProfileAction::Add { path, force } => cmd_add(&path, force, output),
        ProfileAction::Remove { name } => cmd_remove(&name, output),
        ProfileAction::Init { force } => cmd_init(force, output),
    }
}

fn cmd_list(output: CliOutput) -> Result<()> {
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
        .collect::<Result<Vec<_>, _>>()?;

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

fn cmd_show(name: &str, output: CliOutput) -> Result<()> {
    let store = ProfileStore::new()?;
    let profile = store.get(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.show",
            "profile": profile,
        }))?;
    } else {
        let yaml = profile.to_yaml()?;
        print!("{yaml}");
    }

    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool, output: CliOutput) -> Result<()> {
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

fn cmd_remove(name: &str, output: CliOutput) -> Result<()> {
    let store = ProfileStore::new()?;
    store.remove(name)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.remove",
            "profile": name,
            "removed": true,
        }))?;
    } else {
        println!("Removed profile `{name}`.");
    }

    Ok(())
}

fn cmd_init(force: bool, output: CliOutput) -> Result<()> {
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
