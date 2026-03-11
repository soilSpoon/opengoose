//! Standalone helper functions for the Discord gateway.
//!
//! Free functions used by the Discord gateway event loop: message relay
//! preparation, chunked sending, slash command registration, interaction
//! handling, and ephemeral responses.

use tracing::{debug, error, info};

use twilight_http::Client as HttpClient;
use twilight_model::application::command::{CommandOption, CommandOptionType};
use twilight_model::application::interaction::application_command::CommandOptionValue;
use twilight_model::application::interaction::{Interaction, InteractionData, InteractionType};
use twilight_model::channel::message::Message;
use twilight_model::http::interaction::{
    InteractionResponse, InteractionResponseData, InteractionResponseType,
};
use twilight_model::id::Id;
use twilight_model::id::marker::ApplicationMarker;

use opengoose_core::message_utils::split_message;
use opengoose_core::{GatewayBridge, StreamResponder};
use opengoose_types::{Platform, SessionKey};

use super::DISCORD_MAX_LEN;

pub(super) fn split_discord_chunks(body: &str) -> Vec<&str> {
    split_message(body, DISCORD_MAX_LEN)
}

pub(super) fn prepare_discord_relay(
    author_is_bot: bool,
    content: &str,
    guild_id: Option<&str>,
    channel_id: &str,
    author_name: Option<&str>,
) -> Option<(SessionKey, Option<String>, String)> {
    if author_is_bot {
        return None;
    }

    let text = content.trim();
    if text.is_empty() {
        return None;
    }

    let session_key = match guild_id {
        Some(gid) => SessionKey::new(Platform::Discord, gid.to_string(), channel_id),
        None => SessionKey::direct(Platform::Discord, channel_id),
    };

    Some((
        session_key,
        author_name.map(str::to_string),
        text.to_string(),
    ))
}

