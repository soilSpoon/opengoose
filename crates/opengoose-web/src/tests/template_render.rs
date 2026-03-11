use crate::data::{
    ActivityItem, AlertCard, DashboardView, MessageBubble, MetaRow, MetricCard, Notice,
    QueueDetailView, QueueMessageView, RemoteAgentRowView, RemoteAgentsPageView, RunListItem,
    ScheduleEditorView, ScheduleHistoryItem, ScheduleListItem, SchedulesPageView, SelectOption,
    SessionDetailView, SessionListItem, SessionsPageView, StatusSegment, TrendBar,
    TriggerDetailView, WorkflowAutomationView, WorkflowDetailView, WorkflowListItem,
    WorkflowRunView, WorkflowStepView, WorkflowsPageView,
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

fn sample_remote_agents_page() -> RemoteAgentsPageView {
    RemoteAgentsPageView {
        mode_label: "Live registry".into(),
        mode_tone: "success",
        stream_summary: "Registry snapshots refresh over SSE.".into(),
        snapshot_label: "Snapshot 12:34:56 UTC".into(),
        metrics: vec![MetricCard {
            label: "Connected".into(),
            value: "1".into(),
            note: "Currently registered remote agents".into(),
            tone: "cyan",
        }],
        agents: vec![RemoteAgentRowView {
            name: "remote-a".into(),
            capabilities: vec!["execute".into(), "relay".into()],
            capabilities_text: "execute, relay".into(),
            endpoint: "ws://remote-a:9000".into(),
            connected_for: "5m 12s".into(),
            connected_sort: "312".into(),
            heartbeat_age: "4s".into(),
            heartbeat_sort: "4".into(),
            status_label: "Healthy".into(),
            status_tone: "success",
            disconnect_path: "/remote-agents/remote-a/disconnect".into(),
        }],
        websocket_url: "ws://opengoose.test/api/agents/connect".into(),
        heartbeat_interval_label: "5s".into(),
        heartbeat_timeout_label: "30s".into(),
        handshake_preview: "{\n  \"type\": \"handshake\"\n}".into(),
    }
}

fn sample_trigger_detail() -> TriggerDetailView {
    TriggerDetailView {
        name: "on-pr-open".into(),
        trigger_type: "webhook_received".into(),
        team_name: "code-review".into(),
        input: "Review this PR".into(),
        condition_json: "{\"event\":\"pull_request\"}".into(),
        enabled: true,
        fire_count: 3,
        last_fired_at: "2026-03-10 12:35".into(),
        created_at: "2026-03-09 08:00".into(),
        meta: vec![
            MetaRow {
                label: "Type".into(),
                value: "Webhook".into(),
            },
            MetaRow {
                label: "Team".into(),
                value: "code-review".into(),
            },
        ],
        status_label: "enabled".into(),
        status_tone: "success",
        delete_api_url: "/api/triggers/on-pr-open".into(),
        toggle_enabled_api_url: "/api/triggers/on-pr-open/enabled".into(),
        test_api_url: "/api/triggers/on-pr-open/test".into(),
        update_api_url: "/api/triggers/on-pr-open".into(),
        notice: Some(Notice {
            text: "Trigger saved.".into(),
            tone: "success",
        }),
        is_placeholder: false,
    }
}

fn sample_trigger_placeholder() -> TriggerDetailView {
    TriggerDetailView {
        name: String::new(),
        trigger_type: String::new(),
        team_name: String::new(),
        input: String::new(),
        condition_json: "{}".into(),
        enabled: false,
        fire_count: 0,
        last_fired_at: "never".into(),
        created_at: String::new(),
        meta: vec![],
        status_label: "none".into(),
        status_tone: "neutral",
        delete_api_url: String::new(),
        toggle_enabled_api_url: String::new(),
        test_api_url: String::new(),
        update_api_url: String::new(),
        notice: Some(Notice {
            text: "Create your first trigger.".into(),
            tone: "neutral",
        }),
        is_placeholder: true,
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
fn remote_agents_live_template_renders_table_and_connection_panels() {
    let html = routes::test_support::render_remote_agents_live(sample_remote_agents_page())
        .expect("remote agents live partial renders");

    assert!(html.contains("Connected agents"));
    assert!(html.contains("Search agents"));
    assert!(html.contains("remote-a"));
    assert!(html.contains("Handshake payload"));
    assert!(html.contains("/remote-agents/remote-a/disconnect"));
}

#[test]
fn trigger_detail_template_renders_actions_and_create_card() {
    let html = routes::test_support::render_trigger_detail(sample_trigger_detail())
        .expect("trigger detail renders");

    assert!(html.contains("Trigger saved."));
    assert!(html.contains("Run test"));
    assert!(html.contains("Save changes"));
    assert!(html.contains("Create new trigger"));
    assert!(html.contains("new-trigger-name"));
}

#[test]
fn trigger_detail_placeholder_template_renders_empty_state_form() {
    let html = routes::test_support::render_trigger_detail(sample_trigger_placeholder())
        .expect("trigger placeholder renders");

    assert!(html.contains("No triggers configured"));
    assert!(html.contains("Create your first trigger."));
    assert!(html.contains("trigger-create-form"));
    assert!(html.contains("Create trigger"));
    assert!(!html.contains("Create new trigger"));
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

    assert!(html.contains("Search workflows"));
    assert!(html.contains("data-bind:workflow-trigger-input"));
    assert!(html.contains("/workflows/feature-dev/trigger"));
    assert!(html.contains("Recent runs"));
}
