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
        let mut safe_end = max_len;
        while !remaining.is_char_boundary(safe_end) {
            safe_end -= 1;
        }
        // Try to split at last newline within the safe range
        let split_at = remaining[..safe_end]
            .rfind('\n')
            .unwrap_or(safe_end);
        chunks.push(&remaining[..split_at]);
        remaining = remaining[split_at..].trim_start_matches('\n');
    }
    chunks
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_short_message() {
        let chunks = split_message("hello world", 2000);
        assert_eq!(chunks, vec!["hello world"]);
    }

    #[test]
    fn split_at_newline() {
        let line_a = "a".repeat(15);
        let line_b = "b".repeat(10);
        let text = format!("{}\n{}", line_a, line_b);
        let chunks = split_message(&text, 20);
        assert_eq!(chunks[0], line_a.as_str());
        assert_eq!(chunks[1], line_b.as_str());
    }

    #[test]
    fn split_no_newline() {
        let text = "a".repeat(50);
        let chunks = split_message(&text, 20);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 20);
        assert_eq!(chunks[1].len(), 20);
        assert_eq!(chunks[2].len(), 10);
    }

    #[test]
    fn split_empty() {
        let chunks = split_message("", 2000);
        assert_eq!(chunks, vec![""]);
    }

    #[test]
    fn split_exact_limit() {
        let text = "x".repeat(2000);
        let chunks = split_message(&text, 2000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 2000);
    }

    #[test]
    fn split_unicode() {
        // Each emoji is 4 bytes. Build a string that must split mid-way.
        let emoji = "\u{1F600}"; // 4-byte char
        let text: String = std::iter::repeat(emoji).take(10).collect(); // 40 bytes
        let chunks = split_message(&text, 15);
        // Should not panic and every chunk should be valid UTF-8
        for chunk in &chunks {
            assert!(chunk.len() <= 15);
            // Verify it's valid UTF-8 (it is since &str guarantees it)
            assert!(!chunk.is_empty() || chunks.len() == 1);
        }
    }
}
