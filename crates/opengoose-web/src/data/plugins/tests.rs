use std::sync::Arc;

use opengoose_persistence::{Database, Plugin, PluginStore};
use opengoose_profiles::SkillStore;
use opengoose_types::PluginStatusSnapshot;

use super::catalog::build_page_with_skill_store;
use super::detail::{build_plugin_detail, build_plugin_list_item, placeholder_plugin_detail};
use super::state::{PluginState, PluginStatusBucket};
use super::*;
use crate::data::views::{Notice, PluginDetailView};

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

fn sample_plugin_state() -> PluginState {
    let plugin = Plugin {
        id: 1,
        name: "file-tools".into(),
        version: "1.2.3".into(),
        author: Some("Agentic Dev".into()),
        description: Some(
            "Provides filesystem and command utilities for local automation tasks.".into(),
        ),
        capabilities: "skill,channel_adapter".into(),
        source_path: "/tmp/plugins/file-tools".into(),
        enabled: true,
        created_at: "2026-03-10 09:00".into(),
        updated_at: "2026-03-11 10:30".into(),
    };

    let snapshot = PluginStatusSnapshot {
        name: plugin.name.clone(),
        version: plugin.version.clone(),
        enabled: plugin.enabled,
        source_path: plugin.source_path.clone(),
        capabilities: vec!["skill".into(), "channel_adapter".into()],
        runtime_initialized: true,
        registered_skills: vec!["file/read".into(), "file/write".into()],
        missing_skills: vec!["shell/exec".into()],
        runtime_note: Some("Runtime initialized with 2 registered skill(s).".into()),
    };

    PluginState {
        plugin,
        snapshot,
        bucket: PluginStatusBucket::Operational,
        status_label: "Ready".into(),
        status_tone: "success",
        lifecycle_label: "Enabled".into(),
        lifecycle_tone: "sage",
        runtime_label: "Runtime initialized".into(),
        runtime_tone: "success",
        status_summary: "2 declared skill(s) are registered in the active runtime.".into(),
    }
}

fn meta_value<'a>(detail: &'a PluginDetailView, label: &str) -> &'a str {
    detail
        .meta
        .iter()
        .find(|row| row.label == label)
        .map(|row| row.value.as_str())
        .unwrap()
}

#[test]
fn load_plugins_page_empty_returns_placeholder() {
    let page = load_plugins_page(test_db(), None).unwrap();
    assert!(page.plugins.is_empty());
    assert!(page.selected.is_placeholder);
    assert_eq!(page.mode_tone, "neutral");
    assert_eq!(page.filters.len(), 4);
}

#[test]
fn load_plugins_page_selects_first_plugin_by_default() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    store
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();
    store
        .install("beta", "2.0.0", "/tmp/b", Some("Dev"), None, "")
        .unwrap();

    let page = load_plugins_page(db, None).unwrap();
    assert_eq!(page.plugins.len(), 2);
    assert_eq!(page.selected.title, "alpha");
    assert!(page.plugins[0].active);
}

#[test]
fn load_plugins_page_named_selection_marks_active_item() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    store
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();
    store
        .install(
            "beta",
            "2.0.0",
            "/tmp/b",
            Some("Dev"),
            None,
            "skill,channel_adapter",
        )
        .unwrap();

    let page = load_plugins_page(db, Some("beta".into())).unwrap();
    assert_eq!(page.selected.title, "beta");
    assert!(
        page.plugins
            .iter()
            .find(|plugin| plugin.title == "beta")
            .unwrap()
            .active
    );
}

#[test]
fn install_plugin_from_path_requires_path() {
    let page = install_plugin_from_path(
        test_db(),
        PluginInstallInput {
            source_path: "  ".into(),
        },
    )
    .unwrap();

    assert!(page.selected.is_placeholder);
    assert_eq!(
        page.selected.notice.unwrap().text,
        "Plugin path is required."
    );
}

#[test]
fn toggle_plugin_state_updates_notice_and_status() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    store
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();

    let page = toggle_plugin_state(db.clone(), "alpha".into()).unwrap();
    assert_eq!(page.selected.lifecycle_label, "Disabled");

    let toggled = PluginStore::new(db).get_by_name("alpha").unwrap().unwrap();
    assert!(!toggled.enabled);
}

#[test]
fn delete_plugin_requires_confirmation() {
    let db = test_db();
    PluginStore::new(db.clone())
        .install("alpha", "1.0.0", "/tmp/a", None, None, "skill")
        .unwrap();

    let page = delete_plugin(db, "alpha".into(), false).unwrap();
    assert_eq!(page.selected.title, "alpha");
    assert_eq!(page.selected.notice.unwrap().tone, "danger");
}