/// Register the `/team` slash command globally.
pub(super) async fn register_slash_commands(
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
pub(super) async fn handle_interaction(
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
    let response_text = bridge.handle_pairing(&session_key, args);

    respond_ephemeral(http, application_id, interaction, &response_text).await;
}

/// Send an ephemeral response to an interaction (only visible to the invoking user).
pub(super) async fn respond_ephemeral(
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

pub(super) async fn handle_message(
    bridge: &GatewayBridge,
    responder: &dyn StreamResponder,
    msg: &Message,
) {
    let channel_id = msg.channel_id.to_string();
    let guild_id = msg.guild_id.as_ref().map(ToString::to_string);

    let Some((session_key, display_name, content)) = prepare_discord_relay(
        msg.author.bot,
        &msg.content,
        guild_id.as_deref(),
        &channel_id,
        Some(&msg.author.name),
    ) else {
        return;
    };

    if !bridge.is_accepting_messages() {
        info!(channel_id = %channel_id, "ignoring discord message during shutdown drain");
        return;
    }

    debug!(
        channel_id = %channel_id,
        author = %msg.author.name,
        content_len = content.len(),
        "relaying discord message to engine"
    );

    if let Err(e) = bridge
        .relay_and_drive_stream(
            &session_key,
            display_name,
            &content,
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

#[cfg(test)]
mod tests {
    use super::*;
    use opengoose_types::{Platform, SessionKey};

    // --- split_discord_chunks ---

    #[test]
    fn test_split_chunks_short_message_is_one_chunk() {
        let chunks = split_discord_chunks("hello");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "hello");
    }

    #[test]
    fn test_split_chunks_empty_string_is_one_chunk() {
        let chunks = split_discord_chunks("");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "");
    }

    #[test]
    fn test_split_chunks_exactly_at_limit_is_one_chunk() {
        let text = "a".repeat(DISCORD_MAX_LEN);
        let chunks = split_discord_chunks(&text);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn test_split_chunks_one_over_limit_produces_two_chunks() {
        let text = "a".repeat(DISCORD_MAX_LEN + 1);
        let chunks = split_discord_chunks(&text);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), DISCORD_MAX_LEN);
        assert_eq!(chunks[1].len(), 1);
    }

    #[test]
    fn test_split_chunks_content_fully_preserved() {
        let text = "hello world, this is a test!";
        let chunks = split_discord_chunks(text);
        let reconstructed = chunks.join("");
        assert_eq!(reconstructed, text);
    }

    #[test]
    fn test_split_chunks_triple_limit_produces_three_chunks() {
        let text = "a".repeat(DISCORD_MAX_LEN * 3);
        let chunks = split_discord_chunks(&text);
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_split_chunks_unicode_not_split_mid_codepoint() {
        // Emoji at boundary must not create invalid UTF-8 slices
        let mut text = "a".repeat(DISCORD_MAX_LEN - 1);
        text.push('\u{1F600}'); // 4-byte emoji
        text.push_str("trailing");
        let chunks = split_discord_chunks(&text);
        for chunk in &chunks {
            assert!(std::str::from_utf8(chunk.as_bytes()).is_ok());
        }
    }

    // --- prepare_discord_relay ---

    #[test]
    fn test_relay_skips_bot_message() {
        assert!(prepare_discord_relay(true, "hello", None, "chan", Some("bot")).is_none());
    }

    #[test]
    fn test_relay_skips_empty_content() {
        assert!(prepare_discord_relay(false, "", None, "chan", Some("alice")).is_none());
    }

    #[test]
    fn test_relay_skips_whitespace_only_content() {
        assert!(prepare_discord_relay(false, "   \t\n  ", None, "chan", Some("alice")).is_none());
    }

    #[test]
    fn test_relay_skips_bot_in_guild_context() {
        assert!(
            prepare_discord_relay(true, "hello", Some("guild-1"), "chan", Some("bot")).is_none()
        );
    }

    #[test]
    fn test_relay_trims_leading_trailing_whitespace() {
        let (_, _, content) = prepare_discord_relay(false, "  hello  ", None, "chan", Some("u"))
            .expect("should relay");
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_relay_preserves_inner_whitespace() {
        let (_, _, content) =
            prepare_discord_relay(false, "  hello   world  ", None, "chan", Some("u"))
                .expect("should relay");
        assert_eq!(content, "hello   world");
    }

    #[test]
    fn test_relay_dm_produces_no_namespace() {
        let (key, _, _) =
            prepare_discord_relay(false, "hello", None, "chan42", Some("alice")).unwrap();
        assert_eq!(key.namespace, None);
        assert_eq!(key.channel_id, "chan42");
    }

    #[test]
    fn test_relay_guild_produces_namespace() {
        let (key, _, _) =
            prepare_discord_relay(false, "hello", Some("guild-1"), "chan", Some("alice")).unwrap();
        assert_eq!(key, SessionKey::new(Platform::Discord, "guild-1", "chan"));
        assert_eq!(key.namespace, Some("guild-1".to_string()));
    }

    #[test]
    fn test_relay_no_author_name_gives_none_display_name() {
        let (_, display_name, _) =
            prepare_discord_relay(false, "hello", None, "chan", None).expect("should relay");
        assert_eq!(display_name, None);
    }

    #[test]
    fn test_relay_author_name_preserved() {
        let (_, display_name, _) =
            prepare_discord_relay(false, "hello", None, "chan", Some("bob")).expect("should relay");
        assert_eq!(display_name, Some("bob".to_string()));
    }

    #[test]
    fn test_relay_different_guilds_produce_different_keys() {
        let (key_a, _, _) =
            prepare_discord_relay(false, "hi", Some("guild-a"), "chan", Some("u")).unwrap();
        let (key_b, _, _) =
            prepare_discord_relay(false, "hi", Some("guild-b"), "chan", Some("u")).unwrap();
        assert_ne!(key_a.to_stable_id(), key_b.to_stable_id());
    }

    #[test]
    fn test_relay_long_content_not_truncated() {
        let long = "x".repeat(10_000);
        let (_, _, content) =
            prepare_discord_relay(false, &long, None, "chan", Some("alice")).expect("should relay");
        assert_eq!(content.len(), 10_000);
    }
}
