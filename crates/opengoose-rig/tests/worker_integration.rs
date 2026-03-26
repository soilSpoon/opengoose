//! Board → Worker integration tests.
//! Tests claim/submit/retry logic at the Board API level.
//! No LLM calls — pure state transitions.

use opengoose_board::Board;
use opengoose_board::work_item::{PostWorkItem, Priority, RigId, Status};
use std::sync::Arc;

fn post_req(title: &str) -> PostWorkItem {
    PostWorkItem {
        title: title.to_string(),
        description: format!("Description for {title}"),
        created_by: RigId::new("human"),
        priority: Priority::P1,
        tags: vec![],
    }
}

#[tokio::test]
async fn post_claim_submit_lifecycle() {
    let board = Board::in_memory().await.expect("board init should succeed");
    let worker_id = RigId::new("worker-1");
    board
        .register_rig("worker-1", "ai", None, None)
        .await
        .expect("register_rig should succeed");

    let item = board
        .post(post_req("test task"))
        .await
        .expect("post should succeed");
    assert_eq!(item.status, Status::Open);

    let claimed = board
        .claim(item.id, &worker_id)
        .await
        .expect("claim should succeed");
    assert_eq!(claimed.status, Status::Claimed);
    assert_eq!(claimed.claimed_by.as_ref(), Some(&worker_id));

    board
        .submit(item.id, &worker_id)
        .await
        .expect("submit should succeed");
    let done = board
        .get(item.id)
        .await
        .expect("get should succeed")
        .expect("item should exist");
    assert_eq!(done.status, Status::Done);
}

#[tokio::test]
async fn worker_skips_blocked_items() {
    let board = Board::in_memory().await.expect("board init should succeed");

    let blocker = board
        .post(post_req("blocker"))
        .await
        .expect("post should succeed");
    let blocked = board
        .post(post_req("blocked"))
        .await
        .expect("post should succeed");
    board
        .add_dependency(blocker.id, blocked.id)
        .await
        .expect("add_dependency should succeed");

    let ready = board.ready().await.expect("ready should succeed");
    let ready_ids: Vec<i64> = ready.iter().map(|i| i.id).collect();

    assert!(ready_ids.contains(&blocker.id), "blocker should be ready");
    assert!(
        !ready_ids.contains(&blocked.id),
        "blocked item should NOT be ready"
    );
}

#[tokio::test]
async fn claim_then_mark_stuck() {
    let board = Board::in_memory().await.expect("board init should succeed");
    let worker_id = RigId::new("worker-1");
    board
        .register_rig("worker-1", "ai", None, None)
        .await
        .expect("register_rig should succeed");

    let item = board
        .post(post_req("failing task"))
        .await
        .expect("post should succeed");
    board
        .claim(item.id, &worker_id)
        .await
        .expect("claim should succeed");

    board
        .mark_stuck(item.id, &worker_id)
        .await
        .expect("mark_stuck should succeed");

    let stuck = board
        .get(item.id)
        .await
        .expect("get should succeed")
        .expect("item should exist");
    assert_eq!(stuck.status, Status::Stuck);
}

#[tokio::test]
async fn concurrent_workers_no_double_claim() {
    let board = Arc::new(Board::in_memory().await.expect("board init should succeed"));
    let item = board
        .post(post_req("contested task"))
        .await
        .expect("post should succeed");

    for i in 0..2 {
        board
            .register_rig(&format!("w-{i}"), "ai", None, None)
            .await
            .expect("register_rig should succeed");
    }

    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let board1 = Arc::clone(&board);
    let board2 = Arc::clone(&board);
    let b1 = Arc::clone(&barrier);
    let b2 = Arc::clone(&barrier);

    let h1 = tokio::spawn(async move {
        b1.wait().await;
        board1.claim(item.id, &RigId::new("w-0")).await
    });
    let h2 = tokio::spawn(async move {
        b2.wait().await;
        board2.claim(item.id, &RigId::new("w-1")).await
    });

    let (r1, r2) = tokio::join!(h1, h2);
    let r1 = r1.expect("task should not panic");
    let r2 = r2.expect("task should not panic");

    let successes = [r1.is_ok(), r2.is_ok()];
    assert_eq!(
        successes.iter().filter(|&&s| s).count(),
        1,
        "exactly one worker should claim successfully"
    );
}
