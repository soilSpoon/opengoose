use std::sync::Arc;

use opengoose_persistence::{AlertCondition, AlertMetric, AlertStore, Database};

use super::{AlertAction, create, run};

fn make_store() -> AlertStore {
    let db = Arc::new(Database::open_in_memory().unwrap());
    AlertStore::new(db)
}

#[test]
fn alert_metric_parse_valid_queue_backlog() {
    assert!(AlertMetric::parse("queue_backlog").is_some());
}

#[test]
fn alert_metric_parse_valid_failed_runs() {
    assert!(AlertMetric::parse("failed_runs").is_some());
}

#[test]
fn alert_metric_parse_valid_error_rate() {
    assert!(AlertMetric::parse("error_rate").is_some());
}

#[test]
fn alert_metric_parse_invalid_returns_none() {
    assert!(AlertMetric::parse("cpu_usage").is_none());
    assert!(AlertMetric::parse("").is_none());
    assert!(AlertMetric::parse("QUEUE_BACKLOG").is_none());
}

#[test]
fn alert_metric_variants_are_non_empty() {
    assert!(!AlertMetric::variants().is_empty());
}

#[test]
fn alert_condition_parse_valid_operators() {
    assert!(AlertCondition::parse("gt").is_some());
    assert!(AlertCondition::parse("lt").is_some());
    assert!(AlertCondition::parse("gte").is_some());
    assert!(AlertCondition::parse("lte").is_some());
}

#[test]
fn alert_condition_parse_invalid_returns_none() {
    assert!(AlertCondition::parse("eq").is_none());
    assert!(AlertCondition::parse("").is_none());
    assert!(AlertCondition::parse("GT").is_none());
}

#[test]
fn alert_condition_evaluate_greater_than() {
    let cond = AlertCondition::parse("gt").unwrap();
    assert!(cond.evaluate(10.0, 5.0));
    assert!(!cond.evaluate(5.0, 5.0));
    assert!(!cond.evaluate(4.0, 5.0));
}

#[test]
fn alert_condition_evaluate_less_than() {
    let cond = AlertCondition::parse("lt").unwrap();
    assert!(cond.evaluate(3.0, 5.0));
    assert!(!cond.evaluate(5.0, 5.0));
    assert!(!cond.evaluate(6.0, 5.0));
}

#[test]
fn alert_condition_evaluate_gte() {
    let cond = AlertCondition::parse("gte").unwrap();
    assert!(cond.evaluate(5.0, 5.0));
    assert!(cond.evaluate(6.0, 5.0));
    assert!(!cond.evaluate(4.0, 5.0));
}

#[test]
fn alert_condition_evaluate_lte() {
    let cond = AlertCondition::parse("lte").unwrap();
    assert!(cond.evaluate(5.0, 5.0));
    assert!(cond.evaluate(4.0, 5.0));
    assert!(!cond.evaluate(6.0, 5.0));
}

#[test]
fn create_rejects_invalid_metric() {
    let store = make_store();
    let err = create::run(&store, "my-rule", "bad_metric", "gt", 5.0, None).unwrap_err();
    assert!(err.to_string().contains("bad_metric"));
}

#[test]
fn create_rejects_invalid_condition() {
    let store = make_store();
    let err = create::run(&store, "my-rule", "queue_backlog", "neq", 5.0, None).unwrap_err();
    assert!(err.to_string().contains("neq"));
}

#[test]
fn alert_store_create_and_list() {
    let store = make_store();
    let rule = store
        .create(
            "test-rule",
            None,
            &AlertMetric::parse("queue_backlog").unwrap(),
            &AlertCondition::parse("gt").unwrap(),
            10.0,
            &[],
        )
        .unwrap();
    assert_eq!(rule.name, "test-rule");
    assert_eq!(rule.threshold, 10.0);

    let rules = store.list().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "test-rule");
}

#[test]
fn alert_store_list_empty_when_no_rules() {
    let store = make_store();
    let rules = store.list().unwrap();
    assert!(rules.is_empty());
}

#[test]
fn alert_store_delete_existing_rule() {
    let store = make_store();
    store
        .create(
            "to-delete",
            None,
            &AlertMetric::parse("error_rate").unwrap(),
            &AlertCondition::parse("gt").unwrap(),
            0.5,
            &[],
        )
        .unwrap();

    assert!(store.delete("to-delete").unwrap());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn alert_store_delete_nonexistent_rule_returns_false() {
    let store = make_store();
    assert!(!store.delete("nonexistent").unwrap());
}

#[test]
fn alert_store_set_enabled_toggles_rule() {
    let store = make_store();
    store
        .create(
            "toggle-rule",
            None,
            &AlertMetric::parse("failed_runs").unwrap(),
            &AlertCondition::parse("gte").unwrap(),
            3.0,
            &[],
        )
        .unwrap();

    assert!(store.set_enabled("toggle-rule", false).unwrap());
    let rules = store.list().unwrap();
    assert!(!rules[0].enabled);

    assert!(store.set_enabled("toggle-rule", true).unwrap());
    let rules = store.list().unwrap();
    assert!(rules[0].enabled);
}

#[test]
fn alert_store_set_enabled_nonexistent_returns_false() {
    let store = make_store();
    assert!(!store.set_enabled("missing", true).unwrap());
}

#[test]
fn alert_store_history_empty_initially() {
    let store = make_store();
    let history = store.history(10).unwrap();
    assert!(history.is_empty());
}

#[test]
fn alert_store_record_trigger_adds_history_entry() {
    let store = make_store();
    let rule = store
        .create(
            "fire-rule",
            None,
            &AlertMetric::parse("queue_backlog").unwrap(),
            &AlertCondition::parse("gt").unwrap(),
            5.0,
            &[],
        )
        .unwrap();

    store.record_trigger(&rule, 8.0).unwrap();
    let history = store.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].rule_name, "fire-rule");
    assert_eq!(history[0].value, 8.0);
}

#[test]
fn alert_store_current_metrics_returns_defaults() {
    let store = make_store();
    let metrics = store.current_metrics().unwrap();
    assert!(metrics.queue_backlog >= 0.0);
    assert!(metrics.failed_runs >= 0.0);
    assert!(metrics.error_rate >= 0.0);
}

#[test]
fn dispatch_create_persists_alert_rule() {
    let store = make_store();

    let result = run(
        AlertAction::Create {
            name: "nightly-backlog".to_string(),
            metric: "queue_backlog".to_string(),
            condition: "gt".to_string(),
            threshold: 7.0,
            description: Some("nightly queue backlog".to_string()),
        },
        &store,
    );
    assert!(result.is_ok(), "create should succeed: {result:?}");

    let rules = store.list().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "nightly-backlog");
    assert_eq!(rules[0].threshold, 7.0);
    assert_eq!(
        rules[0].description.as_deref(),
        Some("nightly queue backlog")
    );
}

#[test]
fn dispatch_disable_updates_existing_rule() {
    let store = make_store();
    store
        .create(
            "toggle-me",
            None,
            &AlertMetric::parse("failed_runs").unwrap(),
            &AlertCondition::parse("gte").unwrap(),
            2.0,
            &[],
        )
        .unwrap();

    let result = run(
        AlertAction::Disable {
            name: "toggle-me".to_string(),
        },
        &store,
    );
    assert!(result.is_ok(), "disable should succeed: {result:?}");
    assert!(!store.list().unwrap()[0].enabled);
}
