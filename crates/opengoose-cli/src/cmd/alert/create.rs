use anyhow::{Result, anyhow};

use opengoose_persistence::{
    AlertAction as PersistenceAlertAction, AlertCondition, AlertMetric, AlertStore,
};

pub(super) fn run(
    store: &AlertStore,
    name: &str,
    metric_str: &str,
    condition_str: &str,
    threshold: f64,
    description: Option<&str>,
) -> Result<()> {
    let metric = parse_metric(metric_str)?;
    let condition = parse_condition(condition_str)?;

    let rule = store.create(
        name,
        description,
        &metric,
        &condition,
        threshold,
        &[] as &[PersistenceAlertAction],
    )?;
    println!("Created alert rule `{}` (id: {}).", rule.name, rule.id);
    Ok(())
}

pub(super) fn parse_metric(metric_str: &str) -> Result<AlertMetric> {
    AlertMetric::parse(metric_str).ok_or_else(|| {
        anyhow!(
            "unknown metric `{}`. Valid values: {}",
            metric_str,
            AlertMetric::variants().join(", ")
        )
    })
}

pub(super) fn parse_condition(condition_str: &str) -> Result<AlertCondition> {
    AlertCondition::parse(condition_str).ok_or_else(|| {
        anyhow!(
            "unknown condition `{}`. Valid values: {}",
            condition_str,
            AlertCondition::variants().join(", ")
        )
    })
}
