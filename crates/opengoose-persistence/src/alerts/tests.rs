use std::sync::Arc;

use diesel::prelude::*;

use super::*;
use crate::db::Database;

fn make_store() -> AlertStore {
    let db = Arc::new(Database::open_in_memory().unwrap());
    AlertStore::new(db)
}

#[test]
fn test_create_and_list() {
    let store = make_store();
    store
        .create(
            "high-queue",
            Some("Queue too large"),
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            100.0,
            &[],
        )
        .unwrap();

    let rules = store.list().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "high-queue");
    assert_eq!(rules[0].threshold, 100.0);
    assert!(rules[0].enabled);
}

#[test]
fn test_delete_rule() {
    let store = make_store();
    store
        .create(
            "temp-rule",
            None,
            &AlertMetric::FailedRuns,
            &AlertCondition::GreaterThanOrEqual,
            5.0,
            &[],
        )
        .unwrap();

    assert!(store.delete("temp-rule").unwrap());
    assert!(!store.delete("temp-rule").unwrap());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn test_get_by_name() {
    let store = make_store();
    store
        .create(
            "my-rule",
            None,
            &AlertMetric::ErrorRate,
            &AlertCondition::LessThan,
            10.0,
            &[],
        )
        .unwrap();

    let rule = store.get_by_name("my-rule").unwrap();
    assert!(rule.is_some());
    assert!(store.get_by_name("missing").unwrap().is_none());
}

#[test]
fn test_set_enabled() {
    let store = make_store();
    store
        .create(
            "toggle-rule",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            50.0,
            &[],
        )
        .unwrap();

    assert!(store.set_enabled("toggle-rule", false).unwrap());
    let rule = store.get_by_name("toggle-rule").unwrap().unwrap();
    assert!(!rule.enabled);

    assert!(store.set_enabled("toggle-rule", true).unwrap());
    let rule = store.get_by_name("toggle-rule").unwrap().unwrap();
    assert!(rule.enabled);
}

#[test]
fn test_record_trigger_and_history() {
    let store = make_store();
    let rule = store
        .create(
            "fired-rule",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            10.0,
            &[],
        )
        .unwrap();

    store.record_trigger(&rule, 42.0).unwrap();
    store.record_trigger(&rule, 55.0).unwrap();

    let history = store.history(10).unwrap();
    assert_eq!(history.len(), 2);
    // Newest first
    assert_eq!(history[0].value, 55.0);
}

#[test]
fn test_condition_evaluate() {
    assert!(AlertCondition::GreaterThan.evaluate(10.0, 5.0));
    assert!(!AlertCondition::GreaterThan.evaluate(5.0, 10.0));
    assert!(AlertCondition::LessThan.evaluate(3.0, 10.0));
    assert!(AlertCondition::GreaterThanOrEqual.evaluate(5.0, 5.0));
    assert!(AlertCondition::LessThanOrEqual.evaluate(5.0, 5.0));
}

#[test]
fn test_metric_roundtrip() {
    for m in [
        AlertMetric::QueueBacklog,
        AlertMetric::FailedRuns,
        AlertMetric::ErrorRate,
    ] {
        assert_eq!(AlertMetric::parse(m.as_str()), Some(m));
    }
    assert_eq!(AlertMetric::parse("bogus"), None);
}

#[test]
fn test_condition_roundtrip() {
    for c in [
        AlertCondition::GreaterThan,
        AlertCondition::LessThan,
        AlertCondition::GreaterThanOrEqual,
        AlertCondition::LessThanOrEqual,
    ] {
        assert_eq!(AlertCondition::parse(c.as_str()), Some(c));
    }
    assert_eq!(AlertCondition::parse("bogus"), None);
}

#[test]
fn test_history_empty() {
    let store = make_store();
    let history = store.history(10).unwrap();
    assert!(history.is_empty());
}

#[test]
fn test_current_metrics_empty_db() {
    let store = make_store();
    let metrics = store.current_metrics().unwrap();
    assert_eq!(metrics.queue_backlog, 0.0);
    assert_eq!(metrics.failed_runs, 0.0);
    assert_eq!(metrics.error_rate, 0.0);
}

#[test]
fn test_current_metrics_reflects_queue_and_runs() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = AlertStore::new(db.clone());

    // Ensure session exists (message_queue has FK on session_key)
    db.with(|conn| {
        diesel::sql_query("INSERT INTO sessions (session_key) VALUES ('sess1')").execute(conn)?;
        Ok(())
    })
    .unwrap();

    // Insert a pending message into the queue
    let mq = crate::MessageQueue::new(db.clone());
    mq.enqueue(
        "sess1",
        "run1",
        "agent-a",
        "agent-b",
        "payload",
        crate::MessageType::Task,
    )
    .unwrap();

    // Insert a failed orchestration run
    let orch = crate::OrchestrationStore::new(db.clone());
    orch.create_run("run1", "sess1", "team1", "chain", "input", 2)
        .unwrap();
    orch.fail_run("run1", "error msg").unwrap();

    // Insert an orchestration run with 'error' status via raw SQL
    // (no API method sets this status, but current_metrics queries for it)
    db.with(|conn| {
        diesel::sql_query(
            "INSERT INTO sessions (session_key) VALUES ('sess2') ON CONFLICT DO NOTHING",
        )
        .execute(conn)?;
        diesel::sql_query(
            "INSERT INTO orchestration_runs \
             (team_run_id, session_key, team_name, workflow, input, status, total_steps) \
             VALUES ('run2', 'sess2', 'team2', 'chain', 'input2', 'error', 1)",
        )
        .execute(conn)?;
        Ok(())
    })
    .unwrap();

    let metrics = store.current_metrics().unwrap();
    assert_eq!(metrics.queue_backlog, 1.0);
    assert_eq!(metrics.failed_runs, 1.0);
    assert_eq!(metrics.error_rate, 1.0);
}

