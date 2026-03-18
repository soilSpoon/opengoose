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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Platform;

    fn discord_key() -> SessionKey {
        SessionKey::dm(Platform::Discord, "ch1")
    }

    // ── key() ─────────────────────────────────────────────────────────────────

    #[test]
    fn key_returns_stable_string_for_each_variant() {
        let cases: &[(AppEventKind, &str)] = &[
            (AppEventKind::GooseReady, "goose_ready"),
            (
                AppEventKind::ChannelReady {
                    platform: Platform::Discord,
                },
                "channel_ready",
            ),
            (
                AppEventKind::ChannelDisconnected {
                    platform: Platform::Discord,
                    reason: "test".into(),
                },
                "channel_disconnected",
            ),
            (
                AppEventKind::ChannelReconnecting {
                    platform: Platform::Telegram,
                    attempt: 1,
                    delay_secs: 5,
                },
                "channel_reconnecting",
            ),
            (
                AppEventKind::MessageReceived {
                    session_key: discord_key(),
                    author: "alice".into(),
                    content: "hello".into(),
                },
                "message_received",
            ),
            (
                AppEventKind::ResponseSent {
                    session_key: discord_key(),
                    content: "reply".into(),
                },
                "response_sent",
            ),
            (
                AppEventKind::PairingCodeGenerated { code: "ABC".into() },
                "pairing_code_generated",
            ),
            (
                AppEventKind::PairingCompleted {
                    session_key: discord_key(),
                },
                "pairing_completed",
            ),
            (
                AppEventKind::TeamActivated {
                    session_key: discord_key(),
                    team_name: "alpha".into(),
                },
                "team_activated",
            ),
            (
                AppEventKind::TeamDeactivated {
                    session_key: discord_key(),
                },
                "team_deactivated",
            ),
            (
                AppEventKind::SessionDisconnected {
                    session_key: discord_key(),
                    reason: "bye".into(),
                },
                "session_disconnected",
            ),
            (
                AppEventKind::Error {
                    context: "ctx".into(),
                    message: "oops".into(),
                },
                "error",
            ),
            (
                AppEventKind::TracingEvent {
                    level: "info".into(),
                    message: "msg".into(),
                },
                "tracing_event",
            ),
            (AppEventKind::DashboardUpdated, "dashboard_updated"),
            (
                AppEventKind::SessionUpdated {
                    session_key: discord_key(),
                },
                "session_updated",
            ),
            (
                AppEventKind::RunUpdated {
                    team_run_id: "r1".into(),
                    status: "running".into(),
                },
                "run_updated",
            ),
            (
                AppEventKind::QueueUpdated { team_run_id: None },
                "queue_updated",
            ),
            (
                AppEventKind::StreamStarted {
                    session_key: discord_key(),
                    stream_id: "s1".into(),
                },
                "stream_started",
            ),
            (
                AppEventKind::StreamUpdated {
                    session_key: discord_key(),
                    stream_id: "s1".into(),
                    content_len: 42,
                },
                "stream_updated",
            ),
            (
                AppEventKind::StreamCompleted {
                    session_key: discord_key(),
                    stream_id: "s1".into(),
                    full_text: "done".into(),
                },
                "stream_completed",
            ),
            (
                AppEventKind::TeamRunStarted {
                    team: "t".into(),
                    workflow: "chain".into(),
                    input: "go".into(),
                },
                "team_run_started",
            ),
            (
                AppEventKind::TeamStepStarted {
                    team: "t".into(),
                    agent: "a".into(),
                    step: 1,
                },
                "team_step_started",
            ),
            (
                AppEventKind::TeamStepCompleted {
                    team: "t".into(),
                    agent: "a".into(),
                },
                "team_step_completed",
            ),
            (
                AppEventKind::TeamStepFailed {
                    team: "t".into(),
                    agent: "a".into(),
                    reason: "err".into(),
                },
                "team_step_failed",
            ),
            (
                AppEventKind::TeamRunCompleted { team: "t".into() },
                "team_run_completed",
            ),
            (
                AppEventKind::TeamRunFailed {
                    team: "t".into(),
                    reason: "fail".into(),
                },
                "team_run_failed",
            ),
            (
                AppEventKind::AlertFired {
                    rule_name: "rule".into(),
                    metric: "queue_backlog".into(),
                    value: 5.0,
                    platform: "discord".into(),
                    channel_id: "ch1".into(),
                },
                "alert_fired",
            ),
            (
                AppEventKind::ModelChanged {
                    session_key: discord_key(),
                    model: "gpt-4".into(),
                    mode: "auto".into(),
                },
                "model_changed",
            ),
            (
                AppEventKind::ContextCompacted {
                    session_key: discord_key(),
                },
                "context_compacted",
            ),
            (
                AppEventKind::ExtensionNotification {
                    session_key: discord_key(),
                    extension: "ext".into(),
                },
                "extension_notification",
            ),
            (
                AppEventKind::ShutdownStarted {
                    timeout_secs: 30,
                    active_streams: 2,
                },
                "shutdown_started",
            ),
            (
                AppEventKind::ShutdownCompleted {
                    timed_out: false,
                    remaining_streams: 0,
                },
                "shutdown_completed",
            ),
        ];

        for (event, expected_key) in cases {
            assert_eq!(
                event.key(),
                *expected_key,
                "key() mismatch for variant {:?}",
                event
            );
        }
    }

    // ── source_gateway() ──────────────────────────────────────────────────────

    #[test]
    fn source_gateway_returns_platform_for_channel_events() {
        assert_eq!(
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
            .source_gateway(),
            Some("discord")
        );
        assert_eq!(
            AppEventKind::ChannelDisconnected {
                platform: Platform::Telegram,
                reason: "x".into()
            }
            .source_gateway(),
            Some("telegram")
        );
        assert_eq!(
            AppEventKind::ChannelReconnecting {
                platform: Platform::Slack,
                attempt: 1,
                delay_secs: 5
            }
            .source_gateway(),
            Some("slack")
        );
    }

    #[test]
    fn source_gateway_returns_session_platform_for_session_events() {
        let key = SessionKey::dm(Platform::Telegram, "user1");
        assert_eq!(
            AppEventKind::MessageReceived {
                session_key: key.clone(),
                author: "a".into(),
                content: "c".into()
            }
            .source_gateway(),
            Some("telegram")
        );
        assert_eq!(
            AppEventKind::StreamStarted {
                session_key: key.clone(),
                stream_id: "s".into()
            }
            .source_gateway(),
            Some("telegram")
        );
    }

    #[test]
    fn source_gateway_returns_none_for_non_channel_events() {
        assert_eq!(AppEventKind::GooseReady.source_gateway(), None);
        assert_eq!(AppEventKind::DashboardUpdated.source_gateway(), None);
        assert_eq!(
            AppEventKind::RunUpdated {
                team_run_id: "r".into(),
                status: "running".into()
            }
            .source_gateway(),
            None
        );
    }

    // ── session_key() ─────────────────────────────────────────────────────────

    #[test]
    fn session_key_returns_key_for_session_events() {
        let key = discord_key();
        assert_eq!(
            AppEventKind::MessageReceived {
                session_key: key.clone(),
                author: "a".into(),
                content: "c".into()
            }
            .session_key(),
            Some(&key)
        );
        assert_eq!(
            AppEventKind::PairingCompleted {
                session_key: key.clone()
            }
            .session_key(),
            Some(&key)
        );
        assert_eq!(
            AppEventKind::ContextCompacted {
                session_key: key.clone()
            }
            .session_key(),
            Some(&key)
        );
    }

    #[test]
    fn session_key_returns_none_for_non_session_events() {
        assert_eq!(AppEventKind::GooseReady.session_key(), None);
        assert_eq!(
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
            .session_key(),
            None
        );
        assert_eq!(
            AppEventKind::TeamRunCompleted { team: "t".into() }.session_key(),
            None
        );
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn display_goose_ready() {
        assert_eq!(
            format!("{}", AppEventKind::GooseReady),
            "goose agent system ready"
        );
    }

    #[test]
    fn display_channel_reconnecting_includes_attempt_and_delay() {
        let s = format!(
            "{}",
            AppEventKind::ChannelReconnecting {
                platform: Platform::Discord,
                attempt: 3,
                delay_secs: 10,
            }
        );
        assert!(s.contains("attempt 3"));
        assert!(s.contains("10s"));
    }

    #[test]
    fn display_queue_updated_with_and_without_run_id() {
        assert_eq!(
            format!(
                "{}",
                AppEventKind::QueueUpdated {
                    team_run_id: Some("r42".into())
                }
            ),
            "queue updated: r42"
        );
        assert_eq!(
            format!("{}", AppEventKind::QueueUpdated { team_run_id: None }),
            "queue updated"
        );
    }

    #[test]
    fn display_shutdown_completed_includes_timed_out_and_remaining() {
        let s = format!(
            "{}",
            AppEventKind::ShutdownCompleted {
                timed_out: true,
                remaining_streams: 3,
            }
        );
        assert!(s.contains("timed_out=true"));
        assert!(s.contains("remaining_streams=3"));
    }
}
