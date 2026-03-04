use std::sync::Arc;

use anyhow::Result;
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
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker};
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
                    let channel_id = match session_key.channel_id.parse::<u64>() {
                        Ok(id) => Id::<ChannelMarker>::new(id),
                        Err(_) => {
                            warn!(channel_id = %session_key.channel_id, "invalid channel id");
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

        // Track application_id for slash commands (set on Ready)
        let mut application_id: Option<Id<ApplicationMarker>> = None;

        // Discord event loop
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
                                handle_message(&gateway, &event_bus, &msg.0).await;
                            }
                            Event::Ready(ready) => {
                                let app_id = ready.application.id;
                                application_id = Some(app_id);
                                info!(?app_id, "discord bot connected");
                                event_bus.emit(AppEventKind::DiscordReady);

                                // Register /team slash command
                                if let Err(e) = register_slash_commands(&http, app_id).await {
                                    error!(%e, "failed to register slash commands");
                                }
                            }
                            Event::InteractionCreate(interaction) => {
                                if let Some(app_id) = application_id {
                                    handle_interaction(
                                        &http,
                                        app_id,
                                        &gateway,
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
                            // Stream exhausted -- shard is permanently closed
                            // (invalid token, missing intents, etc.)
                            error!("discord shard closed -- check bot token and privileged intents");
                            event_bus.emit(AppEventKind::Error {
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
}

/// Register the `/team` slash command globally.
async fn register_slash_commands(
    http: &HttpClient,
    application_id: Id<ApplicationMarker>,
) -> Result<()> {
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
    gateway: &OpenGooseGateway,
    interaction: &Interaction,
) {
    // Only handle application commands
    if interaction.kind != InteractionType::ApplicationCommand {
        return;
    }

    let Some(InteractionData::ApplicationCommand(ref cmd_data)) = interaction.data else {
        return;
    };

    if cmd_data.name != "team" {
        return;
    }

    // Build session key from channel
    let channel_id = interaction
        .channel
        .as_ref()
        .map(|c| c.id.to_string());

    let Some(channel_id_str) = channel_id else {
        respond_ephemeral(http, application_id, interaction, "Could not determine channel.").await;
        return;
    };

    let session_key = match interaction.guild_id {
        Some(gid) => SessionKey::new(gid.to_string(), &channel_id_str),
        None => SessionKey::direct(&channel_id_str),
    };

    let engine = gateway.engine();

    // Parse the "name" option
    let name_value = cmd_data.options.iter().find(|o| o.name == "name").and_then(|o| {
        if let CommandOptionValue::String(ref s) = o.value {
            Some(s.clone())
        } else {
            None
        }
    });

    let response_text = match name_value.as_deref() {
        None => {
            // No argument: show current team status
            match engine.active_team_for(&session_key) {
                Some(team) => format!("Active team for this channel: **{team}**"),
                None => "No team active for this channel.".to_string(),
            }
        }
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
                    teams.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n")
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

    let channel_id = msg.channel_id.to_string();
    let guild_id = msg.guild_id.map(|id| id.to_string());

    let session_key = match guild_id {
        Some(gid) => SessionKey::new(gid, &channel_id),
        None => SessionKey::direct(&channel_id),
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
