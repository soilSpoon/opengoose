use super::*;
use std::sync::Mutex;
use std::thread;

#[test]
fn test_concurrent_dequeue_no_duplicates() {
    let db = test_db();
    ensure_session(&db, "s1");
    let mq = Arc::new(MessageQueue::new(db));

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
