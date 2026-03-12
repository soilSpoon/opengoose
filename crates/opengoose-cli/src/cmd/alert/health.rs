use anyhow::Result;

use opengoose_persistence::{AlertMetric, AlertStore};

pub(super) fn run(store: &AlertStore) -> Result<()> {
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