#[test]
fn filtered_page_uses_runtime_snapshot_state() {
    let db = test_db();
    let store = PluginStore::new(db.clone());
    let temp = tempfile::tempdir().expect("temp dir");
    let skill_store = SkillStore::with_dir(temp.path().join("skills"));
    let ready_dir = temp.path().join("ready-tools");
    let missing_dir = temp.path().join("missing-tools");

    std::fs::create_dir_all(&ready_dir).expect("ready plugin dir");
    std::fs::write(
        ready_dir.join("plugin.toml"),
        r#"
name = "ready-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "ls"
cmd = "ls"
"#,
    )
    .expect("ready manifest");
    std::fs::create_dir_all(&missing_dir).expect("missing plugin dir");
    std::fs::write(
        missing_dir.join("plugin.toml"),
        r#"
name = "missing-tools"
version = "1.0.0"
capabilities = ["skill"]

[[skills]]
name = "grep"
cmd = "grep"
"#,
    )
    .expect("missing manifest");

    let ready_manifest =
        opengoose_teams::plugin::load_manifest(&ready_dir.join("plugin.toml")).unwrap();
    let ready_loaded =
        opengoose_teams::plugin::LoadedPlugin::from_manifest(ready_manifest, ready_dir.clone());
    opengoose_teams::plugin::PluginRuntime::init_plugin(&ready_loaded, &skill_store).unwrap();

    store
        .install(
            "ready-tools",
            "1.0.0",
            &ready_dir.to_string_lossy(),
            None,
            Some("Ready plugin"),
            "skill",
        )
        .unwrap();
    store
        .install(
            "missing-tools",
            "1.0.0",
            &missing_dir.to_string_lossy(),
            None,
            Some("Needs registration"),
            "skill",
        )
        .unwrap();
    store
        .install("disabled-tools", "1.0.0", "/tmp/disabled", None, None, "")
        .unwrap();
    store.set_enabled("disabled-tools", false).unwrap();

    let page = build_page_with_skill_store(
        db,
        None,
        PluginStatusFilter::Attention,
        None,
        String::new(),
        Some(&skill_store),
    )
    .unwrap();

    assert_eq!(page.plugins.len(), 1);
    assert_eq!(page.plugins[0].title, "missing-tools");
    assert_eq!(page.plugins[0].status_label, "Missing skills");
    assert!(page.plugins[0].page_url.contains("status=attention"));
    assert_eq!(page.selected.runtime_label, "1 skill(s) missing");
    assert_eq!(page.mode_label, "1 operational · 1 attention · 1 disabled");
}

#[test]
fn filtered_page_shows_placeholder_when_nothing_matches() {
    let db = test_db();
    PluginStore::new(db.clone())
        .install("alpha", "1.0.0", "/tmp/a", None, None, "")
        .unwrap();

    let page = build_page_with_skill_store(
        db,
        None,
        PluginStatusFilter::Disabled,
        None,
        String::new(),
        None,
    )
    .unwrap();

    assert!(page.plugins.is_empty());
    assert!(page.selected.is_placeholder);
    assert_eq!(page.selected.title, "No disabled plugins");
}

#[test]
fn build_plugin_list_item_formats_author_preview_and_selected_url() {
    let mut state = sample_plugin_state();
    state.plugin.name = "file tools".into();
    state.snapshot.name = state.plugin.name.clone();
    state.plugin.description = Some(
        "This description is intentionally long so the sidebar preview has to truncate it before rendering in the plugin catalog list.".into(),
    );
    state.status_summary =
        "This runtime summary is intentionally long so the sidebar status detail has to truncate before it reaches the card boundary.".into();

    let item = build_plugin_list_item(&state, Some("file tools"), PluginStatusFilter::Attention);

    assert_eq!(item.subtitle, "v1.2.3 · Agentic Dev");
    assert!(item.preview.ends_with('…'));
    assert_eq!(item.preview.chars().count(), 85);
    assert!(item.status_detail.ends_with('…'));
    assert_eq!(item.status_detail.chars().count(), 93);
    assert_eq!(
        item.page_url,
        "/plugins?status=attention&plugin=file%20tools"
    );
    assert!(item.active);
}

#[test]
fn build_plugin_list_item_omits_missing_author_and_uses_description_fallback() {
    let mut state = sample_plugin_state();
    state.plugin.author = Some("   ".into());
    state.plugin.description = None;

    let item = build_plugin_list_item(&state, Some("other"), PluginStatusFilter::Disabled);

    assert_eq!(item.subtitle, "v1.2.3");
    assert_eq!(item.preview, "No plugin description provided.");
    assert_eq!(item.page_url, "/plugins?status=disabled&plugin=file-tools");
    assert!(!item.active);
}

