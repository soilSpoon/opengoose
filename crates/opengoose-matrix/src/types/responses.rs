use serde::Deserialize;
use std::collections::HashMap;

/// Response from `GET /account/whoami`.
///
/// Returns the Matrix user ID of the authenticated user (e.g. `@bot:example.com`).
#[derive(Deserialize)]
pub struct WhoAmI {
    /// Fully-qualified Matrix user ID, e.g. `@bot:example.com`.
    pub user_id: String,
}

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
    pub event_id: String,
    /// Event type string, e.g. `m.room.message`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// Fully-qualified sender user ID, e.g. `@alice:example.com`.
    pub sender: String,
    /// Raw event content; structure varies by event type.
    pub content: serde_json::Value,
}

/// Response from `PUT /rooms/{roomId}/send/{eventType}/{txnId}`.
#[derive(Deserialize)]
pub struct SendEventResponse {
    /// Server-assigned event ID for the newly created event.
    pub event_id: String,
}
