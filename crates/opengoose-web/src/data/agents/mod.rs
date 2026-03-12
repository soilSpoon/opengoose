mod catalog;
mod detail;
mod selection;
#[cfg(test)]
mod tests;

use anyhow::Result;

use self::catalog::{
    ProfileCatalogEntry, build_agent_list_items, catalog_mode, load_profiles_catalog,
};
use self::detail::build_agent_detail;
use self::selection::find_selected_entry;
use crate::data::views::AgentsPageView;

/// Load the agents page view-model, optionally selecting an agent by name.
pub fn load_agents_page(selected: Option<String>) -> Result<AgentsPageView> {
    let agents = load_profiles_catalog()?;
    build_agents_page(&agents, selected)
}

fn build_agents_page(
    agents: &[ProfileCatalogEntry],
    selected: Option<String>,
) -> Result<AgentsPageView> {
    let mode = catalog_mode(agents);
    let selected_entry = find_selected_entry(agents, selected)?;

    Ok(AgentsPageView {
        mode_label: mode.label.into(),
        mode_tone: mode.tone,
        agents: build_agent_list_items(agents, &selected_entry.profile.title),
        selected: build_agent_detail(selected_entry)?,
    })
}
