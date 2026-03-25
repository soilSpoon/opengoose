/// Test that kernel download and caching works.
#[test]
fn test_ensure_kernel() {
    let path = opengoose_sandbox::boot::ensure_kernel().expect("kernel download should succeed");
    assert!(path.exists(), "kernel file should exist after download");
    let meta = std::fs::metadata(&path).unwrap();
    assert!(meta.len() > 0, "kernel file should not be empty");
}

/// Test full boot sequence — must produce UART output.
#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(target_os = "macos")]
fn test_boot_prints_to_uart() {
    use opengoose_sandbox::boot::BootedVm;

    let mut vm = BootedVm::boot_default().expect("boot should succeed");
    let output = vm.collect_uart_output(std::time::Duration::from_secs(5));
    assert!(!output.is_empty(), "boot should produce UART output");
}