#[test]
fn build_plugin_detail_surfaces_runtime_meta_counts_and_disable_toggle() {
    let notice = Notice {
        text: "Plugin metadata refreshed.".into(),
        tone: "success",
    };

    let detail = build_plugin_detail(
        &sample_plugin_state(),
        Some(notice),
        "/tmp/plugins/new-source".into(),
    );

    assert_eq!(
        detail.subtitle,
        "Provides filesystem and command utilities for local automation tasks."
    );
    assert_eq!(detail.lifecycle_label, "Enabled");
    assert_eq!(detail.runtime_label, "Runtime initialized");
    assert_eq!(
        detail.runtime_note.as_deref(),
        Some("Runtime initialized with 2 registered skill(s).")
    );
    assert_eq!(detail.toggle_label, "Disable plugin");
    assert_eq!(meta_value(&detail, "Author"), "Agentic Dev");
    assert_eq!(meta_value(&detail, "Registered skills"), "2");
    assert_eq!(meta_value(&detail, "Missing skills"), "1");
    assert_eq!(
        detail.notice.as_ref().unwrap().text,
        "Plugin metadata refreshed."
    );
    assert_eq!(detail.install_source_path, "/tmp/plugins/new-source");
    assert!(!detail.is_placeholder);
}

#[test]
fn build_plugin_detail_uses_fallbacks_for_disabled_plugins() {
    let mut state = sample_plugin_state();
    state.plugin.author = None;
    state.plugin.description = None;
    state.plugin.enabled = false;
    state.snapshot.enabled = false;
    state.snapshot.runtime_initialized = false;
    state.snapshot.registered_skills.clear();
    state.snapshot.missing_skills = vec!["shell/exec".into(), "shell/kill".into()];
    state.snapshot.runtime_note = None;
    state.bucket = PluginStatusBucket::Disabled;
    state.status_label = "Disabled".into();
    state.status_tone = "neutral";
    state.lifecycle_label = "Disabled".into();
    state.lifecycle_tone = "neutral";
    state.runtime_label = "Runtime paused".into();
    state.runtime_tone = "neutral";
    state.status_summary = "Runtime checks pause while the plugin is disabled.".into();

    let detail = build_plugin_detail(&state, None, String::new());

    assert_eq!(
        detail.subtitle,
        "This plugin is installed locally and ready for operator review."
    );
    assert_eq!(detail.lifecycle_label, "Disabled");
    assert_eq!(detail.runtime_label, "Runtime paused");
    assert_eq!(detail.toggle_label, "Enable plugin");
    assert_eq!(meta_value(&detail, "Author"), "Unknown");
    assert_eq!(meta_value(&detail, "Registered skills"), "0");
    assert_eq!(meta_value(&detail, "Missing skills"), "2");
}

#[test]
fn placeholder_plugin_detail_distinguishes_empty_registry_and_filtered_results() {
    let empty =
        placeholder_plugin_detail(None, "/tmp/plugins/new".into(), PluginStatusFilter::All, 0);
    let filtered = placeholder_plugin_detail(
        Some(Notice {
            text: "Filter left no visible plugins.".into(),
            tone: "amber",
        }),
        "/tmp/plugins/new".into(),
        PluginStatusFilter::Disabled,
        3,
    );

    assert_eq!(empty.title, "No plugins installed");
    assert_eq!(empty.status_label, "Awaiting install");
    assert_eq!(
        empty.capabilities_hint,
        "Installed plugin capabilities will appear here."
    );
    assert_eq!(empty.install_source_path, "/tmp/plugins/new");
    assert!(empty.is_placeholder);
    assert!(empty.toggle_label.is_empty());
    assert!(empty.delete_label.is_empty());

    assert_eq!(filtered.title, "No disabled plugins");
    assert_eq!(
        filtered.subtitle,
        "3 installed plugin(s) exist outside the current status filter."
    );
    assert_eq!(filtered.status_label, "Adjust filter");
    assert_eq!(
        filtered.status_summary,
        "Choose another filter or clear the filter to inspect all plugins."
    );
    assert_eq!(
        filtered.capabilities_hint,
        "Visible plugin capabilities will appear here once a filter matches."
    );
    assert_eq!(
        filtered.notice.as_ref().unwrap().text,
        "Filter left no visible plugins."
    );
}
