use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_persistence::{Database, TriggerStore};
use opengoose_teams::triggers;

#[derive(Subcommand)]
pub enum TriggerAction {
    /// Add a new event trigger
    Add {
        /// Unique name for this trigger
        name: String,
        /// Trigger type (file_watch, message_received, schedule_complete, webhook_received)
        #[arg(long, name = "type")]
        trigger_type: String,
        /// Team name to run when the trigger fires
        #[arg(long)]
        team: String,
        /// JSON condition for matching (e.g. '{"channel":"alerts"}')
        #[arg(long, default_value = "{}")]
        condition: String,
        /// Input text for the team (optional)
        #[arg(long, default_value = "")]
        input: String,
    },
    /// List all triggers
    List,
    /// Remove a trigger
    Remove {
        /// Trigger name
        name: String,
    },
    /// Enable a trigger
    Enable {
        /// Trigger name
        name: String,
    },
    /// Disable a trigger
    Disable {
        /// Trigger name
        name: String,
    },
    /// Show status of a specific trigger
    Status {
        /// Trigger name
        name: String,
    },
}

pub fn execute(action: TriggerAction) -> Result<()> {
    match action {
        TriggerAction::Add {
            name,
            trigger_type,
            team,
            condition,
            input,
        } => cmd_add(&name, &trigger_type, &team, &condition, &input),
        TriggerAction::List => cmd_list(),
        TriggerAction::Remove { name } => cmd_remove(&name),
        TriggerAction::Enable { name } => cmd_enable(&name),
        TriggerAction::Disable { name } => cmd_disable(&name),
        TriggerAction::Status { name } => cmd_status(&name),
    }
}

fn cmd_add(name: &str, trigger_type: &str, team: &str, condition: &str, input: &str) -> Result<()> {
    // Validate trigger type
    triggers::validate_trigger_type(trigger_type).map_err(|e| anyhow::anyhow!(e))?;

    // Validate condition JSON
    serde_json::from_str::<serde_json::Value>(condition)
        .map_err(|e| anyhow::anyhow!("invalid condition JSON: {e}"))?;

    // Verify team exists
    let team_store = opengoose_teams::TeamStore::new()?;
    if team_store.get(team).is_err() {
        bail!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        );
    }

    let db = Arc::new(Database::open()?);
    let store = TriggerStore::new(db);

    let trigger = store.create(name, trigger_type, condition, team, input)?;

    println!("Created trigger '{}'.", trigger.name);
    println!("  Type: {}", trigger.trigger_type);
    println!("  Team: {}", trigger.team_name);
    if trigger.condition_json != "{}" {
        println!("  Condition: {}", trigger.condition_json);
    }

    Ok(())
}

fn cmd_list() -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = TriggerStore::new(db);
    let triggers = store.list()?;

    if triggers.is_empty() {
        println!("No triggers found. Use `opengoose trigger add` to create one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<20} {:<20} {:<10}",
        "NAME", "ENABLED", "TYPE", "TEAM", "FIRED"
    );
    for t in &triggers {
        let enabled = if t.enabled { "yes" } else { "no" };
        println!(
            "{:<20} {:<10} {:<20} {:<20} {:<10}",
            t.name, enabled, t.trigger_type, t.team_name, t.fire_count
        );
    }

    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = TriggerStore::new(db);

    if store.remove(name)? {
        println!("Removed trigger '{name}'.");
    } else {
        bail!("trigger '{name}' not found.");
    }

    Ok(())
}

fn cmd_enable(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = TriggerStore::new(db);

    if store.set_enabled(name, true)? {
        println!("Enabled trigger '{name}'.");
    } else {
        bail!("trigger '{name}' not found.");
    }

    Ok(())
}

fn cmd_disable(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = TriggerStore::new(db);

    if store.set_enabled(name, false)? {
        println!("Disabled trigger '{name}'.");
    } else {
        bail!("trigger '{name}' not found.");
    }

    Ok(())
}

fn cmd_status(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = TriggerStore::new(db);

    let trigger = store
        .get_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("trigger '{}' not found", name))?;

    println!("Trigger: {}", trigger.name);
    println!("  Type: {}", trigger.trigger_type);
    println!("  Team: {}", trigger.team_name);
    println!("  Enabled: {}", if trigger.enabled { "yes" } else { "no" });
    println!("  Fire count: {}", trigger.fire_count);
    println!(
        "  Last fired: {}",
        trigger.last_fired_at.as_deref().unwrap_or("never")
    );
    if trigger.condition_json != "{}" {
        println!("  Condition: {}", trigger.condition_json);
    }
    if !trigger.input.is_empty() {
        let preview = if trigger.input.len() > 100 {
            format!(
                "{}...",
                &trigger.input[..trigger.input.floor_char_boundary(100)]
            )
        } else {
            trigger.input.clone()
        };
        println!("  Input: {preview}");
    }
    println!("  Created: {}", trigger.created_at);
    println!("  Updated: {}", trigger.updated_at);

    Ok(())
}
