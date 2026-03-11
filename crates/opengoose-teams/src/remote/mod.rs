//! Remote Agent Protocol for OpenGoose.
//!
//! Enables agents running on remote machines to participate in OpenGoose
//! teams over a WebSocket connection. The protocol supports:
//!
//! - **Handshake**: authenticate and register the remote agent
//! - **Heartbeat**: periodic keep-alive to detect disconnections
//! - **Message relay**: forward messages between local and remote agents
//! - **Reconnect**: client reconnects after a drop with last-seen event ID

mod protocol;
mod registry;
mod state;

pub use protocol::ProtocolMessage;
pub use registry::RemoteAgentRegistry;
pub use state::{ConnectionMetrics, ConnectionState, RemoteAgent, RemoteConfig};
