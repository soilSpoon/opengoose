//! Matrix Client-Server API types for the sync loop and message sending.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// /account/whoami
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WhoAmI {
    pub user_id: String,
}

// ---------------------------------------------------------------------------
// /sync response (heavily pruned — only what we need)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct SyncResponse {
    pub next_batch: String,
    pub rooms: Option<SyncRooms>,
}

#[derive(Deserialize, Default)]
pub struct SyncRooms {
    pub join: Option<HashMap<String, JoinedRoom>>,
}

#[derive(Deserialize)]
pub struct JoinedRoom {
    pub timeline: Option<Timeline>,
}

#[derive(Deserialize)]
pub struct Timeline {
    pub events: Option<Vec<RoomEvent>>,
}

#[derive(Deserialize)]
pub struct RoomEvent {
    /// Kept for future deduplication / threading support.
    #[allow(dead_code)]
    pub event_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub sender: String,
    pub content: serde_json::Value,
}

// ---------------------------------------------------------------------------
// /rooms/{roomId}/send response
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SendEventResponse {
    pub event_id: String,
}

// ---------------------------------------------------------------------------
// Message content builders
// ---------------------------------------------------------------------------

/// Build a plain-text `m.room.message` content object.
pub fn text_content(body: &str) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "m.text",
        "body": body,
    })
}

/// Build an edited `m.room.message` that replaces an earlier event.
///
/// Editors use the `m.replace` relationship as defined in the Matrix spec.
pub fn edit_content(original_event_id: &str, new_body: &str) -> serde_json::Value {
    serde_json::json!({
        "msgtype": "m.text",
        "body": format!("* {new_body}"),
        "m.new_content": {
            "msgtype": "m.text",
            "body": new_body,
        },
        "m.relates_to": {
            "rel_type": "m.replace",
            "event_id": original_event_id,
        },
    })
}

// ---------------------------------------------------------------------------
// Matrix error response (used for logging)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
pub struct MatrixError {
    pub errcode: Option<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Filter — request body to limit /sync traffic
// ---------------------------------------------------------------------------

/// A minimal event filter that keeps only `m.room.message` events and
/// strips presence/receipts/account-data that we don't need.
#[derive(Serialize)]
pub struct SyncFilter {
    pub event_fields: Vec<String>,
    pub room: RoomFilter,
    pub presence: EventFilter,
    pub account_data: EventFilter,
}

#[derive(Serialize)]
pub struct RoomFilter {
    pub timeline: RoomEventFilter,
    pub state: RoomEventFilter,
    pub ephemeral: EventFilter,
    pub account_data: EventFilter,
}

#[derive(Serialize)]
pub struct RoomEventFilter {
    pub types: Vec<String>,
    pub limit: u32,
}

#[derive(Serialize)]
pub struct EventFilter {
    pub not_types: Vec<String>,
}

impl SyncFilter {
    pub fn messages_only() -> Self {
        Self {
            event_fields: vec![
                "event_id".into(),
                "type".into(),
                "sender".into(),
                "content".into(),
            ],
            room: RoomFilter {
                timeline: RoomEventFilter {
                    types: vec!["m.room.message".into()],
                    limit: 50,
                },
                state: RoomEventFilter {
                    types: vec![],
                    limit: 0,
                },
                ephemeral: EventFilter {
                    not_types: vec!["*".into()],
                },
                account_data: EventFilter {
                    not_types: vec!["*".into()],
                },
            },
            presence: EventFilter {
                not_types: vec!["*".into()],
            },
            account_data: EventFilter {
                not_types: vec!["*".into()],
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_whoami() {
        let json = r#"{"user_id":"@bot:example.com"}"#;
        let w: WhoAmI = serde_json::from_str(json).unwrap();
        assert_eq!(w.user_id, "@bot:example.com");
    }

    #[test]
    fn test_deserialize_sync_response_empty() {
        let json = r#"{"next_batch":"s1"}"#;
        let s: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(s.next_batch, "s1");
        assert!(s.rooms.is_none());
    }

    #[test]
    fn test_deserialize_sync_response_with_message() {
        let json = r#"{
            "next_batch":"s2",
            "rooms":{
                "join":{
                    "!room:example.com":{
                        "timeline":{
                            "events":[{
                                "event_id":"$ev1",
                                "type":"m.room.message",
                                "sender":"@alice:example.com",
                                "content":{"msgtype":"m.text","body":"hello"}
                            }]
                        }
                    }
                }
            }
        }"#;
        let s: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(s.next_batch, "s2");
        let rooms = s.rooms.unwrap();
        let joined = rooms.join.unwrap();
        let room = joined.get("!room:example.com").unwrap();
        let events = room.timeline.as_ref().unwrap().events.as_ref().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sender, "@alice:example.com");
        assert_eq!(events[0].content["body"], "hello");
    }

    #[test]
    fn test_text_content() {
        let c = text_content("hello");
        assert_eq!(c["msgtype"], "m.text");
        assert_eq!(c["body"], "hello");
    }

    #[test]
    fn test_edit_content() {
        let c = edit_content("$original", "edited");
        assert_eq!(c["msgtype"], "m.text");
        assert_eq!(c["body"], "* edited");
        assert_eq!(c["m.new_content"]["body"], "edited");
        assert_eq!(c["m.relates_to"]["rel_type"], "m.replace");
        assert_eq!(c["m.relates_to"]["event_id"], "$original");
    }

    #[test]
    fn test_send_event_response() {
        let json = r#"{"event_id":"$ev42"}"#;
        let r: SendEventResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.event_id, "$ev42");
    }

    #[test]
    fn test_sync_filter_serializes() {
        let f = SyncFilter::messages_only();
        let json = serde_json::to_string(&f).unwrap();
        // Check the key fields are present
        assert!(json.contains("m.room.message"));
        assert!(json.contains("event_fields"));
    }
}
