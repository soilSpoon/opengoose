use super::*;

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
fn test_stats_all_statuses() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "w1", "p1", MessageType::Task)
        .unwrap();

    mq.enqueue("s1", "run1", "a", "w2", "proc1", MessageType::Task)
        .unwrap();
    mq.dequeue("w2", 10).unwrap();

    let cid = mq
        .enqueue("s1", "run1", "a", "w3", "c1", MessageType::Task)
        .unwrap();
    mq.dequeue("w3", 10).unwrap();
    mq.complete(cid).unwrap();

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
fn test_list_recent_empty_queue() {
    let db = test_db();
    let mq = MessageQueue::new(db);

    let recent = mq.list_recent(10).unwrap();
    assert!(recent.is_empty());
}

#[test]
fn test_list_for_run_empty_db() {
    let db = test_db();
    let mq = MessageQueue::new(db);

    let msgs = mq.list_for_run("nonexistent").unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_stats_across_multiple_runs() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "w1", "p1", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "a", "w1", "p2", MessageType::Task)
        .unwrap();

    let cid = mq
        .enqueue("s1", "run2", "a", "w2", "c1", MessageType::Task)
        .unwrap();
    mq.dequeue("w2", 10).unwrap();
    mq.complete(cid).unwrap();

    let stats = mq.stats().unwrap();
    assert_eq!(stats.pending, 2);
    assert_eq!(stats.completed, 1);
}
