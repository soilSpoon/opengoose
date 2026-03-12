use super::*;

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
