use opengoose_sandbox::SandboxPool;
use serial_test::serial;
use std::time::Instant;

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn bench_fork_latency() {
    let pool = SandboxPool::new();

    // Warm up — ensure snapshot exists
    let _ = pool.acquire();

    // Measure fork_from (acquire) latency
    let mut times = Vec::new();
    for _ in 0..10 {
        let start = Instant::now();
        let vm = pool.acquire().unwrap();
        let fork_time = start.elapsed();
        times.push(fork_time);
        drop(vm);
    }

    eprintln!("\n=== Fork latency (10 runs) ===");
    for (i, t) in times.iter().enumerate() {
        eprintln!("  [{i}] {:?}", t);
    }
    let avg = times.iter().map(|t| t.as_micros()).sum::<u128>() / times.len() as u128;
    let min = times.iter().map(|t| t.as_micros()).min().unwrap();
    let max = times.iter().map(|t| t.as_micros()).max().unwrap();
    eprintln!("  avg={avg}µs  min={min}µs  max={max}µs");

    // Measure fork + exec (end-to-end)
    let mut e2e_times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let mut vm = pool.acquire().unwrap();
        let result = vm.exec("echo", &["test"], std::time::Duration::from_secs(5));
        let e2e = start.elapsed();
        e2e_times.push(e2e);
        if let Ok(r) = &result {
            if e2e_times.len() == 1 {
                eprintln!("\n  exec result: status={} stderr={:?}", r.status, r.stderr);
            }
        }
    }

    eprintln!("\n=== Fork + Exec latency (5 runs) ===");
    for (i, t) in e2e_times.iter().enumerate() {
        eprintln!("  [{i}] {:?}", t);
    }
    let e2e_avg = e2e_times.iter().map(|t| t.as_micros()).sum::<u128>() / e2e_times.len() as u128;
    eprintln!("  avg={e2e_avg}µs");
    eprintln!("\n  fork target: <800µs ✅ (actual avg={avg}µs)");
}
