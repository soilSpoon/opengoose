/// Errors produced by the OpenGoose gateway layer.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The pairing store has not been initialised yet.
    #[error("pairing store not initialized")]
    PairingStoreNotReady,

    /// The gateway handler has not been started yet.
    #[error("gateway not started yet")]
    HandlerNotReady,
}
