use std::sync::Arc;

use diesel::prelude::*;

use crate::db::Database;
use crate::models::NewSession;
use crate::schema::sessions;

use super::*;

fn test_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

/// Ensure a session row exists so FK constraints are satisfied.
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
fn test_enqueue_dequeue() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue(
            "sess1",
            "run1",
            "user",
            "coder",
            "fix this bug",
            MessageType::Task,
        )
        .unwrap();
    assert!(id > 0);

    // Dequeue for wrong recipient → empty
    let msgs = mq.dequeue("reviewer", 10).unwrap();
    assert!(msgs.is_empty());

    // Dequeue for correct recipient
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "fix this bug");
    assert_eq!(msgs[0].status, MessageStatus::Pending);

    // Dequeue again → empty (already processing)
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_complete() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("sess1", "run1", "user", "coder", "task1", MessageType::Task)
        .unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);

    mq.complete(id).unwrap();

    let msgs = mq.dequeue("coder", 10).unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_fail_and_retry() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("sess1", "run1", "user", "coder", "task1", MessageType::Task)
        .unwrap();
    mq.dequeue("coder", 10).unwrap();

    // Fail → should go back to pending (retry_count 1 < max_retries 3)
    mq.fail(id, "timeout").unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].retry_count, 1);

    // Fail again
    mq.fail(id, "timeout 2").unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].retry_count, 2);

    // Fail third time → dead
    mq.fail(id, "timeout 3").unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert!(msgs.is_empty()); // dead-lettered
}

#[test]
fn test_broadcasts() {
    let db = test_db();
    ensure_session(&db, "sess1");
    let mq = MessageQueue::new(db);

    mq.enqueue(
        "sess1",
        "run1",
        "coder",
        "broadcast",
        "found issue in auth",
        MessageType::Broadcast,
    )
    .unwrap();
    let id2 = mq
        .enqueue(
            "sess1",
            "run1",
            "reviewer",
            "broadcast",
            "tests are passing",
            MessageType::Broadcast,
        )
        .unwrap();
    // Different run
    mq.enqueue(
        "sess1",
        "run2",
        "coder",
        "broadcast",
        "other run",
        MessageType::Broadcast,
    )
    .unwrap();

    let broadcasts = mq.read_broadcasts("run1", None).unwrap();
    assert_eq!(broadcasts.len(), 2);
    assert_eq!(broadcasts[0].content, "found issue in auth");

    // Since id → only newer
    let broadcasts = mq.read_broadcasts("run1", Some(id2 - 1)).unwrap();
    assert_eq!(broadcasts.len(), 1);
    assert_eq!(broadcasts[0].content, "tests are passing");
}

#[test]
fn test_broadcast_deduplication() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id1 = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "broadcast",
            "found bug",
            MessageType::Broadcast,
        )
        .unwrap();
    let id2 = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "broadcast",
            "found bug",
            MessageType::Broadcast,
        )
        .unwrap();
    assert_eq!(id1, id2);

    let broadcasts = mq.read_broadcasts("run1", None).unwrap();
    assert_eq!(broadcasts.len(), 1);

    // Different sender, same content → not a duplicate
    mq.enqueue(
        "s1",
        "run1",
        "reviewer",
        "broadcast",
        "found bug",
        MessageType::Broadcast,
    )
    .unwrap();
    let broadcasts = mq.read_broadcasts("run1", None).unwrap();
    assert_eq!(broadcasts.len(), 2);

    // Same sender, different content → not a duplicate
    mq.enqueue(
        "s1",
        "run1",
        "coder",
        "broadcast",
        "found another bug",
        MessageType::Broadcast,
    )
    .unwrap();
    let broadcasts = mq.read_broadcasts("run1", None).unwrap();
    assert_eq!(broadcasts.len(), 3);
}

