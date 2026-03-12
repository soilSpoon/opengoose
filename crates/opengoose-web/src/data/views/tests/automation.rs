use super::*;

#[test]
fn schedule_list_item_active_and_status_tone() {
    let item = ScheduleListItem {
        title: "Daily Standup".into(),
        subtitle: "0 9 * * 1-5".into(),
        preview: "Run standup".into(),
        source_label: "Live".into(),
        status_label: "Enabled".into(),
        status_tone: "success",
        page_url: "/schedules?schedule=daily-standup".into(),
        active: true,
    };
    assert!(item.active);
    assert_eq!(item.status_tone, "success");
}

#[test]
fn schedule_editor_view_new_schedule_flags() {
    let editor = ScheduleEditorView {
        title: "New Schedule".into(),
        subtitle: "Create a new schedule".into(),
        source_label: "Live".into(),
        original_name: String::new(),
        name: String::new(),
        cron_expression: "0 9 * * 1-5".into(),
        team_name: "feature-dev".into(),
        input: String::new(),
        enabled: true,
        is_new: true,
        name_locked: false,
        meta: vec![],
        team_options: vec![],
        history: vec![],
        history_hint: "No history yet.".into(),
        notice: None,
        save_label: "Create".into(),
        toggle_label: "Disable".into(),
        delete_label: "Delete".into(),
    };
    assert!(editor.is_new);
    assert!(!editor.name_locked);
    assert!(editor.notice.is_none());
    assert!(editor.history.is_empty());
}

#[test]
fn schedule_editor_view_with_notice() {
    let editor = ScheduleEditorView {
        title: "Edit".into(),
        subtitle: "".into(),
        source_label: "Live".into(),
        original_name: "daily".into(),
        name: "daily".into(),
        cron_expression: "0 9 * * *".into(),
        team_name: "bug-triage".into(),
        input: "run triage".into(),
        enabled: false,
        is_new: false,
        name_locked: true,
        meta: vec![],
        team_options: vec![],
        history: vec![],
        history_hint: String::new(),
        notice: Some(Notice {
            text: "Schedule saved.".into(),
            tone: "success",
        }),
        save_label: "Save".into(),
        toggle_label: "Enable".into(),
        delete_label: "Delete".into(),
    };
    assert!(!editor.enabled);
    assert!(editor.name_locked);
    let notice = editor.notice.unwrap();
    assert_eq!(notice.tone, "success");
}

#[test]
fn trigger_list_item_status_tone_and_active_flag() {
    let item = TriggerListItem {
        title: "On message".into(),
        subtitle: "webhook".into(),
        team_label: "feature-dev".into(),
        status_label: "Enabled".into(),
        status_tone: "success",
        last_fired: "10 min ago".into(),
        page_url: "/triggers?trigger=on-message".into(),
        active: false,
    };
    assert_eq!(item.status_tone, "success");
    assert!(!item.active);
}

#[test]
fn trigger_detail_view_placeholder_flag() {
    let detail = TriggerDetailView {
        name: String::new(),
        trigger_type: String::new(),
        team_name: String::new(),
        input: String::new(),
        condition_json: String::new(),
        enabled: false,
        fire_count: 0,
        last_fired_at: String::new(),
        created_at: String::new(),
        meta: vec![],
        status_label: String::new(),
        status_tone: "neutral",
        notice: None,
        is_placeholder: true,
    };
    assert!(detail.is_placeholder);
    assert_eq!(detail.fire_count, 0);
}

#[test]
fn trigger_detail_view_enabled_with_fire_count() {
    let detail = TriggerDetailView {
        name: "on-mention".into(),
        trigger_type: "webhook".into(),
        team_name: "code-review".into(),
        input: "review this".into(),
        condition_json: "{}".into(),
        enabled: true,
        fire_count: 17,
        last_fired_at: "2026-03-10 09:00".into(),
        created_at: "2026-03-01 00:00".into(),
        meta: vec![],
        status_label: "Enabled".into(),
        status_tone: "success",
        notice: Some(Notice {
            text: "Saved".into(),
            tone: "success",
        }),
        is_placeholder: false,
    };
    assert!(detail.enabled);
    assert_eq!(detail.fire_count, 17);
    assert!(!detail.is_placeholder);
}

#[test]
fn gateway_card_connected_tone() {
    let card = GatewayCard {
        platform: "Slack".into(),
        state_label: "Connected".into(),
        state_tone: "success",
        uptime_label: "3d 12h".into(),
        detail: "workspace: opengoose".into(),
    };
    assert_eq!(card.platform, "Slack");
    assert_eq!(card.state_tone, "success");
}

#[test]
fn gateway_card_disconnected_tone() {
    let card = GatewayCard {
        platform: "Discord".into(),
        state_label: "Disconnected".into(),
        state_tone: "danger",
        uptime_label: "—".into(),
        detail: "last seen 2h ago".into(),
    };
    assert_eq!(card.state_tone, "danger");
    assert_eq!(card.state_label, "Disconnected");
}

#[test]
fn gateway_card_clone() {
    let card = GatewayCard {
        platform: "Matrix".into(),
        state_label: "Connecting".into(),
        state_tone: "amber",
        uptime_label: "—".into(),
        detail: "retrying".into(),
    };
    let cloned = card.clone();
    assert_eq!(cloned.platform, card.platform);
    assert_eq!(cloned.state_tone, card.state_tone);
}

#[test]
fn workflow_detail_view_empty_steps_and_automations() {
    let detail = WorkflowDetailView {
        title: "My Workflow".into(),
        subtitle: "feature-dev".into(),
        source_label: "Live".into(),
        status_label: "Active".into(),
        status_tone: "success",
        meta: vec![],
        steps: vec![],
        automations: vec![],
        recent_runs: vec![],
        yaml: "name: my-workflow".into(),
        trigger_api_url: "/api/triggers".into(),
        trigger_input: "{}".into(),
    };
    assert!(detail.steps.is_empty());
    assert!(detail.automations.is_empty());
}

#[test]
fn dashboard_view_holds_all_collection_fields() {
    let dashboard = sample_dashboard_view();
    assert_eq!(dashboard.metric_grid.items.len(), 1);
    assert_eq!(dashboard.gateway_panel.cards.len(), 1);
    assert_eq!(dashboard.intro.mode_tone, "success");
}
