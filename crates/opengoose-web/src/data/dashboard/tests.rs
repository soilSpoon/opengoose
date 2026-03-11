use std::sync::Arc;

use opengoose_persistence::{
    AgentMessage, AgentMessageStatus, AgentMessageStore, Database, HistoryMessage, MessageQueue,
    MessageType, OrchestrationRun, OrchestrationStore, QueueStats, RunStatus, SessionStore,
    SessionSummary,
};
use opengoose_types::{Platform, SessionKey};

use super::activity::{activity_meta, build_dashboard_activities, synthetic_dashboard_activities};
use super::load_dashboard;
use super::metrics::{build_duration_bars, build_status_segments, duration_stats};
use crate::data::sessions::SessionRecord;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().expect("db should open"))
}

fn empty_queue_stats() -> QueueStats {
    QueueStats {
        pending: 0,
        processing: 0,
        completed: 0,
        failed: 0,
        dead: 0,
    }
}

fn sample_run(
    team_run_id: &str,
    status: RunStatus,
    created_at: &str,
    updated_at: &str,
) -> OrchestrationRun {
    OrchestrationRun {
        team_run_id: team_run_id.into(),
        session_key: "discord:test:ops".into(),
        team_name: format!("team-{team_run_id}"),
        workflow: "chain".into(),
        input: "Investigate the live dashboard state".into(),
        status,
        current_step: 1,
        total_steps: 3,
        result: None,
        created_at: created_at.into(),
        updated_at: updated_at.into(),
    }
}

fn sample_session(
    session_key: &str,
    active_team: Option<&str>,
    role: &str,
    content: &str,
) -> SessionRecord {
    SessionRecord {
        summary: SessionSummary {
            session_key: session_key.into(),
            active_team: active_team.map(str::to_string),
            created_at: "2026-03-10 10:00".into(),
            updated_at: "2026-03-10 10:15".into(),
        },
        messages: vec![HistoryMessage {
            role: role.into(),
            content: content.into(),
            author: Some("tester".into()),
            created_at: "2026-03-10 10:15".into(),
        }],
    }
}

#[test]
fn build_duration_bars_scales_with_run_length() {
    let runs = vec![
        sample_run(
            "run-a",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:30:00",
        ),
        sample_run(
            "run-b",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        ),
    ];

    let bars = build_duration_bars(&runs);
    assert_eq!(bars.len(), 2);
    assert_eq!(bars[0].value, "30m 0s");
    assert!(bars[0].height > bars[1].height);
}

#[test]
fn build_status_segments_spreads_zero_totals_evenly() {
    let segments = build_status_segments(vec![
        ("Running", 0, "cyan"),
        ("Completed", 0, "sage"),
        ("Failed", 0, "rose"),
    ]);

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].width, 33);
    assert_eq!(segments[1].width, 33);
    assert_eq!(segments[2].width, 33);
}

#[test]
fn build_status_segments_omits_zero_values_once_total_exists() {
    let segments = build_status_segments(vec![
        ("Running", 2, "cyan"),
        ("Completed", 0, "sage"),
        ("Failed", 1, "rose"),
    ]);

    let labels: Vec<_> = segments
        .iter()
        .map(|segment| segment.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Running", "Failed"]);
    assert_eq!(segments[0].width, 67);
    assert_eq!(segments[1].width, 33);
}

#[test]
fn duration_stats_returns_placeholder_when_no_runs_exist() {
    let stats = duration_stats(&[]);

    assert_eq!(stats.average_label, None);
    assert_eq!(
        stats.note,
        "Run duration will appear once persisted timestamps accumulate."
    );
}

#[test]
fn duration_stats_reports_average_and_longest_durations() {
    let stats = duration_stats(&[
        sample_run(
            "run-a",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        ),
        sample_run(
            "run-b",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:15:00",
        ),
    ]);

    assert_eq!(stats.average_label.as_deref(), Some("10m 0s"));
    assert_eq!(stats.note, "2 captured runs · longest 15m 0s");
}

#[test]
fn build_duration_bars_uses_fixed_height_for_zero_length_runs() {
    let bars = build_duration_bars(&[sample_run(
        "run-a",
        RunStatus::Completed,
        "2026-03-10 10:00:00",
        "2026-03-10 10:00:00",
    )]);

    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].value, "0s");
    assert_eq!(bars[0].height, 34);
}

#[test]
fn build_duration_bars_skips_runs_with_invalid_timestamps() {
    let bars = build_duration_bars(&[
        sample_run("bad-run", RunStatus::Completed, "not-a-time", "also-bad"),
        sample_run(
            "good-run",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:01:00",
        ),
    ]);

    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].label, "team-good-run");
}

#[test]
fn activity_meta_formats_directed_messages() {
    let message = AgentMessage {
        id: 1,
        session_key: "discord:ns:studio-a:ops".into(),
        from_agent: "architect".into(),
        to_agent: Some("reviewer".into()),
        channel: None,
        payload: "Check the dashboard".into(),
        status: AgentMessageStatus::Pending,
        created_at: "2026-03-10 10:00".into(),
        delivered_at: None,
    };

    assert_eq!(
        activity_meta(&message),
        "Directed to reviewer · discord:ns:studio-a:ops · pending"
    );
}

#[test]
fn activity_meta_formats_channel_messages() {
    let message = AgentMessage {
        id: 1,
        session_key: "discord:ns:studio-a:ops".into(),
        from_agent: "architect".into(),
        to_agent: None,
        channel: Some("ops".into()),
        payload: "Check the dashboard".into(),
        status: AgentMessageStatus::Delivered,
        created_at: "2026-03-10 10:00".into(),
        delivered_at: Some("2026-03-10 10:01".into()),
    };

    assert_eq!(
        activity_meta(&message),
        "Published to #ops · discord:ns:studio-a:ops · delivered"
    );
}

