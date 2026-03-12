use std::sync::Arc;
use std::time::Duration;

use opengoose_persistence::{AlertAction, AlertCondition, AlertMetric, AlertStore, Database};
use opengoose_types::EventBus;

use super::*;

fn make_dispatcher() -> AlertDispatcher {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    let event_bus = EventBus::new(64);
    AlertDispatcher::with_cooldown(store, event_bus, Duration::from_millis(0))
}

#[tokio::test]
async fn test_no_rules_returns_empty() {
    let dispatcher = make_dispatcher();
    let fired = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert!(fired.is_empty());
}

#[tokio::test]
async fn test_rule_with_log_action_fires_when_threshold_exceeded() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    // queue_backlog is 0; threshold -1.0 => 0 > -1 triggers
    store
        .create(
            "low-threshold",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::Log],
        )
        .unwrap();

    let event_bus = EventBus::new(64);
    let dispatcher =
        AlertDispatcher::with_cooldown(store.clone(), event_bus, Duration::from_millis(0));

    let fired = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert_eq!(fired, vec!["low-threshold"]);

    let history = store.history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].rule_name, "low-threshold");
}

#[tokio::test]
async fn test_disabled_rule_is_skipped() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    store
        .create(
            "disabled",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::Log],
        )
        .unwrap();
    store.set_enabled("disabled", false).unwrap();

    let dispatcher =
        AlertDispatcher::with_cooldown(store.clone(), EventBus::new(16), Duration::from_millis(0));

    let fired = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert!(fired.is_empty());
    assert!(store.history(10).unwrap().is_empty());
}

#[tokio::test]
async fn test_deduplication_prevents_refiring() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    store
        .create(
            "dedup-rule",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::Log],
        )
        .unwrap();

    // Use non-zero cooldown to test deduplication
    let dispatcher =
        AlertDispatcher::with_cooldown(store.clone(), EventBus::new(16), Duration::from_secs(60));

    let first = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert_eq!(first.len(), 1);

    // Second evaluation within cooldown window — should be suppressed
    let second = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert!(second.is_empty());

    // Only one history entry despite two evaluations
    assert_eq!(store.history(10).unwrap().len(), 1);
}

#[tokio::test]
async fn test_channel_message_action_emits_event() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    store
        .create(
            "channel-rule",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::ChannelMessage {
                platform: "slack".into(),
                channel_id: "C123".into(),
            }],
        )
        .unwrap();

    let event_bus = EventBus::new(64);
    let mut rx = event_bus.subscribe();
    let dispatcher = AlertDispatcher::with_cooldown(store, event_bus, Duration::from_millis(0));

    dispatcher.evaluate_and_dispatch().await.unwrap();

    let event = rx.try_recv().expect("AlertFired event should be emitted");
    assert!(matches!(
        event.kind,
        opengoose_types::AppEventKind::AlertFired {
            ref rule_name,
            ref platform,
            ref channel_id,
            ..
        } if rule_name == "channel-rule" && platform == "slack" && channel_id == "C123"
    ));
}

#[tokio::test]
async fn test_deduplication_is_per_rule() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    store
        .create(
            "rule-a",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::Log],
        )
        .unwrap();
    store
        .create(
            "rule-b",
            None,
            &AlertMetric::FailedRuns,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::Log],
        )
        .unwrap();

    // Long cooldown — both fire on first eval, neither on second
    let dispatcher =
        AlertDispatcher::with_cooldown(store.clone(), EventBus::new(16), Duration::from_secs(60));

    let first = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert_eq!(first.len(), 2);

    let second = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert!(second.is_empty(), "both rules should be deduplicated");
}

#[tokio::test]
async fn test_zero_cooldown_fires_every_evaluation() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    store
        .create(
            "no-cooldown",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            -1.0,
            &[AlertAction::Log],
        )
        .unwrap();

    // Zero cooldown means every evaluation should fire
    let dispatcher =
        AlertDispatcher::with_cooldown(store.clone(), EventBus::new(16), Duration::from_millis(0));

    let first = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert_eq!(first.len(), 1);

    let second = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert_eq!(second.len(), 1, "zero cooldown should allow re-firing");

    assert_eq!(store.history(10).unwrap().len(), 2);
}

#[tokio::test]
async fn test_rule_not_triggered_when_condition_not_met() {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = Arc::new(AlertStore::new(db));
    // queue_backlog is 0; threshold 100.0 => 0 > 100 does NOT trigger
    store
        .create(
            "high-threshold",
            None,
            &AlertMetric::QueueBacklog,
            &AlertCondition::GreaterThan,
            100.0,
            &[AlertAction::Log],
        )
        .unwrap();

    let dispatcher =
        AlertDispatcher::with_cooldown(store.clone(), EventBus::new(16), Duration::from_millis(0));

    let fired = dispatcher.evaluate_and_dispatch().await.unwrap();
    assert!(fired.is_empty());
    assert!(store.history(10).unwrap().is_empty());
}
