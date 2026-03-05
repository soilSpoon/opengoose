mod events;
pub mod streaming;
mod yaml_store;

pub use events::{AppEvent, AppEventKind, EventBus};
pub use streaming::{StreamChunk, StreamId, stream_channel};
pub use yaml_store::{YamlDefinition, YamlFileStore};

/// Messaging platform that a channel belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Discord,
    Telegram,
    Slack,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Slack => "slack",
        }
    }

    /// Parse from a string. Returns `None` for unknown platforms.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "discord" => Some(Self::Discord),
            "telegram" => Some(Self::Telegram),
            "slack" => Some(Self::Slack),
            _ => None,
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Platform-agnostic session identifier.
///
/// A session is scoped by a platform, an optional namespace (e.g. a Discord guild,
/// a Slack workspace), and a required channel identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    /// The messaging platform this session belongs to.
    pub platform: Platform,
    /// Optional namespace for grouping channels (e.g. guild ID, workspace ID).
    pub namespace: Option<String>,
    /// Unique channel/conversation identifier within the namespace.
    pub channel_id: String,
}

impl SessionKey {
    /// Create a namespaced session key (e.g. guild + channel).
    pub fn new(
        platform: Platform,
        namespace: impl Into<String>,
        channel_id: impl Into<String>,
    ) -> Self {
        Self {
            platform,
            namespace: Some(namespace.into()),
            channel_id: channel_id.into(),
        }
    }

    /// Create a session key without a namespace (e.g. DM, CLI session).
    pub fn direct(platform: Platform, channel_id: impl Into<String>) -> Self {
        Self {
            platform,
            namespace: None,
            channel_id: channel_id.into(),
        }
    }

    /// Alias for [`direct`](Self::direct) — create a DM/direct session key.
    pub fn dm(platform: Platform, channel_id: impl Into<String>) -> Self {
        Self::direct(platform, channel_id)
    }

    /// Encode as a stable string identifier for persistence and cross-component use.
    ///
    /// Format: `<platform>:ns:<namespace>:<channel_id>` or `<platform>:direct:<id>`.
    pub fn to_stable_id(&self) -> String {
        let p = self.platform.as_str();
        match &self.namespace {
            Some(ns) => format!("{p}:ns:{ns}:{}", self.channel_id),
            None => format!("{p}:direct:{}", self.channel_id),
        }
    }

    /// Decode from a stable string identifier.
    ///
    /// Supports the new `<platform>:ns:` / `<platform>:direct:` format and falls back
    /// to `Platform::Discord` for legacy IDs without a platform prefix.
    pub fn from_stable_id(id: &str) -> Self {
        // Try new format: <platform>:<rest>
        if let Some((first, rest)) = id.split_once(':') {
            if let Some(platform) = Platform::from_str_opt(first) {
                return Self::parse_after_platform(platform, rest);
            }
        }
        // Legacy format (no platform prefix) — default to Discord
        Self::parse_after_platform(Platform::Discord, id)
    }

