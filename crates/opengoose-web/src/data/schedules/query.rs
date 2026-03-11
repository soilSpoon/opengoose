use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;

use crate::data::views::SchedulesPageView;

use super::shared::{load_catalog, resolve_selection};
use super::view::build_page;

/// Load the schedules page view-model, optionally selecting a schedule by name.
pub fn load_schedules_page(
    db: Arc<Database>,
    selected: Option<String>,
) -> Result<SchedulesPageView> {
    let catalog = load_catalog(db)?;
    let selection = resolve_selection(&catalog, selected);
    build_page(&catalog, selection, None)
}
