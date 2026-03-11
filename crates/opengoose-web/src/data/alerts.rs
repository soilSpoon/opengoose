use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::{AlertCondition, AlertMetric, AlertRule, AlertStore, Database};
use urlencoding::encode;

use crate::data::views::{
    AlertDetailView, AlertHistoryItemView, AlertListItem, AlertsPageView, MetaRow, MetricCard,
    SelectOption,
};

const DEFAULT_METRIC: &str = "queue_backlog";
const DEFAULT_CONDITION: &str = "gt";

/// Load the alerts management page view-model, optionally selecting a rule by name.
pub fn load_alerts_page(db: Arc<Database>, selected: Option<String>) -> Result<AlertsPageView> {
    let store = AlertStore::new(db);
    let rules = store.list()?;
    let rule_lookup = rules
        .iter()
        .map(|rule| (rule.name.clone(), rule))
        .collect::<HashMap<_, _>>();
    let history = build_history_rows(&store.history(50)?, &rule_lookup);
    let metrics = build_metric_cards(store.current_metrics()?);

    let selected_name = if rules.is_empty() {
        String::new()
    } else {
        selected
            .filter(|target| rules.iter().any(|rule| &rule.name == target))
            .unwrap_or_else(|| rules[0].name.clone())
    };
    let selected_rule = rules.iter().find(|rule| rule.name == selected_name);
    let enabled_count = rules.iter().filter(|rule| rule.enabled).count();

    Ok(AlertsPageView {
        mode_label: if rules.is_empty() {
            "No alert rules configured".into()
        } else {
            format!("{enabled_count} active of {}", rules.len())
        },
        mode_tone: if rules.is_empty() {
            "neutral"
        } else if enabled_count == 0 {
            "amber"
        } else {
            "success"
        },
        metrics,
        alerts: rules
            .iter()
            .map(|rule| build_alert_list_item(rule, &selected_name))
            .collect(),
        selected: match selected_rule {
            Some(rule) => build_alert_detail(rule, history.clone()),
            None => placeholder_alert_detail(history),
        },
        history_api_url: "/api/alerts/history".into(),
    })
}

fn build_alert_list_item(rule: &AlertRule, selected_name: &str) -> AlertListItem {
    let metric_label = format_metric_label(rule.metric.as_str());
    let condition_label = format_condition_label(rule.condition.as_str());
    let threshold_label = format_number(rule.threshold);

    AlertListItem {
        title: rule.name.clone(),
        subtitle: format!("{metric_label} {condition_label} {}", threshold_label),
        preview: rule
            .description
            .clone()
            .unwrap_or_else(|| format!("Created {}", rule.created_at)),
        status_label: if rule.enabled {
            "enabled".into()
        } else {
            "disabled".into()
        },
        status_tone: if rule.enabled { "success" } else { "neutral" },
        enabled: rule.enabled,
        metric_key: rule.metric.as_str().into(),
        metric_label: metric_label.clone(),
        condition_key: rule.condition.as_str().into(),
        condition_label,
        threshold_value: rule.threshold,
        threshold_label,
        target_label: format_target_label(rule),
        page_url: format!("/alerts?alert={}", encode(&rule.name)),
        active: rule.name == selected_name,
    }
}

