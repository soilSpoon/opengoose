mod handshake;
mod lifecycle;
mod relay;

use crate::remote::registry::RemoteConfig;

fn test_config() -> RemoteConfig {
    RemoteConfig {
        heartbeat_interval_secs: 5,
        heartbeat_timeout_secs: 15,
        api_keys: vec!["test-key-123".to_string()],
        replay_buffer_capacity: 8,
    }
}
