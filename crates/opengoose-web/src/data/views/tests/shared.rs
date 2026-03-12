use super::*;

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

#[test]
fn notice_text_and_tone() {
    let notice = Notice {
        text: "Saved successfully.".into(),
        tone: "success",
    };
    assert_eq!(notice.text, "Saved successfully.");
    assert_eq!(notice.tone, "success");
}
