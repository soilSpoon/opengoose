use crate::error::{CliError, CliResult};

use opengoose_persistence::TriggerStore;

use super::logic;

pub(super) fn add(
    store: &TriggerStore,
    team_store: &opengoose_teams::TeamStore,
    name: &str,
    trigger_type: &str,
    team: &str,
    condition: &str,
    input: &str,
) -> CliResult<()> {
    logic::validate_add_request(team_store, trigger_type, condition, team)?;

    let trigger = store.create(name, trigger_type, condition, team, input)?;

    println!("Created trigger '{}'.", trigger.name);
    println!("  Type: {}", trigger.trigger_type);
    println!("  Team: {}", trigger.team_name);
    if trigger.condition_json != "{}" {
        println!("  Condition: {}", trigger.condition_json);
    }

    Ok(())
}

pub(super) fn list(store: &TriggerStore) -> CliResult<()> {
    let triggers = store.list()?;
    logic::print_list(&triggers);
    Ok(())
}

pub(super) fn remove(store: &TriggerStore, name: &str) -> CliResult<()> {
    complete_named_mutation(store.remove(name)?, name, "Removed")
}

pub(super) fn enable(store: &TriggerStore, name: &str) -> CliResult<()> {
    set_enabled(store, name, true, "Enabled")
}

pub(super) fn disable(store: &TriggerStore, name: &str) -> CliResult<()> {
    set_enabled(store, name, false, "Disabled")
}

pub(super) fn status(store: &TriggerStore, name: &str) -> CliResult<()> {
    let trigger = store
        .get_by_name(name)?
        .ok_or_else(|| CliError::Validation(format!("trigger '{}' not found", name)))?;

    logic::print_status(&trigger);
    Ok(())
}

fn set_enabled(store: &TriggerStore, name: &str, enabled: bool, verb: &str) -> CliResult<()> {
    complete_named_mutation(store.set_enabled(name, enabled)?, name, verb)
}

fn complete_named_mutation(changed: bool, name: &str, verb: &str) -> CliResult<()> {
    if changed {
        println!("{verb} trigger '{name}'.");
        Ok(())
    } else {
        return Err(CliError::Validation(format!("trigger '{name}' not found.")));
    }
}
