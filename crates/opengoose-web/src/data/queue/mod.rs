mod detail;
mod grouping;
mod loader;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;

use self::detail::build_queue_detail;
use self::loader::{load_queue_detail, load_queue_runs};
use crate::data::runs::build_run_list_items;
use crate::data::utils::choose_selected_run;
use crate::data::views::QueuePageView;

/// Load the queue page view-model, optionally selecting a run by ID.
pub fn load_queue_page(db: Arc<Database>, selected: Option<String>) -> Result<QueuePageView> {
    let loaded = load_queue_runs(db.clone(), 20)?;
    let selected_run_id = choose_selected_run(&loaded.runs, selected);
    let selected_detail = load_queue_detail(db, &loaded, &selected_run_id)?;

    Ok(QueuePageView {
        mode_label: loaded.mode.label.into(),
        mode_tone: loaded.mode.tone,
        runs: build_run_list_items(
            &loaded.runs,
            Some(selected_run_id.clone()),
            loaded.mode.label,
        ),
        selected: build_queue_detail(&selected_detail, loaded.mode.label),
    })
}
