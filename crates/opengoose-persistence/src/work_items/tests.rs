use std::sync::Arc;

use super::{WorkItemStore, WorkStatus};
use crate::prolly::ProllyBeadsStore;

fn test_store() -> WorkItemStore {
    WorkItemStore::new(Arc::new(ProllyBeadsStore::in_memory()))
}

#[test]
fn test_create_and_get() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Fix auth bug", None);
    assert!(id.starts_with("bd-"));

    let item = store.get(&id).unwrap();
    assert_eq!(item.title, "Fix auth bug");
    assert_eq!(item.status, WorkStatus::Pending);
    assert!(item.parent_hash_id.is_none());
}

#[test]
fn test_assign_and_complete() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Step 1", None);

    store.assign(&id, "coder", Some(0));
    let item = store.get(&id).unwrap();
    assert_eq!(item.status, WorkStatus::InProgress);
    assert_eq!(item.assigned_to.as_deref(), Some("coder"));
    assert_eq!(item.workflow_step, Some(0));

    store.set_input(&id, "input text");
    store.set_output(&id, "output text");
    let item = store.get(&id).unwrap();
    assert_eq!(item.status, WorkStatus::Completed);
    assert_eq!(item.output.as_deref(), Some("output text"));
}

#[test]
fn test_parent_children() {
    let store = test_store();
    let parent_id = store.create("sess1", "run1", "Main task", None);
    let child1 = store.create("sess1", "run1", "Step 0", Some(&parent_id));
    let child2 = store.create("sess1", "run1", "Step 1", Some(&parent_id));

    store.assign(&child1, "coder", Some(0));
    store.assign(&child2, "reviewer", Some(1));

    let children = store.get_children(&parent_id);
    assert_eq!(children.len(), 2);
}

#[test]
fn test_find_resume_point() {
    let store = test_store();
    let parent_id = store.create("sess1", "run1", "Chain task", None);

    let step0 = store.create("sess1", "run1", "Step 0", Some(&parent_id));
    let step1 = store.create("sess1", "run1", "Step 1", Some(&parent_id));
    let _step2 = store.create("sess1", "run1", "Step 2", Some(&parent_id));

    store.assign(&step0, "coder", Some(0));
    store.set_output(&step0, "step 0 output");

    store.assign(&step1, "reviewer", Some(1));
    store.set_error(&step1, "timeout");

    let point = store.find_resume_point(&parent_id);
    assert_eq!(point, Some((1, "step 0 output".to_string())));
}

#[test]
fn test_list_for_run() {
    let store = test_store();
    store.create("sess1", "run1", "Task A", None);
    store.create("sess1", "run1", "Task B", None);
    store.create("sess1", "run2", "Task C", None);

    let items = store.list_for_run("run1", None);
    assert_eq!(items.len(), 2);

    let items = store.list_for_run("run1", Some(&WorkStatus::Pending));
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
    let store = test_store();
    let result = store.get("bd-nonexistent");
    assert!(result.is_none());
}

#[test]
fn test_set_error() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Failing task", None);
    store.set_error(&id, "something went wrong");

    let item = store.get(&id).unwrap();
    assert_eq!(item.status, WorkStatus::Failed);
    assert_eq!(item.error.as_deref(), Some("something went wrong"));
}

#[test]
fn test_find_resume_point_no_children() {
    let store = test_store();
    let parent = store.create("sess1", "run1", "Parent", None);
    let result = store.find_resume_point(&parent);
    assert!(result.is_none());
}

#[test]
fn test_list_for_run_filtered_by_status() {
    let store = test_store();
    let id1 = store.create("sess1", "run1", "Task A", None);
    store.create("sess1", "run1", "Task B", None);
    store.set_output(&id1, "done");

    let completed = store.list_for_run("run1", Some(&WorkStatus::Completed));
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].title, "Task A");

    let pending = store.list_for_run("run1", Some(&WorkStatus::Pending));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].title, "Task B");
}

#[test]
fn test_update_status_cancelled() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Cancel me", None);
    store.update_status(&id, WorkStatus::Cancelled);

    let item = store.get(&id).unwrap();
    assert_eq!(item.status, WorkStatus::Cancelled);
}

#[test]
fn test_get_children_empty() {
    let store = test_store();
    let parent_id = store.create("sess1", "run1", "Parent", None);
    let children = store.get_children(&parent_id);
    assert!(children.is_empty());
}

#[test]
fn test_find_resume_point_all_failed() {
    let store = test_store();
    let parent_id = store.create("sess1", "run1", "Chain", None);
    let step0 = store.create("sess1", "run1", "Step 0", Some(&parent_id));
    store.assign(&step0, "coder", Some(0));
    store.set_error(&step0, "crashed");

    let point = store.find_resume_point(&parent_id);
    assert!(point.is_none());
}

#[test]
fn test_set_input() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Process data", None);
    assert_eq!(store.get(&id).unwrap().input, None);

    store.set_input(&id, "raw payload");
    let item = store.get(&id).unwrap();
    assert_eq!(item.input.as_deref(), Some("raw payload"));
    assert_eq!(item.status, WorkStatus::Pending);
}

#[test]
fn test_parent_status_independent_of_children() {
    let store = test_store();
    let parent = store.create("sess1", "run1", "Parent", None);
    let child = store.create("sess1", "run1", "Child", Some(&parent));

    store.set_output(&child, "done");
    let parent_item = store.get(&parent).unwrap();
    assert_eq!(parent_item.status, WorkStatus::Pending);

    store.set_error(&parent, "parent failed");
    let child_item = store.get(&child).unwrap();
    assert_eq!(child_item.status, WorkStatus::Completed);
}

#[test]
fn test_status_transition_pending_to_in_progress_to_completed() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Lifecycle", None);
    assert_eq!(store.get(&id).unwrap().status, WorkStatus::Pending);

    store.update_status(&id, WorkStatus::InProgress);
    assert_eq!(store.get(&id).unwrap().status, WorkStatus::InProgress);

    store.update_status(&id, WorkStatus::Completed);
    assert_eq!(store.get(&id).unwrap().status, WorkStatus::Completed);
}

#[test]
fn test_reassign_work_item() {
    let store = test_store();
    let id = store.create("sess1", "run1", "Reassign me", None);
    store.assign(&id, "agent-a", Some(0));
    assert_eq!(
        store.get(&id).unwrap().assigned_to.as_deref(),
        Some("agent-a")
    );

    store.assign(&id, "agent-b", Some(1));
    let item = store.get(&id).unwrap();
    assert_eq!(item.assigned_to.as_deref(), Some("agent-b"));
    assert_eq!(item.workflow_step, Some(1));
    assert_eq!(item.status, WorkStatus::InProgress);
}

#[test]
fn test_list_for_run_empty() {
    let store = test_store();
    let items = store.list_for_run("nonexistent-run", None);
    assert!(items.is_empty());
}

#[test]
fn test_assign_with_no_step() {
    let store = test_store();
    let id = store.create("sess1", "run1", "No step", None);
    store.assign(&id, "agent", None);

    let item = store.get(&id).unwrap();
    assert_eq!(item.assigned_to.as_deref(), Some("agent"));
    assert_eq!(item.workflow_step, None);
    assert_eq!(item.status, WorkStatus::InProgress);
}
