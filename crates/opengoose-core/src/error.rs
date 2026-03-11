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

impl GatewayError {
    pub fn is_transient(&self) -> bool {
        match self {
            Self::PairingStoreNotReady
            | Self::HandlerNotReady
            | Self::ChannelClosed { .. }
            | Self::TeamStoreNotReady
            | Self::ProfileStoreNotReady => true,
            Self::Profile(err) => err.is_transient(),
            Self::Team(err) => err.is_transient(),
            Self::Persistence(err) => err.is_transient(),
            Self::GooseError(err) => anyhow_error_is_transient(err),
        }
    }
}

fn anyhow_error_is_transient(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(opengoose_types::is_transient_io_error)
            || cause
                .downcast_ref::<reqwest::Error>()
                .is_some_and(reqwest_error_is_transient)
            || cause
                .downcast_ref::<opengoose_persistence::PersistenceError>()
                .is_some_and(opengoose_persistence::PersistenceError::is_transient)
    })
}

fn reqwest_error_is_transient(err: &reqwest::Error) -> bool {
    err.is_timeout()
        || err.is_connect()
        || err
            .status()
            .is_some_and(|status| status.is_server_error() || status.as_u16() == 429)
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

    #[test]
    fn test_gateway_error_store_not_ready_is_transient() {
        assert!(GatewayError::PairingStoreNotReady.is_transient());
        assert!(GatewayError::HandlerNotReady.is_transient());
    }

    #[test]
    fn test_gateway_error_profile_not_found_is_not_transient() {
        let err = GatewayError::Profile(opengoose_profiles::ProfileError::NotFound("p1".into()));
        assert!(!err.is_transient());
    }

    #[test]
    fn test_gateway_error_anyhow_timeout_is_transient() {
        let source = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let err = anyhow::Error::new(source).context("gateway request failed");
        let err = GatewayError::GooseError(err);
        assert!(err.is_transient());
    }
}
