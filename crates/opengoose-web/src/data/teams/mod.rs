mod catalog;
mod editor;
mod view_model;

use anyhow::Result;

use crate::data::views::{TeamEditorView, TeamsPageView};

/// Load the teams page view-model, optionally selecting a team by name.
pub fn load_teams_page(selected: Option<String>) -> Result<TeamsPageView> {
    let teams = catalog::load_teams_catalog()?;
    view_model::build_teams_page(&teams, selected)
}

/// Save edited team YAML and return the refreshed editor view.
pub fn save_team_yaml(original_name: String, yaml: String) -> Result<TeamEditorView> {
    editor::save_team_yaml(original_name, yaml)
}