fn build_alert_detail(rule: &AlertRule, history: Vec<AlertHistoryItemView>) -> AlertDetailView {
    AlertDetailView {
        title: rule.name.clone(),
        subtitle: rule.description.clone().unwrap_or_else(|| {
            format!(
                "{} {} {}",
                format_metric_label(rule.metric.as_str()),
                format_condition_label(rule.condition.as_str()).to_lowercase(),
                format_number(rule.threshold)
            )
        }),
        meta: vec![
            MetaRow {
                label: "Metric".into(),
                value: format_metric_label(rule.metric.as_str()),
            },
            MetaRow {
                label: "Condition".into(),
                value: format_condition_label(rule.condition.as_str()),
            },
            MetaRow {
                label: "Threshold".into(),
                value: format_number(rule.threshold),
            },
            MetaRow {
                label: "Created".into(),
                value: rule.created_at.clone(),
            },
            MetaRow {
                label: "Updated".into(),
                value: rule.updated_at.clone(),
            },
        ],
        status_label: if rule.enabled {
            "enabled".into()
        } else {
            "disabled".into()
        },
        status_tone: if rule.enabled { "success" } else { "neutral" },
        delete_api_url: format!("/api/alerts/{}", encode(&rule.name)),
        test_api_url: "/api/alerts/test".into(),
        create_api_url: "/api/alerts".into(),
        metric_options: build_metric_options(DEFAULT_METRIC),
        condition_options: build_condition_options(DEFAULT_CONDITION),
        history,
        history_hint:
            "No alert rules have fired yet. Run a test snapshot to record the latest results."
                .into(),
        is_placeholder: false,
    }
}

fn placeholder_alert_detail(history: Vec<AlertHistoryItemView>) -> AlertDetailView {
    AlertDetailView {
        title: "No alert rules configured".into(),
        subtitle: "Create a threshold rule to monitor queue backlog, failed runs, or runtime error volume.".into(),
        meta: vec![],
        status_label: "idle".into(),
        status_tone: "neutral",
        delete_api_url: String::new(),
        test_api_url: "/api/alerts/test".into(),
        create_api_url: "/api/alerts".into(),
        metric_options: build_metric_options(DEFAULT_METRIC),
        condition_options: build_condition_options(DEFAULT_CONDITION),
        history,
        history_hint: "No alert rules have fired yet. Run a test snapshot to record the latest results.".into(),
        is_placeholder: true,
    }
}

fn build_metric_options(selected: &str) -> Vec<SelectOption> {
    AlertMetric::variants()
        .iter()
        .map(|metric| SelectOption {
            value: (*metric).into(),
            label: format_metric_label(metric),
            selected: *metric == selected,
        })
        .collect()
}

fn build_condition_options(selected: &str) -> Vec<SelectOption> {
    AlertCondition::variants()
        .iter()
        .map(|condition| SelectOption {
            value: (*condition).into(),
            label: format_condition_label(condition),
            selected: *condition == selected,
        })
        .collect()
}

fn build_history_rows(
    entries: &[opengoose_persistence::AlertHistoryEntry],
    rule_lookup: &HashMap<String, &AlertRule>,
) -> Vec<AlertHistoryItemView> {
    entries
        .iter()
        .map(|entry| {
            let target_label = rule_lookup
                .get(&entry.rule_name)
                .map(|rule| format_target_label(rule))
                .unwrap_or_else(|| "Rule definition unavailable".into());

            AlertHistoryItemView {
                rule_name: entry.rule_name.clone(),
                rule_page_url: format!("/alerts?alert={}", encode(&entry.rule_name)),
                metric_label: format_metric_label(&entry.metric),
                value_label: format_number(entry.value),
                result_label: "Triggered".into(),
                target_label,
                triggered_at: entry.triggered_at.clone(),
            }
        })
        .collect()
}

fn build_metric_cards(metrics: opengoose_persistence::SystemMetrics) -> Vec<MetricCard> {
    vec![
        MetricCard {
            label: "Queue backlog".into(),
            value: format_number(metrics.queue_backlog),
            note: "Pending or failed queue entries currently waiting on recovery.".into(),
            tone: "amber",
        },
        MetricCard {
            label: "Failed runs".into(),
            value: format_number(metrics.failed_runs),
            note: "Persisted orchestration runs marked as failed.".into(),
            tone: "rose",
        },
        MetricCard {
            label: "Error runs".into(),
            value: format_number(metrics.error_rate),
            note: "Persisted orchestration runs marked as error.".into(),
            tone: "cyan",
        },
    ]
}

