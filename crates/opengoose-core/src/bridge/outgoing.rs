use tracing::{info, instrument};

use opengoose_types::{AppEventKind, SessionKey};

use super::GatewayBridge;
use super::pairing::{PAIRING_CONFIRMED_PREFIX, PAIRING_PROMPT};

impl GatewayBridge {
    /// Called from `Gateway::send_message` — handles persistence, pairing detection,
    /// and event emission for outgoing messages from the Goose single-agent path.
    ///
    /// Returns the decoded `SessionKey` so the bridge can route replies back to
    /// the originating channel without adapters re-parsing the stable ID.
    #[instrument(
        skip(self, body),
        fields(
            session_id = %user_id,
            gateway_type = %gateway_type,
            body_len = body.chars().count()
        )
    )]
    pub(super) async fn on_outgoing_message(
        &self,
        user_id: &str,
        body: &str,
        gateway_type: &str,
    ) -> SessionKey {
        info!(gateway_type = %gateway_type, message_type = "response", "outgoing_message");
        let session_key = SessionKey::from_stable_id(user_id);

        // Persist assistant message (from single-agent path)
        self.engine.record_assistant_message(&session_key, body);

        // Emit PairingCompleted when goose confirms pairing
        if body.starts_with(PAIRING_CONFIRMED_PREFIX) {
            self.engine
                .event_bus()
                .emit(AppEventKind::PairingCompleted {
                    session_key: session_key.clone(),
                });
        }

        // Auto-generate pairing code
        if body == PAIRING_PROMPT
            && let Err(e) = self.generate_pairing_code(gateway_type).await
        {
            info!("failed to auto-generate pairing code: {e}");
        }

        self.engine.event_bus().emit(AppEventKind::ResponseSent {
            session_key: session_key.clone(),
            content: body.to_string(),
        });

        session_key
    }

    /// Persist an outgoing Goose response, emit pairing events when needed, and
    /// return the destination channel ID for platform-specific delivery.
    #[instrument(
        skip(self, body),
        fields(
            session_id = %user_id,
            gateway_type = %gateway_type,
            body_len = body.chars().count()
        )
    )]
    pub async fn route_outgoing_text(
        &self,
        user_id: &str,
        body: &str,
        gateway_type: &str,
    ) -> String {
        self.on_outgoing_message(user_id, body, gateway_type)
            .await
            .channel_id
    }
}
