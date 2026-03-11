use super::*;

// ── MetricCard ────────────────────────────────────────────────────────────────

#[test]
fn metric_card_fields_are_accessible() {
    let card = MetricCard {
        label: "Sessions".into(),
        value: "42".into(),
        note: "last 24h".into(),
        tone: "success",
    };
    assert_eq!(card.label, "Sessions");
    assert_eq!(card.value, "42");
    assert_eq!(card.note, "last 24h");
    assert_eq!(card.tone, "success");
}

#[test]
fn metric_card_clone_produces_equal_fields() {
    let card = MetricCard {
        label: "Runs".into(),
        value: "7".into(),
        note: "active".into(),
        tone: "accent",
    };
    let cloned = card.clone();
    assert_eq!(cloned.label, card.label);
    assert_eq!(cloned.value, card.value);
    assert_eq!(cloned.note, card.note);
    assert_eq!(cloned.tone, card.tone);
}

// ── AlertCard ────────────────────────────────────────────────────────────────

#[test]
fn alert_card_fields_are_accessible() {
    let card = AlertCard {
        eyebrow: "Warning".into(),
        title: "Gateway offline".into(),
        description: "Slack gateway lost connection.".into(),
        tone: "danger",
    };
    assert_eq!(card.eyebrow, "Warning");
    assert_eq!(card.title, "Gateway offline");
    assert_eq!(card.tone, "danger");
}

#[test]
fn alert_card_clone_is_independent() {
    let card = AlertCard {
        eyebrow: "Info".into(),
        title: "Deploying".into(),
        description: "Rolling update in progress.".into(),
        tone: "neutral",
    };
    let mut cloned = card.clone();
    cloned.tone = "success";
    assert_eq!(card.tone, "neutral");
    assert_eq!(cloned.tone, "success");
}

// ── StatusSegment ────────────────────────────────────────────────────────────

#[test]
fn status_segment_width_is_stored() {
    let seg = StatusSegment {
        label: "Running".into(),
        value: "3".into(),
        tone: "success",
        width: 40,
    };
    assert_eq!(seg.width, 40);
    assert_eq!(seg.label, "Running");
}

// ── TrendBar ─────────────────────────────────────────────────────────────────

#[test]
fn trend_bar_height_is_stored() {
    let bar = TrendBar {
        label: "Mon".into(),
        value: "1.2s".into(),
        detail: "12 runs".into(),
        tone: "accent",
        height: 75,
    };
    assert_eq!(bar.height, 75);
    assert_eq!(bar.label, "Mon");
}

// ── ActivityItem ─────────────────────────────────────────────────────────────

#[test]
fn activity_item_fields_are_accessible() {
    let item = ActivityItem {
        actor: "goose".into(),
        meta: "ran feature-dev".into(),
        detail: "3 steps".into(),
        timestamp: "10:00".into(),
        tone: "plain",
    };
    assert_eq!(item.actor, "goose");
    assert_eq!(item.tone, "plain");
}

// ── MetaRow ──────────────────────────────────────────────────────────────────

#[test]
fn meta_row_label_and_value() {
    let row = MetaRow {
        label: "Status".into(),
        value: "running".into(),
    };
    assert_eq!(row.label, "Status");
    assert_eq!(row.value, "running");
}

#[test]
fn meta_row_clone_is_independent() {
    let row = MetaRow {
        label: "Team".into(),
        value: "code-review".into(),
    };
    let mut cloned = row.clone();
    cloned.value = "feature-dev".into();
    assert_eq!(row.value, "code-review");
    assert_eq!(cloned.value, "feature-dev");
}

// ── RunListItem ──────────────────────────────────────────────────────────────

#[test]
fn run_list_item_page_url_and_queue_url_differ() {
    let item = RunListItem {
        title: "Run 1".into(),
        subtitle: "team · chain".into(),
        updated_at: "10:05".into(),
        progress_label: "Step 2/3".into(),
        badge: "RUNNING".into(),
        badge_tone: "success",
        page_url: "/runs?run=r1".into(),
        queue_page_url: "/queue?run=r1".into(),
        active: false,
    };
    assert_ne!(item.page_url, item.queue_page_url);
    assert_eq!(item.badge, "RUNNING");
}

