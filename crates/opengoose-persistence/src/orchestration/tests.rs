use super::*;
use crate::test_helpers::{ensure_session, test_db};

#[test]
fn test_create_and_get() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "code-review", "chain", "review this PR", 3)
        .unwrap();

    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.team_name, "code-review");
    assert_eq!(run.workflow, "chain");
    assert_eq!(run.status, RunStatus::Running);
    assert_eq!(run.total_steps, 3);
}

#[test]
fn test_advance_and_complete() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "review", "chain", "input", 2)
        .unwrap();

    store.advance_step("run1", 1).unwrap();
    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.current_step, 1);

    store.complete_run("run1", "all good").unwrap();
    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Completed);
    assert_eq!(run.result.as_deref(), Some("all good"));
}

#[test]
fn test_suspend_incomplete() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "t1", "chain", "i1", 2)
        .unwrap();
    store
        .create_run("run2", "sess1", "t2", "fan_out", "i2", 3)
        .unwrap();
    store.complete_run("run2", "done").unwrap();

    let suspended = store.suspend_incomplete().unwrap();
    assert_eq!(suspended, 1);

    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Suspended);
}

#[test]
fn test_find_suspended() {
    let db = test_db();
    ensure_session(&db, "sess1");
    ensure_session(&db, "sess2");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "t1", "chain", "i1", 2)
        .unwrap();
    store
        .create_run("run2", "sess2", "t2", "chain", "i2", 2)
        .unwrap();
    store.suspend_incomplete().unwrap();

    let runs = store.find_suspended("sess1").unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].team_run_id, "run1");
}

#[test]
fn test_fail_run() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "review", "chain", "input", 2)
        .unwrap();

    store.fail_run("run1", "agent crashed").unwrap();
    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.result.as_deref(), Some("agent crashed"));
}

#[test]
fn test_resume_run() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "t1", "chain", "i1", 3)
        .unwrap();
    store.suspend_incomplete().unwrap();

    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Suspended);

    store.resume_run("run1").unwrap();
    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Running);
}

#[test]
fn test_get_run_nonexistent() {
    let db = test_db();
    let store = OrchestrationStore::new(db);
    let result = store.get_run("no-such-run").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_list_runs_filtered_by_status() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "t1", "chain", "i1", 2)
        .unwrap();
    store
        .create_run("run2", "sess1", "t2", "fan_out", "i2", 3)
        .unwrap();
    store.complete_run("run1", "done").unwrap();

    let running = store.list_runs(Some(&RunStatus::Running), 100).unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].team_run_id, "run2");

    let completed = store.list_runs(Some(&RunStatus::Completed), 100).unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].team_run_id, "run1");

    let all = store.list_runs(None, 100).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_list_runs_respects_limit() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    for i in 0..5 {
        store
            .create_run(&format!("run{i}"), "sess1", "t", "chain", "i", 1)
            .unwrap();
    }

    let limited = store.list_runs(None, 3).unwrap();
    assert_eq!(limited.len(), 3);
}

#[test]
fn test_create_run_auto_creates_session() {
    let db = test_db();
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "new-sess", "team", "chain", "input", 2)
        .unwrap();

    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.session_key, "new-sess");
}

#[test]
fn test_advance_step_then_complete() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "review", "chain", "input", 3)
        .unwrap();

    store.advance_step("run1", 1).unwrap();
    store.advance_step("run1", 2).unwrap();

    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.current_step, 2);
    assert_eq!(run.status, RunStatus::Running);

    store.complete_run("run1", "all done").unwrap();
    let run = store.get_run("run1").unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Completed);
}

#[test]
fn test_find_suspended_empty() {
    let db = test_db();
    let store = OrchestrationStore::new(db);

    let runs = store.find_suspended("nonexistent-session").unwrap();
    assert!(runs.is_empty());
}

#[test]
fn test_suspend_incomplete_no_running_runs() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = OrchestrationStore::new(db);

    store
        .create_run("run1", "sess1", "t1", "chain", "i1", 2)
        .unwrap();
    store.complete_run("run1", "done").unwrap();

    let count = store.suspend_incomplete().unwrap();
    assert_eq!(count, 0);
}
