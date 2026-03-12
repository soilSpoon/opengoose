use askama::Template;
use axum::extract::{Form, State};
use serde::Deserialize;

use crate::data::{GeneratedApiKeyView, Notice, load_api_keys_page};
use crate::routes::{WebResult, internal_error, render_template};
use crate::server::PageState;

#[derive(Deserialize)]
pub(crate) struct ApiKeyActionForm {
    pub(crate) intent: String,
    pub(crate) description: Option<String>,
    pub(crate) key_id: Option<String>,
}

pub(crate) async fn api_keys(State(state): State<PageState>) -> WebResult {
    let page = load_api_keys_page(state.api_key_store).map_err(internal_error)?;
    render_api_keys_page(page)
}

pub(crate) async fn api_key_action(
    State(state): State<PageState>,
    Form(form): Form<ApiKeyActionForm>,
) -> WebResult {
    let notice = match form.intent.as_str() {
        "generate" => {
            let description = form
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let generated = state
                .api_key_store
                .generate(description)
                .map_err(|error| internal_error(error.into()))?;
            let mut page = load_api_keys_page(state.api_key_store).map_err(internal_error)?;
            let generated_id = generated.id.clone();
            page.notice = Some(Notice {
                text: format!("API key `{generated_id}` generated."),
                tone: "success",
            });
            page.generated_key = Some(GeneratedApiKeyView {
                id: generated.id,
                plaintext: generated.plaintext,
                description_label: generated
                    .description
                    .unwrap_or_else(|| "No description".into()),
            });
            return render_api_keys_page(page);
        }
        "revoke" => match form
            .key_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(key_id) => {
                if state
                    .api_key_store
                    .revoke(key_id)
                    .map_err(|error| internal_error(error.into()))?
                {
                    Notice {
                        text: format!("API key `{key_id}` revoked."),
                        tone: "success",
                    }
                } else {
                    Notice {
                        text: format!(
                            "API key `{key_id}` was not found. It may have already been revoked."
                        ),
                        tone: "danger",
                    }
                }
            }
            None => Notice {
                text: "Select an API key to revoke.".into(),
                tone: "danger",
            },
        },
        _ => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::response::Html("Unsupported API key action.".into()),
            ));
        }
    };

    let mut page = load_api_keys_page(state.api_key_store).map_err(internal_error)?;
    page.notice = Some(notice);
    render_api_keys_page(page)
}

fn render_api_keys_page(page: crate::data::ApiKeysPageView) -> WebResult {
    render_template(&ApiKeysTemplate {
        page_title: "API Keys",
        current_nav: "api_keys",
        page,
    })
}

#[derive(Template)]
#[template(path = "api_keys.html")]
struct ApiKeysTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: crate::data::ApiKeysPageView,
}
