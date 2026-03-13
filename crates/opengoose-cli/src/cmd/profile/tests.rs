use std::path::PathBuf;

use tempfile::TempDir;

use super::set::apply_profile_updates;
use super::*;
use opengoose_profiles::{AgentProfile, ProfileSettings, ProfileStore};

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

    let retention = apply_profile_updates(&mut profile, Some(30), false, Some(14), false).unwrap();

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
fn agent_profile_from_yaml_missing_version_is_migrated() {
    // Migration backfills the version field when it is absent (pre-1.0.0 format).
    let yaml = "title: researcher\n";
    let profile = AgentProfile::from_yaml(yaml).unwrap();
    assert_eq!(profile.version, opengoose_profiles::CURRENT_VERSION);
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

#[test]
fn cmd_add_nonexistent_path_returns_error() {
    let path = PathBuf::from("/nonexistent/path/to/profile.yaml");
    let err = add::run(&path, false, text_output()).unwrap_err();
    assert!(err.to_string().contains("not found") || err.to_string().contains("nonexistent"));
}

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

#[test]
fn cmd_list_empty_store_with_json_output_succeeds() {
    let dir = TempDir::new().unwrap();
    let store = ProfileStore::with_dir(dir.path().to_path_buf());
    let names = store.list().unwrap();
    assert!(names.is_empty());
}
