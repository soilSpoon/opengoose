use diesel::prelude::*;

use crate::schema::*;

// ── Alert Rules ──

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = alert_rules)]
pub struct AlertRuleRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub metric: String,
    pub condition: String,
    pub threshold: f64,
    pub enabled: i32,
    pub actions: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = alert_rules)]
pub struct NewAlertRule<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub metric: &'a str,
    pub condition: &'a str,
    pub threshold: f64,
    pub actions: &'a str,
}

// ── Alert History ──

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = alert_history)]
pub struct AlertHistoryRow {
    pub id: i32,
    pub rule_id: String,
    pub rule_name: String,
    pub metric: String,
    pub value: f64,
    pub triggered_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = alert_history)]
pub struct NewAlertHistory<'a> {
    pub rule_id: &'a str,
    pub rule_name: &'a str,
    pub metric: &'a str,
    pub value: f64,
}

// ── Event History ──

#[derive(Queryable, Selectable, Clone)]
#[diesel(table_name = event_history)]
pub struct EventHistoryRow {
    pub id: i32,
    pub event_kind: String,
    pub timestamp: String,
    pub source_gateway: Option<String>,
    pub session_key: Option<String>,
    pub payload: String,
}

impl std::fmt::Debug for EventHistoryRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventHistoryRow")
            .field("id", &self.id)
            .field("event_kind", &self.event_kind)
            .field("timestamp", &self.timestamp)
            .field("source_gateway", &self.source_gateway)
            .field("session_key", &"<redacted>")
            .field("payload", &"<redacted>")
            .finish()
    }
}

#[derive(Insertable)]
#[diesel(table_name = event_history)]
pub struct NewEventHistory<'a> {
    pub event_kind: &'a str,
    pub source_gateway: Option<&'a str>,
    pub session_key: Option<&'a str>,
    pub payload: &'a str,
}

impl std::fmt::Debug for NewEventHistory<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewEventHistory")
            .field("event_kind", &self.event_kind)
            .field("source_gateway", &self.source_gateway)
            .field("session_key", &"<redacted>")
            .field("payload", &"<redacted>")
            .finish()
    }
}
