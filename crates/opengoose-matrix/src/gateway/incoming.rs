use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use opengoose_core::{StreamResponder, ThrottlePolicy};
use opengoose_types::{AppEventKind, Platform, SessionKey};

use crate::types::{RoomEvent, SyncResponse};

use super::{MATRIX_MAX_LEN, MAX_RECONNECT_ATTEMPTS, MatrixGateway};

pub(super) struct IncomingTextEvent<'a> {
    pub(super) room_id: &'a str,
    pub(super) sender: &'a str,
    pub(super) body: &'a str,
}

impl MatrixGateway {
    /// Build the SessionKey for a Matrix room.
    ///
    /// Namespace = server name extracted from the bot's user_id (e.g. `example.com`).
    /// Channel ID = room_id (e.g. `!room:example.com`).
    pub(super) fn session_key(server_name: &str, room_id: &str) -> SessionKey {
        SessionKey::new(Platform::Custom("matrix".to_string()), server_name, room_id)
    }

    /// Extract the server name from a Matrix user_id (`@user:server.com` → `server.com`).
    pub(super) fn server_name_from_user_id(user_id: &str) -> &str {
        user_id
            .split_once(':')
            .map(|(_, server)| server)
            .filter(|server| !server.is_empty())
            .unwrap_or("matrix.org")
    }

    /// Handle the `!team` bot command and reply in the room.
    async fn handle_team_command(&self, session_key: &SessionKey, room_id: &str, args: &str) {
        let response = self.bridge.handle_pairing(session_key, args);
        if let Err(e) = self.post_message(room_id, &response).await {
            error!(%e, "failed to reply to !team command");
        }
    }

    /// Run the /sync loop until cancelled.
    pub(super) async fn run_sync_loop(
        &self,
        cancel: &CancellationToken,
        bot_user_id: &str,
        filter_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let server_name = Self::server_name_from_user_id(bot_user_id);
        let mut next_batch: Option<String> = None;
        let mut reconnect_attempts: u32 = 0;

        loop {
            if cancel.is_cancelled() {
                break;
            }

            let result = self.sync(next_batch.as_deref(), filter_id).await;

            match result {
                Ok(sync_resp) => {
                    if reconnect_attempts > 0 {
                        info!("matrix gateway reconnected");
                        self.metrics.set_connected("matrix");
                        self.event_bus.emit(AppEventKind::ChannelReady {
                            platform: Platform::Custom("matrix".to_string()),
                        });
                    }
                    reconnect_attempts = 0;
                    let batch = sync_resp.next_batch.clone();
                    self.process_sync_response(sync_resp, bot_user_id, server_name)
                        .await;
                    next_batch = Some(batch);
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    if reconnect_should_give_up(reconnect_attempts) {
                        error!(%e, "matrix sync loop giving up after max reconnect attempts");
                        return Err(e);
                    }

                    let delay = reconnect_delay(reconnect_attempts);
                    let delay_secs = delay.as_secs();
                    warn!(%e, attempt = reconnect_attempts, ?delay, "matrix /sync error, retrying...");
                    self.metrics.record_reconnect("matrix", Some(e.to_string()));
                    self.event_bus.emit(AppEventKind::ChannelReconnecting {
                        platform: Platform::Custom("matrix".to_string()),
                        attempt: reconnect_attempts,
                        delay_secs,
                    });
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }

        Ok(())
    }

    async fn process_sync_response(
        &self,
        sync_resp: SyncResponse,
        bot_user_id: &str,
        server_name: &str,
    ) {
        if let Some(rooms) = sync_resp.rooms
            && let Some(joined) = rooms.join
        {
            for (room_id, room) in joined {
                let Some(timeline) = room.timeline else {
                    continue;
                };
                let Some(events) = timeline.events else {
                    continue;
                };

                for event in events {
                    let Some(message) = parse_room_message(&room_id, &event, bot_user_id) else {
                        continue;
                    };
                    self.process_incoming_message(server_name, message).await;
                }
            }
        }
    }

    async fn process_incoming_message(&self, server_name: &str, message: IncomingTextEvent<'_>) {
        let session_key = Self::session_key(server_name, message.room_id);
        let display_name = Some(message.sender.to_string());

        debug!(
            room_id = %message.room_id,
            sender = %message.sender,
            body_len = message.body.len(),
            "processing matrix room message"
        );

        if let Some(args) = message.body.strip_prefix("!team") {
            self.handle_team_command(&session_key, message.room_id, args.trim())
                .await;
            return;
        }

        if !self.bridge.is_accepting_messages() {
            info!(
                room_id = %message.room_id,
                "ignoring matrix message during shutdown drain"
            );
            return;
        }

        if let Err(e) = self
            .bridge
            .relay_and_drive_stream(
                &session_key,
                display_name,
                message.body,
                self as &dyn StreamResponder,
                message.room_id,
                ThrottlePolicy::matrix(),
                MATRIX_MAX_LEN,
            )
            .await
        {
            error!(%e, "failed to relay matrix message");
        }
    }
}

pub(super) fn parse_room_message<'a>(
    room_id: &'a str,
    event: &'a RoomEvent,
    bot_user_id: &str,
) -> Option<IncomingTextEvent<'a>> {
    if !should_process_event(
        &event.event_type,
        &event.sender,
        bot_user_id,
        &event.content,
    ) {
        return None;
    }

    let body = extract_message_body(&event.content)?;
    Some(IncomingTextEvent {
        room_id,
        sender: &event.sender,
        body,
    })
}

pub(super) fn should_process_event(
    event_type: &str,
    sender: &str,
    bot_user_id: &str,
    content: &serde_json::Value,
) -> bool {
    if event_type != "m.room.message" {
        return false;
    }
    if sender == bot_user_id {
        return false;
    }
    if content.get("msgtype").and_then(|value| value.as_str()) != Some("m.text") {
        return false;
    }
    if content
        .get("m.relates_to")
        .and_then(|value| value.get("rel_type"))
        .and_then(|value| value.as_str())
        == Some("m.replace")
    {
        return false;
    }
    true
}

pub(super) fn extract_message_body(content: &serde_json::Value) -> Option<&str> {
    let body = content.get("body")?.as_str()?.trim();
    if body.is_empty() { None } else { Some(body) }
}

pub(super) fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(2u64.pow(attempt.min(5)))
}

pub(super) fn reconnect_should_give_up(attempt: u32) -> bool {
    attempt >= MAX_RECONNECT_ATTEMPTS
}
