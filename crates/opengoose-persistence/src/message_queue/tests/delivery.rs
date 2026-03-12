use super::*;

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

    let msgs = mq.dequeue("reviewer", 10).unwrap();
    assert!(msgs.is_empty());

    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "fix this bug");
    assert_eq!(msgs[0].status, MessageStatus::Pending);

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

    mq.fail(id, "timeout").unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].retry_count, 1);

    mq.fail(id, "timeout 2").unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].retry_count, 2);

    mq.fail(id, "timeout 3").unwrap();
    let msgs = mq.dequeue("coder", 10).unwrap();
    assert!(msgs.is_empty());
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
fn test_large_payload_enqueue_dequeue() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

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
fn test_dequeue_zero_limit() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    let msgs = mq.dequeue("worker", 0).unwrap();
    assert!(msgs.is_empty());

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
fn test_ordering_within_channel_fifo_across_senders() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

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

#[test]
fn test_enqueue_unicode_and_special_chars() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let unicode_content = "こんにちは 🦀 émojis: 💾🔧 中文测试";
    let id = mq
        .enqueue(
            "s1",
            "run1",
            "发送者",
            "受信者",
            unicode_content,
            MessageType::Task,
        )
        .unwrap();
    assert!(id > 0);

    let msgs = mq.dequeue("受信者", 10).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, unicode_content);
    assert_eq!(msgs[0].sender, "发送者");
    assert_eq!(msgs[0].recipient, "受信者");
}

#[test]
fn test_complete_idempotent() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();
    mq.dequeue("worker", 10).unwrap();
    mq.complete(id).unwrap();
    mq.complete(id).unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Completed);
}

#[test]
fn test_enqueue_multiple_sessions_same_run() {
    let db = test_db();
    ensure_session(&db, "sess_a");
    ensure_session(&db, "sess_b");
    let mq = MessageQueue::new(db);

    mq.enqueue(
        "sess_a",
        "run1",
        "sender",
        "worker",
        "from a",
        MessageType::Task,
    )
    .unwrap();
    mq.enqueue(
        "sess_b",
        "run1",
        "sender",
        "worker",
        "from b",
        MessageType::Task,
    )
    .unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs.len(), 2);

    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 2);
}
