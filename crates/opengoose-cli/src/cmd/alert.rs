use std::sync::Arc;

use anyhow::{Result, bail};
use clap::Subcommand;

use opengoose_persistence::{AlertCondition, AlertMetric, AlertStore, Database};

use crate::cmd::output::format_table;

#[derive(Subcommand)]
/// Subcommands for `opengoose alert`.
pub enum AlertAction {
    /// List all alert rules
    List,
    /// Create a new alert rule
    Create {
        /// Rule name (must be unique)
        name: String,
        /// Metric to monitor: queue_backlog, failed_runs, error_rate
        #[arg(long, short)]
        metric: String,
        /// Condition operator: gt, lt, gte, lte
        #[arg(long, short)]
        condition: String,
        /// Threshold value
        #[arg(long, short)]
        threshold: f64,
        /// Optional description
        #[arg(long, short)]
        description: Option<String>,
    },
    /// Delete an alert rule by name
    Delete {
        /// Rule name
        name: String,
    },
    /// Enable an alert rule
    Enable {
        /// Rule name
        name: String,
    },
    /// Disable an alert rule
    Disable {
        /// Rule name
        name: String,
    },
    /// Run a health check: evaluate all enabled rules against current system metrics
    Test,
    /// Show recent alert history
    History {
        /// Number of entries to show
        #[arg(long, default_value = "20")]
        limit: i64,
    },
}

/// Dispatch and execute the selected alert subcommand.
pub fn execute(action: AlertAction) -> Result<()> {
    match action {
        AlertAction::List => cmd_list(),
        AlertAction::Create {
            name,
            metric,
            condition,
            threshold,
            description,
        } => cmd_create(
            &name,
            &metric,
            &condition,
            threshold,
            description.as_deref(),
        ),
        AlertAction::Delete { name } => cmd_delete(&name),
        AlertAction::Enable { name } => cmd_set_enabled(&name, true),
        AlertAction::Disable { name } => cmd_set_enabled(&name, false),
        AlertAction::Test => cmd_test(),
        AlertAction::History { limit } => cmd_history(limit),
    }
}

fn open_store() -> Result<AlertStore> {
    let db = Arc::new(Database::open()?);
    Ok(AlertStore::new(db))
}

fn cmd_list() -> Result<()> {
    let store = open_store()?;
    let rules = store.list()?;

    if rules.is_empty() {
        println!("No alert rules defined. Use `opengoose alert create` to add one.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = rules
        .iter()
        .map(|rule| {
            vec![
                rule.name.clone(),
                rule.metric.to_string(),
                rule.condition.to_string(),
                rule.threshold.to_string(),
                if rule.enabled { "enabled" } else { "disabled" }.to_string(),
            ]
        })
        .collect();
    print!(
        "{}",
        format_table(&["NAME", "METRIC", "OP", "THRESHOLD", "STATUS"], &rows)
    );
    Ok(())
}

fn cmd_create(
    name: &str,
    metric_str: &str,
    condition_str: &str,
    threshold: f64,
    description: Option<&str>,
) -> Result<()> {
    let metric = AlertMetric::parse(metric_str).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown metric `{}`. Valid values: {}",
            metric_str,
            AlertMetric::variants().join(", ")
        )
    })?;

    let condition = AlertCondition::parse(condition_str).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown condition `{}`. Valid values: {}",
            condition_str,
            AlertCondition::variants().join(", ")
        )
    })?;

    let store = open_store()?;
    let rule = store.create(name, description, &metric, &condition, threshold)?;
    println!("Created alert rule `{}` (id: {}).", rule.name, rule.id);
    Ok(())
}

fn cmd_delete(name: &str) -> Result<()> {
    let store = open_store()?;
    if store.delete(name)? {
        println!("Deleted alert rule `{name}`.");
    } else {
        bail!("No alert rule named `{name}` found.");
    }
    Ok(())
}

fn cmd_set_enabled(name: &str, enabled: bool) -> Result<()> {
    let store = open_store()?;
    let action = if enabled { "Enabled" } else { "Disabled" };
    if store.set_enabled(name, enabled)? {
        println!("{action} alert rule `{name}`.");
    } else {
        bail!("No alert rule named `{name}` found.");
    }
    Ok(())
}

fn cmd_test() -> Result<()> {
    let store = open_store()?;
    let rules = store.list()?;

    let enabled_rules: Vec<_> = rules.iter().filter(|r| r.enabled).collect();
    if enabled_rules.is_empty() {
        println!("No enabled alert rules. Use `opengoose alert create` to add one.");
        return Ok(());
    }

    let metrics = store.current_metrics()?;

    println!("Health Check Results");
    println!("{}", "=".repeat(50));

    let mut triggered = 0u32;
    for rule in &enabled_rules {
        let value = match rule.metric {
            AlertMetric::QueueBacklog => metrics.queue_backlog,
            AlertMetric::FailedRuns => metrics.failed_runs,
            AlertMetric::ErrorRate => metrics.error_rate,
        };

        let fires = rule.condition.evaluate(value, rule.threshold);
        let status = if fires { "ALERT" } else { "ok" };
        println!(
            "  [{status:<5}] {}: {} {} {} (current: {value})",
            rule.name, rule.metric, rule.condition, rule.threshold
        );

        if fires {
            triggered += 1;
            store.record_trigger(rule, value)?;
        }
    }

    println!();
    println!(
        "Metrics: queue_backlog={}, failed_runs={}, error_rate={}",
        metrics.queue_backlog, metrics.failed_runs, metrics.error_rate
    );

    if triggered > 0 {
        println!("\n{triggered} rule(s) triggered — entries recorded in alert history.");
    } else {
        println!("\nAll rules within thresholds.");
    }

    Ok(())
}

fn cmd_history(limit: i64) -> Result<()> {
    let store = open_store()?;
    let history = store.history(limit)?;

    if history.is_empty() {
        println!("No alert history found.");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = history
        .iter()
        .map(|entry| {
            vec![
                entry.triggered_at.clone(),
                entry.rule_name.clone(),
                entry.metric.clone(),
                entry.value.to_string(),
            ]
        })
        .collect();
    print!(
        "{}",
        format_table(&["TRIGGERED AT", "RULE", "METRIC", "VALUE"], &rows)
    );
    Ok(())
}
