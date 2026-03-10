use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_profiles::{Skill, SkillStore};

#[derive(Subcommand)]
pub enum SkillAction {
    /// List all installed skills
    List,
    /// Show a skill's full YAML
    Show {
        /// Skill name (e.g. git-tools)
        name: String,
    },
    /// Install a skill from a YAML file
    Add {
        /// Path to the skill YAML file
        path: PathBuf,
        /// Overwrite if the skill already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove a skill
    Remove {
        /// Skill name (e.g. git-tools)
        name: String,
    },
    /// Install bundled default skills
    Init {
        /// Overwrite existing skills
        #[arg(long)]
        force: bool,
    },
}

pub fn execute(action: SkillAction) -> Result<()> {
    match action {
        SkillAction::List => cmd_list(),
        SkillAction::Show { name } => cmd_show(&name),
        SkillAction::Add { path, force } => cmd_add(&path, force),
        SkillAction::Remove { name } => cmd_remove(&name),
        SkillAction::Init { force } => cmd_init(force),
    }
}

fn cmd_list() -> Result<()> {
    let store = SkillStore::new()?;
    let names = store.list()?;

    if names.is_empty() {
        println!("No skills found. Use `opengoose skill init` to install defaults.");
        return Ok(());
    }

    println!("Skills:");
    for name in &names {
        let skill = store.get(name)?;
        let desc = skill.description.as_deref().unwrap_or("(no description)");
        let ext_count = skill.extensions.len();
        println!("  {:<20} [{} extension(s)] {}", name, ext_count, desc);
    }
    Ok(())
}

fn cmd_show(name: &str) -> Result<()> {
    let store = SkillStore::new()?;
    let skill = store.get(name)?;
    let yaml = skill.to_yaml()?;
    print!("{yaml}");
    Ok(())
}

fn cmd_add(path: &PathBuf, force: bool) -> Result<()> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let content = std::fs::read_to_string(path)?;
    let skill = Skill::from_yaml(&content)?;
    let name = skill.name.clone();

    let store = SkillStore::new()?;
    store.save(&skill, force)?;

    println!("Added skill `{name}`.");
    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let store = SkillStore::new()?;
    store.remove(name)?;
    println!("Removed skill `{name}`.");
    Ok(())
}

fn cmd_init(force: bool) -> Result<()> {
    let store = SkillStore::new()?;
    let count = store.install_defaults(force)?;

    if count == 0 {
        println!("All default skills already exist. Use --force to overwrite.");
    } else {
        println!("Installed {count} default skill(s).");
    }
    Ok(())
}
