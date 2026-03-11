use criterion::{Criterion, criterion_group, criterion_main};
use opengoose_web::fixtures::{sample_dashboard_view, sample_dashboard_view_large};
use opengoose_web::render_dashboard_live_partial;

fn bench_render_dashboard_live_small(c: &mut Criterion) {
    let dashboard = sample_dashboard_view();

    c.bench_function("render_dashboard_live_small", |b| {
        b.iter(|| render_dashboard_live_partial(dashboard.clone()).unwrap());
    });
}

fn bench_render_dashboard_live_large(c: &mut Criterion) {
    let dashboard = sample_dashboard_view_large(50);

    c.bench_function("render_dashboard_live_large_50", |b| {
        b.iter(|| render_dashboard_live_partial(dashboard.clone()).unwrap());
    });
}

criterion_group!(
    benches,
    bench_render_dashboard_live_small,
    bench_render_dashboard_live_large,
);
criterion_main!(benches);
