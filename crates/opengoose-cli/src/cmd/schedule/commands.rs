use crate::error::{CliError, CliResult};

use opengoose_persistence::ScheduleStore;

use super::logic;

pub(super) fn add(
    store: &ScheduleStore,
    team_store: &opengoose_teams::TeamStore,
    name: &str,
    cron_expr: &str,
    team: &str,
    input: &str,
) -> CliResult<()> {
    logic::validate_cron_expression(cron_expr)?;
    logic::ensure_team_exists(team_store, team)?;

    let next = logic::next_run_at(cron_expr);
    let schedule = store.create(name, cron_expr, team, input, next.as_deref())?;

    println!("Created schedule '{}'.", schedule.name);
    println!("  Team: {}", schedule.team_name);
    println!("  Cron: {}", schedule.cron_expression);
    if let Some(ref next_run) = schedule.next_run_at {
        println!("  Next run: {next_run}");
    }

    Ok(())
}

pub(super) fn list(store: &ScheduleStore) -> CliResult<()> {
    let schedules = store.list()?;

    if schedules.is_empty() {
        println!("No schedules found. Use `opengoose schedule add` to create one.");
        return Ok(());
    }

    println!(
        "{:<20} {:<10} {:<20} {:<20} {:<20}",
        "NAME", "ENABLED", "CRON", "TEAM", "NEXT RUN"
    );
    for schedule in &schedules {
        let enabled = if schedule.enabled { "yes" } else { "no" };
        let next = schedule.next_run_at.as_deref().unwrap_or("-");
        println!(
            "{:<20} {:<10} {:<20} {:<20} {:<20}",
            schedule.name, enabled, schedule.cron_expression, schedule.team_name, next
        );
    }

    Ok(())
}

pub(super) fn remove(store: &ScheduleStore, name: &str) -> CliResult<()> {
    if store.remove(name)? {
        println!("Removed schedule '{name}'.");
    } else {
        return Err(CliError::Validation(format!(
            "schedule '{name}' not found."
        )));
    }

    Ok(())
}

pub(super) fn enable(store: &ScheduleStore, name: &str) -> CliResult<()> {
    if let Some(schedule) = store.get_by_name(name)? {
        let next = logic::next_run_at(&schedule.cron_expression);
        store.mark_run(name, next.as_deref())?;
    }

    if store.set_enabled(name, true)? {
        println!("Enabled schedule '{name}'.");
    } else {
        return Err(CliError::Validation(format!(
            "schedule '{name}' not found."
        )));
    }

    Ok(())
}

pub(super) fn disable(store: &ScheduleStore, name: &str) -> CliResult<()> {
    if store.set_enabled(name, false)? {
        println!("Disabled schedule '{name}'.");
    } else {
        return Err(CliError::Validation(format!(
            "schedule '{name}' not found."
        )));
    }

    Ok(())
}

pub(super) fn status(store: &ScheduleStore, name: &str) -> CliResult<()> {
    let schedule = store
        .get_by_name(name)?
        .ok_or_else(|| CliError::Validation(format!("schedule '{}' not found", name)))?;

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
        println!("  Input: {}", logic::preview_input(&schedule.input));
    }
    println!("  Created: {}", schedule.created_at);
    println!("  Updated: {}", schedule.updated_at);

    Ok(())
}
