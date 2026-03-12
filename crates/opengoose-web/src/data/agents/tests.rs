use std::collections::HashMap;

use opengoose_profiles::{AgentProfile, ExtensionRef, ProfileSettings};

use super::catalog::{ProfileCatalogEntry, build_agent_list_items, catalog_mode};
use super::detail::{build_agent_detail, capability_line, profile_settings};
use super::selection::find_selected_entry;
use super::*;

fn minimal_profile(title: &str) -> AgentProfile {
    AgentProfile {
        version: "1.0.0".into(),
        title: title.into(),
        description: None,
        instructions: None,
        prompt: None,
        extensions: vec![],
        skills: vec![],
        settings: None,
        activities: None,
        response: None,
        sub_recipes: None,
        parameters: None,
    }
}

fn catalog_entry(title: &str, source_label: &str, is_live: bool) -> ProfileCatalogEntry {
    ProfileCatalogEntry {
        profile: minimal_profile(title),
        source_label: source_label.into(),
        is_live,
    }
}

// --- capability_line ---

#[test]
fn capability_line_with_provider_and_model() {
    let mut profile = minimal_profile("test-agent");
    profile.settings = Some(ProfileSettings {
        goose_provider: Some("anthropic".into()),
        goose_model: Some("claude-4".into()),
        ..ProfileSettings::default()
    });
    let line = capability_line(&profile);
    assert_eq!(line, "anthropic / claude-4");
}

#[test]
fn capability_line_no_settings_shows_unset() {
    let profile = minimal_profile("test-agent");
    let line = capability_line(&profile);
    assert_eq!(line, "provider unset / model unset");
}

#[test]
fn capability_line_provider_only() {
    let mut profile = minimal_profile("test-agent");
    profile.settings = Some(ProfileSettings {
        goose_provider: Some("openai".into()),
        ..ProfileSettings::default()
    });
    let line = capability_line(&profile);
    assert!(line.starts_with("openai / "));
    assert!(line.contains("model unset"));
}

// --- profile_settings ---

#[test]
fn profile_settings_no_settings_returns_placeholder_row() {
    let profile = minimal_profile("test-agent");
    let rows = profile_settings(&profile);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Settings");
    assert!(rows[0].value.contains("No explicit settings"));
}

#[test]
fn profile_settings_with_provider_and_model() {
    let mut profile = minimal_profile("test-agent");
    profile.settings = Some(ProfileSettings {
        goose_provider: Some("anthropic".into()),
        goose_model: Some("claude-4".into()),
        ..ProfileSettings::default()
    });
    let rows = profile_settings(&profile);
    let labels: Vec<_> = rows.iter().map(|row| row.label.as_str()).collect();
    assert!(labels.contains(&"Provider"));
    assert!(labels.contains(&"Model"));
}

#[test]
fn profile_settings_with_temperature() {
    let mut profile = minimal_profile("test-agent");
    profile.settings = Some(ProfileSettings {
        temperature: Some(0.7),
        ..ProfileSettings::default()
    });
    let rows = profile_settings(&profile);
    let temp_row = rows.iter().find(|row| row.label == "Temperature");
    assert!(temp_row.is_some());
    assert!(temp_row.unwrap().value.contains("0.7"));
}

#[test]
fn profile_settings_with_max_turns_and_retries() {
    let mut profile = minimal_profile("test-agent");
    profile.settings = Some(ProfileSettings {
        max_turns: Some(10),
        max_retries: Some(3),
        ..ProfileSettings::default()
    });
    let rows = profile_settings(&profile);
    let labels: Vec<_> = rows.iter().map(|row| row.label.as_str()).collect();
    assert!(labels.contains(&"Max turns"));
    assert!(labels.contains(&"Retries"));
}

#[test]
fn profile_settings_empty_settings_block_returns_placeholder() {
    let mut profile = minimal_profile("test-agent");
    profile.settings = Some(ProfileSettings::default());
    let rows = profile_settings(&profile);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].label, "Settings");
    assert!(rows[0].value.contains("No explicit settings"));
}

