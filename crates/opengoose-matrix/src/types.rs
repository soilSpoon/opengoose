//! Matrix Client-Server API types for the sync loop and message sending.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// /account/whoami
// ---------------------------------------------------------------------------

/// Response from `GET /account/whoami`.
///
/// Returns the Matrix user ID of the authenticated user (e.g. `@bot:example.com`).
#[derive(Deserialize)]
pub struct WhoAmI {
    /// Fully-qualified Matrix user ID, e.g. `@bot:example.com`.
    pub user_id: String,
}

// ---------------------------------------------------------------------------
// /sync response (heavily pruned — only what we need)
// ---------------------------------------------------------------------------

/// Pruned response from `GET /sync`.
///
/// Contains only the fields consumed by the sync loop.
/// Unknown fields from the homeserver are silently ignored by serde.
#[derive(Deserialize, Default)]
pub struct SyncResponse {
    /// Opaque pagination token; pass back as `since=` on the next sync call.
    pub next_batch: String,
    /// Room events received in this sync window, if any.
    pub rooms: Option<SyncRooms>,
}

/// Top-level rooms object in a sync response.
///
/// The full Matrix spec also includes `invite`, `knock`, and `leave` maps,
/// but we only need `join` for message processing.
#[derive(Deserialize, Default)]
pub struct SyncRooms {
    /// Rooms the bot has joined, keyed by room ID (e.g. `!room:example.com`).
    pub join: Option<HashMap<String, JoinedRoom>>,
}

/// State and timeline for a single joined room.
#[derive(Deserialize)]
pub struct JoinedRoom {
    /// Recent timeline events; `None` when nothing changed since last sync.
    pub timeline: Option<Timeline>,
}

/// A room's timeline slice from a sync response.
#[derive(Deserialize)]
pub struct Timeline {
    /// List of room events in chronological order.
    pub events: Option<Vec<RoomEvent>>,
}

/// A single room event from the Matrix sync timeline.
#[derive(Deserialize)]
pub struct RoomEvent {
    /// Unique event ID (e.g. `$abc123:example.com`).
    /// Kept for future deduplication / threading support.
    #[allow(dead_code)]
    pub event_id: String,
    /// Event type string, e.g. `m.room.message`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Fully-qualified sender user ID, e.g. `@alice:example.com`.
    pub sender: String,
    /// Raw event content; structure varies by event type.
    pub content: serde_json::Value,
}

// ---------------------------------------------------------------------------
// /rooms/{roomId}/send response
// ---------------------------------------------------------------------------

/// Response from `PUT /rooms/{roomId}/send/{eventType}/{txnId}`.
#[derive(Deserialize)]
pub struct SendEventResponse {
    /// Server-assigned event ID for the newly created event.
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

/// Matrix error response body returned on non-2xx status codes.
///
/// See the [Matrix spec error codes](https://spec.matrix.org/v1.6/client-server-api/#standard-error-response).
#[derive(Deserialize, Debug)]
pub struct MatrixError {
    /// Machine-readable error code (e.g. `M_FORBIDDEN`, `M_UNKNOWN`).
    pub errcode: Option<String>,
    /// Human-readable error message.
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Filter — request body to limit /sync traffic
// ---------------------------------------------------------------------------

/// A minimal event filter that keeps only `m.room.message` events and
/// strips presence/receipts/account-data that we don't need.
///
/// Registered via `POST /user/{userId}/filter` before the sync loop starts.
#[derive(Serialize)]
pub struct SyncFilter {
    /// Which event fields to include in the response (reduces payload size).
    pub event_fields: Vec<String>,
    /// Filters applied to room events.
    pub room: RoomFilter,
    /// Strips all presence events.
    pub presence: EventFilter,
    /// Strips all account-data events.
    pub account_data: EventFilter,
}

/// Filters applied to the three room sub-categories (timeline, state, ephemeral).
#[derive(Serialize)]
pub struct RoomFilter {
    /// Timeline event filter — allows only `m.room.message`.
    pub timeline: RoomEventFilter,
    /// State filter — strips all state events.
    pub state: RoomEventFilter,
    /// Ephemeral filter (typing indicators, read receipts) — strips everything.
    pub ephemeral: EventFilter,
    /// Per-room account data filter — strips everything.
    pub account_data: EventFilter,
}

/// Filter for room-scoped event lists (timeline or state).
#[derive(Serialize)]
pub struct RoomEventFilter {
    /// Only include events whose `type` matches one of these strings.
    pub types: Vec<String>,
    /// Maximum number of events to return per room per sync response.
    pub limit: u32,
}

/// Generic event filter used to block entire event categories.
#[derive(Serialize)]
pub struct EventFilter {
    /// Event type globs to exclude; `["*"]` blocks all events in this category.
    pub not_types: Vec<String>,
}

impl SyncFilter {
    /// Create a filter that keeps only `m.room.message` timeline events and
    /// drops presence, receipts, typing, state, and account-data entirely.
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
    fn test_deserialize_sync_response_multiple_rooms() {
        let json = r#"{
            "next_batch":"s3",
            "rooms":{
                "join":{
                    "!room1:example.com":{
                        "timeline":{
                            "events":[{
                                "event_id":"$ev1",
                                "type":"m.room.message",
                                "sender":"@alice:example.com",
                                "content":{"msgtype":"m.text","body":"hi"}
                            }]
                        }
                    },
                    "!room2:example.com":{
                        "timeline":{
                            "events":[{
                                "event_id":"$ev2",
                                "type":"m.room.message",
                                "sender":"@bob:example.com",
                                "content":{"msgtype":"m.text","body":"hey"}
                            }]
                        }
                    }
                }
            }
        }"#;
        let s: SyncResponse = serde_json::from_str(json).unwrap();
        let joined = s.rooms.unwrap().join.unwrap();
        assert_eq!(joined.len(), 2);
        assert!(joined.contains_key("!room1:example.com"));
        assert!(joined.contains_key("!room2:example.com"));
    }

