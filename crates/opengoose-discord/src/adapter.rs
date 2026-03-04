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

pub struct DiscordAdapter {
    token: String,
    gateway: Arc<OpenGooseGateway>,
    response_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<(SessionKey, String)>>,
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
            response_rx: tokio::sync::Mutex::new(response_rx),
            http,
            event_bus,
        }
    }

    pub async fn run(&self, cancel: tokio_util::sync::CancellationToken) -> Result<()> {
        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
        let mut shard = Shard::new(ShardId::ONE, self.token.clone(), intents);

        info!("discord adapter starting");

        // 응답 전달 태스크
        let http = self.http.clone();
        let mut response_rx = self.response_rx.lock().await;
        let cancel_clone = cancel.clone();

        // response 수신 루프를 별도 태스크로
        let response_handle = tokio::spawn({
            let http = http.clone();
            // mpsc receiver를 옮기기 위해 take
            let mut rx = tokio::sync::mpsc::unbounded_channel::<(SessionKey, String)>().1;
            std::mem::swap(&mut *response_rx, &mut rx);
            drop(response_rx);

            async move {
                while let Some((session_key, body)) = rx.recv().await {
                    let channel_id = match session_key.thread_id.parse::<u64>() {
                        Ok(id) => Id::<ChannelMarker>::new(id),
                        Err(_) => {
                            warn!(thread_id = %session_key.thread_id, "invalid channel id");
                            continue;
                        }
                    };
                    if let Err(e) = http
                        .create_message(channel_id)
                        .content(&body)
                        .await
                    {
                        error!(%e, "failed to send discord message");
                    }
                }
            }
        });

        // Discord 이벤트 루프
        loop {
            tokio::select! {
                _ = cancel_clone.cancelled() => {
                    info!("discord adapter shutting down");
                    break;
                }
                event = shard.next_event(EventTypeFlags::all()) => {
                    match event {
                        Some(Ok(event)) => match event {
                            Event::MessageCreate(msg) => {
                                self.handle_message(&msg.0).await;
                            }
                            Event::Ready(_) => {
                                info!("discord bot connected");
                                self.event_bus.emit(AppEventKind::DiscordReady);
                            }
                            _ => {}
                        },
                        Some(Err(e)) => {
                            warn!(%e, "discord gateway error, twilight will auto-reconnect");
                        }
                        None => {
                            // Stream exhausted — shard is permanently closed
                            // (invalid token, missing intents, etc.)
                            error!("discord shard closed — check bot token and privileged intents");
                            self.event_bus.emit(AppEventKind::Error {
                                context: "discord".into(),
                                message: "Discord connection closed. Verify your bot token and that MESSAGE_CONTENT intent is enabled in the Developer Portal.".into(),
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

    async fn handle_message(&self, msg: &Message) {
        // 봇 메시지 무시
        if msg.author.bot {
            return;
        }

        let content = msg.content.trim();
        if content.is_empty() {
            return;
        }

        // thread_id = 채널 ID (스레드이면 스레드 ID, 아니면 채널 ID)
        let thread_id = msg.channel_id.to_string();
        let guild_id = msg.guild_id.map(|id| id.to_string());

        let session_key = match guild_id {
            Some(gid) => SessionKey::new(gid, &thread_id),
            None => SessionKey::dm(&thread_id),
        };

        let display_name = Some(msg.author.name.clone());

        if let Err(e) = self
            .gateway
            .relay_message(&session_key, display_name, content)
            .await
        {
            self.event_bus.emit(AppEventKind::Error {
                context: "relay".into(),
                message: e.to_string(),
            });
            error!(%e, "failed to relay message to goose");
        }
    }
}
