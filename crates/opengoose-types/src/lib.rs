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

    /// Alias for [`direct`](Self::direct) — create a DM/direct session key.
    pub fn dm(channel_id: impl Into<String>) -> Self {
        Self::direct(channel_id)
    }

    /// Encode as a stable string identifier for persistence and cross-component use.
    ///
    /// Format: `ns:<namespace>:<channel_id>` for namespaced keys, `direct:<id>` for direct keys.
    pub fn to_stable_id(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("ns:{ns}:{}", self.channel_id),
            None => format!("direct:{}", self.channel_id),
        }
    }

    /// Decode from a stable string identifier.
    ///
    /// Supports the current `ns:` / `direct:` format as well as the legacy `dm:` prefix.
    pub fn from_stable_id(id: &str) -> Self {
        if let Some(rest) = id.strip_prefix("direct:") {
            Self::direct(rest)
        } else if let Some(rest) = id.strip_prefix("dm:") {
            Self::direct(rest)
        } else if let Some(rest) = id.strip_prefix("ns:") {
            if let Some((ns, channel)) = rest.split_once(':') {
                Self::new(ns, channel)
            } else {
                // Malformed ns: prefix with no second colon — treat as direct
                Self::direct(rest)
            }
        } else if let Some((ns, channel)) = id.split_once(':') {
            // Legacy format: `namespace:channel` (before the `ns:` prefix was introduced).
            Self::new(ns, channel)
        } else {
            Self::direct(id)
        }
    }
}

/// Sanitize a string for use as a filename or database key.
///
/// Replaces any character that isn't ASCII alphanumeric, `-`, or `_` with `_`.
/// Shared across crates to avoid duplicating sanitization logic.
pub fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_stable_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key_new() {
        let key = SessionKey::new("guild1", "thread1");
        assert_eq!(key.namespace, Some("guild1".into()));
        assert_eq!(key.channel_id, "thread1");
    }

    #[test]
    fn test_session_key_dm() {
        let key = SessionKey::dm("user42");
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user42");
    }

    #[test]
    fn test_session_key_direct() {
        let key = SessionKey::direct("user42");
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user42");
        // dm and direct should produce the same result
        assert_eq!(key, SessionKey::dm("user42"));
    }

    #[test]
    fn test_to_stable_id_namespaced() {
        let key = SessionKey::new("g", "t");
        assert_eq!(key.to_stable_id(), "ns:g:t");
    }

    #[test]
    fn test_to_stable_id_direct() {
        let key = SessionKey::dm("u");
        assert_eq!(key.to_stable_id(), "direct:u");
    }

    #[test]
    fn test_from_stable_id_direct() {
        let key = SessionKey::from_stable_id("direct:user1");
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user1");
    }

    #[test]
    fn test_from_stable_id_namespaced() {
        let key = SessionKey::from_stable_id("ns:guild1:thread1");
        assert_eq!(key.namespace, Some("guild1".into()));
        assert_eq!(key.channel_id, "thread1");
    }

    #[test]
    fn test_from_stable_id_dm_prefix() {
        let key = SessionKey::from_stable_id("dm:user123");
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user123");
        // Should be equivalent to a direct session key
        assert_eq!(key, SessionKey::direct("user123"));
    }

    #[test]
    fn test_from_stable_id_bare() {
        let key = SessionKey::from_stable_id("barevalue");
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "barevalue");
    }

    #[test]
    fn test_from_stable_id_legacy_namespaced() {
        // Old format before the `ns:` prefix was introduced: `namespace:channel`
        let key = SessionKey::from_stable_id("guild1:thread1");
        assert_eq!(key.namespace, Some("guild1".into()));
        assert_eq!(key.channel_id, "thread1");
    }

    #[test]
    fn test_roundtrip_namespaced_direct_namespace() {
        // A namespace literally called "direct" must roundtrip correctly
        // through the current ns: format.
        let key = SessionKey::new("direct", "ch1");
        assert_eq!(key.to_stable_id(), "ns:direct:ch1");
        assert_eq!(SessionKey::from_stable_id(&key.to_stable_id()), key);
    }

    #[test]
    fn test_roundtrip_encoding() {
        let guild_key = SessionKey::new("guild123", "thread456");
        let dm_key = SessionKey::dm("user789");

        assert_eq!(
            SessionKey::from_stable_id(&guild_key.to_stable_id()),
            guild_key
        );
        assert_eq!(
            SessionKey::from_stable_id(&dm_key.to_stable_id()),
            dm_key
        );
    }

    #[test]
    fn test_session_key_display() {
        let guild_key = SessionKey::new("g1", "t2");
        assert_eq!(format!("{}", guild_key), "ns:g1:t2");

        let dm_key = SessionKey::dm("u3");
        assert_eq!(format!("{}", dm_key), "direct:u3");
    }
}