#[test]
fn test_dequeue_delegations() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue(
        "s1",
        "run1",
        "coder",
        "reviewer",
        "check auth",
        MessageType::Delegation,
    )
    .unwrap();
    mq.enqueue(
        "s1",
        "run1",
        "coder",
        "tester",
        "run tests",
        MessageType::Delegation,
    )
    .unwrap();
    mq.enqueue("s1", "run1", "user", "coder", "fix bug", MessageType::Task)
        .unwrap();
    mq.enqueue(
        "s1",
        "run2",
        "coder",
        "reviewer",
        "other run",
        MessageType::Delegation,
    )
    .unwrap();

    let msgs = mq.dequeue_delegations("run1", 10).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].content, "check auth");
    assert_eq!(msgs[0].recipient, "reviewer");
    assert_eq!(msgs[1].content, "run tests");
    assert_eq!(msgs[1].recipient, "tester");

    let msgs = mq.dequeue_delegations("run1", 10).unwrap();
    assert!(msgs.is_empty());

    let msgs = mq.dequeue_delegations("run2", 10).unwrap();
    assert_eq!(msgs.len(), 1);
}

#[test]
fn test_dequeue_delegations_only_pending() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id1 = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "reviewer",
            "msg1",
            MessageType::Delegation,
        )
        .unwrap();
    mq.enqueue(
        "s1",
        "run1",
        "coder",
        "tester",
        "msg2",
        MessageType::Delegation,
    )
    .unwrap();

    let msgs = mq.dequeue_delegations("run1", 1).unwrap();
    assert_eq!(msgs.len(), 1);
    mq.complete(id1).unwrap();

    let msgs = mq.dequeue_delegations("run1", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "msg2");
}

#[test]
fn test_get_dead_letters() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "reviewer",
            "bad task",
            MessageType::Delegation,
        )
        .unwrap();

    mq.dequeue("reviewer", 10).unwrap();
    mq.fail(id, "err1").unwrap();
    mq.dequeue("reviewer", 10).unwrap();
    mq.fail(id, "err2").unwrap();
    mq.dequeue("reviewer", 10).unwrap();
    mq.fail(id, "err3").unwrap();

    let dead = mq.get_dead_letters("run1").unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].content, "bad task");
    assert_eq!(dead[0].status, MessageStatus::Dead);

    let dead = mq.get_dead_letters("run2").unwrap();
    assert!(dead.is_empty());
}

#[test]
fn test_message_status_as_str() {
    assert_eq!(MessageStatus::Pending.as_str(), "pending");
    assert_eq!(MessageStatus::Processing.as_str(), "processing");
    assert_eq!(MessageStatus::Completed.as_str(), "completed");
    assert_eq!(MessageStatus::Failed.as_str(), "failed");
    assert_eq!(MessageStatus::Dead.as_str(), "dead");
}

#[test]
fn test_message_status_parse_roundtrip() {
    for s in [
        MessageStatus::Pending,
        MessageStatus::Processing,
        MessageStatus::Completed,
        MessageStatus::Failed,
        MessageStatus::Dead,
    ] {
        assert_eq!(MessageStatus::parse(s.as_str()).unwrap(), s);
    }
}

#[test]
fn test_message_status_parse_invalid() {
    let err = MessageStatus::parse("unknown").unwrap_err();
    assert!(err.to_string().contains("MessageStatus"));
}

#[test]
fn test_message_type_as_str() {
    assert_eq!(MessageType::Task.as_str(), "task");
    assert_eq!(MessageType::Result.as_str(), "result");
    assert_eq!(MessageType::Delegation.as_str(), "delegation");
    assert_eq!(MessageType::Broadcast.as_str(), "broadcast");
}

#[test]
fn test_message_type_parse_roundtrip() {
    for t in [
        MessageType::Task,
        MessageType::Result,
        MessageType::Delegation,
        MessageType::Broadcast,
    ] {
        assert_eq!(MessageType::parse(t.as_str()).unwrap(), t);
    }
}

#[test]
fn test_message_type_parse_invalid() {
    let err = MessageType::parse("bogus").unwrap_err();
    assert!(err.to_string().contains("MessageType"));
}

