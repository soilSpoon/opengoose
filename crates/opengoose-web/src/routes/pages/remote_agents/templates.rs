use askama::Template;

use crate::data::RemoteAgentsPageView;

#[derive(Template)]
#[template(path = "remote_agents.html")]
pub(super) struct RemoteAgentsTemplate {
    pub(super) page_title: &'static str,
    pub(super) current_nav: &'static str,
    pub(super) page: RemoteAgentsPageView,
    pub(super) live_html: String,
}

#[derive(Template)]
#[template(path = "partials/remote_agents_live.html")]
pub(super) struct RemoteAgentsLiveTemplate {
    pub(super) page: RemoteAgentsPageView,
}

#[derive(Template)]
#[template(path = "partials/remote_agents_action_status.html")]
pub(super) struct RemoteAgentActionStatusTemplate {
    pub(super) message: String,
    pub(super) tone: &'static str,
}
