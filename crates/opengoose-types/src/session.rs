use crate::Platform;

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
        // Try new format: <platform>:<kind>:<rest> where kind is direct/dm/ns
        if let Some((first, rest)) = id.split_once(':') {
            // Check for known platforms first
            if let Some(platform) = Platform::from_str_opt(first) {
                return Self::parse_after_platform(platform, rest);
            }
            // Check if rest looks like new-format (starts with direct:/dm:/ns:)
            // to detect Custom platform prefixes vs legacy IDs
            if rest.starts_with("direct:") || rest.starts_with("dm:") || rest.starts_with("ns:") {
                return Self::parse_after_platform(Platform::Custom(first.to_string()), rest);
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

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_stable_id())
    }
}

impl serde::Serialize for SessionKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_stable_id())
    }
}

impl<'de> serde::Deserialize<'de> for SessionKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let stable_id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Ok(Self::from_stable_id(&stable_id))
    }
}
