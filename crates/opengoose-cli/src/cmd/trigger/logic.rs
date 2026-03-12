use anyhow::{Result, bail};

use opengoose_persistence::Trigger;
use opengoose_teams::triggers as trigger_types;

pub(super) fn validate_add_request(
    team_store: &opengoose_teams::TeamStore,
    trigger_type: &str,
    condition: &str,
    team: &str,
) -> Result<()> {
    validate_trigger_type(trigger_type)?;
    validate_condition_json(condition)?;
    ensure_team_exists(team_store, team)?;
    Ok(())
}

pub(super) fn validate_trigger_type(trigger_type: &str) -> Result<()> {
    trigger_types::validate_trigger_type(trigger_type)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!(err))
}

pub(super) fn validate_condition_json(condition: &str) -> Result<()> {
    serde_json::from_str::<serde_json::Value>(condition)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("invalid condition JSON: {err}"))
}

pub(super) fn ensure_team_exists(
    team_store: &opengoose_teams::TeamStore,
    team: &str,
) -> Result<()> {
    if team_store.get(team).is_err() {
        bail!(
            "team '{}' not found. Use `opengoose team list` to see available teams.",
            team
        );
    }

    Ok(())
}

pub(super) fn print_list(triggers: &[Trigger]) {
    if triggers.is_empty() {
        println!("No triggers found. Use `opengoose trigger add` to create one.");
        return;
    }

    println!(
        "{:<20} {:<10} {:<20} {:<20} {:<10}",
        "NAME", "ENABLED", "TYPE", "TEAM", "FIRED"
    );
    for trigger in triggers {
        let enabled = if trigger.enabled { "yes" } else { "no" };
        println!(
            "{:<20} {:<10} {:<20} {:<20} {:<10}",
            trigger.name, enabled, trigger.trigger_type, trigger.team_name, trigger.fire_count
        );
    }
}

pub(super) fn print_status(trigger: &Trigger) {
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
        println!("  Input: {}", preview_input(&trigger.input));
    }
    println!("  Created: {}", trigger.created_at);
    println!("  Updated: {}", trigger.updated_at);
}

pub(super) fn preview_input(input: &str) -> String {
    if input.len() > 100 {
        format!("{}...", &input[..input.floor_char_boundary(100)])
    } else {
        input.to_owned()
    }
}
