/// WebSocket gateway and REST endpoints for remote agent connections.
mod admin;
mod connection;
mod listing;

use opengoose_teams::remote::RemoteAgentRegistry;

pub use admin::{disconnect_remote, gateway_health};
pub use connection::ws_connect;
pub use listing::list_remote;

/// Shared state for the remote agent gateway.
#[derive(Clone)]
pub struct RemoteGatewayState {
    pub registry: RemoteAgentRegistry,
}

#[cfg(test)]
mod tests;
