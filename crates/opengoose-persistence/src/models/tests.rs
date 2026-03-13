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
        hash_id: None,
        is_ephemeral: 0,
        priority: 3,
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
        hash_id: None,
        is_ephemeral: 0,
        priority: 3,
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
        hash_id: None,
        is_ephemeral: 0,
        priority: 3,
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
