use crate::data::{RemoteAgentsPageView, load_remote_agents_page};
use crate::routes::{PartialResult, WebResult, internal_error, render_partial, render_template};
use crate::server::PageState;

use super::templates::{
    RemoteAgentActionStatusTemplate, RemoteAgentsLiveTemplate, RemoteAgentsTemplate,
};

pub(super) async fn render_remote_agents_page(state: &PageState, websocket: String) -> WebResult {
    let page = load_page(state, websocket).await?;
    let live_html = render_remote_agents_live_partial(page.clone())?;

    render_template(&RemoteAgentsTemplate {
        page_title: "Remote Agents",
        current_nav: "remote_agents",
        page,
        live_html,
    })
}

pub(super) async fn render_remote_agents_live_shell(
    state: &PageState,
    websocket: String,
) -> PartialResult {
    let page = load_page(state, websocket).await?;
    render_remote_agents_live_shell_from_page(page)
}

pub(super) async fn render_remote_agents_stream_html(
    state: &PageState,
    websocket: String,
) -> PartialResult {
    render_remote_agents_live_shell(state, websocket).await
}

pub(super) fn render_remote_agents_action_status(
    message: String,
    tone: &'static str,
) -> PartialResult {
    render_partial(&RemoteAgentActionStatusTemplate { message, tone })
}

pub(super) fn remote_agents_stream_error_html() -> &'static str {
    r#"
<div id="remote-agents-live">
  <section class="callout tone-danger">
    <p class="eyebrow">Registry unavailable</p>
    <h2>Remote agent snapshot unavailable</h2>
    <p>The board keeps listening for the next registry update and will patch back in as soon as the stream resumes.</p>
  </section>
</div>
"#
}

async fn load_page(
    state: &PageState,
    websocket: String,
) -> Result<RemoteAgentsPageView, (axum::http::StatusCode, axum::response::Html<String>)> {
    load_remote_agents_page(&state.remote_registry, websocket)
        .await
        .map_err(internal_error)
}

fn render_remote_agents_live_shell_from_page(page: RemoteAgentsPageView) -> PartialResult {
    let live_html = render_remote_agents_live_partial(page)?;
    Ok(format!(r#"<div id="remote-agents-live">{live_html}</div>"#))
}

fn render_remote_agents_live_partial(page: RemoteAgentsPageView) -> PartialResult {
    render_partial(&RemoteAgentsLiveTemplate { page })
}
