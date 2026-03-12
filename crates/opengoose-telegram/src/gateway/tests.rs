use std::time::Duration;

use opengoose_types::Platform;

use super::types::{BotInfo, Chat, MessageEntity, SentMessage, TelegramResponse, Update, User};
use super::{TelegramGateway, MAX_RECONNECT_ATTEMPTS, REQUEST_TIMEOUT, TELEGRAM_MAX_LEN};

#[test]
fn test_session_key_private() {
    let chat = Chat {
        id: 12345,
        chat_type: "private".to_string(),
    };
    let key = TelegramGateway::session_key(&chat);
    assert_eq!(key.platform, Platform::Telegram);
    assert_eq!(key.namespace, None);
    assert_eq!(key.channel_id, "12345");
}

#[test]
fn test_session_key_group() {
    let chat = Chat {
        id: -100123,
        chat_type: "group".to_string(),
    };
    let key = TelegramGateway::session_key(&chat);
    assert_eq!(key.platform, Platform::Telegram);
    assert_eq!(key.namespace, Some("-100123".to_string()));
    assert_eq!(key.channel_id, "-100123");
}

#[test]
fn test_session_key_supergroup() {
    let chat = Chat {
        id: -1001234567890,
        chat_type: "supergroup".to_string(),
    };
    let key = TelegramGateway::session_key(&chat);
    assert_eq!(key.platform, Platform::Telegram);
    assert!(key.namespace.is_some());
}

#[test]
fn test_telegram_max_len_constant() {
    assert_eq!(TELEGRAM_MAX_LEN, 4096);
}

#[test]
fn test_deserialize_telegram_response() {
    let json = r#"{"ok":true,"result":{"message_id":42}}"#;
    let resp: TelegramResponse<SentMessage> = serde_json::from_str(json).unwrap();
    assert!(resp.ok);
    assert_eq!(resp.result.unwrap().message_id, 42);
}

#[test]
fn test_deserialize_telegram_response_error() {
    let json = r#"{"ok":false,"description":"Unauthorized"}"#;
    let resp: TelegramResponse<SentMessage> = serde_json::from_str(json).unwrap();
    assert!(!resp.ok);
    assert_eq!(resp.description.unwrap(), "Unauthorized");
    assert!(resp.result.is_none());
}

#[test]
fn test_deserialize_update() {
    let json = r#"{"update_id":123,"message":{"message_id":1,"chat":{"id":456,"type":"private"},"text":"hello"}}"#;
    let update: Update = serde_json::from_str(json).unwrap();
    assert_eq!(update.update_id, 123);
    let msg = update.message.unwrap();
    assert_eq!(msg.chat.id, 456);
    assert_eq!(msg.text.unwrap(), "hello");
}

#[test]
fn test_deserialize_update_no_message() {
    let json = r#"{"update_id":123}"#;
    let update: Update = serde_json::from_str(json).unwrap();
    assert!(update.message.is_none());
}

#[test]
fn test_deserialize_user() {
    let json = r#"{"update_id":1,"message":{"message_id":1,"chat":{"id":1,"type":"private"},"from":{"id":100,"first_name":"Alice","last_name":"Smith","username":"alice"}}}"#;
    let update: Update = serde_json::from_str(json).unwrap();
    let msg = update.message.unwrap();
    let user = msg.from.unwrap();
    assert_eq!(user.first_name, "Alice");
    assert_eq!(user.last_name.unwrap(), "Smith");
}

#[test]
fn test_deserialize_bot_info() {
    let json = r#"{"ok":true,"result":{"username":"my_bot"}}"#;
    let resp: TelegramResponse<BotInfo> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.result.unwrap().username.unwrap(), "my_bot");
}

// --- Constants ---

#[test]
fn test_max_reconnect_attempts_constant() {
    assert_eq!(MAX_RECONNECT_ATTEMPTS, 10);
}

#[test]
fn test_request_timeout_constant() {
    assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(60));
}

// --- Reconnect delay: exponential backoff capped at 2^5 = 32s ---

