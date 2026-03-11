//! Matrix gateway implementation: /sync long-polling event loop.
//!
//! [`MatrixGateway`] implements the `Gateway` trait using the Matrix
//! Client-Server API v3. It polls `/sync` for new room events and sends
//! messages via the room event PUT endpoint. Uses atomic counters for
//! monotonic transaction IDs and supports reconnection on error.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tracing::{debug, error, info, warn};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::message_utils::{split_message, truncate_for_display};
use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform, SessionKey};

use crate::types::{
    MatrixError, SendEventResponse, SyncFilter, SyncResponse, WhoAmI, edit_content, text_content,
};

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

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn v3_url(&self, path: &str) -> String {
        format!("{}/_matrix/client/v3{}", self.homeserver_url, path)
    }

    fn next_txn_id(&self) -> String {
        let n = self.txn_counter.fetch_add(1, Ordering::Relaxed);
        format!("opengoose-{}-{}", std::process::id(), n)
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// GET /account/whoami — returns the bot's Matrix user ID.
    async fn whoami(&self) -> anyhow::Result<String> {
        let resp: WhoAmI = self
            .client
            .get(self.v3_url("/account/whoami"))
            .header("Authorization", self.auth_header())
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.user_id)
    }

    /// Register a minimal sync filter and return the filter ID.
    async fn register_filter(&self, user_id: &str) -> anyhow::Result<String> {
        let encoded_user = urlencoding::encode(user_id).into_owned();
        let filter = SyncFilter::messages_only();
        let resp: serde_json::Value = self
            .client
            .post(self.v3_url(&format!("/user/{encoded_user}/filter")))
            .header("Authorization", self.auth_header())
            .json(&filter)
            .send()
            .await?
            .json()
            .await?;
        resp.get("filter_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no filter_id in response"))
    }

    /// GET /sync — long-poll for new events.
    async fn sync(
        &self,
        since: Option<&str>,
        filter_id: Option<&str>,
    ) -> anyhow::Result<SyncResponse> {
        let mut req = self
            .client
            .get(self.v3_url("/sync"))
            .header("Authorization", self.auth_header())
            .query(&[("timeout", SYNC_TIMEOUT_MS.to_string())]);

        if let Some(s) = since {
            req = req.query(&[("since", s)]);
        }
        if let Some(f) = filter_id {
            req = req.query(&[("filter", f)]);
        }

        Ok(req.send().await?.json().await?)
    }

    /// PUT /rooms/{roomId}/send/{eventType}/{txnId} — send a message event.
    async fn send_event(
        &self,
        room_id: &str,
        content: &serde_json::Value,
    ) -> anyhow::Result<String> {
        let encoded_room = urlencoding::encode(room_id).into_owned();
        let txn_id = self.next_txn_id();
        let url = self.v3_url(&format!(
            "/rooms/{encoded_room}/send/m.room.message/{txn_id}"
        ));

        let resp = self
            .client
            .put(&url)
            .header("Authorization", self.auth_header())
            .json(content)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: MatrixError = resp.json().await.unwrap_or(MatrixError {
                errcode: None,
                error: Some("unknown error".into()),
            });
            anyhow::bail!(
                "send_event failed: {} — {}",
                err.errcode.unwrap_or_default(),
                err.error.unwrap_or_default()
            );
        }

        let ev: SendEventResponse = resp.json().await?;
        Ok(ev.event_id)
    }

    /// Build the SessionKey for a Matrix room.
    ///
    /// Namespace = server name extracted from the bot's user_id (e.g. `example.com`).
    /// Channel ID = room_id (e.g. `!room:example.com`).
    fn session_key(server_name: &str, room_id: &str) -> SessionKey {
        SessionKey::new(Platform::Custom("matrix".to_string()), server_name, room_id)
    }

    /// Extract the server name from a Matrix user_id (`@user:server.com` → `server.com`).
    fn server_name_from_user_id(user_id: &str) -> &str {
        user_id
            .split_once(':')
            .map(|(_, server)| server)
            .unwrap_or("matrix.org")
    }

    /// Send a plain-text message to a room, splitting if needed.
    async fn post_message(&self, room_id: &str, text: &str) -> anyhow::Result<()> {
        debug!(room_id = %room_id, text_len = text.len(), "posting matrix message");
        for chunk in split_message(text, MATRIX_MAX_LEN) {
            let content = text_content(chunk);
            if let Err(e) = self.send_event(room_id, &content).await {
                warn!(%e, %room_id, "failed to send matrix message chunk");
            }
        }
        Ok(())
    }

    /// Handle the `!team` bot command and reply in the room.
    async fn handle_team_command(&self, session_key: &SessionKey, room_id: &str, args: &str) {
        let response = self.bridge.handle_pairing(session_key, args);
        if let Err(e) = self.post_message(room_id, &response).await {
            error!(%e, "failed to reply to !team command");
        }
    }

    /// Run the /sync loop until cancelled.
    async fn run_sync_loop(
        &self,
        cancel: &CancellationToken,
        bot_user_id: &str,
        filter_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let server_name = Self::server_name_from_user_id(bot_user_id);
        let mut next_batch: Option<String> = None;
        let mut reconnect_attempts: u32 = 0;

        loop {
            if cancel.is_cancelled() {
                break;
            }

            let result = self.sync(next_batch.as_deref(), filter_id).await;

            match result {
                Ok(sync_resp) => {
                    reconnect_attempts = 0;
                    let batch = sync_resp.next_batch.clone();

                    if let Some(rooms) = sync_resp.rooms
                        && let Some(joined) = rooms.join
                    {
                        for (room_id, room) in joined {
                            let Some(timeline) = room.timeline else {
                                continue;
                            };
                            let Some(events) = timeline.events else {
                                continue;
                            };

                            for event in events {
                                // Only handle m.room.message
                                if event.event_type != "m.room.message" {
                                    continue;
                                }
                                // Ignore our own messages
                                if event.sender == bot_user_id {
                                    continue;
                                }
                                // Only plain text messages
                                if event.content.get("msgtype").and_then(|v| v.as_str())
                                    != Some("m.text")
                                {
                                    continue;
                                }
                                // Ignore edits (m.relates_to with m.replace)
                                if event
                                    .content
                                    .get("m.relates_to")
                                    .and_then(|v| v.get("rel_type"))
                                    .and_then(|v| v.as_str())
                                    == Some("m.replace")
                                {
                                    continue;
                                }

                                let Some(body) = event.content.get("body").and_then(|v| v.as_str())
                                else {
                                    continue;
                                };

                                let body = body.trim();
                                if body.is_empty() {
                                    continue;
                                }

                                let session_key = Self::session_key(server_name, &room_id);
                                let display_name = Some(event.sender.clone());

                                debug!(
                                    room_id = %room_id,
                                    sender = %event.sender,
                                    body_len = body.len(),
                                    "processing matrix room message"
                                );

                                // Check for !team command
                                if let Some(args) = body.strip_prefix("!team") {
                                    let args = args.trim();
                                    self.handle_team_command(&session_key, &room_id, args).await;
                                    continue;
                                }

                                if let Err(e) = self
                                    .bridge
                                    .relay_and_drive_stream(
                                        &session_key,
                                        display_name,
                                        body,
                                        self as &dyn StreamResponder,
                                        &room_id,
                                        opengoose_core::ThrottlePolicy::matrix(),
                                        MATRIX_MAX_LEN,
                                    )
                                    .await
                                {
                                    error!(%e, "failed to relay matrix message");
                                }
                            }
                        }
                    }

                    next_batch = Some(batch);
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                        error!(%e, "matrix sync loop giving up after max reconnect attempts");
                        return Err(e);
                    }
                    let delay = Duration::from_secs(2u64.pow(reconnect_attempts.min(5)));
                    let delay_secs = delay.as_secs();
                    warn!(%e, attempt = reconnect_attempts, ?delay, "matrix /sync error, retrying...");
                    self.metrics.record_reconnect("matrix", Some(e.to_string()));
                    self.event_bus.emit(AppEventKind::ChannelReconnecting {
                        platform: Platform::Custom("matrix".to_string()),
                        attempt: reconnect_attempts,
                        delay_secs,
                    });
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }

        Ok(())
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
            let session_key = self
                .bridge
                .on_outgoing_message(&user.user_id, &body, "matrix")
                .await;

            if let Err(e) = self.post_message(&session_key.channel_id, &body).await {
                error!(%e, "failed to send matrix message");
            }
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
// StreamResponder trait
// ---------------------------------------------------------------------------

#[async_trait]
impl StreamResponder for MatrixGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        MATRIX_MAX_LEN
    }

    async fn create_draft(&self, room_id: &str) -> anyhow::Result<DraftHandle> {
        debug!(room_id = %room_id, "creating matrix draft");
        let content = text_content("Thinking...");
        let event_id = self.send_event(room_id, &content).await?;
        debug!(room_id = %room_id, event_id = %event_id, "matrix draft created");
        Ok(DraftHandle {
            message_id: event_id,
            channel_id: room_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        debug!(room_id = %handle.channel_id, event_id = %handle.message_id, content_len = content.len(), "updating matrix draft");
        let display = truncate_for_display(content, MATRIX_MAX_LEN);
        let ev_content = edit_content(&handle.message_id, display);
        self.send_event(&handle.channel_id, &ev_content).await?;
        Ok(())
    }

    async fn send_new_message(&self, room_id: &str, content: &str) -> anyhow::Result<()> {
        self.post_message(room_id, content).await
    }

    // finalize_draft uses the default implementation from StreamResponder
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
