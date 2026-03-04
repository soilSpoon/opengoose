mod events;

pub use events::{AppEvent, AppEventKind, EventBus};

/// Platform-agnostic session identifier.
///
/// A session is scoped by an optional namespace (e.g. a Discord guild, a Slack workspace)
/// and a required channel identifier (e.g. a Discord channel, a CLI session, a web session).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    /// Optional namespace for grouping channels (e.g. guild ID, workspace ID).
    pub namespace: Option<String>,
    /// Unique channel/conversation identifier within the namespace.
    pub channel_id: String,
}

impl SessionKey {
    /// Create a namespaced session key (e.g. guild + channel).
    pub fn new(namespace: impl Into<String>, channel_id: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            channel_id: channel_id.into(),
        }
    }

    /// Create a session key without a namespace (e.g. DM, CLI session).
    pub fn direct(channel_id: impl Into<String>) -> Self {
        Self {
            namespace: None,
            channel_id: channel_id.into(),
        }
    }

    /// Encode as a stable string identifier for persistence and cross-component use.
    pub fn to_stable_id(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{ns}:{}", self.channel_id),
            None => format!("direct:{}", self.channel_id),
        }
    }

    /// Decode from a stable string identifier.
    pub fn from_stable_id(id: &str) -> Self {
        if let Some(rest) = id.strip_prefix("direct:") {
            Self::direct(rest)
        } else if let Some((ns, channel)) = id.split_once(':') {
            Self::new(ns, channel)
        } else {
            Self::direct(id)
        }
    }
}

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_stable_id())
    }
}