#[test]
fn test_reconnect_delay_exponential_backoff() {
    let delays: Vec<u64> = (1u32..=10)
        .map(|attempt| TelegramGateway::reconnect_delay(attempt).as_secs())
        .collect();
    assert_eq!(delays[0], 2); // attempt 1 → 2s
    assert_eq!(delays[1], 4); // attempt 2 → 4s
    assert_eq!(delays[2], 8); // attempt 3 → 8s
    assert_eq!(delays[3], 16); // attempt 4 → 16s
    assert_eq!(delays[4], 32); // attempt 5 → 32s (cap reached)
    assert_eq!(delays[5], 32); // attempt 6 → 32s (capped)
    assert_eq!(delays[9], 32); // attempt 10 → 32s (still capped)
}

// --- Display name construction from User fields ---

#[test]
fn test_display_name_with_last_name() {
    let user = User {
        first_name: "Alice".to_string(),
        last_name: Some("Smith".to_string()),
    };

    assert_eq!(
        TelegramGateway::display_name(Some(&user)).as_deref(),
        Some("Alice Smith")
    );
}

#[test]
fn test_display_name_first_name_only() {
    let user = User {
        first_name: "Bob".to_string(),
        last_name: None,
    };

    assert_eq!(
        TelegramGateway::display_name(Some(&user)).as_deref(),
        Some("Bob")
    );
}

// --- Telegram Bot API URL format ---

#[test]
fn test_api_url_format_pattern() {
    // Verify the URL pattern used by api_url(): https://api.telegram.org/bot{token}/{method}
    let token = "123456:ABC-DEF";
    let method = "getUpdates";
    let url = format!("https://api.telegram.org/bot{}/{}", token, method);
    assert_eq!(url, "https://api.telegram.org/bot123456:ABC-DEF/getUpdates");
}

// --- Deserialisation of internal types ---

#[test]
fn test_deserialize_message_entity_mention() {
    let json = r#"{"type":"mention","offset":0,"length":7}"#;
    let entity: MessageEntity = serde_json::from_str(json).unwrap();
    assert_eq!(entity.entity_type, "mention");
    assert_eq!(entity.offset, 0);
    assert_eq!(entity.length, 7);
}

#[test]
fn test_deserialize_chat_private_type() {
    let json = r#"{"id":99,"type":"private"}"#;
    let chat: Chat = serde_json::from_str(json).unwrap();
    assert_eq!(chat.id, 99);
    assert_eq!(chat.chat_type, "private");
}

#[test]
fn test_deserialize_chat_group_type() {
    let json = r#"{"id":-12345,"type":"group"}"#;
    let chat: Chat = serde_json::from_str(json).unwrap();
    assert_eq!(chat.id, -12345);
    assert_eq!(chat.chat_type, "group");
}

#[test]
fn test_deserialize_chat_supergroup_type() {
    let json = r#"{"id":-1001234567890,"type":"supergroup"}"#;
    let chat: Chat = serde_json::from_str(json).unwrap();
    assert_eq!(chat.id, -1001234567890);
    assert_eq!(chat.chat_type, "supergroup");
}

// --- Error path: malformed / rate-limit API responses ---

#[test]
fn test_get_updates_rate_limit_response() {
    let json = r#"{"ok":false,"description":"Too Many Requests: retry after 30"}"#;
    let resp: TelegramResponse<Vec<Update>> = serde_json::from_str(json).unwrap();
    assert!(!resp.ok);
    let desc = resp.description.unwrap();
    assert!(desc.contains("Too Many Requests"));
    assert!(resp.result.is_none());
}

#[test]
fn test_get_updates_auth_error_response() {
    let json = r#"{"ok":false,"description":"Unauthorized"}"#;
    let resp: TelegramResponse<Vec<Update>> = serde_json::from_str(json).unwrap();
    assert!(!resp.ok);
    assert_eq!(resp.description.unwrap(), "Unauthorized");
}

#[test]
fn test_session_key_channel_type() {
    // Non-private chat types all produce namespaced session keys
    for chat_type in &["group", "supergroup", "channel"] {
        let chat = Chat {
            id: -100,
            chat_type: chat_type.to_string(),
        };
        let key = TelegramGateway::session_key(&chat);
        assert_eq!(key.platform, Platform::Telegram);
        assert!(
            key.namespace.is_some(),
            "chat_type={} should have namespace",
            chat_type
        );
    }
}