#[test]
fn test_list_for_run() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "b", "msg1", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "c", "d", "msg2", MessageType::Delegation)
        .unwrap();
    mq.enqueue("s1", "run2", "e", "f", "msg3", MessageType::Task)
        .unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs.len(), 2);

    let msgs = mq.list_for_run("run2").unwrap();
    assert_eq!(msgs.len(), 1);

    let msgs = mq.list_for_run("nonexistent").unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_list_recent_and_stats() {
    let db = test_db();
    ensure_session(&db, "sess1");
    ensure_session(&db, "sess2");
    let mq = MessageQueue::new(db);

    let first = mq
        .enqueue(
            "sess1",
            "run1",
            "planner",
            "coder",
            "draft implementation",
            MessageType::Task,
        )
        .unwrap();
    let second = mq
        .enqueue(
            "sess2",
            "run2",
            "coder",
            "reviewer",
            "request review",
            MessageType::Delegation,
        )
        .unwrap();

    mq.complete(first).unwrap();
    mq.dequeue("reviewer", 10).unwrap();
    mq.fail(second, "temporary issue").unwrap();

    let recent = mq.list_recent(10).unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].team_run_id, "run2");
    assert_eq!(recent[1].team_run_id, "run1");

    let stats = mq.stats().unwrap();
    assert_eq!(stats.pending, 1);
    assert_eq!(stats.completed, 1);
    assert_eq!(stats.processing, 0);
    assert_eq!(stats.dead, 0);
}

#[test]
fn test_dequeue_limit_respected() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    for i in 0..5 {
        mq.enqueue(
            "s1",
            "run1",
            "sender",
            "coder",
            &format!("msg{i}"),
            MessageType::Task,
        )
        .unwrap();
    }

    let msgs = mq.dequeue("coder", 2).unwrap();
    assert_eq!(msgs.len(), 2);

    // The remaining 3 are still pending
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 3);
}

#[test]
fn test_fail_stores_error_message() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "sender", "worker", "task", MessageType::Task)
        .unwrap();
    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "connection refused").unwrap();

    // Message retried — check error stored by fetching via list_for_run
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].error.as_deref(), Some("connection refused"));
    assert_eq!(msgs[0].retry_count, 1);
}

#[test]
fn test_result_message_type() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue(
            "s1",
            "run1",
            "worker",
            "planner",
            "done",
            MessageType::Result,
        )
        .unwrap();
    assert!(id > 0);

    let msgs = mq.dequeue("planner", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].msg_type, MessageType::Result);
    assert_eq!(msgs[0].content, "done");
}

#[test]
fn test_dequeue_fifo_ordering() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "worker", "first", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "b", "worker", "second", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "c", "worker", "third", MessageType::Task)
        .unwrap();

    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].content, "first");
    assert_eq!(msgs[1].content, "second");
    assert_eq!(msgs[2].content, "third");
}

#[test]
fn test_stats_all_statuses() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    // pending
    mq.enqueue("s1", "run1", "a", "w1", "p1", MessageType::Task)
        .unwrap();

    // processing (dequeued but not finished)
    mq.enqueue("s1", "run1", "a", "w2", "proc1", MessageType::Task)
        .unwrap();
    mq.dequeue("w2", 10).unwrap();

    // completed
    let cid = mq
        .enqueue("s1", "run1", "a", "w3", "c1", MessageType::Task)
        .unwrap();
    mq.dequeue("w3", 10).unwrap();
    mq.complete(cid).unwrap();

    // dead (exhaust retries: max_retries=3)
    let did = mq
        .enqueue("s1", "run1", "a", "w4", "d1", MessageType::Task)
        .unwrap();
    mq.dequeue("w4", 10).unwrap();
    mq.fail(did, "e").unwrap();
    mq.dequeue("w4", 10).unwrap();
    mq.fail(did, "e").unwrap();
    mq.dequeue("w4", 10).unwrap();
    mq.fail(did, "e").unwrap();

    let stats = mq.stats().unwrap();
    assert_eq!(stats.pending, 1);
    assert_eq!(stats.processing, 1);
    assert_eq!(stats.completed, 1);
    assert_eq!(stats.dead, 1);
    assert_eq!(stats.failed, 0);
}

