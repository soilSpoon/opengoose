//! Template rendering tests for dashboard, sessions, queues, workflows, and schedules.

use crate::data::{
    ActivityItem, AlertCard, DashboardView, MessageBubble, MetaRow, MetricCard, Notice,
    QueueDetailView, QueueMessageView, RunListItem, ScheduleEditorView, ScheduleHistoryItem,
    ScheduleListItem, SchedulesPageView, SelectOption, SessionDetailView, SessionListItem,
    SessionsPageView, StatusSegment, TrendBar, WorkflowAutomationView, WorkflowDetailView,
    WorkflowListItem, WorkflowRunView, WorkflowStepView, WorkflowsPageView,
};
use crate::routes;

fn sample_dashboard() -> DashboardView {
    DashboardView {
        mode_label: "Live runtime".into(),
        mode_tone: "success",
        stream_summary: "Server-sent events stream fresh snapshots.".into(),
        snapshot_label: "Snapshot 12:34:56 UTC".into(),
        metrics: vec![MetricCard {
            label: "Active runs".into(),
            value: "2".into(),
            note: "1 completed in the latest window".into(),
            tone: "amber",
        }],
        queue_cards: vec![MetricCard {
            label: "Pending".into(),
            value: "4".into(),
            note: "Waiting for pickup".into(),
            tone: "cyan",
        }],
        run_segments: vec![StatusSegment {
            label: "Running".into(),
            value: "2".into(),
            tone: "cyan",
            width: 100,
        }],
        queue_segments: vec![StatusSegment {
            label: "Pending".into(),
            value: "4".into(),
            tone: "amber",
            width: 100,
        }],
        duration_bars: vec![TrendBar {
            label: "feature-dev".into(),
            value: "7m 12s".into(),
            detail: "running".into(),
            tone: "cyan",
            height: 76,
        }],
        activities: vec![ActivityItem {
            actor: "frontend-engineer".into(),
            meta: "Directed to reviewer".into(),
            detail: "Live dashboard shell refreshed over SSE.".into(),
            timestamp: "2026-03-10 12:34".into(),
            tone: "cyan",
        }],
        alerts: vec![AlertCard {
            eyebrow: "Runtime Active".into(),
            title: "2 orchestration runs currently active".into(),
            description: "The dashboard is streaming run status and queue pressure.".into(),
            tone: "success",
        }],
        sessions: vec![SessionListItem {
            title: "ops / bridge".into(),
            subtitle: "feature-dev active · Live runtime".into(),
            preview: "Review the live dashboard state.".into(),
            updated_at: "2026-03-10 12:34".into(),
            badge: "DISCORD".into(),
            badge_tone: "cyan",
            page_url: "/sessions?session=ops".into(),
            active: false,
        }],
        runs: vec![RunListItem {
            title: "feature-dev".into(),
            subtitle: "chain workflow · Live runtime".into(),
            updated_at: "2026-03-10 12:34".into(),
            progress_label: "2/4 steps".into(),
            badge: "RUNNING".into(),
            badge_tone: "cyan",
            page_url: "/runs?run=run-1".into(),
            queue_page_url: "/queue?run=run-1".into(),
            active: false,
        }],
        gateways: vec![],
    }
}

fn sample_session_detail() -> SessionDetailView {
    SessionDetailView {
        title: "Session ops".into(),
        subtitle: "discord / ops".into(),
        source_label: "Live runtime".into(),
        meta: vec![MetaRow {
            label: "Stable key".into(),
            value: "discord:ops:bridge".into(),
        }],
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

#[test]
fn dashboard_live_template_renders_monitoring_sections() {
    let html = routes::test_support::render_dashboard_live(sample_dashboard())
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

    assert!(html.contains("/assets/styles/shared.css"));
    assert!(html.contains("/assets/styles/detail.css"));
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

    assert!(html.contains("/assets/styles/shared.css"));
    assert!(html.contains("/assets/styles/detail.css"));
    assert!(html.contains("/assets/styles/schedules.css"));
    assert!(html.contains("Search schedules"));
    assert!(html.contains("New schedule"));
    assert!(html.contains("Pause schedule"));
    assert!(html.contains("Recent matching runs"));
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

    assert!(html.contains("/assets/styles/shared.css"));
    assert!(html.contains("/assets/styles/detail.css"));
    assert!(html.contains("Search workflows"));
    assert!(html.contains("data-workflow-trigger"));
    assert!(html.contains("/api/workflows/feature-dev/trigger"));
    assert!(html.contains("Recent runs"));
}
