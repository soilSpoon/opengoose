use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_teams::{TeamDefinition, TeamStore};

#[derive(Subcommand)]
pub enum TeamAction {
    /// List all team definitions
    List,
    /// Show a team's full YAML
    Show {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Add a team from a YAML file
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the team already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a team
    Remove {
        /// Team name (e.g. code-review)
        name: String,
    },
    /// Install bundled default teams
    Init {
        /// Overwrite existing teams
        #[arg(long)]
        force: bool,
    },
}

pub fn execute(action: TeamAction) -> Result<()> {
    match action {
        TeamAction::List => cmd_list(),
        TeamAction::Show { name } => cmd_show(&name),
        TeamAction::Add { path, force } => cmd_add(&path, force),
        TeamAction::Remove { name } => cmd_remove(&name),
        TeamAction::Init { force } => cmd_init(force),
    }
}

fn cmd_list() -> Result<()> {
    let store = TeamStore::new()?;
    let names = store.list()?;

    if names.is_empty() {
        println!("No teams found. Use `opengoose team init` to install defaults.");
        return Ok(());
    }

    println!("Teams:");
    for name in &names {
        let team = store.get(name)?;
        let desc = team.description.as_deref().unwrap_or("(no description)");
        let workflow = format!("{:?}", team.workflow).to_lowercase();
        println!("  {:<20} [{:<8}] {}", name, workflow, desc);
    }
    Ok(())
}

fn cmd_show(name: &str) -> Result<()> {
    let store = TeamStore::new()?;
    let team = store.get(name)?;
    let yaml = team.to_yaml()?;
    print!("{yaml}");
    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let team = TeamDefinition::from_yaml(&content)?;
    let name = team.title.clone();

    let store = TeamStore::new()?;
    store.save(&team, force)?;

    println!("Added team `{name}`.");
    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let store = TeamStore::new()?;
    store.remove(name)?;
    println!("Removed team `{name}`.");
    Ok(())
}

fn cmd_init(force: bool) -> Result<()> {
    let store = TeamStore::new()?;
    let count = store.install_defaults(force)?;

    if count == 0 {
        println!("All default teams already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default team(s).");
    }
    Ok(())
}
