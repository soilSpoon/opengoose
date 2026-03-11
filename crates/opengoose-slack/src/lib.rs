#![recursion_limit = "256"]

//! Slack channel adapter for OpenGoose.
//!
//! Implements the `Gateway` trait for Slack workspaces using
//! [Socket Mode](https://api.slack.com/apis/connections/socket) (WebSocket)
//! for event delivery and the Slack Web API for message posting.
//!
//! # Features
//!
//! - Slash command (`/team`) for channel-to-team pairing via `GatewayBridge`
//! - Automatic WebSocket reconnection with exponential back-off
//! - Message splitting for responses that exceed Slack's length limit
//! - Channel metrics tracking
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use opengoose_slack::SlackGateway;
//!
//! // `bridge` and `bus` are provided by the host application.
//! let gateway = SlackGateway::new(
//!     "xapp-your-app-level-token",
//!     "xoxb-your-bot-token",
//!     bridge,
//!     bus,
//! );
//! ```

mod gateway;
mod types;

pub use gateway::SlackGateway;