    #[test]
    fn test_deserialize_sync_response_no_events_in_timeline() {
        let json = r#"{
            "next_batch":"s4",
            "rooms":{
                "join":{
                    "!room:example.com":{
                        "timeline":{"events":[]}
                    }
                }
            }
        }"#;
        let s: SyncResponse = serde_json::from_str(json).unwrap();
        let joined = s.rooms.unwrap().join.unwrap();
        let room = joined.get("!room:example.com").unwrap();
        let events = room.timeline.as_ref().unwrap().events.as_ref().unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_deserialize_sync_response_no_timeline() {
        // Room with no timeline key at all
        let json = r#"{
            "next_batch":"s5",
            "rooms":{"join":{"!room:example.com":{}}}
        }"#;
        let s: SyncResponse = serde_json::from_str(json).unwrap();
        let joined = s.rooms.unwrap().join.unwrap();
        let room = joined.get("!room:example.com").unwrap();
        assert!(room.timeline.is_none());
    }

    #[test]
    fn test_deserialize_room_event_with_relates_to() {
        // An edit event has m.relates_to with rel_type = m.replace
        let json = r#"{
            "event_id":"$ev2",
            "type":"m.room.message",
            "sender":"@alice:example.com",
            "content":{
                "msgtype":"m.text",
                "body":"* edited",
                "m.relates_to":{"rel_type":"m.replace","event_id":"$ev1"}
            }
        }"#;
        let ev: RoomEvent = serde_json::from_str(json).unwrap();
        assert_eq!(
            ev.content["m.relates_to"]["rel_type"].as_str(),
            Some("m.replace")
        );
        assert_eq!(
            ev.content["m.relates_to"]["event_id"].as_str(),
            Some("$ev1")
        );
    }

    #[test]
    fn test_deserialize_matrix_error_full() {
        let json = r#"{"errcode":"M_FORBIDDEN","error":"You are not allowed to send messages in this room."}"#;
        let e: MatrixError = serde_json::from_str(json).unwrap();
        assert_eq!(e.errcode.as_deref(), Some("M_FORBIDDEN"));
        assert!(e.error.as_deref().unwrap().contains("not allowed"));
    }

    #[test]
    fn test_deserialize_matrix_error_partial() {
        // Homeservers sometimes omit fields — both should be None-safe
        let json = r#"{}"#;
        let e: MatrixError = serde_json::from_str(json).unwrap();
        assert!(e.errcode.is_none());
        assert!(e.error.is_none());
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
    fn test_edit_content_new_content_msgtype() {
        // m.new_content must also carry the msgtype field per the Matrix spec
        let c = edit_content("$ev", "updated text");
        assert_eq!(c["m.new_content"]["msgtype"], "m.text");
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

    #[test]
    fn test_sync_filter_structure() {
        let f = SyncFilter::messages_only();
        // Timeline allows only m.room.message
        assert_eq!(f.room.timeline.types, vec!["m.room.message"]);
        assert_eq!(f.room.timeline.limit, 50);
        // State is empty
        assert!(f.room.state.types.is_empty());
        assert_eq!(f.room.state.limit, 0);
        // Presence/ephemeral/account-data all block everything
        assert_eq!(f.presence.not_types, vec!["*"]);
        assert_eq!(f.room.ephemeral.not_types, vec!["*"]);
        assert_eq!(f.account_data.not_types, vec!["*"]);
    }

    #[test]
    fn test_sync_filter_event_fields() {
        let f = SyncFilter::messages_only();
        assert!(f.event_fields.contains(&"event_id".to_string()));
        assert!(f.event_fields.contains(&"type".to_string()));
        assert!(f.event_fields.contains(&"sender".to_string()));
        assert!(f.event_fields.contains(&"content".to_string()));
    }
}
