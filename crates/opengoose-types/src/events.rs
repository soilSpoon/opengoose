use std::fmt;
use std::time::Instant;

use tokio::sync::broadcast;

use crate::SessionKey;

#[derive(Debug, Clone)]
pub struct AppEvent {
    pub kind: AppEventKind,
    pub timestamp: Instant,
}

#[derive(Debug, Clone)]
pub enum AppEventKind {
    GooseReady,
    DiscordReady,
    DiscordDisconnected { reason: String },
    MessageReceived { session_key: SessionKey, author: String, content: String },
    ResponseSent { session_key: SessionKey, content: String },
    PairingCodeGenerated { code: String },
    PairingCompleted { session_key: SessionKey },
    TeamActivated { session_key: SessionKey, team_name: String },
    TeamDeactivated { session_key: SessionKey },
    SessionDisconnected { session_key: SessionKey, reason: String },
    Error { context: String, message: String },
    TracingEvent { level: String, message: String },
}

impl fmt::Display for AppEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GooseReady => write!(f, "goose agent system ready"),
            Self::DiscordReady => write!(f, "discord ready"),
            Self::DiscordDisconnected { reason } => write!(f, "discord disconnected: {reason}"),
            Self::MessageReceived { author, .. } => write!(f, "message from {author}"),
            Self::ResponseSent { .. } => write!(f, "response sent"),
            Self::PairingCodeGenerated { code } => write!(f, "pairing code: {code}"),
            Self::PairingCompleted { session_key } => write!(f, "paired: {session_key}"),
            Self::TeamActivated { session_key, team_name } => {
                write!(f, "team activated: {team_name} on {session_key}")
            }
            Self::TeamDeactivated { session_key } => {
                write!(f, "team deactivated on {session_key}")
            }
            Self::SessionDisconnected { session_key, reason } => {
                write!(f, "session disconnected: {session_key} ({reason})")
            }
            Self::Error { context, message } => write!(f, "error [{context}]: {message}"),
            Self::TracingEvent { level, message } => write!(f, "[{level}] {message}"),
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
        bus.emit(AppEventKind::DiscordReady);
        let event = rx.recv().await.unwrap();
        assert!(matches!(event.kind, AppEventKind::DiscordReady));
    }

    #[test]
    fn test_event_bus_no_subscribers_no_panic() {
        let bus = EventBus::new(16);
        bus.emit(AppEventKind::DiscordReady);
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        bus.emit(AppEventKind::DiscordReady);
        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert!(matches!(e1.kind, AppEventKind::DiscordReady));
        assert!(matches!(e2.kind, AppEventKind::DiscordReady));
    }

    #[test]
    fn test_app_event_kind_display() {
        assert_eq!(AppEventKind::DiscordReady.to_string(), "discord ready");
        assert_eq!(
            AppEventKind::DiscordDisconnected { reason: "bye".into() }.to_string(),
            "discord disconnected: bye"
        );
        assert_eq!(
            AppEventKind::PairingCodeGenerated { code: "ABC123".into() }.to_string(),
            "pairing code: ABC123"
        );
        assert_eq!(
            AppEventKind::Error { context: "test".into(), message: "fail".into() }.to_string(),
            "error [test]: fail"
        );
    }
}
