use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{Database, ScheduleStore};

fn bench_create(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());

    c.bench_function("schedule_create", |b| {
        let mut counter = 0u32;
        b.iter(|| {
            counter += 1;
            let store = ScheduleStore::new(Arc::clone(&db));
            store
                .create(
                    &format!("sched-{counter}"),
                    "0 * * * *",
                    "code-review",
                    "review PRs",
                    Some("2026-01-01 00:00:00"),
                )
                .unwrap();
        });
    });
}

fn bench_list(c: &mut Criterion) {
    let mut group = c.benchmark_group("schedule_list");

    for n in [10usize, 100, 500] {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = ScheduleStore::new(Arc::clone(&db));
        for i in 0..n {
            store
                .create(
                    &format!("sched-{i}"),
                    "0 * * * *",
                    "team-a",
                    "",
                    Some("2026-01-01 00:00:00"),
                )
                .unwrap();
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            let s = ScheduleStore::new(Arc::clone(&db));
            b.iter(|| s.list().unwrap());
        });
    }

    group.finish();
}

fn bench_list_due(c: &mut Criterion) {
    let mut group = c.benchmark_group("schedule_list_due");

    for n in [10usize, 100, 500] {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = ScheduleStore::new(Arc::clone(&db));
        // Half schedules are due (next_run_at in the past), half are not.
        for i in 0..n {
            let next_run = if i % 2 == 0 {
                Some("2020-01-01 00:00:00") // past → due
            } else {
                Some("2099-01-01 00:00:00") // future → not due
            };
            store
                .create(&format!("sched-{i}"), "0 * * * *", "team-a", "", next_run)
                .unwrap();
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            let s = ScheduleStore::new(Arc::clone(&db));
            b.iter(|| s.list_due().unwrap());
        });
    }

    group.finish();
}

fn bench_get_by_name(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = ScheduleStore::new(Arc::clone(&db));
    for i in 0..100 {
        store
            .create(&format!("sched-{i:03}"), "0 * * * *", "team-a", "", None)
            .unwrap();
    }

    c.bench_function("schedule_get_by_name", |b| {
        let s = ScheduleStore::new(Arc::clone(&db));
        b.iter(|| s.get_by_name("sched-050").unwrap());
    });
}

criterion_group!(
    benches,
    bench_create,
    bench_list,
    bench_list_due,
    bench_get_by_name,
);
criterion_main!(benches);