#[test]
fn test_alert_metric_display() {
    assert_eq!(format!("{}", AlertMetric::QueueBacklog), "queue_backlog");
    assert_eq!(format!("{}", AlertMetric::FailedRuns), "failed_runs");
    assert_eq!(format!("{}", AlertMetric::ErrorRate), "error_rate");
}

#[test]
fn test_alert_condition_display() {
    assert_eq!(format!("{}", AlertCondition::GreaterThan), "gt");
    assert_eq!(format!("{}", AlertCondition::LessThan), "lt");
    assert_eq!(format!("{}", AlertCondition::GreaterThanOrEqual), "gte");
    assert_eq!(format!("{}", AlertCondition::LessThanOrEqual), "lte");
}

#[test]
fn test_alert_metric_variants() {
    let v = AlertMetric::variants();
    assert_eq!(v.len(), 3);
    assert!(v.contains(&"queue_backlog"));
}

#[test]
fn test_alert_condition_variants() {
    let v = AlertCondition::variants();
    assert_eq!(v.len(), 4);
    assert!(v.contains(&"gt"));
}

#[test]
fn test_set_enabled_nonexistent() {
    let store = make_store();
    let result = store.set_enabled("does-not-exist", false).unwrap();
    assert!(!result);
}

#[test]
fn test_alert_action_serialization_roundtrip() {
    let actions = vec![
        AlertAction::Log,
        AlertAction::Webhook {
            url: "https://example.com/hook".into(),
        },
        AlertAction::ChannelMessage {
            platform: "slack".into(),
            channel_id: "C123".into(),
        },
    ];
    let json = AlertAction::serialize_list(&actions);
    let roundtrip = AlertAction::deserialize_list(&json);
    assert_eq!(actions, roundtrip);
}

#[test]
fn test_alert_action_empty_list() {
    let json = AlertAction::serialize_list(&[]);
    assert_eq!(json, "[]");
    let result = AlertAction::deserialize_list(&json);
    assert!(result.is_empty());
}

#[test]
fn test_create_with_actions() {
    let store = make_store();
    let actions = vec![
        AlertAction::Log,
        AlertAction::Webhook {
            url: "https://hooks.example.com/alert".into(),
        },
    ];
    let rule = store
        .create(
            "action-rule",
            Some("Rule with actions"),
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            10.0,
            &actions,
        )
        .unwrap();

    assert_eq!(rule.actions.len(), 2);
    assert!(matches!(rule.actions[0], AlertAction::Log));
    assert!(
        matches!(&rule.actions[1], AlertAction::Webhook { url } if url == "https://hooks.example.com/alert")
    );

    // Verify persistence via list
    let rules = store.list().unwrap();
    assert_eq!(rules[0].actions.len(), 2);
}
