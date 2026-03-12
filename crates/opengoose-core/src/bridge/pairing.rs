use std::time::{SystemTime, UNIX_EPOCH};

use tracing::{debug, instrument};

use crate::error::GatewayError;
use opengoose_types::{AppEventKind, SessionKey};

use goose::gateway::pairing::PairingStore;

use super::GatewayBridge;

/// Prefix of the Goose response that confirms a successful pairing.
pub(super) const PAIRING_CONFIRMED_PREFIX: &str = "Paired!";

/// Exact Goose response that prompts the user to enter a pairing code.
pub(super) const PAIRING_PROMPT: &str = "Welcome! Enter your pairing code to connect to goose.";

impl GatewayBridge {
    /// Handle a `/team` or `!team` pairing command and return the response string.
    ///
    /// "Pairing" here means associating a channel with a team/profile so that
    /// subsequent messages in that channel are routed to the selected team.
    ///
    /// Centralizes the pairing dispatch so adapter implementations do not need
    /// to reach into `engine()` directly.  Each adapter still owns the
    /// platform-specific delivery of the returned string.
    ///
    /// # Examples
    /// - `args = "code-review"` — activate the "code-review" team for this channel
    /// - `args = "off"` — deactivate the current team
    /// - `args = ""` — return status of the active team
    /// - `args = "list"` — list available teams
    pub fn handle_pairing(&self, session_key: &SessionKey, args: &str) -> String {
        if !self.is_accepting_messages() {
            return self.shutdown_message().to_string();
        }
        self.engine.handle_team_command(session_key, args)
    }

    /// Generate a 6-character pairing code (300s expiry) and emit it on the event bus.
    #[instrument(skip(self), fields(platform = %platform))]
    pub async fn generate_pairing_code(&self, platform: &str) -> Result<String, GatewayError> {
        debug!(gateway_type = %platform, "generate_pairing_code");

        let guard = self.pairing_store.read().await;
        let store = guard.as_ref().ok_or(GatewayError::PairingStoreNotReady)?;

        let code = PairingStore::generate_code();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + 300;

        store
            .store_pending_code(&code, platform, expires_at)
            .await?;

        self.engine
            .event_bus()
            .emit(AppEventKind::PairingCodeGenerated { code: code.clone() });

        Ok(code)
    }
}
