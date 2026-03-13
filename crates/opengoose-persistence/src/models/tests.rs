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
fn test_orchestration_run_row_fields() {
    let row = OrchestrationRunRow {
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
