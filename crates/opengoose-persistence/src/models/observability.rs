use diesel::prelude::*;

use crate::schema::{alert_history, alert_rules, event_history};

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

#[cfg(test)]
mod tests {
    use super::{EventHistoryRow, NewEventHistory};

    #[test]
    fn test_event_history_row_debug_redacts_sensitive_fields() {
        let row = EventHistoryRow {
            id: 7,
            event_kind: "message.received".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            source_gateway: Some("discord".into()),
            session_key: Some("discord:guild:channel".into()),
            payload: "{\"token\":\"secret\"}".into(),
        };

        let debug_output = format!("{row:?}");
        assert!(debug_output.contains("EventHistoryRow"));
        assert!(debug_output.contains("<redacted>"));
        assert!(!debug_output.contains("discord:guild:channel"));
        assert!(!debug_output.contains("{\"token\":\"secret\"}"));
    }

    #[test]
    fn test_new_event_history_debug_redacts_sensitive_fields() {
        let event = NewEventHistory {
            event_kind: "message.received",
            source_gateway: Some("slack"),
            session_key: Some("slack:team:channel"),
            payload: "{\"text\":\"hello\"}",
        };

        let debug_output = format!("{event:?}");
        assert!(debug_output.contains("NewEventHistory"));
        assert!(debug_output.contains("<redacted>"));
        assert!(!debug_output.contains("slack:team:channel"));
        assert!(!debug_output.contains("{\"text\":\"hello\"}"));
    }
}
