use diesel::prelude::*;

use crate::schema::*;

// ── Sessions ──

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession<'a> {
    pub session_key: &'a str,
    pub selected_model: Option<&'a str>,
}

// ── Messages ──

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub session_key: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub author: Option<&'a str>,
}

// ── Message Queue ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = message_queue)]
pub struct QueueMessageRow {
    pub id: i32,
    pub session_key: String,
    pub team_run_id: String,
    pub sender: String,
    pub recipient: String,
    pub content: String,
    pub msg_type: String,
    pub status: String,
    pub retry_count: i32,
    pub max_retries: i32,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = message_queue)]
pub struct NewQueueMessage<'a> {
    pub session_key: &'a str,
    pub team_run_id: &'a str,
    pub sender: &'a str,
    pub recipient: &'a str,
    pub content: &'a str,
    pub msg_type: &'a str,
}

// ── Work Items ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = work_items)]
pub struct WorkItemRow {
    pub id: i32,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_id: Option<i32>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assigned_to: Option<String>,
    pub workflow_step: Option<i32>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = work_items)]
pub struct NewWorkItem<'a> {
    pub session_key: &'a str,
    pub team_run_id: &'a str,
    pub parent_id: Option<i32>,
    pub title: &'a str,
}

// ── Orchestration Runs ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = orchestration_runs)]
pub struct OrchestrationRunRow {
    #[allow(dead_code)]
    pub id: i32,
    pub team_run_id: String,
    pub session_key: String,
    pub team_name: String,
    pub workflow: String,
    pub input: String,
    pub status: String,
    pub current_step: i32,
    pub total_steps: i32,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = orchestration_runs)]
pub struct NewOrchestrationRun<'a> {
    pub team_run_id: &'a str,
    pub session_key: &'a str,
    pub team_name: &'a str,
    pub workflow: &'a str,
    pub input: &'a str,
    pub total_steps: i32,
}

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

// ── Agent Messages ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = agent_messages)]
pub struct AgentMessageRow {
    pub id: i32,
    pub session_key: String,
    pub from_agent: String,
    pub to_agent: Option<String>,
    pub channel: Option<String>,
    pub payload: String,
    pub status: String,
    pub created_at: String,
    pub delivered_at: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = agent_messages)]
pub struct NewAgentMessage<'a> {
    pub session_key: &'a str,
    pub from_agent: &'a str,
    pub to_agent: Option<&'a str>,
    pub channel: Option<&'a str>,
    pub payload: &'a str,
}

// ── Schedules ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = schedules)]
pub struct ScheduleRow {
    pub id: i32,
    pub name: String,
    pub cron_expression: String,
    pub team_name: String,
    pub input: String,
    pub enabled: i32,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = schedules)]
pub struct NewSchedule<'a> {
    pub name: &'a str,
    pub cron_expression: &'a str,
    pub team_name: &'a str,
    pub input: &'a str,
    pub next_run_at: Option<&'a str>,
}

// ── Plugins ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = plugins)]
pub struct PluginRow {
    pub id: i32,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub capabilities: String,
    pub source_path: String,
    pub enabled: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = plugins)]
pub struct NewPlugin<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub author: Option<&'a str>,
    pub description: Option<&'a str>,
    pub capabilities: &'a str,
    pub source_path: &'a str,
}

// ── Triggers ──

#[derive(Queryable, Selectable)]
#[diesel(table_name = triggers)]
pub struct TriggerRow {
    pub id: i32,
    pub name: String,
    pub trigger_type: String,
    pub condition_json: String,
    pub team_name: String,
    pub input: String,
    pub enabled: i32,
    pub last_fired_at: Option<String>,
    pub fire_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = triggers)]
pub struct NewTrigger<'a> {
    pub name: &'a str,
    pub trigger_type: &'a str,
    pub condition_json: &'a str,
    pub team_name: &'a str,
    pub input: &'a str,
}

// ── API Keys ──

