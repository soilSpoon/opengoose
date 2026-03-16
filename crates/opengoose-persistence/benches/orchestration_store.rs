use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{Database, OrchestrationStore, RunStatus};

/// Create an in-memory DB and return an OrchestrationStore ready for use.
fn setup() -> (Arc<Database>, OrchestrationStore) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = OrchestrationStore::new(Arc::clone(&db));
    (db, store)
}

/// Populate a store with `n` runs distributed across three statuses.
///
/// The session-upsert inside `create_run` handles FK requirements, so no
/// separate SessionStore setup is required.
fn populated_store(n: usize) -> (Arc<Database>, OrchestrationStore) {
    let (db, store) = setup();
    for i in 0..n {
        let run_id = format!("run-bench-{i}");
        let session_key = format!("discord:guild-{i}:chan-{i}");
        store
            .create_run(&run_id, &session_key, "bench-team", "chain", "{}", 3)
            .unwrap();
        // Roughly 1/3 completed, 1/5 failed, rest remain running.
        if i % 3 == 0 {
            store.complete_run(&run_id, "ok").unwrap();
        } else if i % 5 == 0 {
            store.fail_run(&run_id, "error").unwrap();
        }
    }
    (db, store)
}

// ── Writes ────────────────────────────────────────────────────────────────────

fn bench_create_run(c: &mut Criterion) {
    let (_db, store) = setup();
    let mut counter = 0u64;

    c.bench_function("or_create_run", |b| {
        b.iter(|| {
            counter += 1;
            store
                .create_run(
                    &format!("run-{counter}"),
                    &format!("discord:guild-{counter}:chan-{counter}"),
                    "bench-team",
                    "chain",
                    "{}",
                    3,
                )
                .unwrap();
        });
    });
}

fn bench_complete_run(c: &mut Criterion) {
    let (_db, store) = setup();
    // Pre-create a single run; we re-complete it each iteration (idempotent UPDATE).
    store
        .create_run(
            "run-complete",
            "discord:guild-0:chan-0",
            "team",
            "chain",
            "{}",
            1,
        )
        .unwrap();

    c.bench_function("or_complete_run", |b| {
        b.iter(|| store.complete_run("run-complete", "ok").unwrap());
    });
}

fn bench_fail_run(c: &mut Criterion) {
    let (_db, store) = setup();
    // Pre-create a single run; we re-fail it each iteration (idempotent UPDATE).
    store
        .create_run(
            "run-fail",
            "discord:guild-0:chan-0",
            "team",
            "chain",
            "{}",
            1,
        )
        .unwrap();

    c.bench_function("or_fail_run", |b| {
        b.iter(|| store.fail_run("run-fail", "transient error").unwrap());
    });
}

// ── Reads ─────────────────────────────────────────────────────────────────────

fn bench_list_runs_all(c: &mut Criterion) {
    // list_runs(None, i64::MAX) is the dashboard hot-path: it returns ALL rows
    // ordered by updated_at DESC (no status filter). This is the exact query
    // that the updated_at index in PR #321 targets.
    let mut group = c.benchmark_group("or_list_runs_all");

    for n in [10usize, 50, 200] {
        let (_db, store) = populated_store(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| store.list_runs(None, i64::MAX).unwrap());
        });
    }

    group.finish();
}

fn bench_list_runs_filtered(c: &mut Criterion) {
    let mut group = c.benchmark_group("or_list_runs_filtered");

    for n in [10usize, 50, 200] {
        let (_db, store) = populated_store(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| store.list_runs(Some(&RunStatus::Completed), 50).unwrap());
        });
    }

    group.finish();
}

fn bench_count_runs(c: &mut Criterion) {
    // Measure count_runs() vs list_runs(None, i64::MAX) for the dashboard use case.
    // count_runs() is O(1) (SQLite internal rowcount); list_runs scans all rows.
    let mut group = c.benchmark_group("or_count_vs_list_all");
    for n in [10usize, 50, 200] {
        let (_db, store) = populated_store(n);
        group.bench_with_input(BenchmarkId::new("count_runs", n), &n, |b, _| {
            b.iter(|| store.count_runs().unwrap());
        });
        group.bench_with_input(BenchmarkId::new("list_runs_all", n), &n, |b, _| {
            b.iter(|| store.list_runs(None, i64::MAX).unwrap());
        });
    }
    group.finish();
}

fn bench_get_run(c: &mut Criterion) {
    let (_db, store) = populated_store(100);

    c.bench_function("or_get_run", |b| {
        // Lookup by the unique team_run_id index — should be O(1).
        b.iter(|| store.get_run("run-bench-50").unwrap());
    });
}

criterion_group!(
    benches,
    bench_create_run,
    bench_complete_run,
    bench_fail_run,
    bench_list_runs_all,
    bench_count_runs,
    bench_list_runs_filtered,
    bench_get_run,
);
criterion_main!(benches);
