use std::fmt;

use crate::{Platform, SessionKey};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEventKind {
    GooseReady,
    ChannelReady {
        platform: Platform,
    },
    ChannelDisconnected {
        platform: Platform,
        reason: String,
    },
    /// Emitted each time a channel adapter begins a reconnect attempt.
    ChannelReconnecting {
        platform: Platform,
        /// Reconnect attempt number (starts at 1).
        attempt: u32,
        /// Seconds until the next reconnect will be attempted.
        delay_secs: u64,
    },
    MessageReceived {
        session_key: SessionKey,
        author: String,
        content: String,
    },
    ResponseSent {
        session_key: SessionKey,
        content: String,
    },
    PairingCodeGenerated {
        code: String,
    },
    PairingCompleted {
        session_key: SessionKey,
    },
    TeamActivated {
        session_key: SessionKey,
        team_name: String,
    },
    TeamDeactivated {
        session_key: SessionKey,
    },
    SessionDisconnected {
        session_key: SessionKey,
        reason: String,
    },
    Error {
        context: String,
        message: String,
    },
    TracingEvent {
        level: String,
        message: String,
    },
    DashboardUpdated,
    SessionUpdated {
        session_key: SessionKey,
    },
    RunUpdated {
        team_run_id: String,
        status: String,
    },
    QueueUpdated {
        team_run_id: Option<String>,
    },

    // Streaming response events
    StreamStarted {
        session_key: SessionKey,
        stream_id: String,
    },
    StreamUpdated {
        session_key: SessionKey,
        stream_id: String,
        content_len: usize,
    },
    StreamCompleted {
        session_key: SessionKey,
        stream_id: String,
        full_text: String,
    },

    // Team orchestration events
    TeamRunStarted {
        team: String,
        workflow: String,
        input: String,
    },
    TeamStepStarted {
        team: String,
        agent: String,
        step: usize,
    },
    TeamStepCompleted {
        team: String,
        agent: String,
    },
    TeamStepFailed {
        team: String,
        agent: String,
        reason: String,
    },
    TeamRunCompleted {
        team: String,
    },
    TeamRunFailed {
        team: String,
        reason: String,
    },
    /// Emitted when an alert fires a ChannelMessage action.
    AlertFired {
        rule_name: String,
        metric: String,
        value: f64,
        platform: String,
        channel_id: String,
    },

    // Goose agent events (forwarded from AgentEvent stream)
    ModelChanged {
        session_key: SessionKey,
        model: String,
        mode: String,
    },
    ContextCompacted {
        session_key: SessionKey,
    },
    ExtensionNotification {
        session_key: SessionKey,
        extension: String,
    },
    /// Emitted when an agent completes its landing protocol.
    AgentLanding {
        team: String,
        agent: String,
    },
    /// Emitted when an agent exceeds the stuck timeout threshold.
    AgentStuck {
        team: String,
        agent: String,
    },
    /// Emitted when an agent exceeds the zombie timeout threshold.
    AgentZombie {
        team: String,
        agent: String,
    },
    ShutdownStarted {
        timeout_secs: u64,
        active_streams: usize,
    },
    ShutdownCompleted {
        timed_out: bool,
        remaining_streams: usize,
    },
}

