use opengoose_sandbox::SandboxPool;
use serial_test::serial;
use std::time::Instant;

#[test]
#[serial]
#[cfg(target_os = "macos")]
fn bench_exec_breakdown() {
    let pool = SandboxPool::new();

    // Warm up
    let vm = pool.acquire().unwrap();
    pool.release(vm);

    // Measure detailed exec timing
    for i in 0..3 {
        let t0 = Instant::now();
        let mut vm = pool.acquire().unwrap();
        let t1 = Instant::now();

        let result = vm.exec("echo", &["hello"], std::time::Duration::from_secs(5));
        let t2 = Instant::now();

        let exits = &vm.exit_counts;
        eprintln!("[run {i}] acquire={:?} exec={:?}",
            t1 - t0, t2 - t1);
        eprintln!("  exits: mmio_r={} mmio_w={} vtimer={} wfi={} sysreg={} hvc={} canceled={}",
            exits.mmio_read, exits.mmio_write, exits.vtimer, exits.wfi, exits.sysreg, exits.hvc, exits.canceled);
        eprintln!("  total exits: {}", exits.mmio_read + exits.mmio_write + exits.vtimer + exits.wfi + exits.sysreg + exits.hvc + exits.canceled);
        if let Ok(r) = &result {
            eprintln!("  result: status={} stderr={:?}", r.status, &r.stderr[..r.stderr.len().min(80)]);
        }
        pool.release(vm);
    }
}
