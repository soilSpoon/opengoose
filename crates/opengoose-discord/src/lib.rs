#![recursion_limit = "256"]

//! Discord channel adapter for OpenGoose.
//!
//! Implements the `Gateway` trait for Discord bots using the
//! [Twilight](https://twilight.rs) library.  Events are received via the
//! Discord Gateway WebSocket and messages are sent via the Discord REST API.
//!
//! # Features
//!
//! - Slash command (`/team`) for channel-to-team pairing via `GatewayBridge`
//! - Draft-based streaming: a "thinking…" placeholder is posted immediately
//!   when Goose starts responding, then replaced in-place with the final text
//! - Deduplication of replayed messages via a bounded seen-message cache
//! - Channel metrics tracking
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use opengoose_discord::DiscordGateway;
//!
//! // `bridge` and `bus` are provided by the host application.
//! let gateway = DiscordGateway::new(
//!     "Bot your_discord_bot_token",
//!     bridge,
//!     bus,
//! );
//! ```

mod gateway;

pub use gateway::DiscordGateway;
