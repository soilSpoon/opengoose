//! Matrix channel adapter for OpenGoose.
//!
//! Implements the goose [`Gateway`] trait for Matrix homeservers using the
//! Matrix Client-Server API v3.  Messages are received via `/sync` long-polling
//! and sent via the room event PUT endpoint.
//!
//! # Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use opengoose_matrix::MatrixGateway;
//!
//! // `bridge` and `bus` are provided by the host application.
//! let gateway = MatrixGateway::new(
//!     "https://matrix.example.com",
//!     "syt_your_access_token_here",
//!     bridge,
//!     bus,
//! );
//! ```

mod gateway;
mod types;

pub use gateway::MatrixGateway;
