use std::fmt;
use std::time::Instant;

use tokio::sync::broadcast;

use crate::{Platform, SessionKey};

#[derive(Debug, Clone)]
pub struct AppEvent {
    pub kind: AppEventKind,
    pub timestamp: Instant,
}

#[derive(Debug, Clone)]
pub enum AppEventKind {
    GooseReady,
    ChannelReady {
        platform: Platform,
    },
    ChannelDisconnected {
        platform: Platform,
        reason: String,
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
}

impl fmt::Display for AppEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GooseReady => write!(f, "goose agent system ready"),
            Self::ChannelReady { platform } => write!(f, "{platform} ready"),
            Self::ChannelDisconnected { platform, reason } => {
                write!(f, "{platform} disconnected: {reason}")
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn emit(&self, kind: AppEventKind) {
        let event = AppEvent {
            kind,
            timestamp: Instant::now(),
        };
        // Ignore error — means no subscribers
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_emit_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
    }

    #[test]
    fn test_event_bus_no_subscribers_no_panic() {
        let bus = EventBus::new(16);
        bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Discord,
        });
        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(
            e1.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
        assert!(matches!(
            e2.kind,
            AppEventKind::ChannelReady {
                platform: Platform::Discord
            }
        ));
    }

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

        // Stream event variants
        assert_eq!(
            AppEventKind::StreamStarted {
                session_key: key.clone(),
                stream_id: "s1".into(),
            }
            .to_string(),
            "stream started: s1"
        );

        assert_eq!(
            AppEventKind::StreamUpdated {
                session_key: key.clone(),
                stream_id: "s1".into(),
                content_len: 42,
            }
            .to_string(),
            "stream updated: s1 (42 bytes)"
        );

        assert_eq!(
            AppEventKind::StreamCompleted {
                session_key: key,
                stream_id: "s1".into(),
                full_text: "done".into(),
            }
            .to_string(),
            "stream completed: s1"
        );
    }
}
