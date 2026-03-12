use std::time::SystemTime;

/// A single message event propagated over the bus.
#[derive(Debug, Clone)]
pub struct BusEvent {
    /// Name of the sending agent.
    pub from: String,
    /// Recipient agent name (for directed messages), or `None` for channel messages.
    pub to: Option<String>,
    /// Channel name (for pub/sub), or `None` for directed messages.
    pub channel: Option<String>,
    /// Message payload.
    pub payload: String,
    /// Wall-clock timestamp (seconds since Unix epoch).
    pub timestamp: u64,
}

impl BusEvent {
    /// Create a new directed message event.
    pub fn directed(
        from: impl Into<String>,
        to: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: Some(to.into()),
            channel: None,
            payload: payload.into(),
            timestamp: unix_now(),
        }
    }

    /// Create a new channel (pub/sub) event.
    pub fn channel(
        from: impl Into<String>,
        channel: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: None,
            channel: Some(channel.into()),
            payload: payload.into(),
            timestamp: unix_now(),
        }
    }

    /// Returns true for directed (point-to-point) events.
    pub fn is_directed(&self) -> bool {
        self.to.is_some()
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
