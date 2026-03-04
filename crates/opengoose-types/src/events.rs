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
    DiscordReady,
    DiscordDisconnected { reason: String },
    MessageReceived { session_key: SessionKey, author: String, content: String },
    ResponseSent { session_key: SessionKey, content: String },
    PairingCodeGenerated { code: String },
    PairingCompleted { session_key: SessionKey },
    Error { context: String, message: String },
    TracingEvent { level: String, message: String },
}

impl fmt::Display for AppEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DiscordReady => write!(f, "discord ready"),
            Self::DiscordDisconnected { reason } => write!(f, "discord disconnected: {reason}"),
            Self::MessageReceived { author, .. } => write!(f, "message from {author}"),
            Self::ResponseSent { .. } => write!(f, "response sent"),
            Self::PairingCodeGenerated { code } => write!(f, "pairing code: {code}"),
            Self::PairingCompleted { session_key } => write!(f, "paired: {session_key}"),
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
