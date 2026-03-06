use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use tracing::{error, info, warn};

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client as HttpClient;
use twilight_model::application::command::{CommandOption, CommandOptionType};
use twilight_model::application::interaction::application_command::CommandOptionValue;
use twilight_model::application::interaction::{Interaction, InteractionData, InteractionType};
use twilight_model::channel::message::Message;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
use twilight_model::id::Id;
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker, MessageMarker};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::message_utils::{split_message, truncate_for_display};
use opengoose_core::{DraftHandle, GatewayBridge, StreamResponder};
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

/// Discord enforces a 2000-character limit per message.
const DISCORD_MAX_LEN: usize = 2000;

/// Maximum number of recently-processed message IDs to keep in memory.
/// Prevents unbounded growth while covering any realistic replay window.
const SEEN_MESSAGES_CAPACITY: usize = 256;

/// Discord channel gateway implementing the goose `Gateway` trait.
///
/// Combines the old `DiscordAdapter` + `OpenGooseGateway` into a single struct.
/// Uses `GatewayBridge` for shared orchestration (team intercept, persistence, pairing).
pub struct DiscordGateway {
    token: String,
    bridge: Arc<GatewayBridge>,
    http: Arc<HttpClient>,
    event_bus: EventBus,
}

impl DiscordGateway {
    pub fn new(token: impl Into<String>, bridge: Arc<GatewayBridge>, event_bus: EventBus) -> Self {
        let token = token.into();
        let http = Arc::new(HttpClient::new(token.clone()));
        Self {
            token,
            bridge,
            http,
            event_bus,
        }
    }

    /// Send a text message to a Discord channel, splitting if needed.
    async fn send_to_channel(&self, channel_id: Id<ChannelMarker>, body: &str) {
        for chunk in split_message(body, DISCORD_MAX_LEN) {
            if let Err(e) = self.http.create_message(channel_id).content(chunk).await {
                error!(%e, "failed to send discord message");
            }
        }
    }
}

#[async_trait]
impl Gateway for DiscordGateway {
    fn gateway_type(&self) -> &str {
        "discord"
    }

