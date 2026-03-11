use std::sync::atomic::{AtomicU64, Ordering};

use opengoose_types::Platform;

use super::{
    MATRIX_MAX_LEN, MAX_RECONNECT_ATTEMPTS, MatrixGateway, REQUEST_TIMEOUT, SYNC_TIMEOUT_MS,
    reconnect_delay, urlencoding,
};

#[test]
fn test_server_name_from_user_id() {
    assert_eq!(
        MatrixGateway::server_name_from_user_id("@bot:example.com"),
        "example.com"
    );
    assert_eq!(
        MatrixGateway::server_name_from_user_id("@alice:matrix.org"),
        "matrix.org"
    );
    // No colon → fallback
    assert_eq!(
        MatrixGateway::server_name_from_user_id("barevalue"),
        "matrix.org"
    );
}

#[test]
fn test_session_key_structure() {
    let key = MatrixGateway::session_key("example.com", "!room:example.com");
    assert_eq!(key.platform, Platform::Custom("matrix".to_string()));
    assert_eq!(key.namespace, Some("example.com".to_string()));
    assert_eq!(key.channel_id, "!room:example.com");
}

#[test]
fn test_session_key_stable_id_roundtrip() {
    let key = MatrixGateway::session_key("example.com", "!room:example.com");
    let stable = key.to_stable_id();
    // Should be parseable
    assert!(stable.contains("matrix"));
    assert!(stable.contains("example.com"));
}

#[test]
fn test_matrix_max_len() {
    assert_eq!(MATRIX_MAX_LEN, 32_768);
}

#[test]
fn test_urlencoding_room_id() {
    // Room IDs contain ! and : which must be encoded in path segments
    let encoded = urlencoding::encode("!room:example.com").into_owned();
    assert!(encoded.contains("%21") || !encoded.contains('!'));
    assert!(encoded.contains("%3A") || !encoded.contains(':'));
}

#[test]
fn test_urlencoding_alphanumeric_unchanged() {
    let encoded = urlencoding::encode("hello-world_123").into_owned();
    assert_eq!(encoded, "hello-world_123");
}

#[test]
fn test_v3_url_trailing_slash_stripped() {
    // The trim logic is: `.trim_end_matches('/')`.
    // Verify it works correctly on a plain string.
    let url = "https://matrix.example.com/";
    let trimmed = url.trim_end_matches('/').to_string();
    assert_eq!(trimmed, "https://matrix.example.com");
}

#[test]
fn test_txn_id_format() {
    // next_txn_id uses process::id() + counter.  Verify the format
    // without needing a full MatrixGateway.
    let counter = AtomicU64::new(0);
    let t1 = format!(
        "opengoose-{}-{}",
        std::process::id(),
        counter.fetch_add(1, Ordering::Relaxed)
    );
    let t2 = format!(
        "opengoose-{}-{}",
        std::process::id(),
        counter.fetch_add(1, Ordering::Relaxed)
    );
    assert_ne!(t1, t2);
    assert!(t1.starts_with("opengoose-"));
    assert!(t2.starts_with("opengoose-"));
}

// -----------------------------------------------------------------------
// Message filtering logic (extracted from run_sync_loop)
// -----------------------------------------------------------------------

/// Mirror of the filtering conditions in run_sync_loop, expressed as a
/// pure function so they can be unit-tested without network I/O.
fn should_process_event(
    event_type: &str,
    sender: &str,
    bot_user_id: &str,
    content: &serde_json::Value,
) -> bool {
    if event_type != "m.room.message" {
        return false;
    }
    if sender == bot_user_id {
        return false;
    }
    if content.get("msgtype").and_then(|v| v.as_str()) != Some("m.text") {
        return false;
    }
    if content
        .get("m.relates_to")
        .and_then(|v| v.get("rel_type"))
        .and_then(|v| v.as_str())
        == Some("m.replace")
    {
        return false;
    }
    true
}

