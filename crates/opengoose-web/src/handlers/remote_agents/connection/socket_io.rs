use axum::extract::ws::{Message, WebSocket};
use opengoose_teams::remote::ProtocolMessage;
use tracing::warn;

/// Receive the next text message from the WebSocket, skipping pings/pongs.
///
/// WebSocket errors — including TLS handshake failures that surface after the
/// HTTP upgrade — are logged and treated as a clean connection close so the
/// registry is not left with stale entries.
pub(super) async fn recv_text(socket: &mut WebSocket) -> Option<String> {
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => return Some(text.to_string()),
            Some(Ok(Message::Close(_))) | None => return None,
            Some(Ok(_)) => continue,
            Some(Err(error)) => {
                log_socket_error(None, &error);
                return None;
            }
        }
    }
}

/// Send a protocol message as JSON text over the WebSocket.
pub(super) async fn send_protocol(
    socket: &mut WebSocket,
    msg: &ProtocolMessage,
) -> Result<(), axum::Error> {
    let json = serde_json::to_string(msg).map_err(axum::Error::new)?;
    socket.send(Message::Text(json.into())).await
}

pub(super) fn log_socket_error(name: Option<&str>, error: &axum::Error) {
    let error_text = error.to_string();
    let error_lower = error_text.to_lowercase();
    if error_lower.contains("tls")
        || error_lower.contains("certificate")
        || error_lower.contains("handshake")
    {
        warn!(error = %error_text, "TLS handshake error on remote agent connection");
    } else if let Some(name) = name {
        warn!(%name, error = %error_text, "websocket receive error");
    } else {
        warn!(error = %error_text, "websocket receive error");
    }
}
