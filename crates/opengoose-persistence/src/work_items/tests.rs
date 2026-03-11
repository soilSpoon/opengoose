use std::sync::Arc;

use diesel::prelude::*;

use super::{WorkItemStore, WorkStatus};
use crate::db::Database;
use crate::models::NewSession;
use crate::schema::sessions;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

fn ensure_session(db: &Arc<Database>, key: &str) {
    db.with(|conn| {
        diesel::insert_into(sessions::table)
            .values(NewSession { session_key: key })
            .on_conflict(sessions::session_key)
            .do_nothing()
            .execute(conn)?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn test_create_and_get() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Fix auth bug", None).unwrap();
    assert!(id > 0);

    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.title, "Fix auth bug");
    assert_eq!(item.status, WorkStatus::Pending);
    assert!(item.parent_id.is_none());
}

#[test]
fn test_assign_and_complete() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Step 1", None).unwrap();

    store.assign(id, "coder", Some(0)).unwrap();
    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.status, WorkStatus::InProgress);
    assert_eq!(item.assigned_to.as_deref(), Some("coder"));
    assert_eq!(item.workflow_step, Some(0));

    store.set_input(id, "input text").unwrap();
    store.set_output(id, "output text").unwrap();
    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.status, WorkStatus::Completed);
    assert_eq!(item.output.as_deref(), Some("output text"));
}

#[test]
fn test_parent_children() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent_id = store.create("sess1", "run1", "Main task", None).unwrap();
    let child1 = store
        .create("sess1", "run1", "Step 0", Some(parent_id))
        .unwrap();
    let child2 = store
        .create("sess1", "run1", "Step 1", Some(parent_id))
        .unwrap();

    store.assign(child1, "coder", Some(0)).unwrap();
    store.assign(child2, "reviewer", Some(1)).unwrap();

    let children = store.get_children(parent_id).unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].workflow_step, Some(0));
    assert_eq!(children[1].workflow_step, Some(1));
}

#[test]
fn test_find_resume_point() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent_id = store.create("sess1", "run1", "Chain task", None).unwrap();

    let step0 = store
        .create("sess1", "run1", "Step 0", Some(parent_id))
        .unwrap();
    let step1 = store
        .create("sess1", "run1", "Step 1", Some(parent_id))
        .unwrap();
    let _step2 = store
        .create("sess1", "run1", "Step 2", Some(parent_id))
        .unwrap();

    store.assign(step0, "coder", Some(0)).unwrap();
    store.set_output(step0, "step 0 output").unwrap();

    store.assign(step1, "reviewer", Some(1)).unwrap();
    store.set_error(step1, "timeout").unwrap();

    let point = store.find_resume_point(parent_id).unwrap();
    assert_eq!(point, Some((1, "step 0 output".to_string())));
}

#[test]
fn test_list_for_run() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    store.create("sess1", "run1", "Task A", None).unwrap();
    store.create("sess1", "run1", "Task B", None).unwrap();
    store.create("sess1", "run2", "Task C", None).unwrap();

    let items = store.list_for_run("run1", None).unwrap();
    assert_eq!(items.len(), 2);

    let items = store
        .list_for_run("run1", Some(&WorkStatus::Pending))
        .unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn test_work_status_as_str() {
    assert_eq!(WorkStatus::Pending.as_str(), "pending");
    assert_eq!(WorkStatus::InProgress.as_str(), "in_progress");
    assert_eq!(WorkStatus::Completed.as_str(), "completed");
    assert_eq!(WorkStatus::Failed.as_str(), "failed");
    assert_eq!(WorkStatus::Cancelled.as_str(), "cancelled");
}

#[test]
fn test_work_status_parse_roundtrip() {
    for s in [
        WorkStatus::Pending,
        WorkStatus::InProgress,
        WorkStatus::Completed,
        WorkStatus::Failed,
        WorkStatus::Cancelled,
    ] {
        assert_eq!(WorkStatus::parse(s.as_str()).unwrap(), s);
    }
}

#[test]
fn test_work_status_parse_invalid() {
    let err = WorkStatus::parse("garbage").unwrap_err();
    assert!(err.to_string().contains("WorkStatus"));
}

#[test]
fn test_get_nonexistent() {
    let db = test_db();
    let store = WorkItemStore::new(db);
    let result = store.get(99999).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_set_error() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Failing task", None).unwrap();
    store.set_error(id, "something went wrong").unwrap();

    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.status, WorkStatus::Failed);
    assert_eq!(item.error.as_deref(), Some("something went wrong"));
}

#[test]
fn test_find_resume_point_no_children() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent = store.create("sess1", "run1", "Parent", None).unwrap();
    let result = store.find_resume_point(parent).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_list_for_run_filtered_by_status() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id1 = store.create("sess1", "run1", "Task A", None).unwrap();
    store.create("sess1", "run1", "Task B", None).unwrap();
    store.set_output(id1, "done").unwrap();

    let completed = store
        .list_for_run("run1", Some(&WorkStatus::Completed))
        .unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].title, "Task A");

    let pending = store
        .list_for_run("run1", Some(&WorkStatus::Pending))
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].title, "Task B");
}

#[test]
fn test_update_status_cancelled() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Cancel me", None).unwrap();
    store.update_status(id, WorkStatus::Cancelled).unwrap();

    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.status, WorkStatus::Cancelled);
}

#[test]
fn test_get_children_empty() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent_id = store.create("sess1", "run1", "Parent", None).unwrap();
    let children = store.get_children(parent_id).unwrap();
    assert!(children.is_empty());
}

