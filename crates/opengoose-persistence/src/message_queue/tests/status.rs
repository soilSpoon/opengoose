use super::*;

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
fn test_fail_immediate_dead_when_max_retries_is_one() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "error1").unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    assert_eq!(msgs[0].retry_count, 1);

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "error2").unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    assert_eq!(msgs[0].retry_count, 2);

    mq.dequeue("worker", 10).unwrap();
    mq.fail(id, "final error").unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Dead);
    assert_eq!(msgs[0].retry_count, 3);
    assert_eq!(msgs[0].error.as_deref(), Some("final error"));
}

#[test]
fn test_status_transitions_pending_to_processing_to_completed() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = MessageQueue::new(db);

    let id = mq
        .enqueue("s1", "run1", "a", "worker", "task", MessageType::Task)
        .unwrap();

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
    assert!(msgs[0].processed_at.is_none());

    mq.dequeue("worker", 10).unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Processing);
    assert!(msgs[0].processed_at.is_some());

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

    mq.dequeue("worker", 10).unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Processing);

    mq.fail(id, "transient error").unwrap();
    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].status, MessageStatus::Pending);
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

    let msgs = mq.list_for_run("run1").unwrap();
    assert_eq!(msgs[0].error.as_deref(), Some("second error"));
    assert_eq!(msgs[0].retry_count, 2);
}
