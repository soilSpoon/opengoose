//! Telegram channel adapter for OpenGoose.
//!
//! Implements the `Gateway` trait for Telegram bots using the
//! [Telegram Bot API](https://core.telegram.org/bots/api).  Updates are
//! received via long-polling (`getUpdates`) and outgoing messages are
//! delegated to goose's built-in `TelegramGateway` for sending.
//!
//! # Features
//!
//! - `!team` text command for channel-to-team pairing via `GatewayBridge`
//! - Long-polling with configurable timeout and exponential back-off on errors
//! - Deduplication via monotonically-increasing `update_id` offset
//! - Support for group chats, supergroups, and private conversations
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use opengoose_telegram::TelegramGateway;
//!
//! // `bridge` and `bus` are provided by the host application.
//! let gateway = TelegramGateway::new(
//!     "your_telegram_bot_token",
//!     bridge,
//!     bus,
//! )?;
//! ```

mod gateway;

pub use gateway::TelegramGateway;
