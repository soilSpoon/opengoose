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
    let json =
        r#"{"errcode":"M_FORBIDDEN","error":"You are not allowed to send messages in this room."}"#;
    let e: MatrixError = serde_json::from_str(json).unwrap();
    assert_eq!(e.errcode.as_deref(), Some("M_FORBIDDEN"));
    assert!(e.error.as_deref().unwrap().contains("not allowed"));
}

#[test]
fn test_deserialize_matrix_error_partial() {
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
    assert!(json.contains("m.room.message"));
    assert!(json.contains("event_fields"));
}

#[test]
fn test_sync_filter_structure() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.room.timeline.types, vec!["m.room.message"]);
    assert_eq!(f.room.timeline.limit, 50);
    assert!(f.room.state.types.is_empty());
    assert_eq!(f.room.state.limit, 0);
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

#[test]
fn test_sync_filter_event_fields_exactly_four() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.event_fields.len(), 4);
}

#[test]
fn test_sync_filter_timeline_limit_is_50() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.room.timeline.limit, 50);
}

#[test]
fn test_sync_filter_state_allows_no_types() {
    let f = SyncFilter::messages_only();
    assert!(f.room.state.types.is_empty());
    assert_eq!(f.room.state.limit, 0);
}

#[test]
fn test_sync_filter_presence_blocks_all() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.presence.not_types, vec!["*"]);
}

#[test]
fn test_sync_filter_room_ephemeral_blocks_all() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.room.ephemeral.not_types, vec!["*"]);
}

#[test]
fn test_sync_filter_room_account_data_blocks_all() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.room.account_data.not_types, vec!["*"]);
}

#[test]
fn test_sync_filter_top_level_account_data_blocks_all() {
    let f = SyncFilter::messages_only();
    assert_eq!(f.account_data.not_types, vec!["*"]);
}

#[test]
fn test_sync_response_unknown_fields_ignored() {
    let json = r#"{
        "next_batch": "s99",
        "device_one_time_keys_count": {},
        "device_unused_fallback_key_types": [],
        "to_device": {"events": []}
    }"#;
    let s: SyncResponse = serde_json::from_str(json).unwrap();
    assert_eq!(s.next_batch, "s99");
    assert!(s.rooms.is_none());
}

#[test]
fn test_room_event_extra_fields_ignored() {
    let json = r#"{
        "event_id": "$abc",
        "type": "m.room.message",
        "sender": "@user:example.com",
        "content": {"msgtype": "m.text", "body": "hello"},
        "origin_server_ts": 1700000000000,
        "unsigned": {"age": 50},
        "room_id": "!room:example.com"
    }"#;
    let ev: RoomEvent = serde_json::from_str(json).unwrap();
    assert_eq!(ev.event_id, "$abc");
    assert_eq!(ev.sender, "@user:example.com");
}
