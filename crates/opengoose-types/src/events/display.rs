use std::fmt;

use crate::SessionKey;

use super::AppEventKind;

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
            | Self::StreamCompleted { session_key, .. } => Some(session_key.platform.as_str()),
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
            | Self::StreamCompleted { session_key, .. } => Some(session_key),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Platform;

    use super::*;

    #[test]
    fn test_app_event_kind_display() {
        assert_eq!(
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
            .to_string(),
            "discord ready"
        );
        assert_eq!(
            AppEventKind::ChannelDisconnected {
                platform: Platform::Discord,
                reason: "bye".into()
            }
            .to_string(),
            "discord disconnected: bye"
        );
        assert_eq!(
            AppEventKind::PairingCodeGenerated {
                code: "ABC123".into()
            }
            .to_string(),
            "pairing code: ABC123"
        );
        assert_eq!(
            AppEventKind::Error {
                context: "test".into(),
                message: "fail".into()
            }
            .to_string(),
            "error [test]: fail"
        );
    }

    #[test]
    fn test_app_event_kind_display_all_variants() {
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");

        assert_eq!(
            AppEventKind::GooseReady.to_string(),
            "goose agent system ready"
        );

        assert_eq!(
            AppEventKind::MessageReceived {
                session_key: key.clone(),
                author: "alice".into(),
                content: "hi".into(),
            }
            .to_string(),
            "message from alice"
        );

        assert_eq!(
            AppEventKind::ResponseSent {
                session_key: key.clone(),
                content: "reply".into(),
            }
            .to_string(),
            "response sent"
        );

        assert_eq!(
            AppEventKind::PairingCompleted {
                session_key: key.clone(),
            }
            .to_string(),
            format!("paired: {key}")
        );

        assert_eq!(
            AppEventKind::TeamActivated {
                session_key: key.clone(),
                team_name: "review".into(),
            }
            .to_string(),
            format!("team activated: review on {key}")
        );

        assert_eq!(
            AppEventKind::TeamDeactivated {
                session_key: key.clone(),
            }
            .to_string(),
            format!("team deactivated on {key}")
        );

        assert_eq!(
            AppEventKind::SessionDisconnected {
                session_key: key.clone(),
                reason: "timeout".into(),
            }
            .to_string(),
            format!("session disconnected: {key} (timeout)")
        );

        assert_eq!(
            AppEventKind::TracingEvent {
                level: "INFO".into(),
                message: "started".into(),
            }
            .to_string(),
            "[INFO] started"
        );

        assert_eq!(
            AppEventKind::DashboardUpdated.to_string(),
            "dashboard updated"
        );

        assert_eq!(
            AppEventKind::SessionUpdated {
                session_key: key.clone(),
            }
            .to_string(),
            format!("session updated: {key}")
        );

        assert_eq!(
            AppEventKind::RunUpdated {
                team_run_id: "run-1".into(),
                status: "running".into(),
            }
            .to_string(),
            "run updated: run-1 (running)"
        );

        assert_eq!(
            AppEventKind::QueueUpdated {
                team_run_id: Some("run-1".into()),
            }
            .to_string(),
            "queue updated: run-1"
        );

        assert_eq!(
            AppEventKind::TeamRunStarted {
                team: "review".into(),
                workflow: "chain".into(),
                input: "check code".into(),
            }
            .to_string(),
            "team run started: review (chain)"
        );

        assert_eq!(
            AppEventKind::TeamStepStarted {
                team: "review".into(),
                agent: "coder".into(),
                step: 0,
            }
            .to_string(),
            "team review: step 0 started (agent: coder)"
        );

        assert_eq!(
            AppEventKind::TeamStepCompleted {
                team: "review".into(),
                agent: "coder".into(),
            }
            .to_string(),
            "team review: agent coder completed"
        );

        assert_eq!(
            AppEventKind::TeamStepFailed {
                team: "review".into(),
                agent: "coder".into(),
                reason: "crash".into(),
            }
            .to_string(),
            "team review: agent coder failed: crash"
        );

        assert_eq!(
            AppEventKind::TeamRunCompleted {
                team: "review".into(),
            }
            .to_string(),
            "team run completed: review"
        );

        assert_eq!(
            AppEventKind::TeamRunFailed {
                team: "review".into(),
                reason: "all failed".into(),
            }
            .to_string(),
            "team run failed: review: all failed"
        );
    }

    #[test]
    fn test_channel_reconnecting_display() {
        assert_eq!(
            AppEventKind::ChannelReconnecting {
                platform: Platform::Slack,
                attempt: 3,
                delay_secs: 5,
            }
            .to_string(),
            "slack reconnecting (attempt 3, delay 5s)"
        );

        assert_eq!(
            AppEventKind::ChannelReconnecting {
                platform: Platform::Discord,
                attempt: 1,
                delay_secs: 0,
            }
            .to_string(),
            "discord reconnecting (attempt 1, delay 0s)"
        );
    }

    #[test]
    fn test_streaming_event_kind_display() {
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");

        assert_eq!(
            AppEventKind::StreamStarted {
                session_key: key.clone(),
                stream_id: "s-42".into(),
            }
            .to_string(),
            "stream started: s-42"
        );

        assert_eq!(
            AppEventKind::StreamUpdated {
                session_key: key.clone(),
                stream_id: "s-42".into(),
                content_len: 128,
            }
            .to_string(),
            "stream updated: s-42 (128 bytes)"
        );

        assert_eq!(
            AppEventKind::StreamCompleted {
                session_key: key,
                stream_id: "s-42".into(),
                full_text: "hello world".into(),
            }
            .to_string(),
            "stream completed: s-42"
        );
    }
}
