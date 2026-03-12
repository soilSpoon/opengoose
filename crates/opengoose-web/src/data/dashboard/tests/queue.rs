use super::*;

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