    async fn start(
        &self,
        handler: GatewayHandler,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        // Register handler with bridge for team orchestration
        self.bridge.on_start(handler).await;

        let intents = Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
        let mut shard = Shard::new(ShardId::ONE, self.token.clone(), intents);

        info!("discord gateway starting");

        // Track application_id for slash commands (set on Ready)
        let mut application_id: Option<Id<ApplicationMarker>> = None;

        // Deduplication cache: tracks recently-processed message IDs to
        // prevent double-handling during Discord WebSocket reconnects/replays.
        let mut seen: HashSet<Id<MessageMarker>> = HashSet::new();
        let mut seen_order: Vec<Id<MessageMarker>> = Vec::new();

        // Discord event loop
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("discord gateway shutting down");
                    self.event_bus.emit(AppEventKind::ChannelDisconnected {
                        platform: Platform::Discord,
                        reason: "shutdown".into(),
                    });
                    break;
                }
                event = shard.next_event(EventTypeFlags::all()) => {
                    match event {
                        Some(Ok(event)) => match event {
                            Event::MessageCreate(msg) => {
                                if !seen.insert(msg.id) {
                                    warn!(msg_id = %msg.id, "duplicate MessageCreate ignored");
                                    continue;
                                }
                                seen_order.push(msg.id);
                                if seen_order.len() > SEEN_MESSAGES_CAPACITY {
                                    let evicted = seen_order.remove(0);
                                    seen.remove(&evicted);
                                }
                                handle_message(&self.bridge, self, &msg.0).await;
                            }
                            Event::Ready(ready) => {
                                let app_id = ready.application.id;
                                application_id = Some(app_id);
                                info!(?app_id, "discord bot connected");
                                self.event_bus.emit(AppEventKind::ChannelReady {
                                    platform: Platform::Discord,
                                });

                                // Register /team slash command
                                if let Err(e) = register_slash_commands(&self.http, app_id).await {
                                    error!(%e, "failed to register slash commands");
                                }
                            }
                            Event::InteractionCreate(interaction) => {
                                if let Some(app_id) = application_id {
                                    handle_interaction(
                                        &self.http,
                                        app_id,
                                        &self.bridge,
                                        &interaction.0,
                                    )
                                    .await;
                                }
                            }
                            _ => {}
                        },
                        Some(Err(e)) => {
                            warn!(%e, "discord gateway error, twilight will auto-reconnect");
                        }
                        None => {
                            error!("discord shard closed -- check bot token and privileged intents");
                            let reason = "Discord connection closed. Verify your bot token and that MESSAGE_CONTENT intent is enabled in the Developer Portal.".to_string();
                            self.event_bus.emit(AppEventKind::ChannelDisconnected {
                                platform: Platform::Discord,
                                reason: reason.clone(),
                            });
                            self.event_bus.emit(AppEventKind::Error {
                                context: "discord".into(),
                                message: reason,
                            });
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn send_message(
        &self,
        user: &PlatformUser,
        message: OutgoingMessage,
    ) -> anyhow::Result<()> {
        if let OutgoingMessage::Text { body } = message {
            // Bridge handles persistence, pairing detection, events and returns the session key
            let session_key = self
                .bridge
                .on_outgoing_message(&user.user_id, &body, "discord")
                .await;

            let channel_id = match session_key.channel_id.parse::<u64>() {
                Ok(id) => Id::<ChannelMarker>::new(id),
                Err(_) => {
                    warn!(channel_id = %session_key.channel_id, "invalid channel id");
                    return Ok(());
                }
            };

            self.send_to_channel(channel_id, &body).await;
        } else {
            tracing::debug!("typing indicator for {}", user.user_id);
        }
        Ok(())
    }

    async fn validate_config(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn info(&self) -> HashMap<String, String> {
        HashMap::from([("type".into(), "discord".into())])
    }
}

#[async_trait]
impl StreamResponder for DiscordGateway {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_message_len(&self) -> usize {
        DISCORD_MAX_LEN
    }

    async fn create_draft(&self, channel_id: &str) -> anyhow::Result<DraftHandle> {
        let ch_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        let msg = self
            .http
            .create_message(ch_id)
            .content("Thinking...")
            .await?
            .model()
            .await?;
        Ok(DraftHandle {
            message_id: msg.id.to_string(),
            channel_id: channel_id.to_string(),
        })
    }

    async fn update_draft(&self, handle: &DraftHandle, content: &str) -> anyhow::Result<()> {
        let ch_id = Id::<ChannelMarker>::new(handle.channel_id.parse()?);
        let msg_id = Id::new(handle.message_id.parse()?);
        let display = truncate_for_display(content, DISCORD_MAX_LEN);
        self.http
            .update_message(ch_id, msg_id)
            .content(Some(display))
            .await?;
        Ok(())
    }

    async fn send_new_message(&self, channel_id: &str, content: &str) -> anyhow::Result<()> {
        let ch_id = Id::<ChannelMarker>::new(channel_id.parse()?);
        self.http.create_message(ch_id).content(content).await?;
        Ok(())
    }

    // finalize_draft uses the default implementation from StreamResponder
}

/// Register the `/team` slash command globally.
async fn register_slash_commands(
    http: &HttpClient,
    application_id: Id<ApplicationMarker>,
) -> anyhow::Result<()> {
    let name_option = CommandOption {
        autocomplete: None,
        channel_types: None,
        choices: None,
        description: "Team name (omit to show current, 'off' to deactivate)".into(),
        description_localizations: None,
        kind: CommandOptionType::String,
        max_length: None,
        max_value: None,
        min_length: None,
        min_value: None,
        name: "name".into(),
        name_localizations: None,
        options: None,
        required: None,
    };

    http.interaction(application_id)
        .create_global_command()
        .chat_input("team", "Activate or deactivate a team for this channel")
        .command_options(&[name_option])
        .await?;

    info!("registered /team slash command");
    Ok(())
}

/// Handle an incoming interaction (slash command).
async fn handle_interaction(
    http: &HttpClient,
    application_id: Id<ApplicationMarker>,
    bridge: &GatewayBridge,
    interaction: &Interaction,
) {
    if interaction.kind != InteractionType::ApplicationCommand {
        return;
    }

    let Some(InteractionData::ApplicationCommand(ref cmd_data)) = interaction.data else {
        return;
    };

    if cmd_data.name != "team" {
        return;
    }

    let channel_id = interaction.channel.as_ref().map(|c| c.id.to_string());

    let Some(channel_id_str) = channel_id else {
        respond_ephemeral(
            http,
            application_id,
            interaction,
            "Could not determine channel.",
        )
        .await;
        return;
    };

    let session_key = match interaction.guild_id {
        Some(gid) => SessionKey::new(Platform::Discord, gid.to_string(), &channel_id_str),
        None => SessionKey::direct(Platform::Discord, &channel_id_str),
    };

    // Parse the "name" option
    let name_value = cmd_data
        .options
        .iter()
        .find(|o| o.name == "name")
        .and_then(|o| {
            if let CommandOptionValue::String(ref s) = o.value {
                Some(s.clone())
            } else {
                None
            }
        });

    let args = name_value.as_deref().unwrap_or("");
    let response_text = bridge.engine().handle_team_command(&session_key, args);

    respond_ephemeral(http, application_id, interaction, &response_text).await;
}

/// Send an ephemeral response to an interaction (only visible to the invoking user).
async fn respond_ephemeral(
    http: &HttpClient,
    application_id: Id<ApplicationMarker>,
    interaction: &Interaction,
    content: &str,
) {
    use twilight_model::channel::message::MessageFlags;

    let response = InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(content.to_string()),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    };

    if let Err(e) = http
        .interaction(application_id)
        .create_response(interaction.id, &interaction.token, &response)
        .await
    {
        error!(%e, "failed to respond to interaction");
    }
}

async fn handle_message(bridge: &GatewayBridge, responder: &dyn StreamResponder, msg: &Message) {
    if msg.author.bot {
        return;
    }

    let content = msg.content.trim();
    if content.is_empty() {
        return;
    }

    let channel_id = msg.channel_id.to_string();
    let guild_id = msg.guild_id.map(|id| id.to_string());

    let session_key = match guild_id {
        Some(gid) => SessionKey::new(Platform::Discord, gid, &channel_id),
        None => SessionKey::direct(Platform::Discord, &channel_id),
    };

    let display_name = Some(msg.author.name.clone());

    if let Err(e) = bridge
        .relay_and_drive_stream(
            &session_key,
            display_name,
            content,
            responder,
            &channel_id,
            opengoose_core::ThrottlePolicy::discord(),
            DISCORD_MAX_LEN,
        )
        .await
    {
        // Error event is emitted by bridge; just log here
        error!(%e, "failed to relay message to goose");
    }
}

// split_message tests are in opengoose_core::message_utils (the canonical location).
