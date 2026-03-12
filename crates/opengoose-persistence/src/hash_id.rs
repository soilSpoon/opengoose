//! Hash-based ID generation for Beads-style work items.
//!
//! Generates IDs like `bd-a3f8` using SHA-256 and base36 encoding.
//! Adaptive length: 4 chars for <500 items, 6 for 500-50K, 8 for >50K.

use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Characters used for base36 encoding.
const BASE36_CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Generate a hash-based ID with `bd-` prefix.
///
/// The hash is computed from `title + timestamp_nanos + nonce`.
/// Length adapts based on `item_count`.
pub fn generate_hash_id(title: &str, item_count: usize) -> String {
    let length = adaptive_length(item_count);
    generate_hash_id_with_nonce(title, 0, length)
}

/// Generate with a specific nonce (for collision retry).
pub fn generate_hash_id_with_nonce(title: &str, nonce: u64, length: usize) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(now.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    let hash = hasher.finalize();

    let encoded = base36_encode(&hash, length);
    format!("bd-{encoded}")
}

/// Determine ID length based on total item count.
fn adaptive_length(count: usize) -> usize {
    if count < 500 {
        4
    } else if count < 50_000 {
        6
    } else {
        8
    }
}

/// Encode bytes as base36 string of the given length.
fn base36_encode(bytes: &[u8], length: usize) -> String {
    let mut result = String::with_capacity(length);
    // Use chunks of the hash to generate each character
    for i in 0..length {
        let idx = if i < bytes.len() {
            bytes[i] as usize % 36
        } else {
            // Wrap around if we need more chars than hash bytes
            bytes[i % bytes.len()] as usize % 36
        };
        result.push(BASE36_CHARS[idx] as char);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_id_has_bd_prefix() {
        let id = generate_hash_id("test task", 0);
        assert!(id.starts_with("bd-"), "ID should start with 'bd-', got: {id}");
    }

    #[test]
    fn test_adaptive_length_short() {
        let id = generate_hash_id("task", 100);
        // bd- prefix + 4 chars
        assert_eq!(id.len(), 7, "Expected 7 chars for <500 items, got: {id}");
    }

    #[test]
    fn test_adaptive_length_medium() {
        let id = generate_hash_id_with_nonce("task", 0, adaptive_length(1000));
        // bd- prefix + 6 chars
        assert_eq!(id.len(), 9, "Expected 9 chars for 500-50K items");
    }

    #[test]
    fn test_adaptive_length_long() {
        let id = generate_hash_id_with_nonce("task", 0, adaptive_length(100_000));
        // bd- prefix + 8 chars
        assert_eq!(id.len(), 11, "Expected 11 chars for >50K items");
    }

    #[test]
    fn test_unique_ids() {
        let id1 = generate_hash_id("task a", 0);
        let id2 = generate_hash_id("task b", 0);
        assert_ne!(id1, id2, "Different titles should produce different IDs");
    }

    #[test]
    fn test_collision_retry_different_nonce() {
        let id1 = generate_hash_id_with_nonce("same title", 0, 4);
        let id2 = generate_hash_id_with_nonce("same title", 1, 4);
        // Different nonces should produce different IDs (same timestamp is fine
        // since nonce changes the hash input)
        assert_ne!(id1, id2, "Different nonces should produce different IDs");
    }

    #[test]
    fn test_base36_chars_only() {
        let id = generate_hash_id("test", 0);
        let suffix = &id[3..]; // strip "bd-"
        for c in suffix.chars() {
            assert!(
                c.is_ascii_lowercase() || c.is_ascii_digit(),
                "ID should only contain base36 chars, got: {c}"
            );
        }
    }

    #[test]
    fn test_adaptive_length_boundaries() {
        assert_eq!(adaptive_length(0), 4);
        assert_eq!(adaptive_length(499), 4);
        assert_eq!(adaptive_length(500), 6);
        assert_eq!(adaptive_length(49_999), 6);
        assert_eq!(adaptive_length(50_000), 8);
    }
}
