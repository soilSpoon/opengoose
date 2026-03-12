use tracing::{info, instrument};

use crate::error::GatewayError;
use opengoose_types::{AppEventKind, SessionKey, StreamChunk};

use goose::gateway::{IncomingMessage, PlatformUser};

use super::GatewayBridge;

/// Parameters for relaying an incoming message with streaming.
pub struct RelayParams<'a> {
    pub session_key: &'a SessionKey,
    pub display_name: Option<String>,
    pub text: &'a str,
    pub responder: &'a dyn crate::StreamResponder,
    pub channel_id: &'a str,
    pub throttle: crate::ThrottlePolicy,
    pub max_display_len: usize,
}

impl GatewayBridge {
    /// Relay an incoming message through the Engine and Goose handler.
    ///
    /// Returns `Some(receiver)` if a team handles the message via streaming.
    /// Returns `None` if no team is active (falls through to Goose single-agent,
    /// which responds via the `Gateway::send_message` callback — no streaming).
    #[instrument(
        skip(self, display_name, text),
        fields(
            session_id = %session_key.to_stable_id(),
            has_display_name = display_name.is_some(),
            text_len = text.chars().count()
        )
    )]
    pub async fn relay_message_streaming(
        &self,
        session_key: &SessionKey,
        display_name: Option<String>,
        text: &str,
    ) -> anyhow::Result<Option<tokio::sync::broadcast::Receiver<StreamChunk>>> {
        info!(gateway_type = "bridge", message_type = "streaming", session_id = %session_key.to_stable_id(), "relay_message");

        if !self.is_accepting_messages() {
            return Err(GatewayError::ShuttingDown.into());
        }

        // Try streaming team orchestration via Engine
        match self
            .engine
            .process_message_streaming(session_key, display_name.as_deref(), text)
            .await?
        {
            Some(rx) => return Ok(Some(rx)),
            None => {
                // No team active — fall through to Goose single-agent
            }
        }

        let guard = self.handler.read().await;
        let handler = guard.as_ref().ok_or(GatewayError::HandlerNotReady)?;

        let incoming = IncomingMessage {
            user: PlatformUser {
                platform: session_key.platform.as_str().to_string(),
                user_id: session_key.to_stable_id(),
                display_name,
            },
            text: text.to_string(),
            platform_message_id: None,
            attachments: vec![],
        };

        handler.handle_message(incoming).await?;
        Ok(None)
    }

    /// Relay an incoming message with streaming, and drive the stream to
    /// completion if a team handles it.
    ///
    /// This combines `relay_message_streaming` + `drive_stream` into a single
    /// call, eliminating the boilerplate duplicated across every channel gateway.
    ///
    /// Returns `true` if a team handled the message (caller should NOT expect
    /// a `send_message` callback), `false` if the Goose single-agent path was
    /// used (response arrives via `Gateway::send_message`).
    #[instrument(
        skip(self, params),
        fields(
            session_id = %params.session_key.to_stable_id(),
            channel_id = %params.channel_id,
            has_display_name = params.display_name.is_some(),
            text_len = params.text.chars().count(),
            max_display_len = params.max_display_len
        )
    )]
    pub async fn relay_and_drive_stream(
        &self,
        params: RelayParams<'_>,
    ) -> anyhow::Result<bool> {
        let result = self.relay_and_drive_stream_inner(params).await;

        // Emit error event centrally so adapters don't have to repeat this
        if let Err(ref e) = result {
            self.engine.event_bus().emit(AppEventKind::Error {
                context: "relay".into(),
                message: e.to_string(),
            });
        }

        result
    }

    async fn relay_and_drive_stream_inner(
        &self,
        params: RelayParams<'_>,
    ) -> anyhow::Result<bool> {
        info!(gateway_type = "bridge", message_type = "streaming", session_id = %params.session_key.to_stable_id(), channel_id = %params.channel_id, "relay_and_drive_stream");

        match self
            .relay_message_streaming(params.session_key, params.display_name, params.text)
            .await?
        {
            Some(rx) => {
                crate::stream_orchestrator::drive_stream(
                    params.responder,
                    params.channel_id,
                    rx,
                    params.throttle,
                    params.max_display_len,
                )
                .await?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