#[test]
fn activity_meta_falls_back_to_session_when_no_target_exists() {
    let message = AgentMessage {
        id: 1,
        session_key: "discord:ns:studio-a:ops".into(),
        from_agent: "architect".into(),
        to_agent: None,
        channel: None,
        payload: "Check the dashboard".into(),
        status: AgentMessageStatus::Acknowledged,
        created_at: "2026-03-10 10:00".into(),
        delivered_at: Some("2026-03-10 10:01".into()),
    };

    assert_eq!(
        activity_meta(&message),
        "discord:ns:studio-a:ops · acknowledged"
    );
}

#[test]
fn build_dashboard_activities_returns_mock_seed_for_empty_preview() {
    let items =
        build_dashboard_activities(test_db(), &[], &[], &empty_queue_stats(), true).unwrap();

    assert_eq!(items.len(), 3);
    assert_eq!(items[0].actor, "architect");
    assert!(items[0].meta.contains("#ops"));
}

#[test]
fn build_dashboard_activities_prefers_persisted_messages() {
    let db = test_db();
    let store = AgentMessageStore::new(db.clone());
    let id = store
        .send_directed(
            "discord:ns:studio-a:ops",
            "architect",
            "reviewer",
            "Check the live dashboard",
        )
        .unwrap();
    store.mark_delivered(id).unwrap();

    let items = build_dashboard_activities(db, &[], &[], &empty_queue_stats(), true).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].actor, "architect");
    assert!(items[0].meta.contains("Directed to reviewer"));
    assert_eq!(items[0].detail, "Check the live dashboard");
    assert_eq!(items[0].tone, "cyan");
}

#[test]
fn build_dashboard_activities_uses_synthetic_feed_for_live_runtime() {
    let items = build_dashboard_activities(
        test_db(),
        &[sample_run(
            "run-a",
            RunStatus::Running,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        )],
        &[sample_session(
            "discord:ns:studio-a:ops",
            Some("feature-dev"),
            "assistant",
            "Follow-up is queued",
        )],
        &empty_queue_stats(),
        false,
    )
    .unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].actor, "team-run-a");
    assert_eq!(items[1].actor, "discord:ns:studio-a:ops");
}

#[test]
fn synthetic_dashboard_activities_includes_dead_letter_notice() {
    let runs = vec![
        sample_run(
            "run-a",
            RunStatus::Running,
            "2026-03-10 10:00:00",
            "2026-03-10 10:05:00",
        ),
        sample_run(
            "run-b",
            RunStatus::Completed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:08:00",
        ),
        sample_run(
            "run-c",
            RunStatus::Failed,
            "2026-03-10 10:00:00",
            "2026-03-10 10:06:00",
        ),
    ];
    let sessions = vec![
        sample_session(
            "discord:ns:studio-a:ops",
            Some("feature-dev"),
            "assistant",
            "Follow-up is queued",
        ),
        sample_session("telegram:direct:founder", None, "user", "What changed?"),
    ];

    let items = synthetic_dashboard_activities(
        &runs,
        &sessions,
        &QueueStats {
            dead: 2,
            ..empty_queue_stats()
        },
    );

    assert_eq!(items.len(), 6);
    assert!(items.iter().any(|item| item.actor == "queue-monitor"));
}

#[test]
fn load_dashboard_returns_mock_preview_for_empty_runtime() {
    let dashboard = load_dashboard(test_db()).unwrap();

    assert_eq!(dashboard.intro.mode_label, "Mock preview");
    assert_eq!(dashboard.intro.mode_tone, "neutral");
    assert_eq!(dashboard.sessions.len(), 2);
    assert_eq!(dashboard.runs.len(), 3);
    assert_eq!(dashboard.gateway_panel.cards.len(), 4);
    assert_eq!(dashboard.alerts[0].eyebrow, "Preview Mode");
}

#[test]
fn load_dashboard_returns_live_runtime_with_queue_and_runtime_alerts() {
    let db = test_db();
    let session_store = SessionStore::new(db.clone());
    let run_store = OrchestrationStore::new(db.clone());
    let queue = MessageQueue::new(db.clone());
    let session_key = SessionKey::new(Platform::Discord, "studio-a", "ops");
    let session_id = session_key.to_stable_id();

    session_store
        .append_user_message(&session_key, "Need a status check", Some("pm"))
        .unwrap();
    session_store
        .set_active_team(&session_key, Some("feature-dev"))
        .unwrap();
    run_store
        .create_run(
            "run-live",
            &session_id,
            "feature-dev",
            "chain",
            "Investigate the live dashboard state",
            3,
        )
        .unwrap();
    queue
        .enqueue(
            &session_id,
            "run-live",
            "planner",
            "developer",
            "Pick up the task",
            MessageType::Task,
        )
        .unwrap();

    let dashboard = load_dashboard(db).unwrap();

    assert_eq!(dashboard.intro.mode_label, "Live runtime");
    assert_eq!(dashboard.sessions.len(), 1);
    assert_eq!(dashboard.runs.len(), 1);
    assert!(
        dashboard
            .alerts
            .iter()
            .any(|alert| alert.eyebrow == "Queue Flow")
    );
    assert!(
        dashboard
            .alerts
            .iter()
            .any(|alert| alert.eyebrow == "Runtime Active")
    );
}
