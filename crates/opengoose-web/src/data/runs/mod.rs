mod loader;
mod selection;
#[cfg(test)]
mod tests;
mod view_model;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;

use self::loader::{load_run_detail, load_run_records};
use self::selection::choose_selected_run_id;
use crate::data::views::RunsPageView;

pub(in crate::data) use self::loader::mock_runs;
pub(in crate::data) use self::view_model::build_run_list_items;

/// Load the runs page view-model, optionally selecting a run by ID.
pub fn load_runs_page(db: Arc<Database>, selected: Option<String>) -> Result<RunsPageView> {
    let loaded = load_run_records(db.clone(), 20)?;
    let selected_run_id = choose_selected_run_id(&loaded.runs, selected);
    let selected_detail = load_run_detail(db, &loaded, &selected_run_id)?;

    Ok(RunsPageView {
        mode_label: loaded.mode.label.into(),
        mode_tone: loaded.mode.tone,
        runs: build_run_list_items(
            &loaded.runs,
            Some(selected_run_id.clone()),
            loaded.mode.label,
        ),
        selected: view_model::build_run_detail(&selected_detail, loaded.mode.label),
    })
}
