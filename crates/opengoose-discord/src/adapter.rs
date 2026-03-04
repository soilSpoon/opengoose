use std::sync::Arc;

use anyhow::Result;
use tracing::{error, info, warn};

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client as HttpClient;
use twilight_model::channel::message::Message;
use twilight_model::id::marker::ChannelMarker;
use twilight_model::id::Id;

use opengoose_core::OpenGooseGateway;
use opengoose_types::{AppEventKind, EventBus, SessionKey};

/// Discord enforces a 2000-character limit per message.
const DISCORD_MAX_LEN: usize = 2000;

pub struct DiscordAdapter {
    token: String,
    gateway: Arc<OpenGooseGateway>,
    response_rx: tokio::sync::mpsc::Receiver<(SessionKey, String)>,
    http: Arc<HttpClient>,
    event_bus: EventBus,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        gateway: Arc<OpenGooseGateway>,
        response_rx: tokio::sync::mpsc::Receiver<(SessionKey, String)>,
        event_bus: EventBus,
    ) -> Self {
        let http = Arc::new(HttpClient::new(token.clone()));
        Self {
            token,
            gateway,
            response_rx,
            http,
            event_bus,
        }
    }

    pub async fn run(self, cancel: tokio_util::sync::CancellationToken) -> Result<()> {
        let Self {
            token,
            gateway,
            response_rx,
            http,
            event_bus,
        } = self;

        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
        let mut shard = Shard::new(ShardId::ONE, token, intents);

        info!("discord adapter starting");

        let cancel_clone = cancel.clone();

        // Spawn response-sending loop in a separate task with graceful drain
        let response_handle = tokio::spawn({
            let http = http.clone();
            let mut rx = response_rx;
            let cancel_resp = cancel.clone();

            async move {
                loop {
                    tokio::select! {
                        _ = cancel_resp.cancelled() => {
                            // Drain remaining messages with a 5-second deadline
                            let deadline = tokio::time::sleep(std::time::Duration::from_secs(5));
                            tokio::pin!(deadline);
                            loop {
                                tokio::select! {
                                    biased;
                                    _ = &mut deadline => {
                                        warn!("response drain deadline exceeded, dropping remaining");
                                        break;
                                    }
                                    msg = rx.recv() => {
                                        match msg {
                                            Some((session_key, body)) => {
                                                send_response(&http, &session_key, &body).await;
                                            }
                                            None => break,
                                        }
                                    }
                                }
                            }
                            break;
                        }
                        msg = rx.recv() => {
                            match msg {
                                Some((session_key, body)) => {
                                    send_response(&http, &session_key, &body).await;
                                }
                                None => break,
                            }
                        }
                    }
                }
            }
        });

        // Discord event loop
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    info!("discord adapter shutting down");
                    event_bus.emit(AppEventKind::DiscordDisconnected {
                        reason: "shutdown".into(),
                    });
                    break;
                }
                event = shard.next_event(EventTypeFlags::all()) => {
                    match event {
                        Some(Ok(event)) => match event {
                            Event::MessageCreate(msg) => {
                                handle_message(&gateway, &event_bus, &msg.0).await;
                            }
                            Event::Ready(_) => {
                                info!("discord bot connected");
                                event_bus.emit(AppEventKind::DiscordReady);
                            }
                            _ => {}
                        },
                        Some(Err(e)) => {
                            warn!(%e, "discord gateway error, twilight will auto-reconnect");
                        }
                        None => {
                            // Stream exhausted -- shard is permanently closed
                            // (invalid token, missing intents, etc.)
                            error!("discord shard closed -- check bot token and privileged intents");
                            let reason = "Discord connection closed. Verify your bot token and that MESSAGE_CONTENT intent is enabled in the Developer Portal.".to_string();
                            event_bus.emit(AppEventKind::DiscordDisconnected {
                                reason: reason.clone(),
                            });
                            event_bus.emit(AppEventKind::Error {
                                context: "discord".into(),
                                message: reason,
                            });
                            break;
                        }
                    }
                }
            }
        }

        // Wait for the response task to drain and exit gracefully
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            response_handle,
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => warn!(%e, "response task panicked"),
            Err(_) => warn!("response task did not finish within timeout"),
        }
        Ok(())
    }
}

