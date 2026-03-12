//! Matrix Client-Server API types for the sync loop and message sending.

mod content;
mod error;
mod filter;
mod responses;

// Keep a flat internal facade so gateway code can continue importing from
// `crate::types` while the underlying implementations live in focused modules.
pub use content::{edit_content, text_content};
pub use error::MatrixError;
pub use filter::SyncFilter;
pub use responses::{RoomEvent, SendEventResponse, SyncResponse, WhoAmI};

#[cfg(test)]
mod tests;
