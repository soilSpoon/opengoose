use std::time::Duration;

use tracing::{error, info, warn};

use opengoose_types::{AppEventKind, Platform};
use tokio_util::sync::CancellationToken;

use super::{BotInfo, TelegramGateway, TelegramResponse, Update};

impl TelegramGateway {
    pub(crate) async fn run_polling_loop(&self, cancel: CancellationToken) -> anyhow::Result<()> {
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

                            let delay = Self::reconnect_delay(reconnect_attempts);
                            warn!(%e, attempt = reconnect_attempts, ?delay, "telegram getUpdates error, retrying...");
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
