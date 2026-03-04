mod events;

pub use events::{AppEvent, AppEventKind, EventBus};

/// Session key based on Discord thread ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub guild_id: Option<String>,
    pub thread_id: String,
}

impl SessionKey {
    pub fn new(guild_id: impl Into<String>, thread_id: impl Into<String>) -> Self {
        Self {
            guild_id: Some(guild_id.into()),
            thread_id: thread_id.into(),
        }
    }

    pub fn dm(user_id: impl Into<String>) -> Self {
        Self {
            guild_id: None,
            thread_id: user_id.into(),
        }
    }

    /// Encode as Goose PlatformUser.user_id
    pub fn to_platform_user_id(&self) -> String {
        match &self.guild_id {
            Some(gid) => format!("{}:{}", gid, self.thread_id),
            None => format!("dm:{}", self.thread_id),
        }
    }

    /// Decode from a Goose PlatformUser.user_id string.
    ///
    /// Accepts the formats produced by [`to_platform_user_id`](Self::to_platform_user_id):
    /// - `"dm:<user_id>"` -> DM session
    /// - `"<guild_id>:<thread_id>"` -> guild session
    /// - bare id (no colon) -> treated as DM
    pub fn from_platform_user_id(id: &str) -> Self {
        if let Some(rest) = id.strip_prefix("dm:") {
            Self::dm(rest)
        } else if let Some((guild, thread)) = id.split_once(':') {
            Self::new(guild, thread)
        } else {
            Self::dm(id)
        }
    }
}

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "discord:{}", self.to_platform_user_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_key_guild_roundtrip() {
        let key = SessionKey::new("guild123", "thread456");
        let encoded = key.to_platform_user_id();
        let decoded = SessionKey::from_platform_user_id(&encoded);
        assert_eq!(key, decoded);
    }

    #[test]
    fn session_key_dm_roundtrip() {
        let key = SessionKey::dm("user789");
        let encoded = key.to_platform_user_id();
        let decoded = SessionKey::from_platform_user_id(&encoded);
        assert_eq!(key, decoded);
    }

    #[test]
    fn session_key_bare_id_fallback() {
        let decoded = SessionKey::from_platform_user_id("bare_id_no_colon");
        assert_eq!(decoded, SessionKey::dm("bare_id_no_colon"));
        assert_eq!(decoded.guild_id, None);
        assert_eq!(decoded.thread_id, "bare_id_no_colon");
    }

    #[test]
    fn session_key_display() {
        let guild_key = SessionKey::new("g1", "t2");
        assert_eq!(format!("{}", guild_key), "discord:g1:t2");

        let dm_key = SessionKey::dm("u3");
        assert_eq!(format!("{}", dm_key), "discord:dm:u3");
    }
}