#[test]
fn test_list_recent_limit() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    for i in 0..5 {
        mq.enqueue(
            "s1",
            "run1",
            "a",
            "b",
            &format!("msg{i}"),
            MessageType::Task,
        )
        .unwrap();
    }

    let recent = mq.list_recent(3).unwrap();
    assert_eq!(recent.len(), 3);
}

#[test]
fn test_broadcast_dedup_does_not_cross_runs() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    // Same sender and content but different team_run_id → not a duplicate
    let id1 = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "broadcast",
            "same content",
            MessageType::Broadcast,
        )
        .unwrap();
    let id2 = mq
        .enqueue(
            "s1",
            "run2",
            "coder",
            "broadcast",
            "same content",
            MessageType::Broadcast,
        )
        .unwrap();

    assert_ne!(id1, id2);

    let b1 = mq.read_broadcasts("run1", None).unwrap();
    assert_eq!(b1.len(), 1);

    let b2 = mq.read_broadcasts("run2", None).unwrap();
    assert_eq!(b2.len(), 1);
}

#[test]
fn test_read_broadcasts_since_id_zero() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "broadcast",
            "hello",
            MessageType::Broadcast,
        )
        .unwrap();

    // since_id=0 returns all messages (id > 0)
    let msgs = mq.read_broadcasts("run1", Some(0)).unwrap();
    assert_eq!(msgs.len(), 1);

    // since_id equal to the message id → empty (strictly greater than)
    let msgs = mq.read_broadcasts("run1", Some(id)).unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_fail_immediate_dead_when_max_retries_is_one() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    // Default max_retries is 3, so we need to exhaust them
    // First fail: retry_count becomes 1, max_retries=3, 1 < 3 → pending
    // Second fail: retry_count becomes 2, max_retries=3, 2 < 3 → pending
    // Third fail: retry_count becomes 3, max_retries=3, 3 >= 3 → dead
    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "error1").unwrap();

    // Still pending after first failure
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    assert_eq!(msgs[0].retry_count, 1);

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "error2").unwrap();

    // Still pending after second failure
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    assert_eq!(msgs[0].retry_count, 2);

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "final error").unwrap();

    // Dead after third failure
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Dead);
    assert_eq!(msgs[0].retry_count, 3);
    assert_eq!(msgs[0].error.as_deref(), Some("final error"));
}

#[test]
fn test_concurrent_dequeue_no_duplicates() {
    use std::sync::Mutex;
    use std::thread;

    let db = test_db();
    ensure_session(&db, "s1");
    let mq = Arc::new(MessageQueue::new(db));

    // Enqueue 10 messages for the same recipient
    for i in 0..10 {
        mq.enqueue(
            "s1",
            "run1",
            "sender",
            "worker",
            &format!("task{i}"),
            MessageType::Task,
        )
        .unwrap();
    }

    let collected: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(Vec::new()));

    // Spawn 5 threads each dequeuing up to 3 messages
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let mq = Arc::clone(&mq);
            let collected = Arc::clone(&collected);
            thread::spawn(move || {
                let msgs = mq.dequeue("worker", 3).unwrap();
                let ids: Vec<i32> = msgs.iter().map(|m| m.id).collect();
                collected.lock().unwrap().extend(ids);
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let mut ids = collected.lock().unwrap().clone();
    ids.sort();

    // Every message was dequeued exactly once (no duplicates)
    assert_eq!(ids.len(), 10);
    let deduped: Vec<i32> = {
        let mut d = ids.clone();
        d.dedup();
        d
    };
    assert_eq!(
        ids, deduped,
        "concurrent dequeue produced duplicate messages"
    );
}

#[test]
fn test_large_payload_enqueue_dequeue() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    // 512 KB payload
    let large_content = "x".repeat(512 * 1024);

    let id = mq
        .enqueue(
            "s1",
            "run1",
            "sender",
            "worker",
            &large_content,
            MessageType::Task,
        )
        .unwrap();
    assert!(id > 0);

    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content.len(), 512 * 1024);
    assert_eq!(msgs[0].content, large_content);
}

