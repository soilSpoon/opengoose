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

// ── types.rs edge-case coverage ─────────────────────────────────────

#[test]
fn test_alert_action_deserialize_malformed_json() {
    let result = AlertAction::deserialize_list("not valid json");
    assert!(result.is_empty(), "malformed JSON should return empty vec");
}

#[test]
fn test_alert_action_deserialize_empty_string() {
    let result = AlertAction::deserialize_list("");
    assert!(result.is_empty());
}

#[test]
fn test_alert_rule_try_from_valid_row() {
    use crate::models::AlertRuleRow;

    let row = AlertRuleRow {
        id: "r1".into(),
        name: "test-rule".into(),
        description: Some("a description".into()),
        metric: "queue_backlog".into(),
        condition: "gt".into(),
        threshold: 100.0,
        enabled: 1,
        actions: r#"[{"type":"log"}]"#.into(),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-02".into(),
    };

    let rule: AlertRule = row.try_into().unwrap();
    assert_eq!(rule.id, "r1");
    assert_eq!(rule.name, "test-rule");
    assert_eq!(rule.description.as_deref(), Some("a description"));
    assert_eq!(rule.metric, AlertMetric::QueueBacklog);
    assert_eq!(rule.condition, AlertCondition::GreaterThan);
    assert_eq!(rule.threshold, 100.0);
    assert!(rule.enabled);
    assert_eq!(rule.actions.len(), 1);
}

#[test]
fn test_alert_rule_try_from_disabled() {
    use crate::models::AlertRuleRow;

    let row = AlertRuleRow {
        id: "r2".into(),
        name: "disabled".into(),
        description: None,
        metric: "failed_runs".into(),
        condition: "gte".into(),
        threshold: 5.0,
        enabled: 0,
        actions: "[]".into(),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-01".into(),
    };

    let rule: AlertRule = row.try_into().unwrap();
    assert!(!rule.enabled);
    assert!(rule.description.is_none());
    assert!(rule.actions.is_empty());
}

#[test]
fn test_alert_rule_try_from_invalid_metric() {
    use crate::models::AlertRuleRow;

    let row = AlertRuleRow {
        id: "r3".into(),
        name: "bad-metric".into(),
        description: None,
        metric: "bogus_metric".into(),
        condition: "gt".into(),
        threshold: 1.0,
        enabled: 1,
        actions: "[]".into(),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-01".into(),
    };

    let result: Result<AlertRule, _> = row.try_into();
    assert!(result.is_err());
}

#[test]
fn test_alert_rule_try_from_invalid_condition() {
    use crate::models::AlertRuleRow;

    let row = AlertRuleRow {
        id: "r4".into(),
        name: "bad-condition".into(),
        description: None,
        metric: "error_rate".into(),
        condition: "unknown_op".into(),
        threshold: 1.0,
        enabled: 1,
        actions: "[]".into(),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-01".into(),
    };

    let result: Result<AlertRule, _> = row.try_into();
    assert!(result.is_err());
}

#[test]
fn test_alert_history_entry_from_row() {
    use crate::models::AlertHistoryRow;

    let row = AlertHistoryRow {
        id: 42,
        rule_id: "r1".into(),
        rule_name: "my-rule".into(),
        metric: "queue_backlog".into(),
        value: 99.5,
        triggered_at: "2026-03-12T10:00:00Z".into(),
    };

    let entry: AlertHistoryEntry = row.into();
    assert_eq!(entry.id, 42);
    assert_eq!(entry.rule_id, "r1");
    assert_eq!(entry.rule_name, "my-rule");
    assert_eq!(entry.metric, "queue_backlog");
    assert_eq!(entry.value, 99.5);
    assert_eq!(entry.triggered_at, "2026-03-12T10:00:00Z");
}

#[test]
fn test_alert_history_query_default() {
    let q = AlertHistoryQuery::default();
    assert_eq!(q.limit, 50);
    assert_eq!(q.offset, 0);
    assert!(q.rule.is_none());
    assert!(q.since.is_none());
}

#[test]
fn test_condition_evaluate_boundary_values() {
    // Exact boundary: gt is strict
    assert!(!AlertCondition::GreaterThan.evaluate(5.0, 5.0));
    // Exact boundary: lt is strict
    assert!(!AlertCondition::LessThan.evaluate(5.0, 5.0));
    // Exact boundary: gte includes equal
    assert!(AlertCondition::GreaterThanOrEqual.evaluate(5.0, 5.0));
    // Exact boundary: lte includes equal
    assert!(AlertCondition::LessThanOrEqual.evaluate(5.0, 5.0));
}

#[test]
fn test_condition_evaluate_negative_values() {
    assert!(AlertCondition::LessThan.evaluate(-10.0, -5.0));
    assert!(AlertCondition::GreaterThan.evaluate(-5.0, -10.0));
}

#[test]
fn test_condition_evaluate_zero() {
    assert!(AlertCondition::GreaterThan.evaluate(0.001, 0.0));
    assert!(AlertCondition::LessThan.evaluate(-0.001, 0.0));
    assert!(AlertCondition::GreaterThanOrEqual.evaluate(0.0, 0.0));
    assert!(AlertCondition::LessThanOrEqual.evaluate(0.0, 0.0));
}

#[test]
fn test_alert_action_webhook_json_format() {
    let actions = vec![AlertAction::Webhook {
        url: "https://example.com".into(),
    }];
    let json = AlertAction::serialize_list(&actions);
    assert!(json.contains("\"type\":\"webhook\""));
    assert!(json.contains("\"url\":\"https://example.com\""));
}

#[test]
fn test_alert_action_channel_message_json_format() {
    let actions = vec![AlertAction::ChannelMessage {
        platform: "discord".into(),
        channel_id: "12345".into(),
    }];
    let json = AlertAction::serialize_list(&actions);
    assert!(json.contains("\"type\":\"channel_message\""));
    assert!(json.contains("\"platform\":\"discord\""));
    assert!(json.contains("\"channel_id\":\"12345\""));
}

#[test]
fn test_alert_rule_try_from_malformed_actions_json() {
    use crate::models::AlertRuleRow;

    let row = AlertRuleRow {
        id: "r5".into(),
        name: "bad-actions".into(),
        description: None,
        metric: "queue_backlog".into(),
        condition: "gt".into(),
        threshold: 1.0,
        enabled: 1,
        actions: "not json".into(),
        created_at: "2026-01-01".into(),
        updated_at: "2026-01-01".into(),
    };

    // Should succeed — deserialize_list returns empty vec for malformed JSON
    let rule: AlertRule = row.try_into().unwrap();
    assert!(rule.actions.is_empty());
}
