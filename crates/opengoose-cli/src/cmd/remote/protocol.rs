use anyhow::Result;
use opengoose_teams::remote::ProtocolMessage;

use super::connect::ConnectFailure;

pub(super) type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
pub(super) type WsWrite =
    futures_util::stream::SplitSink<WsStream, tokio_tungstenite::tungstenite::Message>;
pub(super) type WsRead = futures_util::stream::SplitStream<WsStream>;

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) fn heartbeat_message() -> ProtocolMessage {
    ProtocolMessage::Heartbeat {
        timestamp: now_unix_secs(),
    }
}

fn is_replayable_message(message: &ProtocolMessage) -> bool {
    matches!(
        message,
        ProtocolMessage::MessageRelay { .. }
            | ProtocolMessage::Broadcast { .. }
            | ProtocolMessage::Disconnect { .. }
    )
}

pub(super) fn record_replayable_event(
    last_seen_event_id: &mut u64,
    message: &ProtocolMessage,
) -> Option<u64> {
    if is_replayable_message(message) {
        *last_seen_event_id = last_seen_event_id.saturating_add(1);
        Some(*last_seen_event_id)
    } else {
        None
    }
}

pub(super) fn next_delivery_label(pending_replayed_events: &mut u64) -> &'static str {
    if *pending_replayed_events > 0 {
        *pending_replayed_events -= 1;
        "replay"
    } else {
        "live"
    }
}

pub(super) async fn send_protocol(write: &mut WsWrite, message: &ProtocolMessage) -> Result<()> {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;

    let json = serde_json::to_string(message)?;
    write.send(Message::Text(json.into())).await?;
    Ok(())
}

pub(super) async fn recv_protocol(
    read: &mut WsRead,
    phase: &str,
) -> std::result::Result<ProtocolMessage, ConnectFailure> {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    let message = match read.next().await {
        Some(Ok(message)) => message,
        Some(Err(err)) => {
            return Err(ConnectFailure::Retryable(anyhow::anyhow!(
                "websocket error during {phase}: {err}"
            )));
        }
        None => {
            return Err(ConnectFailure::Retryable(anyhow::anyhow!(
                "connection closed during {phase}"
            )));
        }
    };

    let text = match message {
        Message::Text(text) => text,
        Message::Close(_) => {
            return Err(ConnectFailure::Retryable(anyhow::anyhow!(
                "connection closed during {phase}"
            )));
        }
        other => {
            return Err(ConnectFailure::Terminal(anyhow::anyhow!(
                "unexpected {phase} response: {other:?}"
            )));
        }
    };

    serde_json::from_str(&text)
        .map_err(|err| ConnectFailure::Terminal(anyhow::anyhow!("invalid {phase} response: {err}")))
}
