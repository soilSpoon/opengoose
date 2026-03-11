use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Per-platform connection metrics tracked by channel adapters.
#[derive(Debug, Clone, Default)]
struct ChannelMetrics {
    /// When the channel last successfully connected.
    connected_at: Option<Instant>,
    /// Total number of reconnect attempts since startup.
    reconnect_count: u32,
    /// The last error message from a failed connection attempt.
    last_error: Option<String>,
}

/// A point-in-time snapshot of connection metrics for a single platform.
///
/// Unlike `ChannelMetrics` (which uses `Instant`), this type is fully
/// serialisable and suitable for JSON API responses.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelMetricsSnapshot {
    /// Seconds since the channel last connected, or `None` if never connected.
    pub uptime_secs: Option<u64>,
    /// Total reconnect attempts since startup.
    pub reconnect_count: u32,
    /// Last error message, or `None` if no errors have occurred.
    pub last_error: Option<String>,
}

/// Thread-safe store for per-platform channel connection metrics.
///
/// Passed as `Arc` to both channel adapters (writers) and the web API
/// handler (reader), providing a lightweight shared-memory bridge between
/// the gateway runtime and the HTTP dashboard.
#[derive(Clone, Debug, Default)]
pub struct ChannelMetricsStore(Arc<RwLock<HashMap<String, ChannelMetrics>>>);

impl ChannelMetricsStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful connection for `platform`.
    /// Resets `last_error` and captures the connection timestamp.
    pub fn set_connected(&self, platform: &str) {
        if let Ok(mut map) = self.0.write() {
            let entry = map.entry(platform.to_string()).or_default();
            entry.connected_at = Some(Instant::now());
            entry.last_error = None;
        }
    }

    /// Record a reconnect attempt for `platform`, optionally with the
    /// triggering error message.
    pub fn record_reconnect(&self, platform: &str, error: Option<String>) {
        if let Ok(mut map) = self.0.write() {
            let entry = map.entry(platform.to_string()).or_default();
            entry.reconnect_count += 1;
            if let Some(e) = error {
                entry.last_error = Some(e);
            }
        }
    }

    /// Return a serialisable snapshot of all tracked platforms.
    pub fn snapshot(&self) -> HashMap<String, ChannelMetricsSnapshot> {
        match self.0.read() {
            Ok(map) => map
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        ChannelMetricsSnapshot {
                            uptime_secs: v.connected_at.map(|t| t.elapsed().as_secs()),
                            reconnect_count: v.reconnect_count,
                            last_error: v.last_error.clone(),
                        },
                    )
                })
                .collect(),
            Err(_) => HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_store_is_empty() {
        let store = ChannelMetricsStore::new();
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn test_set_connected_records_platform() {
        let store = ChannelMetricsStore::new();
        store.set_connected("discord");
        let snap = store.snapshot();
        assert!(snap.contains_key("discord"));
        let m = &snap["discord"];
        assert!(m.uptime_secs.is_some());
        assert_eq!(m.reconnect_count, 0);
        assert!(m.last_error.is_none());
    }

    #[test]
    fn test_record_reconnect_increments_count() {
        let store = ChannelMetricsStore::new();
        store.record_reconnect("slack", Some("connection refused".into()));
        store.record_reconnect("slack", None);
        let snap = store.snapshot();
        let m = &snap["slack"];
        assert_eq!(m.reconnect_count, 2);
        // Last error should still be "connection refused" (second call had None)
        assert_eq!(m.last_error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn test_reconnect_followed_by_connect_clears_error() {
        let store = ChannelMetricsStore::new();
        store.record_reconnect("matrix", Some("timeout".into()));
        assert_eq!(
            store.snapshot()["matrix"].last_error.as_deref(),
            Some("timeout")
        );
        store.set_connected("matrix");
        assert!(store.snapshot()["matrix"].last_error.is_none());
    }

    #[test]
    fn test_multiple_platforms_tracked_independently() {
        let store = ChannelMetricsStore::new();
        store.set_connected("discord");
        store.record_reconnect("slack", Some("err".into()));
        let snap = store.snapshot();
        assert!(snap["discord"].last_error.is_none());
        assert!(snap["slack"].last_error.is_some());
        assert_eq!(snap["discord"].reconnect_count, 0);
        assert_eq!(snap["slack"].reconnect_count, 1);
    }

    #[test]
    fn test_snapshot_uptime_secs_is_none_before_connect() {
        let store = ChannelMetricsStore::new();
        store.record_reconnect("telegram", None);
        let snap = store.snapshot();
        assert!(snap["telegram"].uptime_secs.is_none());
    }

    #[test]
    fn test_clone_shares_underlying_state() {
        let store = ChannelMetricsStore::new();
        let clone = store.clone();
        store.set_connected("discord");
        // The clone should see the change (both point to the same Arc)
        assert!(clone.snapshot().contains_key("discord"));
    }
}
