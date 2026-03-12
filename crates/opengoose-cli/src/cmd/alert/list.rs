use anyhow::Result;

use opengoose_persistence::AlertStore;

use crate::cmd::output::format_table;

pub(super) fn run(store: &AlertStore) -> Result<()> {
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
