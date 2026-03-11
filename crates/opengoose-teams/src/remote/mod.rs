/// Remote Agent Protocol for OpenGoose.
///
/// Enables agents running on remote machines to participate in OpenGoose
/// teams over a WebSocket connection. The protocol supports:
///
/// - **Handshake**: authenticate and register the remote agent
/// - **Heartbeat**: periodic keep-alive to detect disconnections
/// - **Message relay**: forward messages between local and remote agents
/// - **Reconnect**: client reconnects after a drop with last-seen event ID
pub mod protocol;
pub mod registry;
mod transport;

#[cfg(test)]
mod tests;

// Re-export public API at the module level for backwards compatibility.
pub use protocol::{ConnectionMetrics, ConnectionState, ProtocolMessage};
pub use registry::{RemoteAgent, RemoteAgentRegistry, RemoteConfig};
pub use transport::ReplayResult;
