#[cfg(all(target_os = "macos", feature = "sandbox-debug"))]
use opengoose_sandbox::SandboxPool;
#[cfg(all(target_os = "macos", feature = "sandbox-debug"))]
use std::time::Duration;

/// Long-running debug harness (500k vCPU iterations).
/// Gated behind `sandbox-debug` feature to avoid running in normal test suites.
#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(all(target_os = "macos", feature = "sandbox-debug"))]
fn fork_exec_debug() {
    let pool = SandboxPool::new();
    let mut vm = match pool.acquire() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("acquire failed: {e}");
            return;
        }
    };

    let json = r#"{"cmd":"ping","args":[]}"#;
    eprintln!("[debug] NOT pushing input yet, running vCPU first");

    let mut wfi = 0u64;
    let mut mmio_w = 0u64;
    let mut mmio_r = 0u64;
    let mut hvc = 0u64;
    let mut sysreg = 0u64;
    let mut vtimer = 0u64;
    let mut unknown = 0u64;
    let mut total = 0u64;

    use opengoose_sandbox::hypervisor::VcpuExit;
    loop {
        match vm.vcpu_run() {
            Ok(exit) => {
                total += 1;
                match &exit {
                    VcpuExit::WaitForEvent => wfi += 1,
                    VcpuExit::MmioWrite { .. } => mmio_w += 1,
                    VcpuExit::MmioRead { .. } => mmio_r += 1,
                    VcpuExit::HypervisorCall { .. } => hvc += 1,
                    VcpuExit::SystemRegAccess { .. } => sysreg += 1,
                    VcpuExit::VtimerActivated => vtimer += 1,
                    VcpuExit::Unknown(code) => {
                        unknown += 1;
                        if unknown <= 3 {
                            eprintln!("Unknown exit code={code:#x}");
                        }
                    }
                }
                vm.handle_exit(exit);
                if total >= 500000 {
                    break;
                }
                if total.is_multiple_of(100000) {
                    eprintln!(
                        "[{total}] wfi={wfi} mmio_w={mmio_w} mmio_r={mmio_r} hvc={hvc} sysreg={sysreg} vtimer={vtimer}"
                    );
                }
            }
            Err(e) => {
                eprintln!("vcpu error: {e}");
                break;
            }
        }
    }

    eprintln!(
        "Total: {total} exits (wfi={wfi} mmio_w={mmio_w} mmio_r={mmio_r} vtimer={vtimer} unknown={unknown})"
    );

    let output = vm.collect_uart_output_raw(Duration::from_secs(1));
    let text = String::from_utf8_lossy(&output);
    eprintln!("[debug] UART output ({} bytes):", output.len());
    for line in text.lines().take(20) {
        eprintln!("  {line}");
    }

    if text.contains("pong") {
        eprintln!(">>> SUCCESS: got pong response!");
    } else if output.is_empty() {
        eprintln!(">>> No UART output — guest may be stuck");
    }

    let _ = json;
}
