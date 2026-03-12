mod catalog;
mod detail;
mod mutations;
mod state;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::Result;
use opengoose_persistence::Database;

use self::catalog::build_page;
use crate::data::views::PluginsPageView;

pub use self::mutations::{
    PluginInstallInput, delete_plugin, install_plugin_from_path, toggle_plugin_state,
};
pub use self::state::PluginStatusFilter;

/// Load the plugins page view-model, optionally selecting a plugin by name.
pub fn load_plugins_page(db: Arc<Database>, selected: Option<String>) -> Result<PluginsPageView> {
    load_plugins_page_filtered(db, selected, PluginStatusFilter::All)
}

/// Load the plugins page view-model with a status filter.
pub fn load_plugins_page_filtered(
    db: Arc<Database>,
    selected: Option<String>,
    filter: PluginStatusFilter,
) -> Result<PluginsPageView> {
    build_page(db, selected, filter, None, String::new())
}
