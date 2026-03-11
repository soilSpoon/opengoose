mod catalog;
mod loader;
mod summary;
mod view_model;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;

use self::catalog::build_workflow_catalog;
use self::loader::load_workflow_page_data;
use crate::data::views::WorkflowsPageView;

/// Load the workflows page view-model, optionally selecting a workflow by name.
pub fn load_workflows_page(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<WorkflowsPageView> {
    let data = load_workflow_page_data(db)?;
    let catalog = build_workflow_catalog(
        &data.teams,
        &data.schedules,
        &data.triggers,
        &data.recent_runs,
    );

    view_model::build_workflows_page(&catalog, data.using_preview, selected)
}
