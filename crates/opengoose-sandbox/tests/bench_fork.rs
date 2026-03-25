#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxPool;
#[cfg(target_os = "macos")]
use std::time::Instant;

#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(target_os = "macos")]
fn bench_fork_latency() {
    let pool = SandboxPool::new();

    // Warm up — ensure snapshot + first VM creation
    let vm = pool.acquire().unwrap();
    pool.release(vm);

    // Measure fork (reset path) latency
    let mut times = Vec::new();
    for _ in 0..20 {
        let start = Instant::now();
        let vm = pool.acquire().unwrap();
        let fork_time = start.elapsed();
        times.push(fork_time);
        pool.release(vm);
    }

    eprintln!("\n=== Fork latency via reset (20 runs) ===");
    for (i, t) in times.iter().enumerate() {
        eprintln!("  [{i:2}] {:?}", t);
    }
    let avg = times.iter().map(|t| t.as_micros()).sum::<u128>() / times.len() as u128;
    let min = times.iter().map(|t| t.as_micros()).min().unwrap();
    let max = times.iter().map(|t| t.as_micros()).max().unwrap();
    eprintln!("  avg={avg}us  min={min}us  max={max}us");
    eprintln!("  target: <800us");

    // Measure fork + exec end-to-end
    let mut e2e_times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let mut vm = pool.acquire().unwrap();
        let r = vm
            .exec("echo", &["test"], std::time::Duration::from_secs(5))
            .expect("exec should not timeout");
        let e2e = start.elapsed();
        e2e_times.push(e2e);
        if e2e_times.len() == 1 {
            eprintln!("\n  exec result: status={} stderr={:?}", r.status, r.stderr);
        }
        pool.release(vm);
    }

    eprintln!("\n=== Fork + Exec (5 runs) ===");
    for (i, t) in e2e_times.iter().enumerate() {
        eprintln!("  [{i}] {:?}", t);
    }
    let e2e_avg = e2e_times.iter().map(|t| t.as_micros()).sum::<u128>() / e2e_times.len() as u128;
    eprintln!("  avg={e2e_avg}us");
}
