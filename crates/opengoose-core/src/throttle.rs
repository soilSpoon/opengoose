use std::time::{Duration, Instant};

/// Rate-limits streaming updates to respect platform API constraints.
///
/// Each platform has different edit rate limits:
/// - Discord: ~5 edits per 5 seconds per channel
/// - Slack: `chat.update` is Tier 3 (~50 req/min)
/// - Telegram: `editMessageText` ~30/sec global but bursty edits get throttled
/// - Matrix: spec recommends avoiding rapid event updates
pub struct ThrottlePolicy {
    min_interval: Duration,
    min_delta_bytes: usize,
    last_update: Option<Instant>,
    last_sent_len: usize,
}

impl ThrottlePolicy {
    /// Discord: update on every chunk. Discord rate-limits edits (~5/5s per channel);
    /// failed updates are logged and skipped — the stream continues buffering regardless.
    pub fn discord() -> Self {
        Self {
            min_interval: Duration::ZERO,
            min_delta_bytes: 0,
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

    /// Matrix: ~1.5 second intervals, minimum 60 bytes of new content.
    ///
    /// The Matrix spec recommends avoiding rapid edits; this mirrors the
    /// Slack policy with slightly looser timing.
    pub fn matrix() -> Self {
        Self {
            min_interval: Duration::from_millis(1500),
            min_delta_bytes: 60,
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
    fn test_discord_always_updates() {
        // discord() has no throttle — every chunk should trigger an update
        let mut policy = ThrottlePolicy::discord();
        assert!(policy.should_update(1));
        policy.record_update(1);
        assert!(policy.should_update(2)); // immediately after — still allowed
        policy.record_update(2);
        assert!(policy.should_update(3)); // 0 new bytes threshold
    }

    #[test]
    fn test_slack_insufficient_delta() {
        let mut policy = ThrottlePolicy::slack();
        policy.record_update(100);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // Only 10 bytes of new content — not enough for slack (min 80)
        assert!(!policy.should_update(110));
    }

    #[test]
    fn test_slack_sufficient_delta_and_time() {
        let mut policy = ThrottlePolicy::slack();
        policy.record_update(100);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        assert!(policy.should_update(200));
    }

    #[test]
    fn test_slack_too_soon() {
        let mut policy = ThrottlePolicy::slack();
        policy.record_update(100);
        // Just recorded — too soon for slack (min 1.2s)
        assert!(!policy.should_update(200));
    }

    #[test]
    fn test_telegram_policy_parameters() {
        let mut policy = ThrottlePolicy::telegram();
        // First update with enough content (min_delta_bytes=50)
        assert!(policy.should_update(50));
        policy.record_update(50);
        // Immediately after — too soon (min 1s) even with enough delta
        assert!(!policy.should_update(200));
        // Simulate time passing
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // Enough time but not enough delta (min 50 bytes, last_sent=50)
        assert!(!policy.should_update(80));
        // Enough time AND enough delta (100 - 50 = 50 >= 50)
        assert!(policy.should_update(100));
    }

    #[test]
    fn test_matrix_policy_parameters() {
        let mut policy = ThrottlePolicy::matrix();
        // First update with enough content (min_delta_bytes=60)
        assert!(policy.should_update(60));
        policy.record_update(100);
        // Immediately after — too soon (min 1.5s)
        assert!(!policy.should_update(200));
        // After enough time but not enough delta (min 60 bytes, last_sent=100)
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        assert!(!policy.should_update(140));
        // Enough time AND delta (170 - 100 = 70 >= 60)
        assert!(policy.should_update(170));
    }

    #[test]
    fn test_first_update_needs_enough_delta() {
        // Discord allows any delta (min_delta_bytes=0)
        let policy = ThrottlePolicy::discord();
        assert!(policy.should_update(0));
        assert!(policy.should_update(1));

        // Non-discord policies require min_delta_bytes even for the first update
        let policy = ThrottlePolicy::slack();
        assert!(!policy.should_update(1)); // 1 < 80
        assert!(policy.should_update(80)); // 80 >= 80

        let policy = ThrottlePolicy::telegram();
        assert!(!policy.should_update(1)); // 1 < 50
        assert!(policy.should_update(50)); // 50 >= 50

        let policy = ThrottlePolicy::matrix();
        assert!(!policy.should_update(1)); // 1 < 60
        assert!(policy.should_update(60)); // 60 >= 60
    }

    #[test]
    fn test_record_update_tracks_sent_length() {
        let mut policy = ThrottlePolicy::slack();
        assert!(policy.should_update(100));
        policy.record_update(100);
        // After time passes, delta is measured from last_sent_len (100)
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // 100 - 100 = 0 delta, not enough
        assert!(!policy.should_update(100));
        // 200 - 100 = 100 delta, enough (min 80)
        assert!(policy.should_update(200));
    }

    #[test]
    fn test_saturating_sub_handles_underflow() {
        let mut policy = ThrottlePolicy::slack();
        policy.record_update(200);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // current_len < last_sent_len — should not update (delta underflows to 0)
        assert!(!policy.should_update(100));
    }

    #[test]
    fn test_slack_exact_delta_boundary() {
        let mut policy = ThrottlePolicy::slack();
        policy.record_update(0);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // 79 bytes — one below the minimum (min_delta_bytes = 80)
        assert!(!policy.should_update(79));
        // 80 bytes — exactly at the minimum
        assert!(policy.should_update(80));
    }

    #[test]
    fn test_telegram_exact_delta_boundary() {
        let mut policy = ThrottlePolicy::telegram();
        policy.record_update(0);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // 49 bytes — one below the minimum (min_delta_bytes = 50)
        assert!(!policy.should_update(49));
        // 50 bytes — exactly at the minimum
        assert!(policy.should_update(50));
    }

    #[test]
    fn test_sequential_record_updates() {
        // Each record_update shifts the baseline; delta is always measured from the last sent len.
        let mut policy = ThrottlePolicy::slack();

        policy.record_update(100);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // delta: 200 - 100 = 100 >= 80 — should update
        assert!(policy.should_update(200));

        policy.record_update(200);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // delta: 280 - 200 = 80 >= 80 — exactly enough
        assert!(policy.should_update(280));

        policy.record_update(280);
        policy.last_update = Some(Instant::now() - Duration::from_secs(2));
        // delta: 300 - 280 = 20 < 80 — not enough
        assert!(!policy.should_update(300));
    }
}
