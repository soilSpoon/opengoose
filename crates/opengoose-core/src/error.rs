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

    /// An error from the workflow subsystem.
    #[error("workflow error: {0}")]
    Workflow(#[from] opengoose_workflows::WorkflowError),

    /// An error propagated from the Goose agent system.
    #[error("goose agent error: {0}")]
    GooseError(#[from] anyhow::Error),
}
