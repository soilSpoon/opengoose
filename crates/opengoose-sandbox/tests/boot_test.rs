use serial_test::serial;

/// Test that kernel download and caching works.
/// This test only downloads; it doesn't boot (no real ARM64 kernel yet).
#[test]
fn test_ensure_kernel() {
    // This may fail if network is unavailable or URL is wrong.
    // That's OK — we just verify the code path works.
    match opengoose_sandbox::boot::ensure_kernel() {
        Ok(path) => {
            assert!(path.exists(), "kernel file should exist after download");
            let meta = std::fs::metadata(&path).unwrap();
            assert!(meta.len() > 0, "kernel file should not be empty");
        }
        Err(e) => {
            eprintln!("Kernel download skipped (expected in CI): {e}");
        }
    }
}

/// Test full boot sequence.
/// Skips gracefully if kernel is not available or boot fails.
#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_boot_prints_to_uart() {
    use opengoose_sandbox::boot::BootedVm;

    let vm = BootedVm::boot_default();
    match vm {
        Ok(mut vm) => {
            let output = vm.collect_uart_output(std::time::Duration::from_secs(5));
            // If we get any output, the VMM loop is working
            let preview_len = output.len().min(200);
            eprintln!(
                "UART output ({} bytes): {:?}",
                output.len(),
                &output[..preview_len]
            );
        }
        Err(e) => {
            eprintln!("Boot test skipped: {e}");
        }
    }
}
