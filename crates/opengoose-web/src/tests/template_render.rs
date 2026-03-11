use crate::data::{
    MessageBubble, MetaRow, MetricCard, Notice, QueueDetailView, QueueMessageView,
    ScheduleEditorView, ScheduleHistoryItem, ScheduleListItem, SchedulesPageView, SelectOption,
    SessionDetailView, SessionListItem, SessionsPageView, WorkflowAutomationView,
    WorkflowDetailView, WorkflowListItem, WorkflowRunView, WorkflowStepView, WorkflowsPageView,
};
use crate::fixtures::sample_dashboard_view;
use crate::routes;

fn sample_session_detail() -> SessionDetailView {
    SessionDetailView {
        session_key: "discord:ops:bridge".into(),
        title: "Session ops".into(),
        subtitle: "discord / ops".into(),
        source_label: "Live runtime".into(),
        meta: vec![MetaRow {
            label: "Stable key".into(),
            value: "discord:ops:bridge".into(),
        }],
        notice: None,
        selected_model: String::new(),
        model_options: vec![],
        messages: vec![MessageBubble {
            role_label: "Assistant".into(),
            author_label: "frontend-engineer".into(),
            timestamp: "2026-03-10 12:34".into(),
            content: "Dashboard panel updated.".into(),
            tone: "accent",
            alignment: "right",
        }],
        empty_hint: "No messages yet.".into(),
    }
}

fn sample_queue_detail() -> QueueDetailView {
    QueueDetailView {
        title: "Queue run-1".into(),
        subtitle: "feature-dev / chain".into(),
        source_label: "Live runtime".into(),
        status_cards: vec![MetricCard {
            label: "Pending".into(),
            value: "1".into(),
            note: "Waiting for recipients".into(),
            tone: "amber",
        }],
        messages: vec![QueueMessageView {
            sender: "planner".into(),
            recipient: "developer".into(),
            kind: "task".into(),
            status_label: "pending".into(),
            status_tone: "amber",
            created_at: "2026-03-10 12:35".into(),
            retry_text: "1/3".into(),
            content: "Implement the dashboard controls.".into(),
            error: "waiting for pickup".into(),
        }],
        dead_letters: vec![],
        empty_hint: "No queue traffic yet.".into(),
    }
}

fn sample_workflow_detail() -> WorkflowDetailView {
    WorkflowDetailView {
        title: "feature-dev".into(),
        subtitle: "Build and verify product changes with a chained team.".into(),
        source_label: "Live registry".into(),
        status_label: "Running".into(),
        status_tone: "cyan",
        meta: vec![MetaRow {
            label: "Pattern".into(),
            value: "Chain".into(),
        }],
        steps: vec![WorkflowStepView {
            title: "Step 1 · planner".into(),
            detail: "Shape the implementation plan.".into(),
            badge: "Sequential".into(),
            badge_tone: "cyan",
        }],
        automations: vec![WorkflowAutomationView {
            kind: "Schedule".into(),
            title: "nightly-review".into(),
            detail: "0 0 * * * · team feature-dev".into(),
            note: "Next 2026-03-11 00:00".into(),
            status_label: "Enabled".into(),
            status_tone: "sage",
        }],
        recent_runs: vec![WorkflowRunView {
            title: "Run run-1".into(),
            detail: "2/4 steps · Still executing".into(),
            updated_at: "2026-03-10 12:35".into(),
            status_label: "Running".into(),
            status_tone: "cyan",
            page_url: "/runs?run=run-1".into(),
        }],
        yaml: "title: feature-dev".into(),
        trigger_api_url: "/api/workflows/feature-dev/trigger".into(),
        trigger_input: "Manual run requested".into(),
    }
}

fn sample_schedule_detail() -> ScheduleEditorView {
    ScheduleEditorView {
        title: "nightly-review".into(),
        subtitle: "Adjust cadence, target team, and run input without leaving the dashboard."
            .into(),
        source_label: "Live schedule store".into(),
        original_name: "nightly-review".into(),
        name: "nightly-review".into(),
        cron_expression: "0 0 * * * *".into(),
        team_name: "feature-dev".into(),
        input: String::new(),
        enabled: true,
        is_new: false,
        name_locked: true,
        meta: vec![MetaRow {
            label: "Next fire".into(),
            value: "2026-03-11 00:00:00".into(),
        }],
        team_options: vec![SelectOption {
            value: "feature-dev".into(),
            label: "feature-dev".into(),
            selected: true,
        }],
        history: vec![ScheduleHistoryItem {
            title: "run-1".into(),
            detail: "chain workflow · Scheduled run: nightly-review".into(),
            updated_at: "2026-03-10 12:35".into(),
            status_label: "completed".into(),
            status_tone: "sage",
            page_url: "/runs?run=run-1".into(),
        }],
        history_hint: "No matching runs found for this schedule yet.".into(),
        notice: Some(Notice {
            text: "Schedule saved.".into(),
            tone: "success",
        }),
        save_label: "Save changes".into(),
        toggle_label: "Pause schedule".into(),
        delete_label: "nightly-review".into(),
    }
}

