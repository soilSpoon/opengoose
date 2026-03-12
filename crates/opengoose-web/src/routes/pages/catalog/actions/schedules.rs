use axum::extract::{Form, State};

use crate::data::{ScheduleSaveInput, delete_schedule, save_schedule, toggle_schedule};
use crate::routes::pages::catalog::pages::{SchedulesSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::ScheduleActionForm;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

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

    render_catalog_spec_page::<SchedulesSpec>(page)
}
