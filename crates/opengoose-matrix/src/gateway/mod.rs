//! Matrix gateway implementation: /sync long-polling event loop.
//!
//! [`MatrixGateway`] implements the `Gateway` trait using the Matrix
//! Client-Server API v3. It polls `/sync` for new room events and sends
//! messages via the room event PUT endpoint. Uses atomic counters for
//! monotonic transaction IDs and supports reconnection on error.

mod api;
mod incoming;
mod outgoing;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use async_trait::async_trait;
use tracing::{error, info, warn};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::GatewayBridge;
use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform};

/// Maximum message length for Matrix rooms.
/// The spec doesn't mandate a limit, but 32 KiB is a safe practical ceiling
/// for most homeservers and clients.
pub(crate) const MATRIX_MAX_LEN: usize = 32_768;

/// Long-poll timeout for /sync (milliseconds).
pub(crate) const SYNC_TIMEOUT_MS: u64 = 30_000;

/// HTTP client timeout. Must exceed SYNC_TIMEOUT_MS (30 s) to avoid
/// cutting off long-poll responses. We add 15 s of buffer for network
/// latency and server processing overhead.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Maximum reconnect attempts before giving up.
pub(crate) const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Matrix channel gateway implementing the goose `Gateway` trait.
///
/// Connects to any Matrix homeserver via the Client-Server API (v3).
/// Uses `/sync` long-polling to receive messages and the rooms event send
/// endpoint to post replies.
///
/// Streaming drafts are implemented via Matrix event replacements
/// (`m.replace` relationship) so users see live typing updates.
pub struct MatrixGateway {
    /// Full homeserver base URL, e.g. `https://matrix.example.com`.
    homeserver_url: String,
    /// Matrix access token for authentication.
    access_token: String,
    client: reqwest::Client,
    bridge: Arc<GatewayBridge>,
    event_bus: EventBus,
    /// Shared connection metrics store.
    metrics: ChannelMetricsStore,
    /// Monotonically increasing transaction ID for idempotent sends.
    txn_counter: AtomicU64,
}

impl MatrixGateway {
    /// Create a new `MatrixGateway` connected to the given homeserver.
    ///
    /// # Arguments
    ///
    /// * `homeserver_url` — Base URL of the Matrix homeserver (e.g. `https://matrix.example.com`).
    ///   Trailing slashes are stripped automatically.
    /// * `access_token` — A valid Matrix access token for the bot account.
    /// * `bridge` — Shared gateway bridge for routing messages to the engine.
    /// * `event_bus` — Application event bus for channel lifecycle events.
    pub fn new(
        homeserver_url: impl Into<String>,
        access_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
    ) -> anyhow::Result<Self> {
        Self::with_metrics(
            homeserver_url,
            access_token,
            bridge,
            event_bus,
            ChannelMetricsStore::new(),
        )
    }

    pub fn with_metrics(
        homeserver_url: impl Into<String>,
        access_token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
        metrics: ChannelMetricsStore,
    ) -> anyhow::Result<Self> {
        // The sync endpoint uses its own long-poll timeout (SYNC_TIMEOUT_MS).
        // For all other requests (whoami, send_event, etc.) we want a shorter
        // deadline so the caller doesn't block indefinitely on network issues.
        // reqwest's per-request timeout overrides the client default, so the
        // 30 s here only applies to non-sync requests.
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build Matrix reqwest client: {e}"))?;
        Ok(Self {
            homeserver_url: homeserver_url.into().trim_end_matches('/').to_string(),
            access_token: access_token.into(),
            client,
            bridge,
            event_bus,
            metrics,
            txn_counter: AtomicU64::new(0),
        })
    }
}

// ---------------------------------------------------------------------------
// Gateway trait
// ---------------------------------------------------------------------------

#[async_trait]
impl Gateway for MatrixGateway {
    fn gateway_type(&self) -> &str {
        "matrix"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        self.bridge.on_start(handler).await;

        let bot_user_id = self.whoami().await?;
        info!(bot_user_id = %bot_user_id, "matrix gateway starting");

        // Register a filter to limit sync traffic to m.room.message events only.
        let filter_id = match self.register_filter(&bot_user_id).await {
            Ok(id) => {
                info!(%id, "matrix sync filter registered");
                Some(id)
            }
            Err(e) => {
                warn!(%e, "failed to register matrix sync filter, syncing without filter");
                None
            }
        };

        info!("matrix gateway connected");
        self.event_bus.emit(AppEventKind::ChannelReady {
            platform: Platform::Custom("matrix".to_string()),
        });
        self.metrics.set_connected("matrix");

        let reason = match self
            .run_sync_loop(&cancel, &bot_user_id, filter_id.as_deref())
            .await
        {
            Ok(()) => "shutdown".to_string(),
            Err(e) => {
                error!(%e, "matrix sync loop failed");
                e.to_string()
            }
        };

        self.event_bus.emit(AppEventKind::ChannelDisconnected {
            platform: Platform::Custom("matrix".to_string()),
            reason,
        });

        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        if let OutgoingMessage::Text { body } = message {
            self.send_outgoing_text(user, &body).await?;
        }
        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        self.whoami().await.map(|_| ())
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "matrix".into())])
    }
}

// ---------------------------------------------------------------------------
// Helper: URL-encode room IDs / user IDs in path segments
// ---------------------------------------------------------------------------

pub(crate) mod urlencoding;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
