use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{AlertStore, Database, MessageQueue, MessageType, OrchestrationStore};

/// Create an in-memory DB populated with `n_runs` orchestration runs
/// (mix of running/completed/failed/error) and `n_queue` pending queue messages.
fn populated_db(n_runs: usize, n_queue: usize) -> Arc<Database> {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let orch = OrchestrationStore::new(Arc::clone(&db));
    let queue = MessageQueue::new(Arc::clone(&db));

    for i in 0..n_runs {
        let run_id = format!("run-{i}");
        let session_key = format!("discord:guild-{i}:chan-{i}");
        orch.create_run(&run_id, &session_key, "bench-team", "chain", "{}", 3)
            .unwrap();
        match i % 4 {
            0 => orch.complete_run(&run_id, "ok").unwrap(),
            1 => orch.fail_run(&run_id, "fail").unwrap(),
            // 'error' status requires update_status directly; leave remaining
            // runs as 'running' to still exercise the COUNT WHERE status='error' path.
            _ => {}
        }
    }

    for i in 0..n_queue {
        let session_key = format!("discord:guild-{i}:chan-{i}");
        queue
            .enqueue(
                &session_key,
                &format!("run-q-{i}"),
                "sender",
                "recipient",
                &format!("task {i}"),
                MessageType::Task,
            )
            .unwrap();
    }

    db
}

fn bench_current_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("alerts_current_metrics");

    for (n_runs, n_queue) in [(0usize, 0usize), (50, 10), (200, 50)] {
        let db = populated_db(n_runs, n_queue);
        let store = AlertStore::new(Arc::clone(&db));

        group.bench_with_input(
            BenchmarkId::new("current_metrics", format!("{n_runs}runs_{n_queue}queue")),
            &store,
            |b, s| b.iter(|| s.current_metrics().unwrap()),
        );
    }

    group.finish();
}

criterion_group!(benches, bench_current_metrics);
criterion_main!(benches);
