use diesel::prelude::*;

use crate::schema::{message_queue, orchestration_runs, work_items};

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

#[cfg(test)]
mod tests {
    use super::{
        NewOrchestrationRun, NewQueueMessage, NewWorkItem, OrchestrationRunRow, QueueMessageRow,
        WorkItemRow,
    };

    #[test]
    fn test_new_queue_message_construction() {
        let queue_message = NewQueueMessage {
            session_key: "sk",
            team_run_id: "run1",
            sender: "agent_a",
            recipient: "agent_b",
            content: "payload",
            msg_type: "request",
        };

        assert_eq!(queue_message.sender, "agent_a");
        assert_eq!(queue_message.recipient, "agent_b");
    }

    #[test]
    fn test_new_work_item_with_parent() {
        let work_item = NewWorkItem {
            session_key: "sk",
            team_run_id: "run1",
            parent_id: Some(42),
            title: "Sub task",
        };

        assert_eq!(work_item.parent_id, Some(42));
    }

    #[test]
    fn test_new_work_item_no_parent() {
        let work_item = NewWorkItem {
            session_key: "sk",
            team_run_id: "run1",
            parent_id: None,
            title: "Root item",
        };

        assert!(work_item.parent_id.is_none());
    }

    #[test]
    fn test_new_orchestration_run_construction() {
        let run = NewOrchestrationRun {
            team_run_id: "run1",
            session_key: "sk",
            team_name: "code-review",
            workflow: "chain",
            input: "review this PR",
            total_steps: 3,
        };

        assert_eq!(run.team_name, "code-review");
        assert_eq!(run.total_steps, 3);
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
