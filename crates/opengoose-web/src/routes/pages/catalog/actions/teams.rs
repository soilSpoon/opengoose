use axum::extract::Form;

use crate::data::{load_teams_page, save_team_yaml};
use crate::routes::pages::catalog::pages::{TeamsSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::TeamSaveForm;
use crate::routes::{WebResult, internal_error};

pub(crate) async fn team_save(Form(form): Form<TeamSaveForm>) -> WebResult {
    let original_name = form.original_name.clone();
    let detail = save_team_yaml(form.original_name, form.yaml).map_err(internal_error)?;
    let active_team = match detail.notice.as_ref().map(|notice| notice.tone) {
        Some("success") => detail.title.clone(),
        _ => original_name,
    };

    let mut page = load_teams_page(Some(active_team)).map_err(internal_error)?;
    page.selected = detail.clone();

    render_catalog_spec_page::<TeamsSpec>(page)
}
