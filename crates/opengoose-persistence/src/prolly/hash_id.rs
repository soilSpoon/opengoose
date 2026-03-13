//! Hash-based ID generation for Beads work items.
//!
//! Algorithm: SHA-256(title + nonce), base36, `bd-` prefix.
//! Adaptive length: 4 chars (<500 items), 6 chars (500-50K), 8 chars (>50K).

use sha2::{Digest, Sha256};

const PREFIX: &str = "bd-";

/// Generate a hash ID for a work item.
pub fn generate_hash_id(title: &str, nonce: u64, total_items: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(nonce.to_le_bytes());
    let hash = hasher.finalize();

    let length = adaptive_length(total_items);
    let encoded = base36_encode(&hash, length);
    format!("{PREFIX}{encoded}")
}

fn adaptive_length(total: usize) -> usize {
    if total < 500 {
        4
    } else if total < 50_000 {
        6
    } else {
        8
    }
}

fn base36_encode(bytes: &[u8], length: usize) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut result = String::with_capacity(length);
    for i in 0..length {
        let idx = bytes[i % bytes.len()] as usize % 36;
        result.push(ALPHABET[idx] as char);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix() {
        let id = generate_hash_id("test", 0, 0);
        assert!(id.starts_with("bd-"));
    }

    #[test]
    fn test_adaptive_length_short() {
        let id = generate_hash_id("test", 0, 100);
        assert_eq!(id.len(), 3 + 4); // "bd-" + 4 chars
    }

    #[test]
    fn test_adaptive_length_medium() {
        let id = generate_hash_id("test", 0, 1000);
        assert_eq!(id.len(), 3 + 6);
    }

    #[test]
    fn test_adaptive_length_long() {
        let id = generate_hash_id("test", 0, 100_000);
        assert_eq!(id.len(), 3 + 8);
    }

    #[test]
    fn test_uniqueness() {
        let id1 = generate_hash_id("task a", 1, 0);
        let id2 = generate_hash_id("task b", 2, 0);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_deterministic() {
        let id1 = generate_hash_id("test", 42, 0);
        let id2 = generate_hash_id("test", 42, 0);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_different_nonce_different_id() {
        let id1 = generate_hash_id("test", 1, 0);
        let id2 = generate_hash_id("test", 2, 0);
        assert_ne!(id1, id2);
    }
}
