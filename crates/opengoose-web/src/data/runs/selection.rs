use anyhow::{Context, Result};
use opengoose_persistence::OrchestrationRun;

use crate::data::utils::choose_selected_run;

pub(in crate::data) fn choose_selected_run_id(
    runs: &[OrchestrationRun],
    selected: Option<String>,
) -> String {
    choose_selected_run(runs, selected)
}

pub(in crate::data) fn find_selected_run<'a>(
    runs: &'a [OrchestrationRun],
    run_id: &str,
) -> Result<&'a OrchestrationRun> {
    runs.iter()
        .find(|run| run.team_run_id == run_id)
        .context("selected run missing")
}
