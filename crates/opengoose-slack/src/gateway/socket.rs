//! Slack Socket Mode connection and reconnect handling.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use opengoose_types::{AppEventKind, Platform};

use crate::types::{AuthTestResponse, ConnectionsOpenResponse, EnvelopeAck, SocketEnvelope};

use super::{MAX_RECONNECT_ATTEMPTS, SlackGateway};

impl SlackGateway {
    /// Open a Socket Mode WebSocket connection.
    async fn connect_websocket(&self) -> anyhow::Result<String> {
        let resp: ConnectionsOpenResponse = self
            .client
            .post("https://slack.com/api/apps.connections.open")
            .bearer_auth(&self.app_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "apps.connections.open failed: {}",
                resp.error.unwrap_or_default()
            );
        }

        resp.url
            .ok_or_else(|| anyhow::anyhow!("no WebSocket URL in response"))
    }

    /// Get the bot's user ID for filtering self-messages.
    pub(super) async fn get_bot_user_id(&self) -> anyhow::Result<String> {
        let resp: AuthTestResponse = self
            .client
            .post("https://slack.com/api/auth.test")
            .bearer_auth(&self.bot_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("auth.test failed: {}", resp.error.unwrap_or_default());
        }

        resp.user_id
            .ok_or_else(|| anyhow::anyhow!("auth.test returned no user_id"))
    }

    fn emit_reconnect(&self, attempts: u32, error: String) -> Option<Duration> {
        let delay = websocket_reconnect_delay(attempts)?;
        self.metrics.record_reconnect("slack", Some(error));
        self.event_bus.emit(AppEventKind::ChannelReconnecting {
            platform: Platform::Slack,
            attempt: attempts,
            delay_secs: delay.as_secs(),
        });
        Some(delay)
    }

    /// Run the WebSocket event loop with reconnection support.
    pub(super) async fn run_socket_mode(
        &self,
        cancel: &CancellationToken,
        bot_user_id: &str,
    ) -> anyhow::Result<()> {
        let mut reconnect_attempts: u32 = 0;

        loop {
            if cancel.is_cancelled() {
                break;
            }

            let ws_url = match self.connect_websocket().await {
                Ok(url) => {
                    reconnect_attempts = 0;
                    url
                }
                Err(error) => {
                    reconnect_attempts += 1;
                    let Some(delay) = self.emit_reconnect(reconnect_attempts, error.to_string())
                    else {
                        return Err(error);
                    };
                    warn!(%error, ?delay, "failed to get WebSocket URL, retrying...");
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            info!("connecting to slack socket mode");

            let (ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                Ok(connection) => connection,
                Err(error) => {
                    reconnect_attempts += 1;
                    let Some(delay) = self.emit_reconnect(reconnect_attempts, error.to_string())
                    else {
                        return Err(error.into());
                    };
                    warn!(%error, ?delay, "WebSocket connect failed, retrying...");
                    tokio::time::sleep(delay).await;
                    continue;
                }
            };

            let (mut ws_write, mut ws_read) = ws_stream.split();

            info!("slack socket mode connected");
            self.metrics.set_connected("slack");
            self.event_bus.emit(AppEventKind::ChannelReady {
                platform: Platform::Slack,
            });

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        let _ = ws_write.close().await;
                        return Ok(());
                    }
                    message = ws_read.next() => {
                        match message {
                            Some(Ok(WsMessage::Text(text))) => {
                                let Ok(envelope) = serde_json::from_str::<SocketEnvelope>(&text) else {
                                    continue;
                                };

                                let ack = EnvelopeAck {
                                    envelope_id: envelope.envelope_id.clone(),
                                    payload: None,
                                };
                                if let Ok(ack_json) = serde_json::to_string(&ack)
                                    && ws_write
                                        .send(WsMessage::Text(ack_json.into()))
                                        .await
                                        .is_err()
                                {
                                    warn!("failed to send ACK, reconnecting...");
                                    break;
                                }

                                self.handle_envelope(&envelope, bot_user_id).await;
                            }
                            Some(Ok(WsMessage::Ping(data))) => {
                                let _ = ws_write.send(WsMessage::Pong(data)).await;
                            }
                            Some(Ok(WsMessage::Close(_))) | None => {
                                info!("slack WebSocket closed, reconnecting...");
                                break;
                            }
                            Some(Err(error)) => {
                                warn!(%error, "slack WebSocket error, reconnecting...");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        Ok(())
    }
}

fn websocket_reconnect_delay(attempts: u32) -> Option<Duration> {
    if attempts >= MAX_RECONNECT_ATTEMPTS {
        None
    } else {
        Some(Duration::from_secs(2u64.pow(attempts.min(5))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_max_len_constant() {
        assert_eq!(super::super::SLACK_MAX_LEN, 4000);
    }

    #[test]
    fn test_max_reconnect_attempts_constant() {
        assert_eq!(MAX_RECONNECT_ATTEMPTS, 10);
    }

    #[test]
    fn test_websocket_reconnect_delay_exhausted() {
        assert!(websocket_reconnect_delay(MAX_RECONNECT_ATTEMPTS).is_none());
        assert!(websocket_reconnect_delay(MAX_RECONNECT_ATTEMPTS - 1).is_some());
    }

    #[test]
    fn test_websocket_reconnect_delay_growth() {
        assert_eq!(
            websocket_reconnect_delay(1).unwrap(),
            Duration::from_secs(2)
        );
        assert_eq!(
            websocket_reconnect_delay(5).unwrap(),
            Duration::from_secs(32)
        );
    }

    #[test]
    fn test_websocket_reconnect_delay_full_sequence() {
        let delays: Vec<u64> = (1..MAX_RECONNECT_ATTEMPTS)
            .map(|attempt| websocket_reconnect_delay(attempt).unwrap().as_secs())
            .collect();
        assert_eq!(delays, vec![2, 4, 8, 16, 32, 32, 32, 32, 32]);
    }

    #[test]
    fn test_websocket_reconnect_delay_attempt_zero_is_one_second() {
        assert_eq!(
            websocket_reconnect_delay(0).unwrap(),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn test_metrics_store_records_reconnect_and_connect() {
        use opengoose_types::ChannelMetricsStore;

        let store = ChannelMetricsStore::new();

        store.record_reconnect("slack", Some("connection refused".into()));
        store.record_reconnect("slack", Some("timeout".into()));

        let snap = store.snapshot();
        assert_eq!(snap["slack"].reconnect_count, 2);
        assert_eq!(snap["slack"].last_error.as_deref(), Some("timeout"));
        assert!(snap["slack"].uptime_secs.is_none());

        store.set_connected("slack");
        let snap = store.snapshot();
        assert_eq!(snap["slack"].reconnect_count, 2);
        assert!(snap["slack"].last_error.is_none());
        assert!(snap["slack"].uptime_secs.is_some());
    }

    #[test]
    fn test_event_bus_emits_channel_reconnecting() {
        use opengoose_types::{AppEventKind, EventBus, Platform};

        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.emit(AppEventKind::ChannelReconnecting {
            platform: Platform::Slack,
            attempt: 1,
            delay_secs: 2,
        });

        let event = rx.try_recv().expect("event should be buffered");
        assert!(matches!(
            event.kind,
            AppEventKind::ChannelReconnecting {
                platform: Platform::Slack,
                attempt: 1,
                delay_secs: 2,
            }
        ));
    }

    #[test]
    fn test_metrics_and_event_bus_coordination() {
        use opengoose_types::{AppEventKind, ChannelMetricsStore, EventBus, Platform};

        let store = ChannelMetricsStore::new();
        let bus = EventBus::new(32);
        let mut rx = bus.subscribe();

        for attempt in 1..=3u32 {
            let delay_secs = 2u64.pow(attempt.min(5));
            store.record_reconnect("slack", Some(format!("attempt {attempt} failed")));
            bus.emit(AppEventKind::ChannelReconnecting {
                platform: Platform::Slack,
                attempt,
                delay_secs,
            });
        }

        let snap = store.snapshot();
        assert_eq!(snap["slack"].reconnect_count, 3);
        assert_eq!(
            snap["slack"].last_error.as_deref(),
            Some("attempt 3 failed")
        );

        for expected_attempt in 1..=3u32 {
            let event = rx.try_recv().expect("event should be buffered");
            match event.kind {
                AppEventKind::ChannelReconnecting { attempt, .. } => {
                    assert_eq!(attempt, expected_attempt);
                }
                _ => panic!("expected ChannelReconnecting event"),
            }
        }

        store.set_connected("slack");
        let snap = store.snapshot();
        assert!(snap["slack"].last_error.is_none());
        assert!(snap["slack"].uptime_secs.is_some());
        assert_eq!(snap["slack"].reconnect_count, 3);
    }

    #[test]
    fn test_websocket_reconnect_delay_at_max_minus_one() {
        let delay = websocket_reconnect_delay(MAX_RECONNECT_ATTEMPTS - 1);
        assert!(delay.is_some());
    }

    #[test]
    fn test_websocket_reconnect_delay_above_max_all_none() {
        for attempt in MAX_RECONNECT_ATTEMPTS..=MAX_RECONNECT_ATTEMPTS + 5 {
            assert!(
                websocket_reconnect_delay(attempt).is_none(),
                "expected None for attempt {attempt}"
            );
        }
    }
}
