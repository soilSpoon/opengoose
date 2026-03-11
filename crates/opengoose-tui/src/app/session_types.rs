use std::time::Instant;

use opengoose_types::SessionKey;

pub const MAX_MESSAGES: usize = 1000;
pub const MAX_EVENTS: usize = 2000;

#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub session_key: SessionKey,
    pub author: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct EventEntry {
    pub summary: String,
    pub level: EventLevel,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLevel {
    Info,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Sessions,
    Messages,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Generating,
}

#[derive(Debug, Clone)]
pub struct SessionListEntry {
    pub session_key: SessionKey,
    pub active_team: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct StatusNotice {
    pub message: String,
    pub level: EventLevel,
}
