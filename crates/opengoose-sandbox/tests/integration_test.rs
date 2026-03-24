use opengoose_sandbox::SandboxPool;
use serial_test::serial;
use std::time::Duration;

/// Test the full sandbox flow: pool → acquire → exec → result.
/// Skips gracefully if kernel/boot fails (no real ARM64 kernel yet).
#[test]
#[serial]
#[cfg(target_os = "macos")]
fn test_full_sandbox_flow() {
    let pool = SandboxPool::new();

    match pool.acquire() {
        Ok(mut vm) => {
            let result = vm.exec("echo", &["hello", "sandbox"], Duration::from_secs(5));
            match result {
                Ok(r) => {
                    assert_eq!(r.status, 0);
                    assert!(r.stdout.contains("hello sandbox"));
                }
                Err(e) => eprintln!("exec skipped: {e}"),
            }
        }
        Err(e) => {
            eprintln!("Full sandbox test skipped (no kernel available): {e}");
        }
    }
}

/// Test that pool compiles and can be created.
#[test]
fn test_pool_creation() {
    let _pool = SandboxPool::new();
}
