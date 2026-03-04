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
