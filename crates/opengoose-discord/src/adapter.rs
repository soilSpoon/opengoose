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
    response_rx: tokio::sync::mpsc::UnboundedReceiver<(SessionKey, String)>,
    http: Arc<HttpClient>,
    event_bus: EventBus,
}

impl DiscordAdapter {
    pub fn new(
        token: String,
        gateway: Arc<OpenGooseGateway>,
        response_rx: tokio::sync::mpsc::UnboundedReceiver<(SessionKey, String)>,
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

        // Spawn response-sending loop in a separate task
        let response_handle = tokio::spawn({
            let http = http.clone();
            let mut rx = response_rx;

            async move {
                while let Some((session_key, body)) = rx.recv().await {
                    let channel_id = match session_key.thread_id.parse::<u64>() {
                        Ok(id) => Id::<ChannelMarker>::new(id),
                        Err(_) => {
                            warn!(thread_id = %session_key.thread_id, "invalid channel id");
                            continue;
                        }
                    };
                    for chunk in split_message(&body, DISCORD_MAX_LEN) {
                        if let Err(e) = http
                            .create_message(channel_id)
                            .content(chunk)
                            .await
                        {
                            error!(%e, "failed to send discord message");
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

        response_handle.abort();
        Ok(())
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
        // Try to split at last newline within limit
        let split_at = remaining[..max_len]
            .rfind('\n')
            .unwrap_or_else(|| {
                // Find last char boundary at or before max_len
                let mut i = max_len;
                while !remaining.is_char_boundary(i) {
                    i -= 1;
                }
                i
            });
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