#[test]
fn test_status_transitions_pending_to_processing_to_completed() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    // Initially Pending
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    assert!(msgs[0].processed_at.is_none());

    // After dequeue → Processing
    mq.dequeue("worker", 10).unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Processing);
    assert!(msgs[0].processed_at.is_some());

    // After complete → Completed
    mq.complete(id).unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Completed);
}

#[test]
fn test_status_transitions_processing_to_pending_on_fail() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    // Pending → Processing
    mq.dequeue("worker", 10).unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Processing);

    // Processing → Pending (retry)
    mq.fail(id, "transient error").unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    // processed_at is cleared on retry
    assert!(msgs[0].processed_at.is_none());
    assert_eq!(msgs[0].retry_count, 1);
    assert_eq!(msgs[0].error.as_deref(), Some("transient error"));
}

#[test]
fn test_fail_preserves_last_error_across_retries() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "first error").unwrap();

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "second error").unwrap();

    // The latest error should overwrite the previous one
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].error.as_deref(), Some("second error"));
    assert_eq!(msgs[0].retry_count, 2);
}

#[test]
fn test_dequeue_zero_limit() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    let msgs = mq.dequeue("worker", 0).unwrap();
    assert!(msgs.is_empty());

    // Message remains pending (not consumed)
    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 1);
}

#[test]
fn test_enqueue_empty_content() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "sender", "worker", "", MessageType::Task)
        .unwrap();
    assert!(id > 0);

    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "");
}

#[test]
fn test_stats_empty_queue() {
    let db = test_db();
    let mq = MessageQueue::new(db);

    let stats = mq.stats().unwrap();
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.processing, 0);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.dead, 0);
}

#[test]
fn test_multiple_message_types_in_single_run() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "b", "task msg", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "b", "a", "result msg", MessageType::Result)
        .unwrap();
    mq.enqueue("s1", "run1", "a", "c", "deleg msg", MessageType::Delegation)
        .unwrap();
    mq.enqueue(
        "s1",
        "run1",
        "a",
        "broadcast",
        "bcast msg",
        MessageType::Broadcast,
    )
    .unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs.len(), 4);

    let types: Vec<&MessageType> = msgs.iter().map(|m| &m.msg_type).collect();
    assert!(types.contains(&&MessageType::Task));
    assert!(types.contains(&&MessageType::Result));
    assert!(types.contains(&&MessageType::Delegation));
    assert!(types.contains(&&MessageType::Broadcast));
}

#[test]
fn test_dequeue_isolation_by_recipient() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    // Two recipients each have their own messages
    for i in 0..3 {
        mq.enqueue(
            "s1",
            "run1",
            "sender",
            "worker_a",
            &format!("a{i}"),
            MessageType::Task,
        )
        .unwrap();
        mq.enqueue(
            "s1",
            "run1",
            "sender",
            "worker_b",
            &format!("b{i}"),
            MessageType::Task,
        )
        .unwrap();
    }

    let msgs_a = mq.dequeue("worker_a", 10).unwrap();
    assert_eq!(msgs_a.len(), 3);
    assert!(msgs_a.iter().all(|m| m.recipient == "worker_a"));

    let msgs_b = mq.dequeue("worker_b", 10).unwrap();
    assert_eq!(msgs_b.len(), 3);
    assert!(msgs_b.iter().all(|m| m.recipient == "worker_b"));
}

#[test]
fn test_dead_letter_error_stored_on_final_fail() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "err1").unwrap();
    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "err2").unwrap();
    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "terminal failure").unwrap();

    let dead = mq.get_dead_letters("run1").unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].status, MessageStatus::Dead);
    assert_eq!(dead[0].error.as_deref(), Some("terminal failure"));
    assert_eq!(dead[0].retry_count, 3);
}

#[test]
fn test_ordering_within_channel_fifo_across_senders() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    // Messages from different senders to the same recipient should still be FIFO
    mq.enqueue("s1", "run1", "alpha", "worker", "first", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "beta", "worker", "second", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "gamma", "worker", "third", MessageType::Task)
        .unwrap();

    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].content, "first");
    assert_eq!(msgs[1].content, "second");
    assert_eq!(msgs[2].content, "third");
}
