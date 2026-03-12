use serde::Serialize;

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
