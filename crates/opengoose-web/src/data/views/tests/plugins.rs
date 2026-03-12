use super::*;

#[test]
fn plugin_list_item_tracks_status_and_selection() {
    let item = PluginListItem {
        title: "ops-tools".into(),
        subtitle: "v1.2.3".into(),
        preview: "Operational helpers".into(),
        status_detail: "All declared runtime skills are registered.".into(),
        search_text: "skill ready-tools/ls".into(),
        source_label: "/tmp/ops-tools".into(),
        source_badge: "ops-tools".into(),
        status_label: "Ready".into(),
        status_tone: "success",
        page_url: "/plugins?plugin=ops-tools".into(),
        active: true,
    };
    assert!(item.active);
    assert_eq!(item.status_tone, "success");
}

#[test]
fn plugin_detail_view_placeholder_flag_is_accessible() {
    let detail = PluginDetailView {
        title: "No plugins installed".into(),
        subtitle: "Install a plugin".into(),
        source_label: "Local plugin registry".into(),
        status_label: "Awaiting install".into(),
        status_tone: "neutral",
        lifecycle_label: "Awaiting selection".into(),
        lifecycle_tone: "neutral",
        runtime_label: "No runtime data".into(),
        runtime_tone: "neutral",
        status_summary: "Install a plugin to inspect runtime readiness.".into(),
        runtime_note: None,
        meta: vec![],
        capabilities: vec![],
        capabilities_hint: "Capabilities will appear here.".into(),
        registered_skills: vec![],
        missing_skills: vec![],
        notice: None,
        install_source_path: String::new(),
        toggle_label: String::new(),
        delete_label: String::new(),
        is_placeholder: true,
    };
    assert!(detail.is_placeholder);
    assert_eq!(detail.status_label, "Awaiting install");
}
