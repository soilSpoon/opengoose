use std::time::Duration;

use tracing::{error, info, warn};

use opengoose_types::{AppEventKind, Platform};
use tokio_util::sync::CancellationToken;

use super::{BotInfo, TelegramGateway, TelegramResponse, Update};

impl TelegramGateway {
    pub(crate) async fn run_polling_loop(&self, cancel: CancellationToken) -> anyhow::Result<()> {
        self.run_polling_loop_with(cancel, Self::reconnect_delay)
            .await
    }

    async fn run_polling_loop_with<F>(
        &self,
        cancel: CancellationToken,
        reconnect_delay: F,
    ) -> anyhow::Result<()>
    where
        F: Fn(u32) -> Duration,
    {
        let bot_username = self.get_bot_username().await.unwrap_or_default();
        info!(bot_username = %bot_username, "telegram gateway starting");

        let mut offset: Option<i64> = None;
        let mut ready_emitted = false;
        let mut reconnect_attempts: u32 = 0;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("telegram gateway shutting down");
                    self.emit_disconnect("shutdown");
                    break;
                }
                result = self.get_updates(offset) => {
                    match result {
                        Ok(updates) => {
                            reconnect_attempts = 0;
                            if !ready_emitted {
                                info!("telegram gateway connected");
                                self.event_bus.emit(AppEventKind::ChannelReady {
                                    platform: Platform::Telegram,
                                });
                                ready_emitted = true;
                            }

                            for update in updates {
                                offset = Some(update.update_id + 1);
                                self.handle_update(update, &bot_username).await;
                            }
                        }
                        Err(e) => {
                            reconnect_attempts += 1;
                            if reconnect_attempts >= super::MAX_RECONNECT_ATTEMPTS {
                                let reason = format!(
                                    "getUpdates failed after {} attempts: {e}",
                                    super::MAX_RECONNECT_ATTEMPTS
                                );
                                error!(%e, "telegram gateway giving up after max reconnect attempts");
                                self.emit_disconnect(reason.clone());
                                self.event_bus.emit(AppEventKind::Error {
                                    context: "telegram".into(),
                                    message: reason,
                                });
                                break;
                            }

                            let delay = reconnect_delay(reconnect_attempts);
                            warn!(%e, attempt = reconnect_attempts, ?delay, "telegram getUpdates error, retrying...");
                            ready_emitted = false;
                            self.event_bus.emit(AppEventKind::ChannelReconnecting {
                                platform: Platform::Telegram,
                                attempt: reconnect_attempts,
                                delay_secs: delay.as_secs(),
                            });
                            tokio::select! {
                                _ = cancel.cancelled() => {
                                    info!("telegram gateway shutting down during reconnect");
                                    self.emit_disconnect("shutdown");
                                    break;
                                }
                                _ = tokio::time::sleep(delay) => {}
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn get_updates(&self, offset: Option<i64>) -> anyhow::Result<Vec<Update>> {
        let mut params = serde_json::json!({ "timeout": 30 });
        if let Some(off) = offset {
            params["offset"] = serde_json::json!(off);
        }

        let resp: TelegramResponse<Vec<Update>> = self
            .client
            .post(self.api_url("getUpdates"))
            .json(&params)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!(
                "getUpdates failed: {}",
                resp.description.unwrap_or_default()
            );
        }

        Ok(resp.result.unwrap_or_default())
    }

    async fn get_bot_username(&self) -> Option<String> {
        let resp: TelegramResponse<BotInfo> = self
            .client
            .post(self.api_url("getMe"))
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        resp.result.and_then(|bot| bot.username)
    }

    fn emit_disconnect(&self, reason: impl Into<String>) {
        self.event_bus.emit(AppEventKind::ChannelDisconnected {
            platform: Platform::Telegram,
            reason: reason.into(),
        });
    }

    pub(crate) fn reconnect_delay(reconnect_attempts: u32) -> Duration {
        Duration::from_secs(2u64.pow(reconnect_attempts.min(5)))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use opengoose_types::{AppEventKind, EventBus, Platform};

    use crate::gateway::MAX_RECONNECT_ATTEMPTS;
    use crate::gateway::test_support::{MockResponse, MockTelegramApi, test_gateway};

    fn drain_event_kinds(
        rx: &mut tokio::sync::broadcast::Receiver<opengoose_types::AppEvent>,
    ) -> Vec<AppEventKind> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event.kind);
        }
        events
    }

    #[tokio::test]
    async fn get_updates_posts_timeout_and_offset() {
        let api = MockTelegramApi::spawn(vec![MockResponse::json(serde_json::json!({
            "ok": true,
            "result": []
        }))])
        .await;
        let gateway = test_gateway(&api.base_url, EventBus::new(16));

        let updates = gateway.get_updates(Some(42)).await.unwrap();

        assert!(updates.is_empty());

        let requests = api.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/bottest-token/getUpdates");
        assert_eq!(requests[0].body["timeout"], 30);
        assert_eq!(requests[0].body["offset"], 42);
    }

    #[tokio::test]
    async fn get_updates_returns_api_and_json_errors() {
        let api = MockTelegramApi::spawn(vec![
            MockResponse::json(serde_json::json!({
                "ok": false,
                "description": "Unauthorized"
            })),
            MockResponse::raw("not-json"),
        ])
        .await;
        let gateway = test_gateway(&api.base_url, EventBus::new(16));

        let api_error = match gateway.get_updates(None).await {
            Ok(_) => panic!("expected getUpdates API error"),
            Err(error) => error,
        };
        assert!(
            api_error
                .to_string()
                .contains("getUpdates failed: Unauthorized")
        );

        let json_error = match gateway.get_updates(None).await {
            Ok(_) => panic!("expected getUpdates JSON error"),
            Err(error) => error,
        };
        let decode_error = json_error
            .downcast_ref::<reqwest::Error>()
            .expect("expected reqwest decode error");
        assert!(decode_error.is_decode());
    }

    #[tokio::test]
    async fn get_bot_username_returns_none_when_lookup_fails() {
        let api = MockTelegramApi::spawn(vec![
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": {}
            })),
            MockResponse::raw("not-json"),
        ])
        .await;
        let gateway = test_gateway(&api.base_url, EventBus::new(16));

        assert_eq!(gateway.get_bot_username().await, None);
        assert_eq!(gateway.get_bot_username().await, None);
    }

    #[test]
    fn emit_disconnect_publishes_channel_disconnected_event() {
        let bus = EventBus::new(16);
        let mut events = bus.subscribe();
        let gateway = test_gateway("http://127.0.0.1:1", bus.clone());

        gateway.emit_disconnect("network timeout");

        let event = events.try_recv().expect("disconnect event");
        assert!(matches!(
            event.kind,
            AppEventKind::ChannelDisconnected {
                platform: Platform::Telegram,
                reason,
            } if reason == "network timeout"
        ));
    }

    #[tokio::test]
    async fn run_polling_loop_emits_ready_once_advances_offset_and_falls_back_without_username() {
        let api = MockTelegramApi::spawn(vec![
            MockResponse::raw("not-json"),
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": [
                    { "update_id": 100 },
                    { "update_id": 105 }
                ]
            })),
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": []
            })),
        ])
        .await;
        let bus = EventBus::new(16);
        let mut events = bus.subscribe();
        let gateway = test_gateway(&api.base_url, bus.clone());
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();

        let task = tokio::spawn(async move {
            gateway
                .run_polling_loop_with(task_cancel, |_| Duration::from_millis(1))
                .await
        });

        api.wait_for_requests(3).await;
        cancel.cancel();
        task.await.unwrap().unwrap();

        let requests = api.requests();
        assert_eq!(requests[0].path, "/bottest-token/getMe");
        assert_eq!(requests[1].path, "/bottest-token/getUpdates");
        assert_eq!(requests[1].body["timeout"], 30);
        assert!(requests[1].body.get("offset").is_none());
        assert_eq!(requests[2].body["offset"], 106);

        let event_kinds = drain_event_kinds(&mut events);
        assert_eq!(
            event_kinds
                .iter()
                .filter(|event| matches!(
                    event,
                    AppEventKind::ChannelReady {
                        platform: Platform::Telegram
                    }
                ))
                .count(),
            1
        );
        assert!(event_kinds.iter().any(|event| matches!(
            event,
            AppEventKind::ChannelDisconnected {
                platform: Platform::Telegram,
                reason,
            } if reason == "shutdown"
        )));
    }

    #[tokio::test]
    async fn run_polling_loop_stops_after_max_reconnect_attempts() {
        let mut responses = vec![MockResponse::json(serde_json::json!({
            "ok": true,
            "result": { "username": "my_bot" }
        }))];
        responses.extend((0..MAX_RECONNECT_ATTEMPTS).map(|_| {
            MockResponse::json(serde_json::json!({
                "ok": false,
                "description": "Unauthorized"
            }))
        }));

        let api = MockTelegramApi::spawn(responses).await;
        let bus = EventBus::new(16);
        let mut events = bus.subscribe();
        let gateway = test_gateway(&api.base_url, bus);

        gateway
            .run_polling_loop_with(CancellationToken::new(), |_| Duration::from_millis(1))
            .await
            .unwrap();

        let event_kinds = drain_event_kinds(&mut events);
        assert!(!event_kinds.iter().any(|event| matches!(
            event,
            AppEventKind::ChannelReady {
                platform: Platform::Telegram
            }
        )));

        let disconnect_reason = event_kinds.iter().find_map(|event| match event {
            AppEventKind::ChannelDisconnected {
                platform: Platform::Telegram,
                reason,
            } => Some(reason.as_str()),
            _ => None,
        });
        assert_eq!(
            disconnect_reason,
            Some("getUpdates failed after 10 attempts: getUpdates failed: Unauthorized")
        );
        assert!(event_kinds.iter().any(|event| matches!(
            event,
            AppEventKind::Error { context, message }
            if context == "telegram"
                && message == "getUpdates failed after 10 attempts: getUpdates failed: Unauthorized"
        )));
    }

    #[tokio::test]
    async fn run_polling_loop_can_cancel_during_reconnect_sleep() {
        let api = MockTelegramApi::spawn(vec![
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": { "username": "my_bot" }
            })),
            MockResponse::json(serde_json::json!({
                "ok": false,
                "description": "temporary failure"
            })),
        ])
        .await;
        let bus = EventBus::new(16);
        let mut events = bus.subscribe();
        let gateway = test_gateway(&api.base_url, bus);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();

        let task = tokio::spawn(async move {
            gateway
                .run_polling_loop_with(task_cancel, |_| Duration::from_secs(60))
                .await
        });

        api.wait_for_requests(2).await;
        cancel.cancel();
        task.await.unwrap().unwrap();

        assert_eq!(api.requests().len(), 2);

        let event_kinds = drain_event_kinds(&mut events);
        assert!(event_kinds.iter().any(|event| matches!(
            event,
            AppEventKind::ChannelDisconnected {
                platform: Platform::Telegram,
                reason,
            } if reason == "shutdown"
        )));
        assert!(!event_kinds.iter().any(|event| matches!(
            event,
            AppEventKind::Error { context, .. } if context == "telegram"
        )));
    }
    #[tokio::test]
    async fn run_polling_loop_emits_reconnecting_on_transient_error() {
        let api = MockTelegramApi::spawn(vec![
            // getMe
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": { "username": "my_bot" }
            })),
            // First getUpdates: success — emits ChannelReady
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": []
            })),
            // Second getUpdates: error — should emit ChannelReconnecting and reset ready
            MockResponse::json(serde_json::json!({
                "ok": false,
                "description": "temporarily unavailable"
            })),
            // Third getUpdates: success after error — should re-emit ChannelReady
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": []
            })),
            // Fourth getUpdates: success — cancel here
            MockResponse::json(serde_json::json!({
                "ok": true,
                "result": []
            })),
        ])
        .await;
        let bus = EventBus::new(32);
        let mut events = bus.subscribe();
        let gateway = test_gateway(&api.base_url, bus.clone());
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();

        let task = tokio::spawn(async move {
            gateway
                .run_polling_loop_with(task_cancel, |_| Duration::from_millis(1))
                .await
        });

        api.wait_for_requests(5).await;
        cancel.cancel();
        task.await.unwrap().unwrap();

        let event_kinds = drain_event_kinds(&mut events);

        // Should have emitted ChannelReconnecting on the error
        assert!(event_kinds.iter().any(|event| matches!(
            event,
            AppEventKind::ChannelReconnecting {
                platform: Platform::Telegram,
                attempt: 1,
                ..
            }
        )));

        // Should have emitted ChannelReady twice: once on first connect, once on recovery
        let ready_count = event_kinds
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    AppEventKind::ChannelReady {
                        platform: Platform::Telegram
                    }
                )
            })
            .count();
        assert_eq!(
            ready_count, 2,
            "ChannelReady should be emitted on connect and re-emitted on recovery"
        );
    }
}
