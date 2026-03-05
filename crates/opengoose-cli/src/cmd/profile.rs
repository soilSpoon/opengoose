use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_profiles::{AgentProfile, ProfileStore};

#[derive(Subcommand)]
pub enum ProfileAction {
    /// List all agent profiles
    List,
    /// Show a profile's full YAML
    Show {
        /// Profile name (e.g. researcher)
        name: String,
    },
    /// Add a profile from a YAML file
    Add {
        /// Path to the YAML file
        path: PathBuf,
        /// Overwrite if the profile already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a profile
    Remove {
        /// Profile name (e.g. researcher)
        name: String,
    },
    /// Install bundled default profiles
    Init {
        /// Overwrite existing profiles
        #[arg(long)]
        force: bool,
    },
}

pub fn execute(action: ProfileAction) -> Result<()> {
    match action {
        ProfileAction::List => cmd_list(),
        ProfileAction::Show { name } => cmd_show(&name),
        ProfileAction::Add { path, force } => cmd_add(&path, force),
        ProfileAction::Remove { name } => cmd_remove(&name),
        ProfileAction::Init { force } => cmd_init(force),
    }
}

fn cmd_list() -> Result<()> {
    let store = ProfileStore::new()?;
    let names = store.list()?;

    if names.is_empty() {
        println!("No profiles found. Use `opengoose profile init` to install defaults.");
        return Ok(());
    }

    println!("Agent profiles:");
    for name in &names {
        let profile = store.get(name)?;
        let desc = profile.description.as_deref().unwrap_or("(no description)");
        println!("  {:<16} {}", name, desc);
    }
    Ok(())
}

fn cmd_show(name: &str) -> Result<()> {
    let store = ProfileStore::new()?;
    let profile = store.get(name)?;
    let yaml = profile.to_yaml()?;
    print!("{yaml}");
    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let profile = AgentProfile::from_yaml(&content)?;
    let name = profile.title.clone();

    let store = ProfileStore::new()?;
    store.save(&profile, force)?;

    println!("Added profile `{name}`.");
    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let store = ProfileStore::new()?;
    store.remove(name)?;
    println!("Removed profile `{name}`.");
    Ok(())
}

fn cmd_init(force: bool) -> Result<()> {
    let store = ProfileStore::new()?;
    let count = store.install_defaults(force)?;

    if count == 0 {
        println!("All default profiles already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default profile(s).");
    }
    Ok(())
}
