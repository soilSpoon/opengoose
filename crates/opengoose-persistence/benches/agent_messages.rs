use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{AgentMessageStore, Database};

const SK: &str = "discord:guild123:channel456";

fn setup() -> AgentMessageStore {
    let db = Arc::new(Database::open_in_memory().unwrap());
    AgentMessageStore::new(db)
}

fn bench_send_directed(c: &mut Criterion) {
    let store = setup();

    c.bench_function("agent_msg_send_directed", |b| {
        b.iter(|| {
            store
                .send_directed(SK, "agent-planner", "agent-coder", "implement feature X")
                .unwrap()
        });
    });
}

fn bench_publish_channel(c: &mut Criterion) {
    let store = setup();

    c.bench_function("agent_msg_publish_channel", |b| {
        b.iter(|| {
            store
                .publish(SK, "agent-planner", "announcements", "status update")
                .unwrap()
        });
    });
}

fn bench_receive_pending(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_msg_receive_pending");

    for n in [1usize, 10, 50] {
        let store = setup();
        for i in 0..n {
            store
                .send_directed(SK, "sender", "receiver", &format!("task-{i}"))
                .unwrap();
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| store.receive_pending(SK, "receiver").unwrap());
        });
    }

    group.finish();
}

fn bench_channel_history(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_msg_channel_history");

    for n in [10usize, 100, 500] {
        let store = setup();
        let mut last_id = 0i32;
        for i in 0..n {
            last_id = store
                .publish(SK, "broadcaster", "events", &format!("event-{i}"))
                .unwrap();
        }
        let since_id = Some(last_id / 2);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| store.channel_history(SK, "events", since_id).unwrap());
        });
    }

    group.finish();
}

fn bench_list_recent(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_msg_list_recent");

    for limit in [10i64, 50, 100] {
        let store = setup();
        for i in 0..200 {
            if i % 2 == 0 {
                store
                    .send_directed(SK, "agent-a", "agent-b", &format!("msg-{i}"))
                    .unwrap();
            } else {
                store
                    .publish(SK, "agent-a", "general", &format!("broadcast-{i}"))
                    .unwrap();
            }
        }

        group.bench_with_input(BenchmarkId::from_parameter(limit), &limit, |b, &limit| {
            b.iter(|| store.list_recent(SK, limit).unwrap());
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_send_directed,
    bench_publish_channel,
    bench_receive_pending,
    bench_channel_history,
    bench_list_recent,
);
criterion_main!(benches);
