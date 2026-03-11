use axum::extract::{Form, State};

use crate::data::{Notice, load_sessions_page};
use crate::routes::pages::catalog::pages::{SessionsSpec, render_catalog_spec_page};
use crate::routes::pages::catalog_forms::SessionActionForm;
use crate::routes::{WebResult, internal_error};
use crate::server::PageState;
use opengoose_persistence::SessionStore;
use opengoose_types::SessionKey;

pub(crate) async fn session_action(
    State(state): State<PageState>,
    Form(form): Form<SessionActionForm>,
) -> WebResult {
    let session_key = SessionKey::from_stable_id(&form.session_key);
    let store = SessionStore::new(state.db.clone());

    let notice = match form.intent.as_str() {
        "save" => {
            let model = form
                .selected_model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            store
                .set_selected_model(&session_key, model)
                .map_err(|e| internal_error(e.into()))?;
            Notice {
                text: match model {
                    Some(m) => format!("Model override set to `{m}`."),
                    None => "Model override cleared.".into(),
                },
                tone: "success",
            }
        }
        "clear" => {
            store
                .set_selected_model(&session_key, None)
                .map_err(|e| internal_error(e.into()))?;
            Notice {
                text: "Model override cleared.".into(),
                tone: "success",
            }
        }
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported session action.".into()),
            ));
        }
    };

    let mut page =
        load_sessions_page(state.db.clone(), Some(form.session_key)).map_err(internal_error)?;
    page.selected.notice = Some(notice);

    render_catalog_spec_page::<SessionsSpec>(page)
}
