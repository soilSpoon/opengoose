use askama::Template;
use axum::extract::{Query, State};
use serde::Deserialize;

use super::{WebResult, internal_error, render_partial, render_template};
use crate::data::{TriggerDetailView, TriggersPageView, load_triggers_page};
use crate::server::PageState;

#[derive(Deserialize, Default)]
pub(crate) struct TriggerQuery {
    pub(crate) trigger: Option<String>,
}

pub(crate) async fn triggers(
    State(state): State<PageState>,
    Query(query): Query<TriggerQuery>,
) -> WebResult {
    let page = load_triggers_page(state.db, query.trigger).map_err(internal_error)?;
    let detail_html = render_partial(&TriggerDetailTemplate {
        detail: page.selected.clone(),
    })?;

    render_template(&TriggersTemplate {
        page_title: "Triggers",
        current_nav: "triggers",
        page,
        detail_html,
    })
}

#[derive(Template)]
#[template(path = "triggers.html")]
struct TriggersTemplate {
    page_title: &'static str,
    current_nav: &'static str,
    page: TriggersPageView,
    detail_html: String,
}

#[derive(Template)]
#[template(path = "partials/trigger_detail.html")]
struct TriggerDetailTemplate {
    detail: TriggerDetailView,
}
