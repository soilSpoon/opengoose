use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_persistence::{Database, SessionStore};
use opengoose_types::{Platform, SessionKey};

fn test_key() -> SessionKey {
    SessionKey::new(Platform::Discord, "guild123".to_string(), "channel456")
}

fn bench_append_user_message(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = SessionStore::new(Arc::clone(&db));
    let key = test_key();

    c.bench_function("append_user_message", |b| {
        b.iter(|| {
            store
                .append_user_message(&key, "hello world", Some("alice"))
                .unwrap();
        });
    });
}

fn bench_append_assistant_message(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = SessionStore::new(Arc::clone(&db));
    let key = test_key();

    c.bench_function("append_assistant_message", |b| {
        b.iter(|| {
            store
                .append_assistant_message(&key, "here is my response")
                .unwrap();
        });
    });
}

fn bench_load_history_10(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = SessionStore::new(Arc::clone(&db));
    let key = test_key();

    // Pre-populate with 50 messages.
    for i in 0..50usize {
        if i % 2 == 0 {
            store
                .append_user_message(&key, &format!("user msg {i}"), Some("alice"))
                .unwrap();
        } else {
            store
                .append_assistant_message(&key, &format!("assistant reply {i}"))
                .unwrap();
        }
    }

    c.bench_function("load_history_limit_10", |b| {
        b.iter(|| {
            store.load_history(&key, 10).unwrap();
        });
    });
}

fn bench_load_history_50(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = SessionStore::new(Arc::clone(&db));
    let key = test_key();

    for i in 0..50usize {
        if i % 2 == 0 {
            store
                .append_user_message(&key, &format!("user msg {i}"), Some("alice"))
                .unwrap();
        } else {
            store
                .append_assistant_message(&key, &format!("assistant reply {i}"))
                .unwrap();
        }
    }

    c.bench_function("load_history_limit_50", |b| {
        b.iter(|| {
            store.load_history(&key, 50).unwrap();
        });
    });
}

fn bench_set_active_team(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = SessionStore::new(Arc::clone(&db));
    let key = test_key();

    c.bench_function("set_active_team", |b| {
        b.iter(|| {
            store.set_active_team(&key, Some("code-review")).unwrap();
        });
    });
}

fn bench_get_active_team(c: &mut Criterion) {
    let db = Arc::new(Database::open_in_memory().unwrap());
    let store = SessionStore::new(Arc::clone(&db));
    let key = test_key();
    store.set_active_team(&key, Some("code-review")).unwrap();

    c.bench_function("get_active_team", |b| {
        b.iter(|| {
            store.get_active_team(&key).unwrap();
        });
    });
}

fn bench_list_sessions(c: &mut Criterion) {
    let mut group = c.benchmark_group("list_sessions");

    for n in [10usize, 100, 500] {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let store = SessionStore::new(Arc::clone(&db));

        // Create N distinct sessions, each with a unique channel.
        for i in 0..n {
            let key = SessionKey::new(Platform::Discord, "guild123", format!("channel-{i}"));
            store
                .append_user_message(&key, "hello", Some("user"))
                .unwrap();
        }

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let s = SessionStore::new(Arc::clone(&db));
            b.iter(|| s.list_sessions(n as i64).unwrap());
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_append_user_message,
    bench_append_assistant_message,
    bench_load_history_10,
    bench_load_history_50,
    bench_set_active_team,
    bench_get_active_team,
    bench_list_sessions,
);
criterion_main!(benches);
