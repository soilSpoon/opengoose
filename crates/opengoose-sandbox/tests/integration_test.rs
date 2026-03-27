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
    let mut vm = match pool.acquire() {
        Ok(v) => v,
        Err(_) => return, // VM infrastructure not available
    };
    let r = vm
        .exec("echo", &["hello", "sandbox"], Duration::from_secs(5))
        .expect("exec should succeed");
    assert_eq!(r.status, 0);
    assert_eq!(r.stdout.trim(), "hello sandbox");
}

/// Test multi-exec in a single VM session.
#[test]
#[cfg_attr(target_os = "macos", serial_test::serial)]
#[cfg(target_os = "macos")]
fn test_multi_exec() {
    let pool = SandboxPool::new();
    let mut vm = match pool.acquire() {
        Ok(v) => v,
        Err(_) => return,
    };

    let r1 = vm.exec("echo", &["one"], Duration::from_secs(5)).unwrap();
    let r2 = vm.exec("echo", &["two"], Duration::from_secs(5)).unwrap();
    let r3 = vm
        .exec("cat", &["/proc/version"], Duration::from_secs(5))
        .unwrap();

    assert_eq!(r1.stdout.trim(), "one");
    assert_eq!(r2.stdout.trim(), "two");
    assert_eq!(r3.status, 0);
}

/// Test that pool compiles and can be created.
#[test]
fn test_pool_creation() {
    let _pool = opengoose_sandbox::SandboxPool::new();
}
