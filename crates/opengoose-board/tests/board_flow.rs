// Integration tests for the Board -> Worker flow.
//
// These test the Board-level operations that Worker depends on,
// without requiring a Goose Agent (LLM runtime).

use std::sync::Arc;

use opengoose_board::board::Board;
use opengoose_board::work_item::{BoardError, PostWorkItem, Priority, RigId, Status};

fn post_req(title: &str) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P1,
        tags: vec![],
        parent_id: None,
    }
}

// ── Scenario 1: Happy path — post → ready → claim → submit ──

#[tokio::test]
async fn happy_path_post_ready_claim_submit() {
    let board = Board::in_memory().await.expect("board should initialize");

    // Post a work item
    let item = board
        .post(post_req("implement feature X"))
        .await
        .expect("post should succeed");
    assert_eq!(item.status, Status::Open);

    // Verify it appears in ready()
    let ready = board.ready().await.expect("ready should succeed");
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, item.id);

    // Claim it
    let rig = RigId::new("worker-1");
    let claimed = board
        .claim(item.id, &rig)
        .await
        .expect("claim should succeed");
    assert_eq!(claimed.status, Status::Claimed);
    assert_eq!(claimed.claimed_by, Some(rig.clone()));

    // Verify it's no longer in ready() but is in claimed_by(rig)
    let ready_after_claim = board.ready().await.expect("ready should succeed");
    assert!(
        ready_after_claim.is_empty(),
        "claimed item should not appear in ready()"
    );

    let claimed_items = board
        .claimed_by(&rig)
        .await
        .expect("claimed_by should succeed");
    assert_eq!(claimed_items.len(), 1);
    assert_eq!(claimed_items[0].id, item.id);

    // Submit it
    let done = board
        .submit(item.id, &rig)
        .await
        .expect("submit should succeed");
    assert_eq!(done.status, Status::Done);

    // Verify it's gone from both ready and claimed
    let ready_after_submit = board.ready().await.expect("ready should succeed");
    assert!(ready_after_submit.is_empty());

    let claimed_after_submit = board
        .claimed_by(&rig)
        .await
        .expect("claimed_by should succeed");
    assert!(claimed_after_submit.is_empty());
}

// ── Scenario 2: Claim competition ──

#[tokio::test]
async fn claim_competition() {
    let board = Board::in_memory().await.expect("board should initialize");
    let item = board
        .post(post_req("contested task"))
        .await
        .expect("post should succeed");

    // First claim succeeds
    let rig1 = RigId::new("worker-1");
    board
        .claim(item.id, &rig1)
        .await
        .expect("first claim should succeed");

    // Second claim fails with AlreadyClaimed
    let rig2 = RigId::new("worker-2");
    let result = board.claim(item.id, &rig2).await;
    assert!(
        matches!(result, Err(BoardError::AlreadyClaimed { id, .. }) if id == item.id),
        "expected AlreadyClaimed, got {result:?}"
    );

    // Item is still claimed by worker-1
    let claimed = board
        .claimed_by(&rig1)
        .await
        .expect("claimed_by should succeed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, item.id);

    let not_claimed = board
        .claimed_by(&rig2)
        .await
        .expect("claimed_by should succeed");
    assert!(not_claimed.is_empty());
}

// ── Scenario 3: Abandon flow ──

#[tokio::test]
async fn abandon_returns_to_ready() {
    let board = Board::in_memory().await.expect("board should initialize");
    let item = board
        .post(post_req("abandonable task"))
        .await
        .expect("post should succeed");

    let rig = RigId::new("worker-1");
    board
        .claim(item.id, &rig)
        .await
        .expect("claim should succeed");

    // Verify claimed
    assert!(
        board
            .ready()
            .await
            .expect("ready should succeed")
            .is_empty()
    );

    // Unclaim (abandon = Claimed -> Open) to return to ready
    board
        .unclaim(item.id, &rig)
        .await
        .expect("unclaim should succeed");

    // Verify it's back in ready()
    let ready = board.ready().await.expect("ready should succeed");
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, item.id);
    assert_eq!(ready[0].status, Status::Open);
}

// ── Scenario 4: Mark stuck ──

#[tokio::test]
async fn stuck_items_not_in_ready() {
    let board = Board::in_memory().await.expect("board should initialize");
    let item = board
        .post(post_req("will get stuck"))
        .await
        .expect("post should succeed");

    let rig = RigId::new("worker-1");
    board
        .claim(item.id, &rig)
        .await
        .expect("claim should succeed");

    // Mark stuck
    let stuck = board
        .mark_stuck(item.id, &rig)
        .await
        .expect("mark_stuck should succeed");
    assert_eq!(stuck.status, Status::Stuck);

    // Stuck items should NOT be in ready()
    let ready = board.ready().await.expect("ready should succeed");
    assert!(ready.is_empty(), "stuck items should not appear in ready()");

    // Also not in claimed_by (status is Stuck, not Claimed)
    let claimed = board
        .claimed_by(&rig)
        .await
        .expect("claimed_by should succeed");
    assert!(
        claimed.is_empty(),
        "stuck items should not appear in claimed_by()"
    );
}

// ── Scenario 5: Concurrent claim race ──

#[tokio::test]
async fn concurrent_claim_race() {
    let board = Arc::new(Board::in_memory().await.expect("board should initialize"));
    let item = board
        .post(post_req("race target"))
        .await
        .expect("post should succeed");

    let mut handles = Vec::new();
    for i in 0..10 {
        let board = Arc::clone(&board);
        let item_id = item.id;
        handles.push(tokio::spawn(async move {
            let rig = RigId::new(format!("racer-{i}"));
            board.claim(item_id, &rig).await
        }));
    }

    let mut successes = Vec::new();
    let mut failures = 0;
    for handle in handles {
        match handle.await.expect("task should not panic") {
            Ok(claimed) => successes.push(claimed),
            Err(BoardError::AlreadyClaimed { .. }) => failures += 1,
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    assert_eq!(successes.len(), 1, "exactly one racer should win");
    assert_eq!(failures, 9, "nine racers should get AlreadyClaimed");

    // Verify the item is claimed by exactly one rig
    let winner_rig = successes[0]
        .claimed_by
        .as_ref()
        .expect("winner should have claimed_by");
    let claimed = board
        .claimed_by(winner_rig)
        .await
        .expect("claimed_by should succeed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, item.id);
}
