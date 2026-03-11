use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use opengoose_core::{Engine, ThrottlePolicy, split_message, truncate_for_display};
use opengoose_persistence::Database;
use opengoose_types::{EventBus, Platform, SessionKey};

fn test_key() -> SessionKey {
    SessionKey::new(Platform::Discord, "guild123".to_string(), "channel456")
}

// ── Message utilities ────────────────────────────────────────────

fn bench_split_message(c: &mut Criterion) {
    let mut group = c.benchmark_group("split_message");

    // Short message (no split needed)
    let short = "Hello, world!".to_string();
    group.bench_function("short_no_split", |b| {
        b.iter(|| split_message(black_box(&short), 2000));
    });

    // Long message requiring multiple splits
    let long = "a".repeat(6000);
    group.bench_function("6k_bytes", |b| {
        b.iter(|| split_message(black_box(&long), 2000));
    });

    // Message with newlines (preferred split points)
    let with_newlines = (0..30)
        .map(|i| format!("Line {i}: {}", "x".repeat(180)))
        .collect::<Vec<_>>()
        .join("\n");
    group.bench_function("newline_splits", |b| {
        b.iter(|| split_message(black_box(&with_newlines), 2000));
    });

    group.finish();
}

fn bench_truncate_for_display(c: &mut Criterion) {
    let mut group = c.benchmark_group("truncate_for_display");

    let text = "a".repeat(4000);
    for limit in [100, 500, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(limit), &limit, |b, &limit| {
            b.iter(|| truncate_for_display(black_box(&text), limit));
        });
    }

    // UTF-8 multibyte content
    let emoji_text = "🦆".repeat(500);
    group.bench_function("utf8_multibyte", |b| {
        b.iter(|| truncate_for_display(black_box(&emoji_text), 500));
    });

    group.finish();
}

// ── Throttle policy ──────────────────────────────────────────────

fn bench_throttle_should_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("throttle_should_update");

    // Discord (no throttle — always true)
    group.bench_function("discord", |b| {
        let policy = ThrottlePolicy::discord();
        b.iter(|| policy.should_update(black_box(500)));
    });

    // Slack with accumulated state
    group.bench_function("slack_with_state", |b| {
        let mut policy = ThrottlePolicy::slack();
        policy.record_update(100);
        b.iter(|| policy.should_update(black_box(300)));
    });

    group.finish();
}

// ── Engine construction + message recording ──────────────────────

fn bench_engine_new(c: &mut Criterion) {
    c.bench_function("engine_new", |b| {
        b.iter(|| {
            let event_bus = EventBus::new(16);
            let db = Database::open_in_memory().unwrap();
            Engine::new_with_team_store(event_bus, db, None)
        });
    });
}

fn bench_record_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("record_messages");

    let event_bus = EventBus::new(16);
    let db = Database::open_in_memory().unwrap();
    let engine = Engine::new_with_team_store(event_bus, db, None);
    let key = test_key();

    group.bench_function("record_user_message", |b| {
        b.iter(|| {
            engine.record_user_message(black_box(&key), black_box("hello world"), Some("alice"));
        });
    });

    group.bench_function("record_assistant_message", |b| {
        b.iter(|| {
            engine.record_assistant_message(black_box(&key), black_box("here is my response"));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_split_message,
    bench_truncate_for_display,
    bench_throttle_should_update,
    bench_engine_new,
    bench_record_messages,
);
criterion_main!(benches);
