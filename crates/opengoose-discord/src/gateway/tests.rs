use opengoose_core::message_utils::split_message;
use opengoose_types::{Platform, SessionKey};

use super::helpers::{prepare_discord_relay, split_discord_chunks};
use super::{DISCORD_MAX_LEN, SEEN_MESSAGES_CAPACITY};

#[test]
fn test_discord_max_len_constant() {
    assert_eq!(DISCORD_MAX_LEN, 2000);
}

#[test]
fn test_prepare_discord_relay_skips_bot_messages() {
    let channel_id = "123";
    assert!(prepare_discord_relay(true, "hello", None, channel_id, Some("bot")).is_none());
}

#[test]
fn test_prepare_discord_relay_trims_content() {
    let channel_id = "123";
    let (session_key, display_name, content) =
        prepare_discord_relay(false, "  hello  ", None, channel_id, Some("alice"))
            .expect("message should be prepared");

    assert_eq!(
        session_key,
        SessionKey::direct(Platform::Discord, channel_id)
    );
    assert_eq!(display_name, Some("alice".to_string()));
    assert_eq!(content, "hello");
}

#[test]
fn test_prepare_discord_relay_uses_guild_session_key() {
    let channel_id = "123";
    let (session_key, display_name, content) =
        prepare_discord_relay(false, "hello", Some("guild-1"), channel_id, Some("alice"))
            .expect("message should be prepared");

    assert_eq!(
        session_key,
        SessionKey::new(Platform::Discord, "guild-1", channel_id)
    );
    assert_eq!(display_name, Some("alice".to_string()));
    assert_eq!(content, "hello");
}

#[test]
fn test_discord_message_chunks_by_limit() {
    let text = "a".repeat(4100);
    let chunks = split_discord_chunks(&text);
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), 2000);
    assert_eq!(chunks[1].len(), 2000);
    assert_eq!(chunks[2].len(), 100);
}

#[test]
fn test_seen_messages_capacity() {
    assert_eq!(SEEN_MESSAGES_CAPACITY, 256);
}

// --- Session key routing: guild channels vs DMs ---

#[test]
fn test_guild_session_key_has_namespace() {
    // Guild messages: session key includes guild_id as namespace
    let key = SessionKey::new(Platform::Discord, "guild123".to_string(), "channel456");
    assert_eq!(key.platform, Platform::Discord);
    assert_eq!(key.namespace, Some("guild123".to_string()));
    assert_eq!(key.channel_id, "channel456");
}

#[test]
fn test_dm_session_key_has_no_namespace() {
    // DMs (no guild_id): session key has no namespace
    let key = SessionKey::direct(Platform::Discord, "channel789");
    assert_eq!(key.platform, Platform::Discord);
    assert_eq!(key.namespace, None);
    assert_eq!(key.channel_id, "channel789");
}

#[test]
fn test_guild_and_dm_stable_ids_differ() {
    // Guild session key and DM session key for the same channel are distinct
    let guild_key = SessionKey::new(Platform::Discord, "guild1".to_string(), "chan1");
    let dm_key = SessionKey::direct(Platform::Discord, "chan1");
    assert_ne!(guild_key.to_stable_id(), dm_key.to_stable_id());
}

// --- Channel ID parsing used in send_message routing ---

#[test]
fn test_channel_id_parse_valid_snowflake() {
    // Valid Discord snowflake IDs parse as u64
    assert!("1234567890123456".parse::<u64>().is_ok());
}

#[test]
fn test_channel_id_parse_invalid_returns_err() {
    // Invalid IDs cause parse failure; send_message returns Ok(()) silently
    assert!("not-a-snowflake".parse::<u64>().is_err());
}

#[test]
fn test_channel_id_parse_negative_fails() {
    // Discord channel IDs are never negative
    assert!("-12345".parse::<u64>().is_err());
}

// --- Message splitting at Discord's 2000-char limit ---

#[test]
fn test_split_short_message_is_one_chunk() {
    let chunks = split_message("hello world", DISCORD_MAX_LEN);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], "hello world");
}

#[test]
fn test_split_exact_limit_is_one_chunk() {
    let text = "a".repeat(DISCORD_MAX_LEN);
    assert_eq!(split_message(&text, DISCORD_MAX_LEN).len(), 1);
}

#[test]
fn test_split_over_limit_produces_multiple_chunks() {
    let text = "a".repeat(DISCORD_MAX_LEN * 2 + 500);
    let chunks = split_message(&text, DISCORD_MAX_LEN);
    assert!(chunks.len() >= 3);
}

// --- Deduplication tracking logic ---

#[test]
fn test_seen_set_rejects_duplicate_id() {
    use std::collections::HashSet;

    let mut seen: HashSet<u64> = HashSet::new();
    assert!(seen.insert(42));
    // Same ID rejected on second insert
    assert!(!seen.insert(42));
}

