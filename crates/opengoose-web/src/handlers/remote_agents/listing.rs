use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use opengoose_teams::remote::RemoteAgent;
use serde::Serialize;

use super::RemoteGatewayState;

/// GET /api/agents/remote — list currently connected remote agents.
pub async fn list_remote(
    State(state): State<Arc<RemoteGatewayState>>,
) -> Json<Vec<RemoteAgentInfo>> {
    let agents = state.registry.list().await;
    Json(agents.into_iter().map(RemoteAgentInfo::from).collect())
}

/// JSON response describing a currently connected remote agent.
#[derive(Serialize)]
pub struct RemoteAgentInfo {
    /// Registered agent name.
    pub name: String,
    /// Capabilities advertised during handshake.
    pub capabilities: Vec<String>,
    /// Network endpoint the agent connected from.
    pub endpoint: String,
    /// Seconds since the agent connected.
    pub connected_secs: u64,
    /// Seconds since the last heartbeat was received.
    pub last_heartbeat_secs: u64,
}

impl From<RemoteAgent> for RemoteAgentInfo {
    fn from(agent: RemoteAgent) -> Self {
        Self {
            name: agent.name,
            capabilities: agent.capabilities,
            endpoint: agent.endpoint,
            connected_secs: agent.connected_at.elapsed().as_secs(),
            last_heartbeat_secs: agent.last_heartbeat.elapsed().as_secs(),
        }
    }
}
