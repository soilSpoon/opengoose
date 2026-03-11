use axum::extract::{Form, Query, State};
use serde::Deserialize;

use crate::data::{
    ScheduleSaveInput, delete_schedule, load_schedules_page, save_schedule, toggle_schedule,
};
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

use super::render::render_schedules_page;

#[derive(Deserialize, Default)]
pub(crate) struct ScheduleQuery {
    pub(crate) schedule: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ScheduleActionForm {
    pub(crate) intent: String,
    pub(crate) original_name: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) cron_expression: Option<String>,
    pub(crate) team_name: Option<String>,
    pub(crate) input: Option<String>,
    pub(crate) enabled: Option<String>,
    pub(crate) confirm_delete: Option<String>,
}

pub(crate) async fn schedules(
    State(state): State<PageState>,
    Query(query): Query<ScheduleQuery>,
) -> WebResult {
    let page = load_schedules_page(state.db, query.schedule).map_err(internal_error)?;
    render_schedules_page(page)
}

pub(crate) async fn schedule_action(
    State(state): State<PageState>,
    Form(form): Form<ScheduleActionForm>,
) -> WebResult {
    let target_name = form
        .original_name
        .clone()
        .or_else(|| form.name.clone())
        .unwrap_or_default();
    let page = match form.intent.as_str() {
        "save" => save_schedule(
            state.db,
            ScheduleSaveInput {
                original_name: form.original_name,
                name: form.name.unwrap_or_default(),
                cron_expression: form.cron_expression.unwrap_or_default(),
                team_name: form.team_name.unwrap_or_default(),
                input: form.input.unwrap_or_default(),
                enabled: form.enabled.is_some(),
            },
        ),
        "toggle" => toggle_schedule(state.db, target_name),
        "delete" => delete_schedule(
            state.db,
            target_name,
            form.confirm_delete.as_deref() == Some("yes"),
        ),
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported schedule action.".into()),
            ));
        }
    }
    .map_err(internal_error)?;

    render_schedules_page(page)
}
