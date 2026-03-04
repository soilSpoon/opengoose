mod events;

pub use events::{AppEvent, AppEventKind, EventBus};
use serde::{Deserialize, Serialize};

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
}

impl std::fmt::Display for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "discord:{}", self.to_platform_user_id())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub discord: DiscordConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    #[serde(default = "default_bot_token_env")]
    pub bot_token_env: String,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            bot_token_env: default_bot_token_env(),
        }
    }
}

fn default_bot_token_env() -> String {
    "DISCORD_BOT_TOKEN".into()
}
