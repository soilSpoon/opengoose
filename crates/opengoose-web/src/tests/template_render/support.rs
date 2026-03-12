use crate::data::{
    MessageBubble, MetaRow, MetricCard, Notice, PluginDetailView, QueueDetailView,
    QueueMessageView, ScheduleEditorView, ScheduleHistoryItem, SelectOption, SessionDetailView,
    SessionExportAction, TriggerDetailView, WorkflowAutomationView, WorkflowDetailView,
    WorkflowRunView, WorkflowStepView,
};

pub(super) fn sample_session_detail() -> SessionDetailView {
    SessionDetailView {
        session_key: "discord:ops:bridge".into(),
        title: "Session ops".into(),
        subtitle: "discord / ops".into(),
        source_label: "Live runtime".into(),
        export_actions: vec![
            SessionExportAction {
                label: "Export JSON".into(),
                href: "/api/sessions/discord%3Aops%3Abridge/export?format=json".into(),
            },
            SessionExportAction {
                label: "Export Markdown".into(),
                href: "/api/sessions/discord%3Aops%3Abridge/export?format=md".into(),
            },
        ],
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

pub(super) fn sample_queue_detail() -> QueueDetailView {
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

pub(super) fn sample_plugin_detail() -> PluginDetailView {
    PluginDetailView {
        title: "ops-tools".into(),
        subtitle: "Operational helpers for dashboards and workflows.".into(),
        source_label: "/home/dh/.opengoose/plugins/ops-tools".into(),
        status_label: "Ready".into(),
        status_tone: "success",
        lifecycle_label: "Enabled".into(),
        lifecycle_tone: "sage",
        runtime_label: "Runtime initialized".into(),
        runtime_tone: "success",
        status_summary: "1 declared skill is registered in the active runtime.".into(),
        runtime_note: Some("registered 1 declared skill(s)".into()),
        meta: vec![
            MetaRow {
                label: "Version".into(),
                value: "1.2.3".into(),
            },
            MetaRow {
                label: "Runtime".into(),
                value: "Runtime initialized".into(),
            },
        ],
        capabilities: vec!["skill".into(), "channel_adapter".into()],
        capabilities_hint: "No capabilities declared.".into(),
        registered_skills: vec!["ops-tools/check-health".into()],
        missing_skills: vec![],
        notice: Some(Notice {
            text: "Installed plugin `ops-tools`.".into(),
            tone: "success",
        }),
        install_source_path: String::new(),
        toggle_label: "Disable plugin".into(),
        delete_label: "ops-tools".into(),
        is_placeholder: false,
    }
}

pub(super) fn sample_workflow_detail() -> WorkflowDetailView {
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

pub(super) fn sample_schedule_detail() -> ScheduleEditorView {
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

pub(super) fn sample_new_schedule_detail() -> ScheduleEditorView {
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

pub(super) fn sample_trigger_detail() -> TriggerDetailView {
    TriggerDetailView {
        name: "on-pr-open".into(),
        trigger_type: "webhook_received".into(),
        team_name: "code-review".into(),
        input: "Review the latest pull request".into(),
        condition_json: r#"{"path":"/pr"}"#.into(),
        enabled: true,
        fire_count: 4,
        last_fired_at: "2026-03-10 12:34".into(),
        created_at: "2026-03-09 09:00".into(),
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
        notice: Some(Notice {
            text: "Trigger saved.".into(),
            tone: "success",
        }),
        is_placeholder: false,
    }
}

pub(super) fn sample_trigger_placeholder_detail() -> TriggerDetailView {
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
        notice: Some(Notice {
            text: "Name, type, and team are required to create a trigger.".into(),
            tone: "danger",
        }),
        is_placeholder: true,
    }
}
