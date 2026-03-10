use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{
    Database, MessageQueue, MessageType, OrchestrationStore, SessionStore,
};
use opengoose_types::{Platform, SessionKey};
use opengoose_web::data::{load_dashboard, load_queue_page, load_runs_page, load_sessions_page};

fn empty_db() -> Arc<Database> {
    Arc::new(Database::open_in_memory().unwrap())
}

/// Populate a DB with `n` sessions (each with `msgs_per` messages), `n` runs, and `n` queue items.
fn populated_db(n: usize, msgs_per: usize) -> Arc<Database> {
    let db = empty_db();
    let ss = SessionStore::new(db.clone());
    let mq = MessageQueue::new(db.clone());
    let os = OrchestrationStore::new(db.clone());

    for i in 0..n {
        let key = SessionKey::new(Platform::Discord, format!("guild-{i}"), format!("chan-{i}"));
        ss.append_user_message(&key, "hello", Some("user")).unwrap();
        for j in 0..msgs_per {
            ss.append_assistant_message(&key, &format!("reply {j}"))
                .unwrap();
        }

        let run_id = format!("run-bench-{i}");
        let sk = key.to_stable_id();
        os.create_run(&run_id, &sk, "bench-team", "chain", "{}", 3)
            .unwrap();
        if i % 3 == 0 {
            os.complete_run(&run_id, "ok").unwrap();
        } else if i % 5 == 0 {
            os.fail_run(&run_id, "error").unwrap();
        }

        mq.enqueue(
            &sk,
            &run_id,
            "planner",
            "coder",
            &format!("task {i}"),
            MessageType::Task,
        )
        .unwrap();
    }

    db
}

// ---------- load_dashboard ----------

fn bench_load_dashboard_empty(c: &mut Criterion) {
    let db = empty_db();
    c.bench_function("load_dashboard_empty", |b| {
        b.iter(|| load_dashboard(db.clone()).unwrap());
    });
}

fn bench_load_dashboard_populated(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_dashboard_populated");

    for n in [10usize, 50, 200] {
        let db = populated_db(n, 5);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| load_dashboard(db.clone()).unwrap());
        });
    }

    group.finish();
}

// ---------- load_sessions_page ----------

fn bench_load_sessions_page(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_sessions_page");

    for n in [10usize, 50, 200] {
        let db = populated_db(n, 10);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| load_sessions_page(db.clone(), None).unwrap());
        });
    }

    group.finish();
}

// ---------- load_runs_page ----------

fn bench_load_runs_page(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_runs_page");

    for n in [10usize, 50, 200] {
        let db = populated_db(n, 2);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| load_runs_page(db.clone(), None).unwrap());
        });
    }

    group.finish();
}

// ---------- load_queue_page ----------

fn bench_load_queue_page(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_queue_page");

    for n in [10usize, 50, 200] {
        let db = populated_db(n, 1);
        // Use the first run id as the selected run.
        let run_id = Some("run-bench-0".to_string());
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| load_queue_page(db.clone(), run_id.clone()).unwrap());
        });
    }

    group.finish();
}

// ---------- dashboard render round-trip ----------

fn bench_dashboard_render_roundtrip(c: &mut Criterion) {
    let db = populated_db(20, 5);
    c.bench_function("dashboard_render_roundtrip_20", |b| {
        b.iter(|| {
            let view = load_dashboard(db.clone()).unwrap();
            opengoose_web::render_dashboard_live_partial(view).unwrap()
        });
    });
}

criterion_group!(
    benches,
    bench_load_dashboard_empty,
    bench_load_dashboard_populated,
    bench_load_sessions_page,
    bench_load_runs_page,
    bench_load_queue_page,
    bench_dashboard_render_roundtrip,
);
criterion_main!(benches);