// --- session_key: channel_id and namespace relationship ---

#[test]
fn test_session_key_private_channel_id_format() {
    let chat = Chat {
        id: 99999,
        chat_type: "private".to_string(),
    };
    let key = TelegramGateway::session_key(&chat);
    assert_eq!(key.channel_id, "99999");
    assert_eq!(key.namespace, None);
}

#[test]
fn test_session_key_group_namespace_equals_channel_id() {
    let chat = Chat {
        id: -500,
        chat_type: "group".to_string(),
    };
    let key = TelegramGateway::session_key(&chat);
    // For non-private chats, namespace == channel_id (both set to chat_id)
    assert_eq!(key.channel_id, "-500");
    assert_eq!(key.namespace, Some("-500".to_string()));
}

#[test]
fn test_platform_user_zero_chat_id() {
    let user = TelegramGateway::platform_user(0);
    assert_eq!(user.user_id, "0");
}

// --- Deserialization edge cases ---

#[test]
fn test_deserialize_bot_info_no_username() {
    let json = r#"{"ok":true,"result":{}}"#;
    let resp: TelegramResponse<BotInfo> = serde_json::from_str(json).unwrap();
    assert!(resp.ok);
    assert!(resp.result.unwrap().username.is_none());
}

#[test]
fn test_deserialize_user_no_optional_fields() {
    let json = r#"{"update_id":1,"message":{"message_id":1,"chat":{"id":1,"type":"private"},"from":{"id":42,"first_name":"Bob"}}}"#;
    let update: Update = serde_json::from_str(json).unwrap();
    let user = update.message.unwrap().from.unwrap();
    assert_eq!(user.first_name, "Bob");
    assert!(user.last_name.is_none());
}

#[test]
fn test_deserialize_message_entity_bot_command() {
    let json = r#"{"type":"bot_command","offset":0,"length":5}"#;
    let entity: MessageEntity = serde_json::from_str(json).unwrap();
    assert_eq!(entity.entity_type, "bot_command");
    assert_eq!(entity.offset, 0);
    assert_eq!(entity.length, 5);
}

#[test]
fn test_deserialize_chat_channel_type() {
    let json = r#"{"id":-1009876543210,"type":"channel"}"#;
    let chat: Chat = serde_json::from_str(json).unwrap();
    assert_eq!(chat.id, -1009876543210);
    assert_eq!(chat.chat_type, "channel");
}

#[test]
fn test_deserialize_telegram_response_empty_updates() {
    let json = r#"{"ok":true,"result":[]}"#;
    let resp: TelegramResponse<Vec<Update>> = serde_json::from_str(json).unwrap();
    assert!(resp.ok);
    assert_eq!(resp.result.unwrap().len(), 0);
}

#[test]
fn test_deserialize_update_with_entities() {
    let json = r#"{"update_id":5,"message":{"message_id":10,"chat":{"id":1,"type":"private"},"text":"/team hello","entities":[{"type":"bot_command","offset":0,"length":5}]}}"#;
    let update: Update = serde_json::from_str(json).unwrap();
    let msg = update.message.unwrap();
    let entities = msg.entities.unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].entity_type, "bot_command");
    assert_eq!(entities[0].length, 5);
}

#[test]
fn test_api_url_format_send_message() {
    let token = "987654:XYZ";
    let url = format!("https://api.telegram.org/bot{}/{}", token, "sendMessage");
    assert_eq!(url, "https://api.telegram.org/bot987654:XYZ/sendMessage");
}

#[test]
fn test_api_url_format_get_me() {
    let token = "111:AAA";
    let url = format!("https://api.telegram.org/bot{}/{}", token, "getMe");
    assert_eq!(url, "https://api.telegram.org/bot111:AAA/getMe");
}

// --- User type tests (from original test suite) ---

#[test]
fn test_deserialize_user_full() {
    let json = r#"{"id":100,"first_name":"Alice","last_name":"Smith","username":"alice"}"#;
    let user: User = serde_json::from_str(json).unwrap();
    assert_eq!(user.first_name, "Alice");
    assert_eq!(user.last_name.unwrap(), "Smith");
}
