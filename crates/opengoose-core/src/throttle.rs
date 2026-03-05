use std::time::{Duration, Instant};

/// Rate-limits streaming updates to respect platform API constraints.
///
/// Each platform has different edit rate limits:
/// - Discord: ~5 edits per 5 seconds per channel
/// - Slack: `chat.update` is Tier 3 (~50 req/min)
/// - Telegram: `editMessageText` ~30/sec global but bursty edits get throttled
pub struct ThrottlePolicy {
    min_interval: Duration,
    min_delta_bytes: usize,
    last_update: Option<Instant>,
    last_sent_len: usize,
}

impl ThrottlePolicy {
    /// Discord: ~1 edit/second, minimum 50 bytes of new content.
    pub fn discord() -> Self {
        Self {
            min_interval: Duration::from_secs(1),
            min_delta_bytes: 50,
            last_update: None,
            last_sent_len: 0,
        }
    }

    /// Slack: ~1.2 second intervals, minimum 80 bytes of new content.
    pub fn slack() -> Self {
        Self {
            min_interval: Duration::from_millis(1200),
            min_delta_bytes: 80,
            last_update: None,
            last_sent_len: 0,
        }
    }

    /// Telegram: ~1 second intervals, minimum 50 bytes of new content.
    pub fn telegram() -> Self {
        Self {
            min_interval: Duration::from_secs(1),
            min_delta_bytes: 50,
            last_update: None,
            last_sent_len: 0,
        }
    }

    /// Returns `true` if enough time has passed and enough new content
    /// has accumulated to warrant sending an update.
    pub fn should_update(&self, current_len: usize) -> bool {
        let enough_delta = current_len.saturating_sub(self.last_sent_len) >= self.min_delta_bytes;
        let enough_time = self
            .last_update
            .map(|t| t.elapsed() >= self.min_interval)
            .unwrap_or(true);
        enough_delta && enough_time
    }

    /// Record that an update was just sent.
    pub fn record_update(&mut self, sent_len: usize) {
        self.last_update = Some(Instant::now());
        self.last_sent_len = sent_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_update_allowed() {
        let policy = ThrottlePolicy::discord();
        assert!(policy.should_update(100));
    }

    #[test]
    fn test_insufficient_delta() {
        let mut policy = ThrottlePolicy::discord();
        policy.record_update(100);
        // Force enough time
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // Only 10 bytes of new content — not enough
        assert!(!policy.should_update(110));
    }

    #[test]
    fn test_sufficient_delta_and_time() {
        let mut policy = ThrottlePolicy::discord();
        policy.record_update(100);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        assert!(policy.should_update(200));
    }

    #[test]
    fn test_too_soon() {
        let mut policy = ThrottlePolicy::discord();
        policy.record_update(100);
        // Just recorded — too soon
        assert!(!policy.should_update(200));
    }
}
