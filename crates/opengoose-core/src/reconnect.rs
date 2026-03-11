use std::time::Duration;

/// Shared exponential reconnect backoff for channel gateways.
///
/// The delay grows as `2^attempt` seconds and caps the exponent at 5,
/// producing `1, 2, 4, 8, 16, 32, 32, ...` seconds. Returning `None`
/// signals that the caller has exhausted its reconnect budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExponentialBackoff {
    max_attempts: u32,
    exponent_cap: u32,
}

impl ExponentialBackoff {
    pub const fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            exponent_cap: 5,
        }
    }

    pub fn delay_for_attempt(&self, attempt: u32) -> Option<Duration> {
        if attempt >= self.max_attempts {
            None
        } else {
            Some(Duration::from_secs(
                2u64.pow(attempt.min(self.exponent_cap)),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExponentialBackoff;
    use std::time::Duration;

    #[test]
    fn attempt_zero_starts_at_one_second() {
        let backoff = ExponentialBackoff::new(10);
        assert_eq!(backoff.delay_for_attempt(0), Some(Duration::from_secs(1)));
    }

    #[test]
    fn delay_caps_at_thirty_two_seconds() {
        let backoff = ExponentialBackoff::new(10);
        let delays: Vec<u64> = (1..9)
            .map(|attempt| backoff.delay_for_attempt(attempt).unwrap().as_secs())
            .collect();
        assert_eq!(delays, vec![2, 4, 8, 16, 32, 32, 32, 32]);
    }

    #[test]
    fn returns_none_after_max_attempts() {
        let backoff = ExponentialBackoff::new(10);
        assert_eq!(backoff.delay_for_attempt(10), None);
    }
}