impl fmt::Display for AppEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GooseReady => write!(f, "goose agent system ready"),
            Self::ChannelReady { platform } => write!(f, "{platform} ready"),
            Self::ChannelDisconnected { platform, reason } => {
                write!(f, "{platform} disconnected: {reason}")
            }
            Self::ChannelReconnecting {
                platform,
                attempt,
                delay_secs,
            } => {
                write!(
                    f,
                    "{platform} reconnecting (attempt {attempt}, delay {delay_secs}s)"
                )
            }
            Self::MessageReceived { author, .. } => write!(f, "message from {author}"),
            Self::ResponseSent { .. } => write!(f, "response sent"),
            Self::PairingCodeGenerated { code } => write!(f, "pairing code: {code}"),
            Self::PairingCompleted { session_key } => write!(f, "paired: {session_key}"),
            Self::TeamActivated {
                session_key,
                team_name,
            } => {
                write!(f, "team activated: {team_name} on {session_key}")
            }
            Self::TeamDeactivated { session_key } => {
                write!(f, "team deactivated on {session_key}")
            }
            Self::SessionDisconnected {
                session_key,
                reason,
            } => {
                write!(f, "session disconnected: {session_key} ({reason})")
            }
            Self::Error { context, message } => write!(f, "error [{context}]: {message}"),
            Self::TracingEvent { level, message } => write!(f, "[{level}] {message}"),
            Self::DashboardUpdated => write!(f, "dashboard updated"),
            Self::SessionUpdated { session_key } => write!(f, "session updated: {session_key}"),
            Self::RunUpdated {
                team_run_id,
                status,
            } => write!(f, "run updated: {team_run_id} ({status})"),
            Self::QueueUpdated { team_run_id } => match team_run_id {
                Some(team_run_id) => write!(f, "queue updated: {team_run_id}"),
                None => write!(f, "queue updated"),
            },

            Self::StreamStarted { stream_id, .. } => {
                write!(f, "stream started: {stream_id}")
            }
            Self::StreamUpdated {
                stream_id,
                content_len,
                ..
            } => {
                write!(f, "stream updated: {stream_id} ({content_len} bytes)")
            }
            Self::StreamCompleted { stream_id, .. } => {
                write!(f, "stream completed: {stream_id}")
            }

            Self::TeamRunStarted { team, workflow, .. } => {
                write!(f, "team run started: {team} ({workflow})")
            }
            Self::TeamStepStarted { team, agent, step } => {
                write!(f, "team {team}: step {step} started (agent: {agent})")
            }
            Self::TeamStepCompleted { team, agent } => {
                write!(f, "team {team}: agent {agent} completed")
            }
            Self::TeamStepFailed {
                team,
                agent,
                reason,
            } => {
                write!(f, "team {team}: agent {agent} failed: {reason}")
            }
            Self::TeamRunCompleted { team } => write!(f, "team run completed: {team}"),
            Self::TeamRunFailed { team, reason } => {
                write!(f, "team run failed: {team}: {reason}")
            }
            Self::AlertFired {
                rule_name,
                platform,
                channel_id,
                ..
            } => {
                write!(f, "alert fired: {rule_name} -> {platform}:{channel_id}")
            }
            Self::ModelChanged { model, mode, .. } => write!(f, "model changed: {model} ({mode})"),
            Self::ContextCompacted { session_key } => write!(f, "context compacted: {session_key}"),
            Self::ExtensionNotification { extension, .. } => {
                write!(f, "extension notification: {extension}")
            }
            Self::AgentLanding { team, agent } => {
                write!(f, "agent landed: {agent} in team {team}")
            }
            Self::AgentStuck { team, agent } => {
                write!(f, "agent stuck: {agent} in team {team}")
            }
            Self::AgentZombie { team, agent } => {
                write!(f, "agent zombie: {agent} in team {team}")
            }
            Self::ShutdownStarted {
                timeout_secs,
                active_streams,
            } => {
                write!(
                    f,
                    "shutdown started: draining {active_streams} stream(s), timeout {timeout_secs}s"
                )
            }
            Self::ShutdownCompleted {
                timed_out,
                remaining_streams,
            } => {
                write!(
                    f,
                    "shutdown completed: timed_out={timed_out}, remaining_streams={remaining_streams}"
                )
            }
        }
    }
}

