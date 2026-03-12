use anyhow::{Result, bail};
use serde_json::json;

use crate::cmd::output::CliOutput;
use opengoose_profiles::{AgentProfile, ProfileSettings, ProfileStore};

pub(super) fn run(
    name: &str,
    message_retention_days: Option<u32>,
    clear_message_retention_days: bool,
    event_retention_days: Option<u32>,
    clear_event_retention_days: bool,
    output: CliOutput,
) -> Result<()> {
    let store = ProfileStore::new()?;
    let mut profile = store.get(name)?;
    let (message_retention_days, event_retention_days) = apply_profile_updates(
        &mut profile,
        message_retention_days,
        clear_message_retention_days,
        event_retention_days,
        clear_event_retention_days,
    )?;
    store.save(&profile, true)?;

    if output.is_json() {
        output.print_json(&json!({
            "ok": true,
            "command": "profile.set",
            "profile": name,
            "message_retention_days": message_retention_days,
            "event_retention_days": event_retention_days,
        }))?;
    } else {
        println!("Updated profile `{name}`.");
        match message_retention_days {
            Some(days) => println!("  Message retention: {days} day(s)"),
            None => println!("  Message retention: forever"),
        }
        match event_retention_days {
            Some(days) => println!("  Event retention: {days} day(s)"),
            None => println!("  Event retention: runtime default"),
        }
    }

    Ok(())
}

pub(super) fn apply_profile_updates(
    profile: &mut AgentProfile,
    message_retention_days: Option<u32>,
    clear_message_retention_days: bool,
    event_retention_days: Option<u32>,
    clear_event_retention_days: bool,
) -> Result<(Option<u32>, Option<u32>)> {
    if message_retention_days.is_none()
        && !clear_message_retention_days
        && event_retention_days.is_none()
        && !clear_event_retention_days
    {
        bail!(
            "no settings specified. Pass `--message-retention-days <N>`, `--event-retention-days <N>`, or the corresponding clear flag."
        );
    }

    if let Some(days) = message_retention_days {
        let settings = profile
            .settings
            .get_or_insert_with(ProfileSettings::default);
        settings.message_retention_days = Some(days);
    }

    if clear_message_retention_days && let Some(settings) = profile.settings.as_mut() {
        settings.message_retention_days = None;
    }

    if let Some(days) = event_retention_days {
        let settings = profile
            .settings
            .get_or_insert_with(ProfileSettings::default);
        settings.event_retention_days = Some(days);
    }

    if clear_event_retention_days && let Some(settings) = profile.settings.as_mut() {
        settings.event_retention_days = None;
    }

    if profile
        .settings
        .as_ref()
        .is_some_and(ProfileSettings::is_empty)
    {
        profile.settings = None;
    }

    Ok((
        profile
            .settings
            .as_ref()
            .and_then(|settings| settings.message_retention_days),
        profile
            .settings
            .as_ref()
            .and_then(|settings| settings.event_retention_days),
    ))
}
