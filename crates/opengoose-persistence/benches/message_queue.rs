use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{Database, MessageQueue, MessageType, SessionStore};
use opengoose_types::{Platform, SessionKey};

/// Session key used across all benchmarks.
fn bench_key() -> SessionKey {
    SessionKey::new(Platform::Discord, "bench-guild", "bench-channel")
}

/// Derive the stable string representation used as `session_key` in the DB.
fn bench_session_key_str() -> String {
    bench_key().to_stable_id()
}

/// Create an in-memory DB, ensure a session row exists (via SessionStore upsert),
/// and return the MessageQueue ready for benchmarking.
fn setup() -> (Arc<Database>, MessageQueue) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    // Upsert a session so FK on message_queue.session_key is satisfied.
    let ss = SessionStore::new(Arc::clone(&db));
    ss.append_user_message(&bench_key(), "seed", None).unwrap();
    let mq = MessageQueue::new(Arc::clone(&db));
    (db, mq)
}

fn bench_enqueue_task(c: &mut Criterion) {
    let (_db, mq) = setup();
    let session_key = bench_session_key_str();

    c.bench_function("mq_enqueue_task", |b| {
        b.iter(|| {
            mq.enqueue(
                &session_key,
                "run-bench",
                "planner",
                "coder",
                "implement feature X",
                MessageType::Task,
            )
            .unwrap()
        });
    });
}

fn bench_enqueue_broadcast(c: &mut Criterion) {
    let (_db, mq) = setup();
    let session_key = bench_session_key_str();

    c.bench_function("mq_enqueue_broadcast", |b| {
        let mut i = 0u64;
        b.iter(|| {
            // Use unique content so broadcast dedup does not short-circuit every call.
            i += 1;
            mq.enqueue(
                &session_key,
                "run-bench",
                "coder",
                "broadcast",
                &format!("status update {i}"),
                MessageType::Broadcast,
            )
            .unwrap()
        });
    });
}

fn bench_dequeue(c: &mut Criterion) {
    let mut group = c.benchmark_group("mq_dequeue");

    for limit in [1usize, 10, 50] {
        group.bench_with_input(BenchmarkId::from_parameter(limit), &limit, |b, &limit| {
            let (_db, mq) = setup();
            let session_key = bench_session_key_str();
            // Pre-fill the queue.
            for i in 0..limit {
                mq.enqueue(
                    &session_key,
                    "run-bench",
                    "planner",
                    "coder",
                    &format!("task-{i}"),
                    MessageType::Task,
                )
                .unwrap();
            }
            b.iter(|| {
                // Dequeue then re-enqueue to keep queue populated.
                let msgs = mq.dequeue("coder", limit).unwrap();
                for m in &msgs {
                    mq.complete(m.id).unwrap();
                    mq.enqueue(
                        &session_key,
                        "run-bench",
                        "planner",
                        "coder",
                        &m.content,
                        MessageType::Task,
                    )
                    .unwrap();
                }
            });
        });
    }

    group.finish();
}

fn bench_stats(c: &mut Criterion) {
    let (_db, mq) = setup();
    let session_key = bench_session_key_str();
    // Pre-populate with a mix of statuses.
    for i in 0..100 {
        let id = mq
            .enqueue(
                &session_key,
                "run-bench",
                "planner",
                "coder",
                &format!("task-{i}"),
                MessageType::Task,
            )
            .unwrap();
        if i % 3 == 0 {
            let msgs = mq.dequeue("coder", 1).unwrap();
            if let Some(m) = msgs.first() {
                mq.complete(m.id).unwrap();
            }
        } else if i % 5 == 0 {
            let _ = mq.dequeue("coder", 1);
            mq.fail(id, "transient error").unwrap();
        }
    }

    c.bench_function("mq_stats", |b| {
        b.iter(|| mq.stats().unwrap());
    });
}

fn bench_read_broadcasts(c: &mut Criterion) {
    let (_db, mq) = setup();
    let session_key = bench_session_key_str();
    for i in 0..50 {
        mq.enqueue(
            &session_key,
            "run-bench",
            &format!("agent-{i}"),
            "broadcast",
            &format!("broadcast message {i}"),
            MessageType::Broadcast,
        )
        .unwrap();
    }

    c.bench_function("mq_read_broadcasts_50", |b| {
        b.iter(|| mq.read_broadcasts("run-bench", None).unwrap());
    });
}

criterion_group!(
    benches,
    bench_enqueue_task,
    bench_enqueue_broadcast,
    bench_dequeue,
    bench_stats,
    bench_read_broadcasts,
);
criterion_main!(benches);