#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = api_keys)]
pub struct ApiKeyRow {
    pub id: String,
    pub key_hash: String,
    pub description: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Insertable)]
#[diesel(table_name = api_keys)]
pub struct NewApiKey<'a> {
    pub id: &'a str,
    pub key_hash: &'a str,
    pub description: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_construction() {
        let s = NewSession {
            session_key: "discord:guild:chan",
            selected_model: None,
        };
        assert_eq!(s.session_key, "discord:guild:chan");
        assert!(s.selected_model.is_none());
    }

    #[test]
    fn test_new_message_with_author() {
        let m = NewMessage {
            session_key: "key",
            role: "user",
            content: "hello",
            author: Some("alice"),
        };
        assert_eq!(m.role, "user");
        assert_eq!(m.author, Some("alice"));
    }

    #[test]
    fn test_new_message_without_author() {
        let m = NewMessage {
            session_key: "key",
            role: "assistant",
            content: "hi",
            author: None,
        };
        assert!(m.author.is_none());
    }

    #[test]
    fn test_new_queue_message_construction() {
        let q = NewQueueMessage {
            session_key: "sk",
            team_run_id: "run1",
            sender: "agent_a",
            recipient: "agent_b",
            content: "payload",
            msg_type: "request",
        };
        assert_eq!(q.sender, "agent_a");
        assert_eq!(q.recipient, "agent_b");
    }

    #[test]
    fn test_new_work_item_with_parent() {
        let w = NewWorkItem {
            session_key: "sk",
            team_run_id: "run1",
            parent_id: Some(42),
            title: "Sub task",
        };
        assert_eq!(w.parent_id, Some(42));
    }

    #[test]
    fn test_new_work_item_no_parent() {
        let w = NewWorkItem {
            session_key: "sk",
            team_run_id: "run1",
            parent_id: None,
            title: "Root item",
        };
        assert!(w.parent_id.is_none());
    }

    #[test]
    fn test_new_orchestration_run_construction() {
        let r = NewOrchestrationRun {
            team_run_id: "run1",
            session_key: "sk",
            team_name: "code-review",
            workflow: "chain",
            input: "review this PR",
            total_steps: 3,
        };
        assert_eq!(r.team_name, "code-review");
        assert_eq!(r.total_steps, 3);
    }

    #[test]
    fn test_queue_message_row_fields() {
        let row = QueueMessageRow {
            id: 1,
            session_key: "sk".into(),
            team_run_id: "run1".into(),
            sender: "a".into(),
            recipient: "b".into(),
            content: "msg".into(),
            msg_type: "request".into(),
            status: "pending".into(),
            retry_count: 0,
            max_retries: 3,
            created_at: "2026-01-01".into(),
            processed_at: None,
            error: None,
        };
        assert_eq!(row.id, 1);
        assert!(row.processed_at.is_none());
        assert!(row.error.is_none());
    }

    #[test]
    fn test_work_item_row_fields() {
        let row = WorkItemRow {
            id: 10,
            session_key: "sk".into(),
            team_run_id: "run1".into(),
            parent_id: Some(5),
            title: "Task".into(),
            description: Some("Details".into()),
            status: "completed".into(),
            assigned_to: Some("agent1".into()),
            workflow_step: Some(2),
            input: Some("input".into()),
            output: Some("output".into()),
            error: None,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-02".into(),
        };
        assert_eq!(row.parent_id, Some(5));
        assert_eq!(row.workflow_step, Some(2));
    }

    #[test]
    fn test_orchestration_run_row_fields() {
        let row = OrchestrationRunRow {
            id: 1,
            team_run_id: "run1".into(),
            session_key: "sk".into(),
            team_name: "devops".into(),
            workflow: "fan_out".into(),
            input: "deploy".into(),
            status: "running".into(),
            current_step: 1,
            total_steps: 4,
            result: None,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
        };
        assert_eq!(row.current_step, 1);
        assert_eq!(row.total_steps, 4);
        assert!(row.result.is_none());
    }
}