// ── WorkItemView ─────────────────────────────────────────────────────────────

#[test]
fn work_item_view_indent_classes() {
    let root = WorkItemView {
        title: "Root task".into(),
        detail: "agent · 10:00".into(),
        status_label: "done".into(),
        status_tone: "success",
        step_label: "Step 1".into(),
        indent_class: "is-root",
    };
    let child = WorkItemView {
        title: "Sub task".into(),
        detail: "agent · 10:01".into(),
        status_label: "in progress".into(),
        status_tone: "accent",
        step_label: "Root item".into(),
        indent_class: "is-child",
    };
    assert_eq!(root.indent_class, "is-root");
    assert_eq!(child.indent_class, "is-child");
}

// ── SelectOption ─────────────────────────────────────────────────────────────

#[test]
fn select_option_selected_flag() {
    let opt_a = SelectOption {
        value: "team-a".into(),
        label: "Team A".into(),
        selected: true,
    };
    let opt_b = SelectOption {
        value: "team-b".into(),
        label: "Team B".into(),
        selected: false,
    };
    assert!(opt_a.selected);
    assert!(!opt_b.selected);
}

#[test]
fn select_option_clone() {
    let opt = SelectOption {
        value: "v1".into(),
        label: "Version 1".into(),
        selected: false,
    };
    let mut cloned = opt.clone();
    cloned.selected = true;
    assert!(!opt.selected);
    assert!(cloned.selected);
}

// ── Notice ───────────────────────────────────────────────────────────────────

#[test]
fn notice_text_and_tone() {
    let notice = Notice {
        text: "Saved successfully.".into(),
        tone: "success",
    };
    assert_eq!(notice.text, "Saved successfully.");
    assert_eq!(notice.tone, "success");
}

// ── ScheduleListItem ─────────────────────────────────────────────────────────

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

// ── ScheduleEditorView ───────────────────────────────────────────────────────

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

// ── TriggerListItem ──────────────────────────────────────────────────────────

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

// ── TriggerDetailView ────────────────────────────────────────────────────────

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
        delete_api_url: String::new(),
        toggle_enabled_api_url: String::new(),
        test_api_url: String::new(),
        update_api_url: String::new(),
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
        delete_api_url: "/api/triggers/on-mention".into(),
        toggle_enabled_api_url: "/api/triggers/on-mention/toggle".into(),
        test_api_url: "/api/triggers/on-mention/test".into(),
        update_api_url: "/api/triggers/on-mention".into(),
        is_placeholder: false,
    };
    assert!(detail.enabled);
    assert_eq!(detail.fire_count, 17);
    assert!(!detail.is_placeholder);
}

// ── GatewayCard ──────────────────────────────────────────────────────────────

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

// ── WorkflowDetailView ───────────────────────────────────────────────────────

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

// ── DashboardView ────────────────────────────────────────────────────────────

#[test]
fn dashboard_view_holds_all_collection_fields() {
    let dashboard = DashboardView {
        mode_label: "Live runtime".into(),
        mode_tone: "success",
        stream_summary: "2 active streams".into(),
        snapshot_label: "10:00".into(),
        metrics: vec![MetricCard {
            label: "Sessions".into(),
            value: "5".into(),
            note: "active".into(),
            tone: "accent",
        }],
        queue_cards: vec![],
        run_segments: vec![],
        queue_segments: vec![],
        duration_bars: vec![],
        activities: vec![],
        alerts: vec![],
        sessions: vec![],
        runs: vec![],
        gateways: vec![GatewayCard {
            platform: "Slack".into(),
            state_label: "Connected".into(),
            state_tone: "success",
            uptime_label: "1d".into(),
            detail: "".into(),
        }],
    };
    assert_eq!(dashboard.metrics.len(), 1);
    assert_eq!(dashboard.gateways.len(), 1);
    assert_eq!(dashboard.mode_tone, "success");
}
