mod edit;
mod lifecycle;
mod shared;
mod test_run;

use axum::extract::{Form, State};

use crate::routes::pages::catalog::pages::{TriggersSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::TriggerActionForm;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

pub(crate) async fn trigger_action(
    State(state): State<PageState>,
    Form(form): Form<TriggerActionForm>,
) -> WebResult {
    let target_name = form
        .original_name
        .clone()
        .or_else(|| form.name.clone())
        .unwrap_or_default();

    let page = match form.intent.as_str() {
        "create" => edit::create_trigger_page(&state.db, form),
        "update" => edit::update_trigger_page(&state.db, target_name, form),
        "toggle" => lifecycle::toggle_trigger_page(&state.db, target_name),
        "delete" => lifecycle::delete_trigger_page(&state.db, target_name),
        "test" => {
            test_run::test_trigger_page(state.db.clone(), state.event_bus.clone(), target_name)
        }
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported trigger action.".into()),
            ));
        }
    }
    .map_err(internal_error)?;

    render_catalog_spec_page::<TriggersSpec>(page)
}
