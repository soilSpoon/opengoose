use axum::extract::{Form, Query};
use serde::Deserialize;

use crate::data::{load_teams_page, save_team_yaml};
use crate::routes::{WebResult, internal_error};

use super::render::render_teams_page;

#[derive(Deserialize, Default)]
pub(crate) struct TeamQuery {
    pub(crate) team: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TeamSaveForm {
    pub(crate) original_name: String,
    pub(crate) yaml: String,
}

pub(crate) async fn teams(Query(query): Query<TeamQuery>) -> WebResult {
    let page = load_teams_page(query.team).map_err(internal_error)?;
    render_teams_page(page)
}

pub(crate) async fn team_save(Form(form): Form<TeamSaveForm>) -> WebResult {
    let original_name = form.original_name.clone();
    let detail = save_team_yaml(form.original_name, form.yaml).map_err(internal_error)?;
    let active_team = match detail.notice.as_ref().map(|notice| notice.tone) {
        Some("success") => detail.title.clone(),
        _ => original_name,
    };

    let mut page = load_teams_page(Some(active_team)).map_err(internal_error)?;
    page.selected = detail;

    render_teams_page(page)
}