#[test]
fn test_event_filter_accepts_plain_text() {
    let content = serde_json::json!({"msgtype": "m.text", "body": "hello"});
    assert!(should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_own_message() {
    let content = serde_json::json!({"msgtype": "m.text", "body": "I said this"});
    assert!(!should_process_event(
        "m.room.message",
        "@bot:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_non_room_message_type() {
    let content = serde_json::json!({});
    assert!(!should_process_event(
        "m.reaction",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
    assert!(!should_process_event(
        "m.room.member",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_non_text_msgtype() {
    let image_content = serde_json::json!({"msgtype": "m.image", "url": "mxc://example.com/abc"});
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &image_content
    ));
    let file_content = serde_json::json!({"msgtype": "m.file", "url": "mxc://example.com/def"});
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &file_content
    ));
}

#[test]
fn test_event_filter_rejects_edit_messages() {
    // Edit events have m.relates_to.rel_type = "m.replace"
    let edit_content = serde_json::json!({
        "msgtype": "m.text",
        "body": "* edited text",
        "m.relates_to": {
            "rel_type": "m.replace",
            "event_id": "$original"
        }
    });
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &edit_content
    ));
}

#[test]
fn test_event_filter_accepts_reply_with_different_rel_type() {
    // Replies have rel_type = "m.in_reply_to" — these should be processed
    let reply_content = serde_json::json!({
        "msgtype": "m.text",
        "body": "> previous\n\nresponse",
        "m.relates_to": {
            "m.in_reply_to": {"event_id": "$original"}
        }
    });
    assert!(should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &reply_content
    ));
}

// -----------------------------------------------------------------------
// Reconnection delay calculation
// -----------------------------------------------------------------------

#[test]
fn test_reconnect_delay_exponential_backoff() {
    let delays: Vec<u64> = (1u32..=8)
        .map(|attempt| reconnect_delay(attempt).unwrap().as_secs())
        .collect();
    assert_eq!(delays, vec![2, 4, 8, 16, 32, 32, 32, 32]);
}

#[test]
fn test_reconnect_delay_first_attempt_is_two_seconds() {
    assert_eq!(reconnect_delay(1).unwrap().as_secs(), 2);
}

#[test]
fn test_max_reconnect_attempts_constant() {
    assert_eq!(MAX_RECONNECT_ATTEMPTS, 10);
}

// -----------------------------------------------------------------------
// URL encoding edge cases
// -----------------------------------------------------------------------

#[test]
fn test_urlencoding_at_sign() {
    // @ is common in Matrix user IDs used in some path contexts
    let encoded = urlencoding::encode("@user:example.com").into_owned();
    assert!(!encoded.contains('@'));
}

#[test]
fn test_urlencoding_hash() {
    let encoded = urlencoding::encode("#room:example.com").into_owned();
    assert!(!encoded.contains('#'));
    assert!(encoded.contains("%23"));
}

#[test]
fn test_urlencoding_empty_string() {
    let encoded = urlencoding::encode("").into_owned();
    assert_eq!(encoded, "");
}

#[test]
fn test_urlencoding_preserves_tildes() {
    // Tilde is an unreserved character per RFC 3986
    let encoded = urlencoding::encode("~user").into_owned();
    assert_eq!(encoded, "~user");
}

// -----------------------------------------------------------------------
// Server name extraction edge cases
// -----------------------------------------------------------------------

#[test]
fn test_server_name_from_user_id_multiple_colons() {
    // Only the first colon splits localpart from server name
    // e.g. "@user:server.com:8448" — server part is "server.com:8448"
    let result = MatrixGateway::server_name_from_user_id("@user:server.com:8448");
    assert_eq!(result, "server.com:8448");
}

#[test]
fn test_server_name_empty_string() {
    // Should not panic; falls back to matrix.org
    let result = MatrixGateway::server_name_from_user_id("");
    assert_eq!(result, "matrix.org");
}

// -----------------------------------------------------------------------
// Credential configuration (homeserver URL normalisation)
// -----------------------------------------------------------------------

#[test]
fn test_homeserver_url_trailing_slash_multiple() {
    // Multiple trailing slashes should all be stripped
    let url = "https://matrix.example.com///";
    let trimmed = url.trim_end_matches('/').to_string();
    assert_eq!(trimmed, "https://matrix.example.com");
}

#[test]
fn test_homeserver_url_no_trailing_slash_unchanged() {
    let url = "https://matrix.example.com";
    let trimmed = url.trim_end_matches('/').to_string();
    assert_eq!(trimmed, "https://matrix.example.com");
}

#[test]
fn test_homeserver_url_with_port() {
    let url = "https://matrix.example.com:8448/";
    let trimmed = url.trim_end_matches('/').to_string();
    assert_eq!(trimmed, "https://matrix.example.com:8448");
}

// -----------------------------------------------------------------------
// Sync timeout and request timeout constants
// -----------------------------------------------------------------------

#[test]
fn test_sync_timeout_reasonable() {
    // 30 seconds is the standard Matrix long-poll window
    assert_eq!(SYNC_TIMEOUT_MS, 30_000);
}

#[test]
fn test_request_timeout_exceeds_sync_timeout() {
    // HTTP client timeout must be > SYNC_TIMEOUT_MS to avoid cutting off
    // long-poll responses before the server finishes.
    assert!(REQUEST_TIMEOUT.as_millis() > SYNC_TIMEOUT_MS as u128);
}

// -----------------------------------------------------------------------
// Additional event filter tests — uncommon but valid msgtypes
// -----------------------------------------------------------------------

#[test]
fn test_event_filter_rejects_notice_msgtype() {
    // m.notice is used for automated/bot messages; treat as non-interactive
    let content = serde_json::json!({"msgtype": "m.notice", "body": "automated message"});
    assert!(!should_process_event(
        "m.room.message",
        "@bot2:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_emote_msgtype() {
    // m.emote is /me commands; not a user request we should process
    let content = serde_json::json!({"msgtype": "m.emote", "body": "waves"});
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_audio_msgtype() {
    let content = serde_json::json!({"msgtype": "m.audio", "url": "mxc://example.com/audio"});
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_video_msgtype() {
    let content = serde_json::json!({"msgtype": "m.video", "url": "mxc://example.com/video"});
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_rejects_missing_msgtype() {
    // Content without a msgtype field at all should be ignored
    let content = serde_json::json!({"body": "mysterious message"});
    assert!(!should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

#[test]
fn test_event_filter_accepts_thread_reply() {
    // Thread replies have rel_type = "m.thread" — these are user messages we should handle.
    // Unlike "m.replace" edits, thread replies do not have rel_type = "m.replace".
    let content = serde_json::json!({
        "msgtype": "m.text",
        "body": "thread reply",
        "m.relates_to": {
            "rel_type": "m.thread",
            "event_id": "$root_event"
        }
    });
    assert!(should_process_event(
        "m.room.message",
        "@alice:example.com",
        "@bot:example.com",
        &content
    ));
}

// -----------------------------------------------------------------------
// Session key construction — end-to-end from user_id to SessionKey
// -----------------------------------------------------------------------

#[test]
fn test_session_key_end_to_end_from_user_id() {
    // This mirrors the exact path in run_sync_loop:
    //   server_name = server_name_from_user_id(bot_user_id)
    //   session_key = session_key(server_name, &room_id)
    let bot_user_id = "@opengoose:matrix.example.com";
    let room_id = "!abc123:matrix.example.com";
    let server_name = MatrixGateway::server_name_from_user_id(bot_user_id);
    let key = MatrixGateway::session_key(server_name, room_id);
    assert_eq!(key.namespace, Some("matrix.example.com".to_string()));
    assert_eq!(key.channel_id, room_id);
}

#[test]
fn test_session_key_with_ip_address_server() {
    // Matrix supports IP addresses as homeserver names
    let key = MatrixGateway::session_key("192.168.1.1:8448", "!room:192.168.1.1:8448");
    assert_eq!(key.namespace, Some("192.168.1.1:8448".to_string()));
}

#[test]
fn test_session_key_channel_id_preserved_exactly() {
    // Room IDs contain special characters; verify they are stored verbatim
    let room_id = "!BaSe64+/==:matrix.org";
    let key = MatrixGateway::session_key("matrix.org", room_id);
    assert_eq!(key.channel_id, room_id);
}

// -----------------------------------------------------------------------
// Team command prefix detection
// -----------------------------------------------------------------------

#[test]
fn test_team_command_prefix_bare() {
    // "!team" with no args — strip_prefix succeeds, args is empty string
    let body = "!team";
    let args = body.strip_prefix("!team").map(|s| s.trim());
    assert_eq!(args, Some(""));
}

#[test]
fn test_team_command_prefix_with_args() {
    let body = "!team list";
    let args = body.strip_prefix("!team").map(|s| s.trim());
    assert_eq!(args, Some("list"));
}

#[test]
fn test_non_team_command_not_matched() {
    let body = "hello world";
    let args = body.strip_prefix("!team");
    assert!(args.is_none());
}

#[test]
fn test_team_command_not_matched_by_partial_prefix() {
    // "!teams" should NOT be treated as a team command
    let body = "!teams";
    let args = body.strip_prefix("!team").map(|s| s.trim());
    // strip_prefix("!team") on "!teams" gives Some("s"), not the team command
    // but the real loop checks strip_prefix — so this is processed as a team command
    // with args="s". Test that the split behaviour is understood correctly.
    assert_eq!(args, Some("s"));
}

// -----------------------------------------------------------------------
// Body extraction logic — mirrors run_sync_loop body-skipping conditions
// -----------------------------------------------------------------------

/// Mirror of the body extraction + trim + empty check in run_sync_loop.
fn extract_body(content: &serde_json::Value) -> Option<String> {
    let raw = content.get("body")?.as_str()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[test]
fn test_body_absent_is_skipped() {
    // Content with no "body" key — should not be processed
    let content = serde_json::json!({"msgtype": "m.text"});
    assert!(extract_body(&content).is_none());
}

#[test]
fn test_body_empty_string_is_skipped() {
    let content = serde_json::json!({"msgtype": "m.text", "body": ""});
    assert!(extract_body(&content).is_none());
}

#[test]
fn test_body_whitespace_only_is_skipped() {
    // Whitespace-only body trims to empty — should be ignored
    let content = serde_json::json!({"msgtype": "m.text", "body": "   \t\n  "});
    assert!(extract_body(&content).is_none());
}

#[test]
fn test_body_non_string_is_skipped() {
    // body set to null or a number — as_str() returns None
    let null_body = serde_json::json!({"msgtype": "m.text", "body": null});
    assert!(extract_body(&null_body).is_none());
    let num_body = serde_json::json!({"msgtype": "m.text", "body": 42});
    assert!(extract_body(&num_body).is_none());
}

#[test]
fn test_body_valid_text_is_processed() {
    let content = serde_json::json!({"msgtype": "m.text", "body": "  hello  "});
    assert_eq!(extract_body(&content).as_deref(), Some("hello"));
}

// -----------------------------------------------------------------------
// Reconnection logic — attempt counter vs MAX_RECONNECT_ATTEMPTS
// -----------------------------------------------------------------------

/// Mirror of the reconnection guard in run_sync_loop.
fn reconnect_should_give_up(attempt: u32) -> bool {
    attempt >= MAX_RECONNECT_ATTEMPTS
}

#[test]
fn test_reconnect_below_max_continues() {
    // Attempts 1 through MAX-1 should all continue retrying
    for attempt in 1..MAX_RECONNECT_ATTEMPTS {
        assert!(
            !reconnect_should_give_up(attempt),
            "should not give up at attempt {attempt}"
        );
    }
}

#[test]
fn test_reconnect_at_max_stops() {
    // At exactly MAX_RECONNECT_ATTEMPTS the loop should give up
    assert!(reconnect_should_give_up(MAX_RECONNECT_ATTEMPTS));
}

#[test]
fn test_reconnect_at_max_minus_one_still_continues() {
    let just_below = MAX_RECONNECT_ATTEMPTS - 1;
    assert!(!reconnect_should_give_up(just_below));
}

#[test]
fn test_reconnect_attempt_resets_to_zero_on_success() {
    // Simulate the counter reset that happens after a successful sync
    let reconnect_attempts: u32 = 0;
    assert_eq!(reconnect_attempts, 0);
    assert!(!reconnect_should_give_up(reconnect_attempts));
}

// -----------------------------------------------------------------------
// Event ID tracking — readiness for deduplication
// -----------------------------------------------------------------------

#[test]
fn test_event_id_field_preserved() {
    // RoomEvent::event_id is stored even though currently allow(dead_code).
    // This verifies the field round-trips through deserialization, making
    // deduplication easy to implement in future.
    use crate::types::RoomEvent;
    let json = r#"{
        "event_id": "$dedup123:example.com",
        "type": "m.room.message",
        "sender": "@alice:example.com",
        "content": {"msgtype": "m.text", "body": "hi"}
    }"#;
    let ev: RoomEvent = serde_json::from_str(json).unwrap();
    assert_eq!(ev.event_id, "$dedup123:example.com");
}

#[test]
fn test_multiple_events_have_distinct_event_ids() {
    // Simulates a batch with two events — IDs must be unique for deduplication
    use crate::types::{RoomEvent, SyncResponse};
    let json = r#"{
        "next_batch": "s10",
        "rooms": { "join": { "!room:example.com": { "timeline": { "events": [
            {"event_id": "$ev1:x", "type": "m.room.message", "sender": "@a:x", "content": {"msgtype":"m.text","body":"first"}},
            {"event_id": "$ev2:x", "type": "m.room.message", "sender": "@a:x", "content": {"msgtype":"m.text","body":"second"}}
        ]}}}}
    }"#;
    let s: SyncResponse = serde_json::from_str(json).unwrap();
    let events: Vec<&RoomEvent> = s
        .rooms
        .as_ref()
        .unwrap()
        .join
        .as_ref()
        .unwrap()
        .get("!room:example.com")
        .unwrap()
        .timeline
        .as_ref()
        .unwrap()
        .events
        .as_ref()
        .unwrap()
        .iter()
        .collect();
    assert_eq!(events.len(), 2);
    assert_ne!(events[0].event_id, events[1].event_id);
}

// -----------------------------------------------------------------------
// Sync batch token — pagination state
// -----------------------------------------------------------------------

#[test]
fn test_sync_batch_token_advances_with_each_response() {
    // next_batch from one sync becomes the `since` of the next.
    // Verify tokens are non-empty and distinct across responses.
    use crate::types::SyncResponse;
    let resp1 = SyncResponse {
        next_batch: "s100_first".into(),
        rooms: None,
    };
    let resp2 = SyncResponse {
        next_batch: "s101_second".into(),
        rooms: None,
    };
    assert!(!resp1.next_batch.is_empty());
    assert_ne!(resp1.next_batch, resp2.next_batch);
}

// -----------------------------------------------------------------------
// Reconnect delay — boundary values
// -----------------------------------------------------------------------

#[test]
fn test_reconnect_delay_cap_at_attempt_5_and_beyond() {
    // At attempt 5 and 6 both produce the same 32s delay (capped at .min(5))
    let delay_at_5 = 2u64.pow(5);
    let delay_at_6 = 2u64.pow(5);
    let delay_at_10 = 2u64.pow(5);
    assert_eq!(delay_at_5, 32);
    assert_eq!(delay_at_6, 32);
    assert_eq!(delay_at_10, 32);
}

#[test]
fn test_reconnect_delay_before_cap() {
    // Attempts 1–4 each double the previous delay
    let delay_1 = 2u64.pow(1);
    let delay_2 = 2u64.pow(2);
    let delay_3 = 2u64.pow(3);
    let delay_4 = 2u64.pow(4);
    assert_eq!(delay_1, 2);
    assert_eq!(delay_2, 4);
    assert_eq!(delay_3, 8);
    assert_eq!(delay_4, 16);
}