fn format_metric_label(metric: &str) -> String {
    match metric {
        "queue_backlog" => "Queue backlog".into(),
        "failed_runs" => "Failed runs".into(),
        "error_rate" => "Error runs".into(),
        other => other.replace('_', " "),
    }
}

fn format_condition_label(condition: &str) -> String {
    match condition {
        "gt" => "Greater than".into(),
        "lt" => "Less than".into(),
        "gte" => "Greater than or equal".into(),
        "lte" => "Less than or equal".into(),
        other => other.into(),
    }
}

fn format_condition_symbol(condition: &str) -> &'static str {
    match condition {
        "gt" => ">",
        "lt" => "<",
        "gte" => ">=",
        "lte" => "<=",
        _ => "=?",
    }
}

fn format_target_label(rule: &AlertRule) -> String {
    format!(
        "{} {} {}",
        format_metric_label(rule.metric.as_str()),
        format_condition_symbol(rule.condition.as_str()),
        format_number(rule.threshold)
    )
}

fn format_number(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn load_alerts_page_without_rules_returns_placeholder() {
        let page = load_alerts_page(test_db(), None).expect("page should load");

        assert_eq!(page.mode_label, "No alert rules configured");
        assert_eq!(page.mode_tone, "neutral");
        assert!(page.alerts.is_empty());
        assert!(page.selected.is_placeholder);
        assert_eq!(page.metrics.len(), 3);
    }

    #[test]
    fn load_alerts_page_selects_named_rule_and_builds_history_links() {
        let db = test_db();
        let store = AlertStore::new(db.clone());
        let first = store
            .create(
                "backlog-high",
                Some("Queue pressure is rising"),
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                10.0,
                &[],
            )
            .unwrap();
        let second = store
            .create(
                "errors-high",
                None,
                &AlertMetric::ErrorRate,
                &AlertCondition::GreaterThanOrEqual,
                2.0,
                &[],
            )
            .unwrap();
        store.record_trigger(&first, 12.0).unwrap();
        store.record_trigger(&second, 3.0).unwrap();

        let page = load_alerts_page(db, Some("errors-high".into())).expect("page should load");

        assert_eq!(page.mode_label, "2 active of 2");
        assert_eq!(page.selected.title, "errors-high");
        assert_eq!(page.selected.status_tone, "success");
        assert_eq!(page.selected.history.len(), 2);
        assert_eq!(page.selected.history[0].rule_name, "errors-high");
        assert_eq!(page.selected.history[0].result_label, "Triggered");
        assert_eq!(page.selected.history[0].target_label, "Error runs >= 2");
        assert_eq!(
            page.selected.history[0].rule_page_url,
            "/alerts?alert=errors-high"
        );
        assert_eq!(page.alerts[0].metric_key, "queue_backlog");
        assert_eq!(page.alerts[0].target_label, "Queue backlog > 10");
    }

    #[test]
    fn load_alerts_page_invalid_selection_falls_back_to_first_rule() {
        let db = test_db();
        let store = AlertStore::new(db.clone());
        store
            .create(
                "only-rule",
                None,
                &AlertMetric::FailedRuns,
                &AlertCondition::GreaterThan,
                5.0,
                &[],
            )
            .unwrap();

        let page = load_alerts_page(db, Some("missing-rule".into())).expect("page should load");

        assert_eq!(page.selected.title, "only-rule");
    }

    #[test]
    fn load_alerts_page_reports_disabled_rules_in_mode_label() {
        let db = test_db();
        let store = AlertStore::new(db.clone());
        store
            .create(
                "queue-watch",
                None,
                &AlertMetric::QueueBacklog,
                &AlertCondition::GreaterThan,
                5.0,
                &[],
            )
            .unwrap();
        store
            .set_enabled("queue-watch", false)
            .expect("rule should toggle");

        let page = load_alerts_page(db, None).expect("page should load");

        assert_eq!(page.mode_label, "0 active of 1");
        assert_eq!(page.mode_tone, "amber");
        assert_eq!(page.alerts[0].status_label, "disabled");
    }
}
