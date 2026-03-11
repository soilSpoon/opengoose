use opengoose_types::AppEventKind;

use super::super::state::*;

pub(super) fn apply(app: &mut App, kind: &AppEventKind) {
    match kind {
        AppEventKind::GooseReady => {}
        AppEventKind::ChannelReady { platform } => {
            app.connected_platforms.insert(platform.clone());
        }
        AppEventKind::ChannelDisconnected { platform, .. } => {
            app.connected_platforms.remove(platform);
        }
        AppEventKind::MessageReceived {
            session_key,
            author,
            content,
        } => {
            app.cache_message(MessageEntry {
                session_key: session_key.clone(),
                author: author.clone(),
                content: content.clone(),
            });
            app.refresh_sessions();
        }
        AppEventKind::ResponseSent {
            session_key,
            content,
        } => {
            app.cache_message(MessageEntry {
                session_key: session_key.clone(),
                author: "goose".into(),
                content: content.clone(),
            });
            app.refresh_sessions();
        }
        AppEventKind::PairingCodeGenerated { code } => {
            app.pairing_code = Some(code.clone());
        }
        AppEventKind::PairingCompleted { session_key } => {
            app.active_sessions.insert(session_key.clone());
            app.refresh_sessions();
        }
        AppEventKind::SessionDisconnected { session_key, .. } => {
            app.active_sessions.remove(session_key);
            app.refresh_sessions();
        }
        AppEventKind::TeamActivated {
            session_key,
            team_name,
        } => {
            app.active_teams
                .insert(session_key.clone(), team_name.clone());
            app.refresh_sessions();
        }
        AppEventKind::TeamDeactivated { session_key } => {
            app.active_teams.remove(session_key);
            app.refresh_sessions();
        }
        AppEventKind::Error { .. } => {
            app.set_agent_status(AgentStatus::Idle, None);
        }
        AppEventKind::TracingEvent { .. } => {}
        AppEventKind::StreamStarted { session_key, .. } => {
            app.set_agent_status(AgentStatus::Thinking, Some(session_key.clone()));
        }
        AppEventKind::StreamUpdated { session_key, .. } => {
            app.set_agent_status(AgentStatus::Generating, Some(session_key.clone()));
        }
        AppEventKind::StreamCompleted { session_key, .. } => {
            app.set_agent_status(AgentStatus::Idle, Some(session_key.clone()));
        }
        AppEventKind::TeamRunStarted { .. }
        | AppEventKind::TeamStepStarted { .. }
        | AppEventKind::TeamStepCompleted { .. }
        | AppEventKind::TeamStepFailed { .. }
        | AppEventKind::TeamRunCompleted { .. }
        | AppEventKind::TeamRunFailed { .. }
        | AppEventKind::ChannelReconnecting { .. }
        | AppEventKind::DashboardUpdated
        | AppEventKind::SessionUpdated { .. }
        | AppEventKind::RunUpdated { .. }
        | AppEventKind::QueueUpdated { .. }
        | AppEventKind::AlertFired { .. }
        | AppEventKind::ModelChanged { .. }
        | AppEventKind::ContextCompacted { .. }
        | AppEventKind::ExtensionNotification { .. }
        | AppEventKind::ShutdownStarted { .. }
        | AppEventKind::ShutdownCompleted { .. } => {}
    }
}

pub(super) fn shows_in_messages(kind: &AppEventKind) -> bool {
    matches!(
        kind,
        AppEventKind::MessageReceived { .. } | AppEventKind::ResponseSent { .. }
    )
}
