/// Messaging platform that a channel belongs to.
///
/// Known platforms have dedicated variants for type-safe matching.
/// The `Custom` variant allows adding new platforms without modifying
/// this crate, fulfilling the "channel addition = new crate, no core
/// changes" architectural principle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Discord,
    Telegram,
    Slack,
    /// A platform not known at compile time. The string must be a
    /// lowercase identifier (e.g. `"matrix"`, `"teams"`).
    #[serde(untagged)]
    Custom(String),
}

impl Platform {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Slack => "slack",
            Self::Custom(s) => s,
        }
    }

    /// Parse from a string. Returns the matching known variant, or
    /// `Custom` for unrecognised platforms.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "discord" => Self::Discord,
            "telegram" => Self::Telegram,
            "slack" => Self::Slack,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Parse from a string. Returns `Some` only for known platform names,
    /// `None` for anything else (including empty strings).
    ///
    /// Used by `SessionKey::from_stable_id` to distinguish known platform
    /// prefixes from legacy IDs. For accepting unknown platforms as
    /// `Custom`, use [`from_str_lossy`](Self::from_str_lossy).
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
