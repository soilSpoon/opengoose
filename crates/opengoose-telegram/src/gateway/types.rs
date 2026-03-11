/// Telegram Bot API types needed for the polling loop.
/// Sending and validation are delegated to goose's TelegramGateway.
#[derive(serde::Deserialize)]
pub struct TelegramResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub chat: Chat,
    pub from: Option<User>,
    pub text: Option<String>,
    pub entities: Option<Vec<MessageEntity>>,
}

#[derive(serde::Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
}

#[derive(serde::Deserialize)]
#[allow(dead_code)]
pub struct User {
    pub id: i64,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct MessageEntity {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub offset: usize,
    pub length: usize,
}

#[derive(serde::Deserialize)]
pub struct BotInfo {
    pub username: Option<String>,
}

/// Minimal response from sendMessage — only what we need for draft tracking.
#[derive(serde::Deserialize)]
pub struct SentMessage {
    pub message_id: i64,
}
