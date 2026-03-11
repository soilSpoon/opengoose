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
    ShutdownStarted {
        timeout_secs: u64,
        active_streams: usize,
    },
    ShutdownCompleted {
        timed_out: bool,
        remaining_streams: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_event_kind_serializes_with_type_tag() {
        let value = serde_json::to_value(AppEventKind::MessageReceived {
            session_key: SessionKey::from_stable_id("discord:ns:ops:bridge"),
            author: "alice".into(),
            content: "hello".into(),
        })
        .expect("event should serialize");

        assert_eq!(value["type"], "message_received");
        assert_eq!(value["session_key"], "discord:ns:ops:bridge");
    }
}