impl AppEventKind {
    /// Stable snake_case key for persistence and filtering.
    pub fn key(&self) -> &'static str {
        match self {
            Self::GooseReady => "goose_ready",
            Self::ChannelReady { .. } => "channel_ready",
            Self::ChannelDisconnected { .. } => "channel_disconnected",
            Self::ChannelReconnecting { .. } => "channel_reconnecting",
            Self::MessageReceived { .. } => "message_received",
            Self::ResponseSent { .. } => "response_sent",
            Self::PairingCodeGenerated { .. } => "pairing_code_generated",
            Self::PairingCompleted { .. } => "pairing_completed",
            Self::TeamActivated { .. } => "team_activated",
            Self::TeamDeactivated { .. } => "team_deactivated",
            Self::SessionDisconnected { .. } => "session_disconnected",
            Self::Error { .. } => "error",
            Self::TracingEvent { .. } => "tracing_event",
            Self::DashboardUpdated => "dashboard_updated",
            Self::SessionUpdated { .. } => "session_updated",
            Self::RunUpdated { .. } => "run_updated",
            Self::QueueUpdated { .. } => "queue_updated",
            Self::StreamStarted { .. } => "stream_started",
            Self::StreamUpdated { .. } => "stream_updated",
            Self::StreamCompleted { .. } => "stream_completed",
            Self::TeamRunStarted { .. } => "team_run_started",
            Self::TeamStepStarted { .. } => "team_step_started",
            Self::TeamStepCompleted { .. } => "team_step_completed",
            Self::TeamStepFailed { .. } => "team_step_failed",
            Self::TeamRunCompleted { .. } => "team_run_completed",
            Self::TeamRunFailed { .. } => "team_run_failed",
            Self::AlertFired { .. } => "alert_fired",
            Self::ModelChanged { .. } => "model_changed",
            Self::ContextCompacted { .. } => "context_compacted",
            Self::ExtensionNotification { .. } => "extension_notification",
            Self::AgentLanding { .. } => "agent_landing",
            Self::AgentStuck { .. } => "agent_stuck",
            Self::AgentZombie { .. } => "agent_zombie",
            Self::ShutdownStarted { .. } => "shutdown_started",
            Self::ShutdownCompleted { .. } => "shutdown_completed",
        }
    }

    /// Derive the originating gateway when the event is tied to one.
    pub fn source_gateway(&self) -> Option<&str> {
        match self {
            Self::ChannelReady { platform }
            | Self::ChannelDisconnected { platform, .. }
            | Self::ChannelReconnecting { platform, .. } => Some(platform.as_str()),
            Self::MessageReceived { session_key, .. }
            | Self::ResponseSent { session_key, .. }
            | Self::PairingCompleted { session_key }
            | Self::TeamActivated { session_key, .. }
            | Self::TeamDeactivated { session_key }
            | Self::SessionDisconnected { session_key, .. }
            | Self::SessionUpdated { session_key }
            | Self::StreamStarted { session_key, .. }
            | Self::StreamUpdated { session_key, .. }
            | Self::StreamCompleted { session_key, .. }
            | Self::ModelChanged { session_key, .. }
            | Self::ContextCompacted { session_key }
            | Self::ExtensionNotification { session_key, .. } => {
                Some(session_key.platform.as_str())
            }
            Self::AlertFired { platform, .. } => Some(platform.as_str()),
            _ => None,
        }
    }

    /// Return the associated session key when present.
    pub fn session_key(&self) -> Option<&SessionKey> {
        match self {
            Self::MessageReceived { session_key, .. }
            | Self::ResponseSent { session_key, .. }
            | Self::PairingCompleted { session_key }
            | Self::TeamActivated { session_key, .. }
            | Self::TeamDeactivated { session_key }
            | Self::SessionDisconnected { session_key, .. }
            | Self::SessionUpdated { session_key }
            | Self::StreamStarted { session_key, .. }
            | Self::StreamUpdated { session_key, .. }
            | Self::StreamCompleted { session_key, .. }
            | Self::ModelChanged { session_key, .. }
            | Self::ContextCompacted { session_key }
            | Self::ExtensionNotification { session_key, .. } => Some(session_key),
            _ => None,
        }
    }
}
