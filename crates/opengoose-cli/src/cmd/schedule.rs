use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_persistence::{Database, ScheduleStore};
use opengoose_teams::scheduler;

#[derive(Subcommand)]
pub enum ScheduleAction {
    /// Add a new cron schedule
    Add {
        /// Unique name for this schedule
        name: String,
        /// Cron expression (6-field: sec min hour day month weekday)
        #[arg(long)]
        cron: String,
        /// Team name to run
        #[arg(long)]
        team: String,
        /// Input text for the team (optional)
        #[arg(long, default_value = "")]
        input: String,
    },
    /// List all schedules
    List,
    /// Remove a schedule
    Remove {
        /// Schedule name
        name: String,
    },
    /// Enable a schedule
    Enable {
        /// Schedule name
        name: String,
    },
    /// Disable a schedule
    Disable {
        /// Schedule name
        name: String,
    },
    /// Show status of a specific schedule
    Status {
        /// Schedule name
        name: String,
    },
}

pub fn execute(action: ScheduleAction) -> Result<()> {
    match action {
        ScheduleAction::Add {
            name,
            cron,
            team,
            input,
        } => cmd_add(&name, &cron, &team, &input),
        ScheduleAction::List => cmd_list(),
        ScheduleAction::Remove { name } => cmd_remove(&name),
        ScheduleAction::Enable { name } => cmd_enable(&name),
        ScheduleAction::Disable { name } => cmd_disable(&name),
        ScheduleAction::Status { name } => cmd_status(&name),
    }
}

fn cmd_add(name: &str, cron_expr: &str, team: &str, input: &str) -> Result<()> {
    // Validate the cron expression
    scheduler::validate_cron(cron_expr).map_err(|e| anyhow::anyhow!(e))?;

    // Verify team exists
    let team_store = opengoose_teams::TeamStore::new()?;
    if team_store.get(team).is_err() {
        bail!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        );
    }

    let db = Arc::new(Database::open()?);
    let store = ScheduleStore::new(db);

    let next = scheduler::next_fire_time(cron_expr);
    let schedule = store.create(name, cron_expr, team, input, next.as_deref())?;

    println!("Created schedule '{}'.", schedule.name);
    println!("  Team: {}", schedule.team_name);
    println!("  Cron: {}", schedule.cron_expression);
    if let Some(ref next_run) = schedule.next_run_at {
        println!("  Next run: {next_run}");
    }

    Ok(())
}

fn cmd_list() -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = ScheduleStore::new(db);
    let schedules = store.list()?;

    if schedules.is_empty() {
        println!("No schedules found. Use `opengoose schedule add` to create one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<20} {:<20} {:<20}",
        "NAME", "ENABLED", "CRON", "TEAM", "NEXT RUN"
    );
    for s in &schedules {
        let enabled = if s.enabled { "yes" } else { "no" };
        let next = s.next_run_at.as_deref().unwrap_or("-");
        println!(
            "{:<20} {:<10} {:<20} {:<20} {:<20}",
            s.name, enabled, s.cron_expression, s.team_name, next
        );
    }

    Ok(())
}

fn cmd_remove(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = ScheduleStore::new(db);

    if store.remove(name)? {
        println!("Removed schedule '{name}'.");
    } else {
        bail!("schedule '{name}' not found.");
    }

    Ok(())
}

fn cmd_enable(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = ScheduleStore::new(db);

    // Recompute next_run_at when enabling
    if let Some(schedule) = store.get_by_name(name)? {
        let next = scheduler::next_fire_time(&schedule.cron_expression);
        store.mark_run(name, next.as_deref())?;
    }

    if store.set_enabled(name, true)? {
        println!("Enabled schedule '{name}'.");
    } else {
        bail!("schedule '{name}' not found.");
    }

    Ok(())
}

fn cmd_disable(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = ScheduleStore::new(db);

    if store.set_enabled(name, false)? {
        println!("Disabled schedule '{name}'.");
    } else {
        bail!("schedule '{name}' not found.");
    }

    Ok(())
}

fn cmd_status(name: &str) -> Result<()> {
    let db = Arc::new(Database::open()?);
    let store = ScheduleStore::new(db);

    let schedule = store
        .get_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("schedule '{}' not found", name))?;

    println!("Schedule: {}", schedule.name);
    println!("  Team: {}", schedule.team_name);
    println!("  Cron: {}", schedule.cron_expression);
    println!("  Enabled: {}", if schedule.enabled { "yes" } else { "no" });
    println!(
        "  Last run: {}",
        schedule.last_run_at.as_deref().unwrap_or("never")
    );
    println!(
        "  Next run: {}",
        schedule.next_run_at.as_deref().unwrap_or("-")
    );
    if !schedule.input.is_empty() {
        let preview = if schedule.input.len() > 100 {
            format!(
                "{}...",
                &schedule.input[..schedule.input.floor_char_boundary(100)]
            )
        } else {
            schedule.input.clone()
        };
        println!("  Input: {preview}");
    }
    println!("  Created: {}", schedule.created_at);
    println!("  Updated: {}", schedule.updated_at);

    Ok(())
}