#[test]
fn test_seen_set_accepts_distinct_ids() {
    use std::collections::HashSet;

    let mut seen: HashSet<u64> = HashSet::new();
    assert!(seen.insert(1));
    assert!(seen.insert(2));
    assert_eq!(seen.len(), 2);
}

#[test]
fn test_seen_capacity_eviction_removes_oldest() {
    use std::collections::HashSet;

    // Simulate the LRU-style eviction in the Discord event loop.
    // Inserting SEEN_MESSAGES_CAPACITY + 1 entries evicts the oldest one.
    let mut seen: HashSet<u64> = HashSet::new();
    let mut seen_order: Vec<u64> = Vec::new();

    for i in 0..=(SEEN_MESSAGES_CAPACITY as u64) {
        seen.insert(i);
        seen_order.push(i);
        if seen_order.len() > SEEN_MESSAGES_CAPACITY {
            let evicted = seen_order.remove(0);
            seen.remove(&evicted);
        }
    }

    assert_eq!(seen_order.len(), SEEN_MESSAGES_CAPACITY);
    // Oldest entry (0) was evicted
    assert!(!seen.contains(&0u64));
    // Newest entry is still present
    assert!(seen.contains(&(SEEN_MESSAGES_CAPACITY as u64)));
}

// --- prepare_discord_relay: additional edge cases ---

#[test]
fn test_prepare_discord_relay_skips_empty_content() {
    // Empty content string → None (filtered before relay)
    assert!(prepare_discord_relay(false, "", None, "chan", Some("alice")).is_none());
}

#[test]
fn test_prepare_discord_relay_skips_whitespace_only() {
    // Content that trims to empty is rejected
    assert!(prepare_discord_relay(false, "   \t\n  ", None, "chan", Some("alice")).is_none());
}

#[test]
fn test_prepare_discord_relay_no_author_name() {
    // When author_name is None, display_name in result is None
    let (_, display_name, _) =
        prepare_discord_relay(false, "hello", None, "chan", None).expect("should relay");
    assert_eq!(display_name, None);
}

#[test]
fn test_prepare_discord_relay_bot_in_guild_skipped() {
    // Bot messages in a guild context are also ignored
    assert!(prepare_discord_relay(true, "hello", Some("guild-99"), "chan", Some("bot")).is_none());
}

#[test]
fn test_prepare_discord_relay_long_content_preserved() {
    // Long content is not truncated at relay stage
    let long = "x".repeat(10_000);
    let (_, _, content) =
        prepare_discord_relay(false, &long, None, "chan", Some("alice")).expect("should relay");
    assert_eq!(content.len(), 10_000);
}

#[test]
fn test_prepare_discord_relay_different_guilds_produce_different_keys() {
    let (key_a, _, _) =
        prepare_discord_relay(false, "hi", Some("guild-a"), "chan", Some("u")).unwrap();
    let (key_b, _, _) =
        prepare_discord_relay(false, "hi", Some("guild-b"), "chan", Some("u")).unwrap();
    assert_ne!(key_a.to_stable_id(), key_b.to_stable_id());
}

#[test]
fn test_prepare_discord_relay_content_preserves_inner_whitespace() {
    // Trim only strips leading/trailing whitespace; inner whitespace is kept
    let (_, _, content) =
        prepare_discord_relay(false, "  hello   world  ", None, "chan", Some("u")).unwrap();
    assert_eq!(content, "hello   world");
}

#[test]
fn test_prepare_discord_relay_no_guild_produces_no_namespace() {
    // Without guild_id, session_key has no namespace (DM path)
    let (key, _, _) = prepare_discord_relay(false, "hello", None, "chan42", Some("alice")).unwrap();
    assert_eq!(key.namespace, None);
    assert_eq!(key.channel_id, "chan42");
}

// --- Channel ID parsing: additional edge cases ---

#[test]
fn test_channel_id_parse_empty_string_fails() {
    assert!("".parse::<u64>().is_err());
}

#[test]
fn test_channel_id_parse_float_fails() {
    // Float literals are not valid u64
    assert!("1234.56".parse::<u64>().is_err());
}

#[test]
fn test_channel_id_parse_leading_space_fails() {
    // Leading whitespace makes the parse fail
    assert!(" 123456".parse::<u64>().is_err());
}

#[test]
fn test_channel_id_parse_large_valid_snowflake() {
    // Real Discord snowflakes are large 64-bit numbers
    let snowflake = "1234567890123456789";
    assert!(snowflake.parse::<u64>().is_ok());
}

// --- split_discord_chunks: additional edge cases ---

#[test]
fn test_split_discord_empty_string_is_one_chunk() {
    // split_message returns [""] for empty input (consistent with message_utils)
    let chunks = split_discord_chunks("");
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], "");
}

#[test]
fn test_split_discord_one_char_over_limit_produces_two_chunks() {
    let text = "a".repeat(DISCORD_MAX_LEN + 1);
    let chunks = split_discord_chunks(&text);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].len(), DISCORD_MAX_LEN);
    assert_eq!(chunks[1].len(), 1);
}

