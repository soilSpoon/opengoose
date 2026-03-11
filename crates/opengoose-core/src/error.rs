use opengoose_types::SessionKey;

/// Errors produced by the OpenGoose gateway layer.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The pairing store has not been initialised yet.
    #[error("pairing store not initialized")]
    PairingStoreNotReady,

    /// The gateway handler has not been started yet.
    #[error("gateway not started yet")]
    HandlerNotReady,

    /// The runtime has started draining and is no longer accepting new work.
    #[error("shutdown in progress; new messages are not being accepted")]
    ShuttingDown,

    /// The response channel has been closed; the receiver was dropped.
    #[error("response channel closed for session {session_key}")]
    ChannelClosed { session_key: SessionKey },

    /// An error from the profile subsystem.
    #[error("profile error: {0}")]
    Profile(#[from] opengoose_profiles::ProfileError),

    /// An error from the team subsystem.
    #[error("team error: {0}")]
    Team(#[from] opengoose_teams::TeamError),

    /// The team store is not available (not yet initialized).
    #[error("team store not available")]
    TeamStoreNotReady,

    /// The profile store is not available (not yet initialized).
    #[error("profile store not available")]
    ProfileStoreNotReady,

    /// An error from the persistence layer.
    #[error("persistence error: {0}")]
    Persistence(#[from] opengoose_persistence::PersistenceError),

    /// An error propagated from the Goose agent system.
    #[error("goose agent error: {0}")]
    GooseError(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::Platform;

    #[test]
    fn test_gateway_error_display_pairing_not_ready() {
        let err = GatewayError::PairingStoreNotReady;
        assert_eq!(err.to_string(), "pairing store not initialized");
    }

    #[test]
    fn test_gateway_error_display_handler_not_ready() {
        let err = GatewayError::HandlerNotReady;
        assert_eq!(err.to_string(), "gateway not started yet");
    }

    #[test]
    fn test_gateway_error_display_shutting_down() {
        let err = GatewayError::ShuttingDown;
        assert_eq!(
            err.to_string(),
            "shutdown in progress; new messages are not being accepted"
        );
    }

    #[test]
    fn test_gateway_error_display_channel_closed() {
        let key = SessionKey::new(Platform::Discord, "g1", "ch1");
        let err = GatewayError::ChannelClosed {
            session_key: key.clone(),
        };
        assert_eq!(
            err.to_string(),
            format!("response channel closed for session {key}")
        );
    }

    #[test]
    fn test_gateway_error_from_profile_error() {
        let profile_err = opengoose_profiles::ProfileError::NotFound("test".into());
        let err: GatewayError = profile_err.into();
        assert!(err.to_string().contains("profile error"));
    }

    #[test]
    fn test_gateway_error_from_team_error() {
        let team_err = opengoose_teams::TeamError::NotFound("my-team".into());
        let err: GatewayError = team_err.into();
        assert!(err.to_string().contains("team error"));
    }

    #[test]
    fn test_gateway_error_display_store_not_ready() {
        let err = GatewayError::TeamStoreNotReady;
        assert_eq!(err.to_string(), "team store not available");

        let err = GatewayError::ProfileStoreNotReady;
        assert_eq!(err.to_string(), "profile store not available");
    }

    #[test]
    fn test_gateway_error_from_persistence_error() {
        let pe = opengoose_persistence::PersistenceError::NoHomeDir;
        let err: GatewayError = pe.into();
        assert!(err.to_string().contains("persistence error"));
    }

    #[test]
    fn test_gateway_error_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("something failed");
        let err: GatewayError = anyhow_err.into();
        assert!(err.to_string().contains("something failed"));
    }
}
