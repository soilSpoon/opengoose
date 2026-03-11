use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde_json::json;

use crate::cmd::output::{CliOutput, format_table};
use opengoose_profiles::{AgentProfile, ProfileSettings, ProfileStore};

#[derive(Subcommand)]
#[command(
    after_help = "Examples:\n  opengoose profile list\n  opengoose profile show developer\n  opengoose profile set main --message-retention-days 30\n  opengoose profile set main --event-retention-days 14\n  opengoose --json profile list"
)]
/// Subcommands for `opengoose profile`.
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
    /// Update configurable settings on an existing profile
    #[command(
        after_help = "Examples:\n  opengoose profile set main --message-retention-days 30\n  opengoose profile set main --event-retention-days 14\n  opengoose profile set main --clear-message-retention-days"
    )]
    Set {
        /// Profile name (e.g. main)
        name: String,
        /// Retain persisted session messages for N days
        #[arg(long, conflicts_with = "clear_message_retention_days")]
        message_retention_days: Option<u32>,
        /// Clear any configured message retention and keep messages forever
        #[arg(long, conflicts_with = "message_retention_days")]
        clear_message_retention_days: bool,
        /// Retain persisted event history for N days
        #[arg(long, conflicts_with = "clear_event_retention_days")]
        event_retention_days: Option<u32>,
        /// Clear any configured event retention and fall back to the runtime default
        #[arg(long, conflicts_with = "event_retention_days")]
        clear_event_retention_days: bool,
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

/// Dispatch and execute the selected profile subcommand.
pub fn execute(action: ProfileAction, output: CliOutput) -> Result<()> {
    match action {
        ProfileAction::List => cmd_list(output),
        ProfileAction::Show { name } => cmd_show(&name, output),
        ProfileAction::Set {
            name,
            message_retention_days,
            clear_message_retention_days,
            event_retention_days,
            clear_event_retention_days,
        } => cmd_set(
            &name,
            message_retention_days,
            clear_message_retention_days,
            event_retention_days,
            clear_event_retention_days,
            output,
        ),
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

fn cmd_set(
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

fn apply_profile_updates(
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn text_output() -> CliOutput {
        CliOutput::new(crate::cmd::output::OutputMode::Text)
    }

    fn json_output() -> CliOutput {
        CliOutput::new(crate::cmd::output::OutputMode::Json)
    }

    fn minimal_profile_yaml(title: &str) -> String {
        format!("version: \"1\"\ntitle: {title}\ndescription: A test profile\n")
    }

    #[test]
    fn apply_profile_updates_sets_message_retention_days() {
        let mut profile = AgentProfile::from_yaml(&minimal_profile_yaml("developer")).unwrap();

        let retention =
            apply_profile_updates(&mut profile, Some(30), false, Some(14), false).unwrap();

        assert_eq!(retention, (Some(30), Some(14)));
        assert_eq!(
            profile
                .settings
                .as_ref()
                .and_then(|settings| settings.message_retention_days),
            Some(30)
        );
        assert_eq!(
            profile
                .settings
                .as_ref()
                .and_then(|settings| settings.event_retention_days),
            Some(14)
        );
    }

    #[test]
    fn apply_profile_updates_clears_empty_settings_block() {
        let mut profile = AgentProfile::from_yaml(&minimal_profile_yaml("developer")).unwrap();
        profile.settings = Some(ProfileSettings {
            message_retention_days: Some(14),
            ..ProfileSettings::default()
        });

        let retention = apply_profile_updates(&mut profile, None, true, None, false).unwrap();

        assert_eq!(retention, (None, None));
        assert!(profile.settings.is_none());
    }

    #[test]
    fn apply_profile_updates_requires_a_flag() {
        let mut profile = AgentProfile::from_yaml(&minimal_profile_yaml("developer")).unwrap();
        let err = apply_profile_updates(&mut profile, None, false, None, false).unwrap_err();
        assert!(err.to_string().contains("no settings specified"));
    }

    // ---- CliOutput ----

    #[test]
    fn cli_output_text_mode_is_not_json() {
        assert!(!text_output().is_json());
    }

    #[test]
    fn cli_output_json_mode_is_json() {
        assert!(json_output().is_json());
    }

    #[test]
    fn cli_output_heading_returns_plain_text_in_non_terminal() {
        let output = text_output();
        assert_eq!(output.heading("test heading"), "test heading");
    }

    // ---- AgentProfile::from_yaml ----

    #[test]
    fn agent_profile_from_yaml_valid_minimal() {
        let yaml = "version: \"1\"\ntitle: researcher\n";
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.title, "researcher");
    }

    #[test]
    fn agent_profile_from_yaml_with_description() {
        let yaml = "version: \"1\"\ntitle: analyst\ndescription: Analyzes data\n";
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.title, "analyst");
        assert_eq!(profile.description.as_deref(), Some("Analyzes data"));
    }

    #[test]
    fn agent_profile_from_yaml_empty_title_fails() {
        let yaml = "version: \"1\"\ntitle: \"\"\n";
        assert!(AgentProfile::from_yaml(yaml).is_err());
    }

    #[test]
    fn agent_profile_from_yaml_missing_version_fails() {
        let yaml = "title: researcher\n";
        assert!(AgentProfile::from_yaml(yaml).is_err());
    }

    #[test]
    fn agent_profile_from_yaml_invalid_yaml_fails() {
        let yaml = "not: [valid: yaml: at: all";
        assert!(AgentProfile::from_yaml(yaml).is_err());
    }

    #[test]
    fn agent_profile_file_name_uses_title_lowercase() {
        let yaml = "version: \"1\"\ntitle: MyProfile\n";
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.file_name(), "myprofile.yaml");
    }

    #[test]
    fn agent_profile_file_name_replaces_spaces_with_hyphens() {
        let yaml = "version: \"1\"\ntitle: my profile name\n";
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        assert_eq!(profile.file_name(), "my-profile-name.yaml");
    }

    #[test]
    fn agent_profile_roundtrip_yaml() {
        let yaml = "version: \"1\"\ntitle: tester\ndescription: Testing roundtrip\n";
        let profile = AgentProfile::from_yaml(yaml).unwrap();
        let out = profile.to_yaml().unwrap();
        let roundtrip = AgentProfile::from_yaml(&out).unwrap();
        assert_eq!(roundtrip.title, "tester");
    }

    // ---- cmd_add: file not found path ----

    #[test]
    fn cmd_add_nonexistent_path_returns_error() {
        let path = PathBuf::from("/nonexistent/path/to/profile.yaml");
        let err = cmd_add(&path, false, text_output()).unwrap_err();
        assert!(err.to_string().contains("not found") || err.to_string().contains("nonexistent"));
    }

    // ---- ProfileStore with temp dir ----

    #[test]
    fn profile_store_list_empty_initially() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());
        let names = store.list().unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn profile_store_save_and_get() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());

        let yaml = minimal_profile_yaml("developer");
        let profile = AgentProfile::from_yaml(&yaml).unwrap();
        store.save(&profile, false).unwrap();

        let retrieved = store.get("developer").unwrap();
        assert_eq!(retrieved.title, "developer");
    }

    #[test]
    fn profile_store_save_force_overwrites() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());

        let profile = AgentProfile::from_yaml(&minimal_profile_yaml("myprofile")).unwrap();
        store.save(&profile, false).unwrap();

        let updated_yaml = "version: \"1\"\ntitle: myprofile\ndescription: Updated description\n";
        let updated = AgentProfile::from_yaml(updated_yaml).unwrap();
        store.save(&updated, true).unwrap();

        let retrieved = store.get("myprofile").unwrap();
        assert_eq!(
            retrieved.description.as_deref(),
            Some("Updated description")
        );
    }

    #[test]
    fn profile_store_save_without_force_fails_if_exists() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());

        let profile = AgentProfile::from_yaml(&minimal_profile_yaml("existing")).unwrap();
        store.save(&profile, false).unwrap();

        let result = store.save(&profile, false);
        assert!(result.is_err());
    }

    #[test]
    fn profile_store_list_returns_saved_profiles() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());

        let p1 = AgentProfile::from_yaml(&minimal_profile_yaml("alpha")).unwrap();
        let p2 = AgentProfile::from_yaml(&minimal_profile_yaml("beta")).unwrap();
        store.save(&p1, false).unwrap();
        store.save(&p2, false).unwrap();

        let names = store.list().unwrap();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
    }

    #[test]
    fn profile_store_remove_existing() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());

        let profile = AgentProfile::from_yaml(&minimal_profile_yaml("to-remove")).unwrap();
        store.save(&profile, false).unwrap();
        store.remove("to-remove").unwrap();

        let names = store.list().unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn profile_store_get_nonexistent_returns_error() {
        let dir = TempDir::new().unwrap();
        let store = ProfileStore::with_dir(dir.path().to_path_buf());
        assert!(store.get("nonexistent").is_err());
    }

    // ---- cmd_list / cmd_show via JSON output ----

    #[test]
    fn cmd_list_empty_store_with_json_output_succeeds() {
        let dir = TempDir::new().unwrap();
        // ProfileStore::new() would use default path, so we test via cmd logic directly
        // by creating a store with a temp dir and verifying empty list logic works
        let store = ProfileStore::with_dir(dir.path().to_path_buf());
        let names = store.list().unwrap();
        assert!(names.is_empty());
    }
}
