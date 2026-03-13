use super::*;

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
fn test_dequeue_delegations_limit_zero() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "b", "del", MessageType::Delegation)
        .unwrap();

    let msgs = mq.dequeue_delegations("run1", 0).unwrap();
    assert!(msgs.is_empty());

    let msgs = mq.dequeue_delegations("run1", 10).unwrap();
    assert_eq!(msgs.len(), 1);
}

#[test]
fn test_get_dead_letters_excludes_other_statuses() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "w1", "pending", MessageType::Task)
        .unwrap();
    let cid = mq
        .enqueue("s1", "run1", "a", "w2", "completed", MessageType::Task)
        .unwrap();
    mq.dequeue("w2", 10).unwrap();
    mq.complete(cid).unwrap();
    mq.enqueue("s1", "run1", "a", "w3", "processing", MessageType::Task)
        .unwrap();
    mq.dequeue("w3", 10).unwrap();

    let dead = mq.get_dead_letters("run1").unwrap();
    assert!(dead.is_empty(), "no dead messages should exist");
}

#[test]
fn test_dequeue_delegations_limit_respects_bound() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    for i in 0..5 {
        mq.enqueue(
            "s1",
            "run1",
            "sender",
            &format!("r{i}"),
            &format!("del{i}"),
            MessageType::Delegation,
        )
        .unwrap();
    }

    let msgs = mq.dequeue_delegations("run1", 2).unwrap();
    assert_eq!(msgs.len(), 2);

    let msgs = mq.dequeue_delegations("run1", 10).unwrap();
    assert_eq!(msgs.len(), 3);
}
