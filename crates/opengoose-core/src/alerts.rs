//! Alert dispatcher: evaluates alert rules against system metrics and fires actions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opengoose_persistence::{AlertAction, AlertRule, AlertStore, SystemMetrics};
use opengoose_types::{AppEventKind, EventBus};
use tracing::warn;

const DEFAULT_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes

/// Error type for alert dispatch failures.
#[derive(Debug, thiserror::Error)]
pub enum AlertDispatchError {
    #[error("persistence error: {0}")]
    Persistence(#[from] opengoose_persistence::PersistenceError),
    #[error("webhook request failed: {0}")]
    Webhook(#[from] reqwest::Error),
}

/// Evaluates alert rules against current system metrics and fires configured actions.
///
/// Includes deduplication via a per-rule cooldown window to prevent alert storms.
pub struct AlertDispatcher {
    store: Arc<AlertStore>,
    event_bus: EventBus,
    client: reqwest::Client,
    last_fired: Mutex<HashMap<String, Instant>>,
    cooldown: Duration,
}

impl AlertDispatcher {
    /// Create a new dispatcher backed by the given store and event bus.
    pub fn new(store: Arc<AlertStore>, event_bus: EventBus) -> Self {
        Self {
            store,
            event_bus,
            client: reqwest::Client::new(),
            last_fired: Mutex::new(HashMap::new()),
            cooldown: DEFAULT_COOLDOWN,
        }
    }

    /// Create a dispatcher with a custom cooldown duration.
    pub fn with_cooldown(store: Arc<AlertStore>, event_bus: EventBus, cooldown: Duration) -> Self {
        Self {
            store,
            event_bus,
            client: reqwest::Client::new(),
            last_fired: Mutex::new(HashMap::new()),
            cooldown,
        }
    }

    /// Evaluate all enabled rules against current metrics and fire actions for triggered rules.
    ///
    /// Returns the list of rule names that were triggered (after deduplication).
    pub async fn evaluate_and_dispatch(&self) -> Result<Vec<String>, AlertDispatchError> {
        let rules = self.store.list()?;
        let metrics = self.store.current_metrics()?;
        let mut triggered = Vec::new();

        for rule in rules.iter().filter(|r| r.enabled) {
            let value = self.metric_value(&metrics, rule);
            if !rule.condition.evaluate(value, rule.threshold) {
                continue;
            }
            if self.is_deduplicated(&rule.id) {
                continue;
            }

            self.store.record_trigger(rule, value)?;
            self.fire_actions(rule, value).await;
            self.mark_fired(rule.id.clone());
            triggered.push(rule.name.clone());
        }

        Ok(triggered)
    }

    fn metric_value(&self, metrics: &SystemMetrics, rule: &AlertRule) -> f64 {
        use opengoose_persistence::AlertMetric;
        match rule.metric {
            AlertMetric::QueueBacklog => metrics.queue_backlog,
            AlertMetric::FailedRuns => metrics.failed_runs,
            AlertMetric::ErrorRate => metrics.error_rate,
        }
    }

    /// Returns true if the rule fired recently and should be suppressed.
    fn is_deduplicated(&self, rule_id: &str) -> bool {
        let guard = self.last_fired.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .get(rule_id)
            .map(|t| t.elapsed() < self.cooldown)
            .unwrap_or(false)
    }

    fn mark_fired(&self, rule_id: String) {
        let mut guard = self.last_fired.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(rule_id, Instant::now());
    }

    async fn fire_actions(&self, rule: &AlertRule, value: f64) {
        for action in &rule.actions {
            match action {
                AlertAction::Log => {
                    warn!(
                        rule_name = %rule.name,
                        metric = %rule.metric,
                        value = value,
                        threshold = rule.threshold,
                        "alert fired"
                    );
                }
                AlertAction::Webhook { url } => {
                    let payload = serde_json::json!({
                        "rule": rule.name,
                        "metric": rule.metric.as_str(),
                        "value": value,
                        "threshold": rule.threshold,
                        "condition": rule.condition.as_str(),
                    });
                    if let Err(e) = self.client.post(url).json(&payload).send().await {
                        warn!(url = %url, err = %e, "webhook action failed");
                    }
                }
                AlertAction::ChannelMessage {
                    platform,
                    channel_id,
                } => {
                    self.event_bus.emit(AppEventKind::AlertFired {
                        rule_name: rule.name.clone(),
                        metric: rule.metric.as_str().to_string(),
                        value,
                        platform: platform.clone(),
                        channel_id: channel_id.clone(),
                    });
                }
            }
        }
    }

    /// Spawn a background task that evaluates alerts on a fixed interval.
    ///
    /// The task runs until the `cancel` token is cancelled.
    pub fn start_periodic(
        self: Arc<Self>,
        interval: Duration,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        match self.evaluate_and_dispatch().await {
                            Ok(fired) if !fired.is_empty() => {
                                tracing::info!(
                                    count = fired.len(),
                                    rules = ?fired,
                                    "alert evaluation: rules triggered"
                                );
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::error!(err = %e, "alert evaluation failed");
                            }
                        }
                    }
                    _ = cancel.cancelled() => break,
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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

        let dispatcher = AlertDispatcher::with_cooldown(
            store.clone(),
            EventBus::new(16),
            Duration::from_millis(0),
        );

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
        let dispatcher = AlertDispatcher::with_cooldown(
            store.clone(),
            EventBus::new(16),
            Duration::from_secs(60),
        );

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
        let dispatcher = AlertDispatcher::with_cooldown(
            store.clone(),
            EventBus::new(16),
            Duration::from_secs(60),
        );

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
        let dispatcher = AlertDispatcher::with_cooldown(
            store.clone(),
            EventBus::new(16),
            Duration::from_millis(0),
        );

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

        let dispatcher = AlertDispatcher::with_cooldown(
            store.clone(),
            EventBus::new(16),
            Duration::from_millis(0),
        );

        let fired = dispatcher.evaluate_and_dispatch().await.unwrap();
        assert!(fired.is_empty());
        assert!(store.history(10).unwrap().is_empty());
    }
}
