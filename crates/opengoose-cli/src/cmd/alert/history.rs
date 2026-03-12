use anyhow::Result;

use opengoose_persistence::AlertStore;

use crate::cmd::output::format_table;

pub(super) fn run(store: &AlertStore, limit: i64) -> Result<()> {
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