async fn send_response(http: &HttpClient, session_key: &SessionKey, body: &str) {
    let channel_id = match session_key.thread_id.parse::<u64>() {
        Ok(id) => Id::<ChannelMarker>::new(id),
        Err(_) => {
            warn!(thread_id = %session_key.thread_id, "invalid channel id");
            return;
        }
    };
    for chunk in split_message(body, DISCORD_MAX_LEN) {
        if let Err(e) = http.create_message(channel_id).content(chunk).await {
            error!(%e, "failed to send discord message");
        }
    }
}

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining);
            break;
        }
        // Find last char boundary at or before max_len
        let mut boundary = max_len;
        while !remaining.is_char_boundary(boundary) {
            boundary -= 1;
        }
        // Try to split at last newline within that safe boundary
        let split_at = remaining[..boundary]
            .rfind('\n')
            .unwrap_or(boundary);
        chunks.push(&remaining[..split_at]);
        remaining = remaining[split_at..].trim_start_matches('\n');
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_short_message() {
        let chunks = split_message("hello", DISCORD_MAX_LEN);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_exact_boundary() {
        let msg = "a".repeat(DISCORD_MAX_LEN);
        let chunks = split_message(&msg, DISCORD_MAX_LEN);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), DISCORD_MAX_LEN);
    }

    #[test]
    fn test_split_at_newline() {
        let mut msg = "a".repeat(1900);
        msg.push('\n');
        msg.push_str(&"b".repeat(600));
        let chunks = split_message(&msg, DISCORD_MAX_LEN);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 1900);
        assert_eq!(chunks[1], "b".repeat(600));
    }

    #[test]
    fn test_split_no_newline() {
        let msg = "a".repeat(2500);
        let chunks = split_message(&msg, DISCORD_MAX_LEN);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), DISCORD_MAX_LEN);
        assert_eq!(chunks[1].len(), 500);
    }

    #[test]
    fn test_split_utf8_safety() {
        // 4-byte emoji near boundary
        let mut msg = "a".repeat(1999);
        msg.push('\u{1F600}'); // 4-byte emoji
        msg.push_str(&"b".repeat(100));
        let chunks = split_message(&msg, DISCORD_MAX_LEN);
        // Should not panic and each chunk should be valid UTF-8
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
            // Verify valid UTF-8 by accessing as str
            assert!(!chunk.is_empty() || msg.is_empty());
        }
    }

    #[test]
    fn test_split_multiple_chunks() {
        let msg = "a".repeat(5000);
        let chunks = split_message(&msg, DISCORD_MAX_LEN);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), DISCORD_MAX_LEN);
        assert_eq!(chunks[1].len(), DISCORD_MAX_LEN);
        assert_eq!(chunks[2].len(), 1000);
    }

    #[test]
    fn test_split_empty_string() {
        let chunks = split_message("", DISCORD_MAX_LEN);
        assert_eq!(chunks, vec![""]);
    }
}

async fn handle_message(
    gateway: &OpenGooseGateway,
    event_bus: &EventBus,
    msg: &Message,
) {
    if msg.author.bot {
        return;
    }

    let content = msg.content.trim();
    if content.is_empty() {
        return;
    }

    let thread_id = msg.channel_id.to_string();
    let guild_id = msg.guild_id.map(|id| id.to_string());

    let session_key = match guild_id {
        Some(gid) => SessionKey::new(gid, &thread_id),
        None => SessionKey::dm(&thread_id),
    };

    let display_name = Some(msg.author.name.clone());

    if let Err(e) = gateway
        .relay_message(&session_key, display_name, content)
        .await
    {
        event_bus.emit(AppEventKind::Error {
            context: "relay".into(),
            message: e.to_string(),
        });
        error!(%e, "failed to relay message to goose");
    }
}
