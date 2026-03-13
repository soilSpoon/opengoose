//! Alert dispatcher: evaluates alert rules against system metrics and fires actions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opengoose_persistence::{AlertAction, AlertRule, AlertStore, SystemMetrics};
use opengoose_types::{AppEventKind, EventBus};
use tracing::warn;

use super::types::{AlertDispatchError, DEFAULT_COOLDOWN};

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
