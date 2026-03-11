use std::net::SocketAddr;
use std::sync::Arc;

use opengoose_persistence::{ApiKeyStore, Database};
use opengoose_teams::remote::RemoteAgentRegistry;
use opengoose_types::{ChannelMetricsStore, EventBus};

/// Configuration for the web dashboard server.
#[derive(Debug, Clone)]
pub struct WebOptions {
    /// Socket address to bind the HTTP listener to.
    pub bind: SocketAddr,
    /// Path to TLS certificate PEM file. When set alongside `tls_key_path`,
    /// the server serves over HTTPS/WSS instead of plain HTTP/WS.
    pub tls_cert_path: Option<std::path::PathBuf>,
    /// Path to TLS private key PEM file.
    pub tls_key_path: Option<std::path::PathBuf>,
}

impl WebOptions {
    /// Create a plain HTTP options struct bound to the given address.
    pub fn plain(bind: SocketAddr) -> Self {
        Self {
            bind,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

impl Default for WebOptions {
    fn default() -> Self {
        Self::plain(SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, 3000)))
    }
}

#[derive(Clone)]
pub(crate) struct PageState {
    pub(crate) db: Arc<Database>,
    pub(crate) api_key_store: Arc<ApiKeyStore>,
    pub(crate) remote_registry: RemoteAgentRegistry,
    pub(crate) channel_metrics: ChannelMetricsStore,
    #[allow(dead_code)]
    pub(crate) event_bus: EventBus,
}
