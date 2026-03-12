//! Matrix Client-Server API types for the sync loop and message sending.

mod content;
mod error;
mod filter;
mod responses;

// Keep a flat internal facade so gateway code can continue importing from
// `crate::types` while the underlying implementations live in focused modules.
#[allow(unused_imports)]
pub use content::{edit_content, text_content};
#[allow(unused_imports)]
pub use error::MatrixError;
#[allow(unused_imports)]
pub use filter::{EventFilter, RoomEventFilter, RoomFilter, SyncFilter};
#[allow(unused_imports)]
pub use responses::{
    JoinedRoom, RoomEvent, SendEventResponse, SyncResponse, SyncRooms, Timeline, WhoAmI,
};

#[cfg(test)]
mod tests;