    fn parse_after_platform(platform: Platform, rest: &str) -> Self {
        if let Some(rest) = rest.strip_prefix("direct:") {
            Self::direct(platform, rest)
        } else if let Some(rest) = rest.strip_prefix("dm:") {
            Self::direct(platform, rest)
        } else if let Some(rest) = rest.strip_prefix("ns:") {
            if let Some((ns, channel)) = rest.split_once(':') {
                Self::new(platform, ns, channel)
            } else {
                Self::direct(platform, rest)
            }
        } else if let Some((ns, channel)) = rest.split_once(':') {
            // Legacy: `namespace:channel`
            Self::new(platform, ns, channel)
        } else {
            Self::direct(platform, rest)
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
    fn test_platform_display_and_parse() {
        assert_eq!(Platform::Discord.as_str(), "discord");
        assert_eq!(Platform::Telegram.as_str(), "telegram");
        assert_eq!(Platform::Slack.as_str(), "slack");
        assert_eq!(Platform::from_str_opt("discord"), Some(Platform::Discord));
        assert_eq!(Platform::from_str_opt("telegram"), Some(Platform::Telegram));
        assert_eq!(Platform::from_str_opt("slack"), Some(Platform::Slack));
        assert_eq!(Platform::from_str_opt("unknown"), None);
        assert_eq!(format!("{}", Platform::Discord), "discord");
    }

    #[test]
    fn test_session_key_new() {
        let key = SessionKey::new(Platform::Discord, "guild1", "thread1");
        assert_eq!(key.platform, Platform::Discord);
        assert_eq!(key.namespace, Some("guild1".into()));
        assert_eq!(key.channel_id, "thread1");
    }

    #[test]
    fn test_session_key_dm() {
        let key = SessionKey::dm(Platform::Telegram, "user42");
        assert_eq!(key.platform, Platform::Telegram);
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user42");
    }

    #[test]
    fn test_session_key_direct() {
        let key = SessionKey::direct(Platform::Discord, "user42");
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user42");
        assert_eq!(key, SessionKey::dm(Platform::Discord, "user42"));
    }

    #[test]
    fn test_to_stable_id_namespaced() {
        let key = SessionKey::new(Platform::Discord, "g", "t");
        assert_eq!(key.to_stable_id(), "discord:ns:g:t");
    }

    #[test]
    fn test_to_stable_id_direct() {
        let key = SessionKey::dm(Platform::Telegram, "u");
        assert_eq!(key.to_stable_id(), "telegram:direct:u");
    }

    #[test]
    fn test_from_stable_id_new_format_direct() {
        let key = SessionKey::from_stable_id("telegram:direct:user1");
        assert_eq!(key.platform, Platform::Telegram);
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user1");
    }

    #[test]
    fn test_from_stable_id_new_format_namespaced() {
        let key = SessionKey::from_stable_id("slack:ns:workspace1:channel1");
        assert_eq!(key.platform, Platform::Slack);
        assert_eq!(key.namespace, Some("workspace1".into()));
        assert_eq!(key.channel_id, "channel1");
    }

    #[test]
    fn test_from_stable_id_legacy_direct() {
        // Legacy format without platform prefix → defaults to Discord
        let key = SessionKey::from_stable_id("direct:user1");
        assert_eq!(key.platform, Platform::Discord);
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user1");
    }

    #[test]
    fn test_from_stable_id_legacy_namespaced() {
        // Legacy format: `ns:guild1:thread1` (no platform) → Discord
        let key = SessionKey::from_stable_id("ns:guild1:thread1");
        assert_eq!(key.platform, Platform::Discord);
        assert_eq!(key.namespace, Some("guild1".into()));
        assert_eq!(key.channel_id, "thread1");
    }

    #[test]
    fn test_from_stable_id_legacy_dm_prefix() {
        let key = SessionKey::from_stable_id("dm:user123");
        assert_eq!(key.platform, Platform::Discord);
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "user123");
    }

    #[test]
    fn test_from_stable_id_legacy_bare_namespaced() {
        // Legacy: `guild1:thread1` (no prefix at all)
        let key = SessionKey::from_stable_id("guild1:thread1");
        assert_eq!(key.platform, Platform::Discord);
        assert_eq!(key.namespace, Some("guild1".into()));
        assert_eq!(key.channel_id, "thread1");
    }

    #[test]
    fn test_from_stable_id_bare() {
        let key = SessionKey::from_stable_id("barevalue");
        assert_eq!(key.platform, Platform::Discord);
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "barevalue");
    }

    #[test]
    fn test_roundtrip_all_platforms() {
        for platform in [Platform::Discord, Platform::Telegram, Platform::Slack] {
            let ns_key = SessionKey::new(platform, "ns1", "ch1");
            assert_eq!(SessionKey::from_stable_id(&ns_key.to_stable_id()), ns_key);

            let dm_key = SessionKey::dm(platform, "u1");
            assert_eq!(SessionKey::from_stable_id(&dm_key.to_stable_id()), dm_key);
        }
    }

    #[test]
    fn test_roundtrip_namespaced_direct_namespace() {
        let key = SessionKey::new(Platform::Discord, "direct", "ch1");
        assert_eq!(key.to_stable_id(), "discord:ns:direct:ch1");
        assert_eq!(SessionKey::from_stable_id(&key.to_stable_id()), key);
    }

    #[test]
    fn test_session_key_display() {
        let guild_key = SessionKey::new(Platform::Discord, "g1", "t2");
        assert_eq!(format!("{}", guild_key), "discord:ns:g1:t2");

        let dm_key = SessionKey::dm(Platform::Telegram, "u3");
        assert_eq!(format!("{}", dm_key), "telegram:direct:u3");
    }

    #[test]
    fn test_sanitize_name_alphanumeric() {
        assert_eq!(sanitize_name("hello123"), "hello123");
    }

    #[test]
    fn test_sanitize_name_special_chars() {
        assert_eq!(sanitize_name("foo/bar..baz"), "foo_bar__baz");
    }

    #[test]
    fn test_sanitize_name_preserves_dash_underscore() {
        assert_eq!(sanitize_name("my-profile_v2"), "my-profile_v2");
    }

    #[test]
    fn test_sanitize_name_path_traversal() {
        assert_eq!(sanitize_name("../../etc/passwd"), "______etc_passwd");
    }

    #[test]
    fn test_sanitize_name_empty() {
        assert_eq!(sanitize_name(""), "");
    }
}