#[test]
fn test_find_resume_point_all_failed() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent_id = store.create("sess1", "run1", "Chain", None).unwrap();
    let step0 = store
        .create("sess1", "run1", "Step 0", Some(parent_id))
        .unwrap();
    store.assign(step0, "coder", Some(0)).unwrap();
    store.set_error(step0, "crashed").unwrap();

    let point = store.find_resume_point(parent_id).unwrap();
    assert!(point.is_none());
}

#[test]
fn test_set_input() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Process data", None).unwrap();
    assert_eq!(store.get(id).unwrap().unwrap().input, None);

    store.set_input(id, "raw payload").unwrap();
    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.input.as_deref(), Some("raw payload"));
    assert_eq!(item.status, WorkStatus::Pending);
}

#[test]
fn test_parent_status_independent_of_children() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent = store.create("sess1", "run1", "Parent", None).unwrap();
    let child = store
        .create("sess1", "run1", "Child", Some(parent))
        .unwrap();

    store.set_output(child, "done").unwrap();
    let parent_item = store.get(parent).unwrap().unwrap();
    assert_eq!(parent_item.status, WorkStatus::Pending);

    store.set_error(parent, "parent failed").unwrap();
    let child_item = store.get(child).unwrap().unwrap();
    assert_eq!(child_item.status, WorkStatus::Completed);
}

#[test]
fn test_status_transition_pending_to_in_progress_to_completed() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Lifecycle", None).unwrap();
    assert_eq!(store.get(id).unwrap().unwrap().status, WorkStatus::Pending);

    store.update_status(id, WorkStatus::InProgress).unwrap();
    assert_eq!(
        store.get(id).unwrap().unwrap().status,
        WorkStatus::InProgress
    );

    store.update_status(id, WorkStatus::Completed).unwrap();
    assert_eq!(
        store.get(id).unwrap().unwrap().status,
        WorkStatus::Completed
    );
}

#[test]
fn test_reassign_work_item() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Reassign me", None).unwrap();
    store.assign(id, "agent-a", Some(0)).unwrap();
    assert_eq!(
        store.get(id).unwrap().unwrap().assigned_to.as_deref(),
        Some("agent-a")
    );

    store.assign(id, "agent-b", Some(1)).unwrap();
    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.assigned_to.as_deref(), Some("agent-b"));
    assert_eq!(item.workflow_step, Some(1));
    assert_eq!(item.status, WorkStatus::InProgress);
}

#[test]
fn test_find_resume_point_completed_with_no_output() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent = store.create("sess1", "run1", "Chain", None).unwrap();
    let step0 = store
        .create("sess1", "run1", "Step 0", Some(parent))
        .unwrap();
    store.assign(step0, "coder", Some(0)).unwrap();
    store.update_status(step0, WorkStatus::Completed).unwrap();

    let point = store.find_resume_point(parent).unwrap();
    assert_eq!(point, Some((1, String::new())));
}

#[test]
fn test_list_for_run_empty() {
    let db = test_db();
    let store = WorkItemStore::new(db);

    let items = store.list_for_run("nonexistent-run", None).unwrap();
    assert!(items.is_empty());
}

#[test]
fn test_multiple_children_same_workflow_step() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent = store.create("sess1", "run1", "Parent", None).unwrap();
    let c1 = store
        .create("sess1", "run1", "Child A", Some(parent))
        .unwrap();
    let c2 = store
        .create("sess1", "run1", "Child B", Some(parent))
        .unwrap();
    store.assign(c1, "agent-a", Some(0)).unwrap();
    store.assign(c2, "agent-b", Some(0)).unwrap();

    let children = store.get_children(parent).unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].workflow_step, Some(0));
    assert_eq!(children[1].workflow_step, Some(0));
}

#[test]
fn test_assign_with_no_step() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "No step", None).unwrap();
    store.assign(id, "agent", None).unwrap();

    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.assigned_to.as_deref(), Some("agent"));
    assert_eq!(item.workflow_step, None);
    assert_eq!(item.status, WorkStatus::InProgress);
}

#[test]
fn test_overwrite_input_and_output() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let id = store.create("sess1", "run1", "Overwrite", None).unwrap();
    store.set_input(id, "first").unwrap();
    store.set_input(id, "second").unwrap();
    assert_eq!(
        store.get(id).unwrap().unwrap().input.as_deref(),
        Some("second")
    );

    store.set_output(id, "result-1").unwrap();
    store.set_output(id, "result-2").unwrap();
    assert_eq!(
        store.get(id).unwrap().unwrap().output.as_deref(),
        Some("result-2")
    );
}

#[test]
fn test_find_resume_point_picks_highest_completed_step() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let store = WorkItemStore::new(db);

    let parent = store.create("sess1", "run1", "Chain", None).unwrap();
    let s0 = store
        .create("sess1", "run1", "Step 0", Some(parent))
        .unwrap();
    let s1 = store
        .create("sess1", "run1", "Step 1", Some(parent))
        .unwrap();
    let s2 = store
        .create("sess1", "run1", "Step 2", Some(parent))
        .unwrap();

    store.assign(s0, "a", Some(0)).unwrap();
    store.set_output(s0, "out0").unwrap();
    store.assign(s1, "a", Some(1)).unwrap();
    store.set_output(s1, "out1").unwrap();
    store.assign(s2, "a", Some(2)).unwrap();
    store.set_error(s2, "boom").unwrap();

    let point = store.find_resume_point(parent).unwrap();
    assert_eq!(point, Some((2, "out1".to_string())));
}