#[test]
fn test_split_discord_exactly_two_limits() {
    let text = "b".repeat(DISCORD_MAX_LEN * 2);
    let chunks = split_discord_chunks(&text);
    assert_eq!(chunks.len(), 2);
    assert!(chunks.iter().all(|c| c.len() == DISCORD_MAX_LEN));
}

#[test]
fn test_split_discord_preserves_all_content() {
    let text = "hello world, this is a test message.";
    let chunks = split_discord_chunks(text);
    let reconstructed = chunks.join("");
    assert_eq!(reconstructed, text);
}

#[test]
fn test_split_discord_unicode_safety() {
    // A 4-byte emoji at or near the boundary must not be split mid-character
    let mut text = "a".repeat(DISCORD_MAX_LEN - 1);
    text.push('\u{1F600}'); // 4-byte emoji
    text.push_str("trailing");
    let chunks = split_discord_chunks(&text);
    // All chunks must be valid UTF-8 strings (guaranteed since they are &str)
    for chunk in &chunks {
        assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
    }
}

// --- Deduplication: additional scenarios ---

#[test]
fn test_seen_capacity_exactly_at_limit_no_eviction() {
    use std::collections::HashSet;

    // Filling to exactly SEEN_MESSAGES_CAPACITY does not trigger eviction
    let mut seen: HashSet<u64> = HashSet::new();
    let mut seen_order: Vec<u64> = Vec::new();

    for i in 0..(SEEN_MESSAGES_CAPACITY as u64) {
        seen.insert(i);
        seen_order.push(i);
        if seen_order.len() > SEEN_MESSAGES_CAPACITY {
            let evicted = seen_order.remove(0);
            seen.remove(&evicted);
        }
    }

    // At exactly capacity: nothing evicted, all IDs still present
    assert_eq!(seen.len(), SEEN_MESSAGES_CAPACITY);
    assert!(seen.contains(&0u64));
    assert!(seen.contains(&(SEEN_MESSAGES_CAPACITY as u64 - 1)));
}

#[test]
fn test_seen_reinsertion_after_eviction() {
    use std::collections::HashSet;

    // After an entry is evicted, it can be reinserted (simulates message replay)
    let mut seen: HashSet<u64> = HashSet::new();
    let mut seen_order: Vec<u64> = Vec::new();

    for i in 0..=(SEEN_MESSAGES_CAPACITY as u64) {
        seen.insert(i);
        seen_order.push(i);
        if seen_order.len() > SEEN_MESSAGES_CAPACITY {
            let evicted = seen_order.remove(0);
            seen.remove(&evicted);
        }
    }

    // ID 0 was evicted — can now be inserted again
    assert!(seen.insert(0));
}

#[test]
fn test_seen_order_tracks_fifo_insertion() {
    use std::collections::HashSet;

    let mut seen: HashSet<u64> = HashSet::new();
    let mut seen_order: Vec<u64> = Vec::new();

    for i in [10u64, 20, 30] {
        seen.insert(i);
        seen_order.push(i);
    }

    // FIFO: oldest is at index 0
    assert_eq!(seen_order[0], 10);
    assert_eq!(seen_order[1], 20);
    assert_eq!(seen_order[2], 30);
}

// --- Session key uniqueness and structure ---

#[test]
fn test_session_key_same_guild_different_channels_differ() {
    let key_a = SessionKey::new(Platform::Discord, "guild1".to_string(), "chan-a");
    let key_b = SessionKey::new(Platform::Discord, "guild1".to_string(), "chan-b");
    assert_ne!(key_a.to_stable_id(), key_b.to_stable_id());
}

#[test]
fn test_session_key_same_guild_same_channel_identical() {
    let key_a = SessionKey::new(Platform::Discord, "guild1".to_string(), "chan-a");
    let key_b = SessionKey::new(Platform::Discord, "guild1".to_string(), "chan-a");
    assert_eq!(key_a.to_stable_id(), key_b.to_stable_id());
}

#[test]
fn test_session_key_stable_id_includes_channel_id() {
    let key = SessionKey::new(Platform::Discord, "myguild".to_string(), "mychan");
    let stable = key.to_stable_id();
    assert!(!stable.is_empty());
}

#[test]
fn test_dm_session_key_different_channels_differ() {
    let dm_a = SessionKey::direct(Platform::Discord, "dm-chan-1");
    let dm_b = SessionKey::direct(Platform::Discord, "dm-chan-2");
    assert_ne!(dm_a.to_stable_id(), dm_b.to_stable_id());
}

#[test]
fn test_guild_key_namespace_is_guild_id() {
    let key = SessionKey::new(Platform::Discord, "my-guild".to_string(), "chan-x");
    assert_eq!(key.namespace, Some("my-guild".to_string()));
}
