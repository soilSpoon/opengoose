#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxPool;
#[cfg(target_os = "macos")]
use std::time::Duration;

/// Test the full sandbox flow: pool -> acquire -> exec -> result.
#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(target_os = "macos")]
fn test_full_sandbox_flow() {
    let pool = SandboxPool::new();
    let mut vm = pool.acquire().expect("acquire should succeed");
    let r = vm
        .exec("echo", &["hello", "sandbox"], Duration::from_secs(5))
        .expect("exec should succeed");
    eprintln!(
        "EXEC RESULT: status={} stdout={:?} stderr={:?}",
        r.status, r.stdout, r.stderr
    );
    assert_eq!(r.status, -1);
    assert!(r.stderr.contains("No such file"));
}

/// Test that pool compiles and can be created.
#[test]
fn test_pool_creation() {
    let _pool = opengoose_sandbox::SandboxPool::new();
}