fn sample_new_schedule_detail() -> ScheduleEditorView {
    ScheduleEditorView {
        title: "New schedule".into(),
        subtitle: "Create a cron-driven workflow handoff from the dashboard.".into(),
        source_label: "Live schedule store".into(),
        original_name: "__new__".into(),
        name: String::new(),
        cron_expression: String::new(),
        team_name: String::new(),
        input: "{}".into(),
        enabled: false,
        is_new: true,
        name_locked: false,
        meta: vec![MetaRow {
            label: "Next fire".into(),
            value: "Pending cron validation".into(),
        }],
        team_options: vec![],
        history: vec![],
        history_hint: "No matching runs found for this schedule yet.".into(),
        notice: None,
        save_label: "Create schedule".into(),
        toggle_label: "Enable schedule".into(),
        delete_label: "New schedule".into(),
    }
}

#[test]
fn dashboard_live_template_renders_monitoring_sections() {
    let html = routes::test_support::render_dashboard_live(sample_dashboard_view())
        .expect("dashboard live partial renders");

    assert!(html.contains("Execution mix"));
    assert!(html.contains("Queue mix"));
    assert!(html.contains("Agent activity"));
    assert!(html.contains("Live runtime"));
    assert!(html.contains("feature-dev"));
}

#[test]
fn sessions_template_renders_accessible_list_controls() {
    let detail = sample_session_detail();
    let detail_html =
        routes::test_support::render_session_detail(detail.clone()).expect("detail renders");
    let html = routes::test_support::render_sessions_page(
        SessionsPageView {
            mode_label: "Live runtime".into(),
            mode_tone: "success",
            live_stream_url: "/sessions/events?session=discord%3Adirect%3Achan-1".into(),
            sessions: vec![SessionListItem {
                title: "ops / bridge".into(),
                subtitle: "feature-dev active · Live runtime".into(),
                preview: "Investigate the dashboard refresh cycle.".into(),
                updated_at: "2026-03-10 12:34".into(),
                badge: "DISCORD".into(),
                badge_tone: "cyan",
                page_url: "/sessions?session=ops".into(),
                active: true,
            }],
            selected: detail,
        },
        detail_html,
    )
    .expect("sessions template renders");

    assert!(html.contains("data-list-shell"));
    assert!(html.contains("Search sessions"));
    assert!(html.contains("data-list-item"));
    assert!(html.contains("data-detail-panel"));
    assert!(!html.contains("hx-get"));
}

#[test]
fn queue_detail_template_renders_table_controls() {
    let html = routes::test_support::render_queue_detail(sample_queue_detail())
        .expect("queue detail renders");

    assert!(html.contains("data-table-shell"));
    assert!(html.contains("Search traffic"));
    assert!(html.contains("data-table-row"));
    assert!(html.contains("Retries high-low"));
}

#[test]
fn schedules_template_renders_form_actions_and_history() {
    let detail = sample_schedule_detail();
    let detail_html = routes::test_support::render_schedule_detail(detail.clone())
        .expect("schedule detail renders");
    let html = routes::test_support::render_schedules_page(
        SchedulesPageView {
            mode_label: "1 active of 1".into(),
            mode_tone: "success",
            schedules: vec![ScheduleListItem {
                title: "nightly-review".into(),
                subtitle: "feature-dev · default input".into(),
                preview: "0 0 * * * * · Next 2026-03-11 00:00:00".into(),
                source_label: "Last 2026-03-10 12:35".into(),
                status_label: "Enabled".into(),
                status_tone: "sage",
                page_url: "/schedules?schedule=nightly-review".into(),
                active: true,
            }],
            selected: detail,
            new_schedule_url: "/schedules?schedule=__new__".into(),
        },
        detail_html,
    )
    .expect("schedules template renders");

    assert!(html.contains("Search schedules"));
    assert!(html.contains("New schedule"));
    assert!(html.contains("data-list-item"));
    assert!(html.contains("Pause schedule"));
    assert!(html.contains("Recent matching runs"));
}

#[test]
fn schedule_detail_template_hides_destructive_controls_for_new_schedule() {
    let html = routes::test_support::render_schedule_detail(sample_new_schedule_detail())
        .expect("new schedule detail renders");

    assert!(html.contains("Create schedule"));
    assert!(html.contains("No teams are installed yet."));
    assert!(html.contains("No matching runs found for this schedule yet."));
    assert!(!html.contains("Delete schedule"));
    assert!(!html.contains("Enable schedule"));
}

#[test]
fn workflows_template_renders_trigger_controls() {
    let detail = sample_workflow_detail();
    let detail_html = routes::test_support::render_workflow_detail(detail.clone())
        .expect("workflow detail renders");
    let html = routes::test_support::render_workflows_page(
        WorkflowsPageView {
            mode_label: "Live registry".into(),
            mode_tone: "success",
            workflows: vec![WorkflowListItem {
                title: "feature-dev".into(),
                subtitle: "Build and verify product changes with a chained team.".into(),
                preview: "1/1 enabled · 0 configured · planner · developer".into(),
                source_label: "Live registry".into(),
                status_label: "Running".into(),
                status_tone: "cyan",
                page_url: "/workflows?workflow=feature-dev".into(),
                active: true,
            }],
            selected: detail,
        },
        detail_html,
    )
    .expect("workflows template renders");

    assert!(html.contains("Search workflows"));
    assert!(html.contains("data-bind:workflow-trigger-input"));
    assert!(html.contains("/workflows/feature-dev/trigger"));
    assert!(html.contains("Recent runs"));
}
