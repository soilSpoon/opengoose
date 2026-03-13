use super::*;

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
fn test_broadcast_dedup_does_not_cross_runs() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

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

    let msgs = mq.read_broadcasts("run1", Some(0)).unwrap();
    assert_eq!(msgs.len(), 1);

    let msgs = mq.read_broadcasts("run1", Some(id)).unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn test_non_broadcast_not_deduplicated() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id1 = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "worker",
            "same content",
            MessageType::Task,
        )
        .unwrap();
    let id2 = mq
        .enqueue(
            "s1",
            "run1",
            "coder",
            "worker",
            "same content",
            MessageType::Task,
        )
        .unwrap();
    assert_ne!(id1, id2, "non-broadcast messages should not deduplicate");

    let msgs = mq.dequeue("worker", 10).unwrap();
    assert_eq!(msgs.len(), 2);
}

#[test]
fn test_read_broadcasts_ignores_other_message_types() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    mq.enqueue("s1", "run1", "a", "b", "task msg", MessageType::Task)
        .unwrap();
    mq.enqueue("s1", "run1", "a", "b", "result msg", MessageType::Result)
        .unwrap();
    mq.enqueue("s1", "run1", "a", "b", "deleg msg", MessageType::Delegation)
        .unwrap();
    mq.enqueue(
        "s1",
        "run1",
        "a",
        "broadcast",
        "bcast only",
        MessageType::Broadcast,
    )
    .unwrap();

    let broadcasts = mq.read_broadcasts("run1", None).unwrap();
    assert_eq!(broadcasts.len(), 1);
    assert_eq!(broadcasts[0].content, "bcast only");
}
