// Worker → Board cross-crate integration tests.
//
// Agent::new() without a provider + non-git tempdir as repo_dir causes
// worktree acquisition to fail, triggering the abandon path.

use goose::agents::Agent;
use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use opengoose_rig::rig::Worker;
use opengoose_rig::work_mode::TaskMode;
use std::sync::Arc;

async fn setup() -> (Arc<Board>, Worker) {
    let board = Arc::new(Board::in_memory().await.expect("board should init"));
    let agent = Agent::new();
    let worker = Worker::new(
        RigId::new("test-worker"),
        Arc::clone(&board),
        agent,
        TaskMode,
        vec![],
    );
    (board, worker)
}

fn post_req(title: &str) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P1,
        tags: vec![],
    }
}

/// Worker claims an item, fails worktree acquisition (non-git dir), and abandons it.
/// Tests the full cross-crate Worker → Board error flow.
#[tokio::test]
async fn claim_and_abandon_on_worktree_failure() {
    let (board, worker) = setup().await;

    let item = board
        .post(post_req("test task"))
        .await
        .expect("post should succeed");
    assert_eq!(item.status, Status::Open);

    // Non-git tempdir → worktree creation fails → Worker abandons
    let tmp = tempfile::tempdir().expect("tempdir should succeed");
    let result = worker
        .try_claim_and_execute(tmp.path())
        .await
        .expect("should not return Err");
    assert!(result, "should return true (work was found and attempted)");

    // Item should be unclaimed (back to Open)
    let updated = board
        .get(item.id)
        .await
        .expect("get should succeed")
        .expect("item should exist");
    assert_eq!(
        updated.status,
        Status::Open,
        "item should be Open after unclaim"
    );
}

/// Empty board → try_claim_and_execute returns Ok(false).
#[tokio::test]
async fn empty_board_returns_false() {
    let (_, worker) = setup().await;
    let tmp = tempfile::tempdir().expect("tempdir should succeed");
    let result = worker
        .try_claim_and_execute(tmp.path())
        .await
        .expect("should succeed");
    assert!(!result, "empty board should return false");
}

#[tokio::test]
async fn worker_run_exits_on_pre_cancel() {
    let (_, worker) = setup().await;
    worker.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker.run())
        .await
        .expect("worker.run() should exit quickly after cancel");
}

#[tokio::test]
async fn worker_on_empty_board_waits_then_cancels() {
    let (_, worker) = setup().await;
    let cancel = worker.cancel_token();

    let handle = tokio::spawn(async move { worker.run().await });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    cancel.cancel();

    tokio::time::timeout(std::time::Duration::from_secs(5), handle)
        .await
        .expect("should stop within timeout")
        .expect("should not panic");
}
