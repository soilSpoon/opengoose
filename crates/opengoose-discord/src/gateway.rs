use std::collections::HashMap;
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
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker};

use goose::gateway::handler::GatewayHandler;
use goose::gateway::{Gateway, OutgoingMessage, PlatformUser};
use tokio_util::sync::CancellationToken;

use opengoose_core::GatewayBridge;
use opengoose_types::{AppEventKind, EventBus, Platform, SessionKey};

/// Discord enforces a 2000-character limit per message.
const DISCORD_MAX_LEN: usize = 2000;

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
    pub fn new(
        token: impl Into<String>,
        bridge: Arc<GatewayBridge>,
        event_bus: EventBus,
    ) -> Self {
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

        let intents =
            Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT | Intents::DIRECT_MESSAGES;
        let mut shard = Shard::new(ShardId::ONE, self.token.clone(), intents);

        info!("discord gateway starting");

        // Track application_id for slash commands (set on Ready)
        let mut application_id: Option<Id<ApplicationMarker>> = None;

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
                                handle_message(&self.bridge, &self.event_bus, &self.http, &msg.0).await;
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
            // Let bridge handle persistence, pairing detection, events
            self.bridge
                .on_outgoing_message(&user.user_id, &body, "discord")
                .await;

            // Send to Discord channel
            let session_key = SessionKey::from_stable_id(&user.user_id);
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

    let engine = bridge.engine();

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

    let response_text = match name_value.as_deref() {
        None => match engine.active_team_for(&session_key) {
            Some(team) => format!("Active team for this channel: **{team}**"),
            None => "No team active for this channel.".to_string(),
        },
        Some("off") => {
            engine.clear_active_team(&session_key);
            "Team deactivated. Reverting to single-agent mode.".to_string()
        }
        Some("list") => {
            let teams = engine.list_teams();
            if teams.is_empty() {
                "No teams available. Use `opengoose team init` to install defaults.".to_string()
            } else {
                format!(
                    "Available teams:\n{}",
                    teams
                        .iter()
                        .map(|t| format!("- {t}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
        }
        Some(team_name) => {
            if engine.team_exists(team_name) {
                engine.set_active_team(&session_key, team_name.to_string());
                format!("Team **{team_name}** activated for this channel.")
            } else {
                let available = engine.list_teams();
                format!(
                    "Team `{team_name}` not found. Available teams: {}",
                    if available.is_empty() {
                        "none".to_string()
                    } else {
                        available.join(", ")
                    }
                )
            }
        }
    };

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

async fn handle_message(
    bridge: &GatewayBridge,
    event_bus: &EventBus,
    http: &HttpClient,
    msg: &Message,
) {
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

    match bridge
        .relay_message(&session_key, display_name, content)
        .await
    {
        Ok(Some(response)) => {
            // Team handled it — send response directly to Discord
            for chunk in split_message(&response, DISCORD_MAX_LEN) {
                if let Err(e) = http
                    .create_message(msg.channel_id)
                    .content(chunk)
                    .await
                {
                    error!(%e, "failed to send team response to discord");
                }
            }
        }
        Ok(None) => {
            // Goose single-agent will respond via send_message callback
        }
        Err(e) => {
            event_bus.emit(AppEventKind::Error {
                context: "relay".into(),
                message: e.to_string(),
            });
            error!(%e, "failed to relay message to goose");
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
        let mut boundary = max_len;
        while !remaining.is_char_boundary(boundary) {
            boundary -= 1;
        }
        let split_at = remaining[..boundary].rfind('\n').unwrap_or(boundary);
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
        let mut msg = "a".repeat(1999);
        msg.push('\u{1F600}');
        msg.push_str(&"b".repeat(100));
        let chunks = split_message(&msg, DISCORD_MAX_LEN);
        assert!(chunks.len() >= 2);
        for chunk in &chunks {
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
