mod schedules;
mod sessions;
mod teams;
mod triggers;
mod workflows;

use axum::extract::{Form, State};

use crate::data::{
    PluginInstallInput, delete_plugin, install_plugin_from_path, toggle_plugin_state,
};
use crate::routes::pages::catalog::pages::{PluginsSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::PluginActionForm;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;

pub(crate) use schedules::schedule_action;
pub(crate) use sessions::session_action;
pub(crate) use teams::team_save;
pub(crate) use triggers::trigger_action;
pub(crate) use workflows::trigger_workflow_action;

pub(crate) async fn plugin_action(
    State(state): State<PageState>,
    Form(form): Form<PluginActionForm>,
) -> WebResult {
    let target_name = form.original_name.clone().unwrap_or_default();
    let page = match form.intent.as_str() {
        "install" => install_plugin_from_path(
            state.db,
            PluginInstallInput {
                source_path: form.source_path.unwrap_or_default(),
            },
        ),
        "toggle" => toggle_plugin_state(state.db, target_name),
        "delete" => delete_plugin(
            state.db,
            target_name,
            form.confirm_delete.as_deref() == Some("yes"),
        ),
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported plugin action.".into()),
            ));
        }
    }
    .map_err(internal_error)?;

    render_catalog_spec_page::<PluginsSpec>(page)
}
