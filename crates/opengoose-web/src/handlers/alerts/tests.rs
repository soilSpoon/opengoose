use axum::Json;
use axum::extract::{Path, Query, State};

use opengoose_persistence::{AlertCondition, AlertMetric};

use super::{
    AlertHistoryQueryParams, CreateAlertRequest, TestAlertQueryParams, alert_history, create_alert,
    delete_alert, list_alerts, test_alerts,
};
use crate::error::WebError;
use crate::handlers::test_support::make_state;

#[tokio::test]
async fn create_alert_persists_rule_and_list_alerts_returns_it() {
    let state = make_state();

    let Json(created) = create_alert(
        State(state.clone()),
        Json(CreateAlertRequest {
            name: "queue-backlog".into(),
            description: Some("Backlog exceeded".into()),
            metric: "queue_backlog".into(),
            condition: "gt".into(),
            threshold: 5.0,
        }),
    )
    .await
    .expect("create alert should succeed");

    assert_eq!(created.name, "queue-backlog");
    assert_eq!(created.metric, "queue_backlog");
    assert_eq!(created.condition, "gt");
    assert_eq!(created.threshold, 5.0);
    assert!(created.enabled);

    let Json(rules) = list_alerts(State(state))
        .await
        .expect("list alerts should succeed");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, created.id);
    assert_eq!(rules[0].description.as_deref(), Some("Backlog exceeded"));
}

#[tokio::test]
async fn create_alert_rejects_blank_name() {
    let err = create_alert(
        State(make_state()),
        Json(CreateAlertRequest {
            name: "   ".into(),
            description: None,
            metric: "queue_backlog".into(),
            condition: "gt".into(),
            threshold: 1.0,
        }),
    )
    .await
    .expect_err("blank names should be rejected");

    assert!(matches!(err, WebError::UnprocessableEntity(message) if message.contains("`name`")));
}

#[tokio::test]
async fn create_alert_rejects_non_finite_threshold() {
    let err = create_alert(
        State(make_state()),
        Json(CreateAlertRequest {
            name: "bad-threshold".into(),
            description: None,
            metric: "failed_runs".into(),
            condition: "gte".into(),
            threshold: f64::NAN,
        }),
    )
    .await
    .expect_err("non-finite thresholds should be rejected");

    assert!(
        matches!(err, WebError::UnprocessableEntity(message) if message.contains("`threshold`"))
    );
}

#[tokio::test]
async fn delete_alert_returns_deleted_payload_and_missing_rule_errors() {
    let state = make_state();
    state
        .alert_store
        .create(
            "queue-backlog",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            10.0,
            &[],
        )
        .expect("rule should be created");

    let Json(deleted) = delete_alert(State(state.clone()), Path("queue-backlog".into()))
        .await
        .expect("delete should succeed");
    assert_eq!(deleted["deleted"].as_str(), Some("queue-backlog"));

    let err = delete_alert(State(state), Path("queue-backlog".into()))
        .await
        .expect_err("deleting a missing rule should fail");
    assert!(matches!(err, WebError::NotFound(message) if message.contains("queue-backlog")));
}

#[tokio::test]
async fn alert_history_rejects_invalid_since_filter() {
    let err = alert_history(
        State(make_state()),
        Query(AlertHistoryQueryParams {
            since: Some("not-a-range".into()),
            ..Default::default()
        }),
    )
    .await
    .expect_err("invalid since filters should fail");

    assert!(matches!(err, WebError::BadRequest(_)));
}

#[tokio::test]
async fn test_alerts_records_only_enabled_matching_rules_and_exposes_history() {
    let state = make_state();
    state
        .alert_store
        .create(
            "queue-backlog",
            Some("fires on any backlog"),
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[],
        )
        .expect("enabled rule should be created");
    state
        .alert_store
        .create(
            "disabled-rule",
            None,
            &AlertMetric::FailedRuns,
            &AlertCondition::GreaterThan,
            -1.0,
            &[],
        )
        .expect("disabled rule should be created");
    state
        .alert_store
        .set_enabled("disabled-rule", false)
        .expect("rule should be disabled");

    let Json(result) =
        test_alerts(State(state.clone()), Query(TestAlertQueryParams::default()))
            .await
            .expect("test alerts should succeed");

    assert_eq!(result["metrics"]["queue_backlog"].as_f64(), Some(0.0));
    assert_eq!(result["metrics"]["failed_runs"].as_f64(), Some(0.0));
    assert_eq!(result["triggered"], serde_json::json!(["queue-backlog"]));

    let Json(history) = alert_history(State(state), Query(AlertHistoryQueryParams::default()))
        .await
        .expect("alert history should succeed");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].rule_name, "queue-backlog");
    assert_eq!(history[0].metric, "queue_backlog");
    assert_eq!(history[0].value, 0.0);
}

#[tokio::test]
async fn test_alerts_dry_run_does_not_persist_history() {
    let state = make_state();
    state
        .alert_store
        .create(
            "queue-backlog",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[],
        )
        .expect("rule should be created");

    let Json(result) = test_alerts(
        State(state.clone()),
        Query(TestAlertQueryParams {
            dry_run: true,
            ..Default::default()
        }),
    )
    .await
    .expect("dry run should succeed");

    assert_eq!(result["triggered"], serde_json::json!(["queue-backlog"]));
    assert_eq!(result["dry_run"], serde_json::json!(true));

    let Json(history) = alert_history(State(state), Query(AlertHistoryQueryParams::default()))
        .await
        .expect("history lookup should succeed");
    assert!(history.is_empty());
}