// --- selection ---

#[test]
fn find_selected_entry_falls_back_to_first_agent() {
    let entries = vec![
        catalog_entry("alpha", "Bundled default", false),
        catalog_entry("beta", "Bundled default", false),
    ];

    let selected = find_selected_entry(&entries, Some("missing".into())).unwrap();

    assert_eq!(selected.profile.title, "alpha");
}

// --- catalog ---

#[test]
fn catalog_mode_uses_bundled_label_when_all_entries_are_defaults() {
    let entries = vec![
        catalog_entry("alpha", "Bundled default", false),
        catalog_entry("beta", "Bundled default", false),
    ];

    let mode = catalog_mode(&entries);

    assert_eq!(mode.label, "Bundled defaults");
    assert_eq!(mode.tone, "neutral");
}

#[test]
fn build_agent_list_items_marks_selected_entry_active() {
    let entries = vec![
        catalog_entry("alpha", "Bundled default", false),
        catalog_entry("beta", "/tmp/profiles/beta.yaml", true),
    ];

    let items = build_agent_list_items(&entries, "beta");

    assert!(!items[0].active);
    assert!(items[1].active);
    assert_eq!(items[1].source_label, "/tmp/profiles/beta.yaml");
    assert!(items[1].page_url.contains("agent=beta"));
}

// --- build_agent_detail ---

#[test]
fn build_agent_detail_uses_prompt_preview_and_extension_summaries() {
    let mut profile = minimal_profile("ops");
    profile.prompt = Some("You are the ops fallback.".into());
    profile.description = Some("Keeps the lights on".into());
    profile.activities = Some(vec!["triage".into()]);
    profile.skills = vec!["pager".into()];
    profile.extensions = vec![
        ExtensionRef {
            name: "stdio-ext".into(),
            ext_type: "stdio".into(),
            cmd: Some("uvx tool".into()),
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "http-ext".into(),
            ext_type: "streamable_http".into(),
            cmd: None,
            args: vec![],
            uri: Some("https://example.com/mcp".into()),
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
        ExtensionRef {
            name: "py-ext".into(),
            ext_type: "inline_python".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: Some("print('hello')".into()),
            dependencies: None,
        },
        ExtensionRef {
            name: "empty-ext".into(),
            ext_type: "builtin".into(),
            cmd: None,
            args: vec![],
            uri: None,
            timeout: None,
            envs: HashMap::new(),
            env_keys: vec![],
            code: None,
            dependencies: None,
        },
    ];
    let entry = ProfileCatalogEntry {
        profile,
        source_label: "/tmp/profiles/ops.yaml".into(),
        is_live: true,
    };

    let detail = build_agent_detail(&entry).unwrap();

    assert_eq!(detail.title, "ops");
    assert_eq!(detail.subtitle, "Keeps the lights on");
    assert!(detail.instructions_preview.contains("ops fallback"));
    assert_eq!(detail.extensions[0].summary, "uvx tool");
    assert_eq!(detail.extensions[1].summary, "https://example.com/mcp");
    assert_eq!(detail.extensions[2].summary, "inline python");
    assert_eq!(detail.extensions[3].summary, "No runtime configuration");
    assert!(detail.yaml.contains("title: ops"));
}

// --- build_agents_page ---

#[test]
fn build_agents_page_keeps_installed_mode_and_default_selection_behavior() {
    let entries = vec![
        catalog_entry("alpha", "/tmp/profiles/alpha.yaml", true),
        catalog_entry("beta", "/tmp/profiles/beta.yaml", true),
    ];

    let view = build_agents_page(&entries, Some("missing".into())).unwrap();

    assert_eq!(view.mode_label, "Installed catalog");
    assert_eq!(view.mode_tone, "success");
    assert!(view.agents[0].active);
    assert!(!view.agents[1].active);
    assert_eq!(view.selected.title, "alpha");
    assert_eq!(view.selected.source_label, "/tmp/profiles/alpha.yaml");
}
