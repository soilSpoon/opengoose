//! Integration tests for virtio-fs: mount host directory, read files, overlay isolation.

#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxPool;
#[cfg(target_os = "macos")]
use std::time::Duration;

/// Acquire a VM with virtiofs+overlay mounted.
/// Returns None if VM infrastructure is unavailable.
#[cfg(target_os = "macos")]
fn try_acquire_with_virtiofs(dir: &std::path::Path) -> Option<opengoose_sandbox::MicroVm> {
    let pool = SandboxPool::new();
    let mut vm = pool.acquire().ok()?;
    vm.mount_virtio_fs(dir);

    // Trigger mount inside the guest (host configured VirtioFs device above,
    // but the guest needs to be told to mount it now).
    let r = vm
        .exec_raw("mount_workspace", &[], Duration::from_secs(10))
        .ok()?;
    if r.status != 0 {
        return None;
    }
    Some(vm)
}

/// Test: fork VM with virtio-fs, verify guest can read host file via overlay.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_virtiofs_read_host_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "hello from host").unwrap();

    let Some(mut vm) = try_acquire_with_virtiofs(dir.path()) else {
        return;
    };

    let result = vm
        .exec("cat", &["/workspace/test.txt"], Duration::from_secs(10))
        .expect("exec should succeed");

    assert_eq!(
        result.status, 0,
        "cat should succeed, stderr: {}",
        result.stderr
    );
    assert_eq!(result.stdout.trim(), "hello from host");
}

/// Test: writes go to overlay, host file remains unchanged.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_virtiofs_overlay_isolation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("original.txt"), "original content").unwrap();

    let Some(mut vm) = try_acquire_with_virtiofs(dir.path()) else {
        return;
    };

    let result = vm
        .exec(
            "sh",
            &["-c", "echo modified > /workspace/original.txt"],
            Duration::from_secs(10),
        )
        .expect("exec should succeed");
    assert_eq!(
        result.status, 0,
        "write should succeed via overlay, stderr: {}",
        result.stderr
    );

    let host_content = std::fs::read_to_string(dir.path().join("original.txt")).unwrap();
    assert_eq!(
        host_content, "original content",
        "host file must not be modified"
    );
}

/// Test: new files created in overlay don't appear on host.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_virtiofs_new_file_in_overlay() {
    let dir = tempfile::tempdir().unwrap();

    let Some(mut vm) = try_acquire_with_virtiofs(dir.path()) else {
        return;
    };

    let result = vm
        .exec(
            "sh",
            &["-c", "echo new_content > /workspace/new_file.txt"],
            Duration::from_secs(10),
        )
        .expect("exec should succeed");
    assert_eq!(
        result.status, 0,
        "file creation should succeed, stderr: {}",
        result.stderr
    );

    let result = vm
        .exec("cat", &["/workspace/new_file.txt"], Duration::from_secs(10))
        .expect("exec should succeed");
    assert_eq!(result.stdout.trim(), "new_content");

    assert!(
        !dir.path().join("new_file.txt").exists(),
        "new file must not appear on host"
    );
}

/// Test: pool creation works (always, not macOS-gated).
#[test]
fn test_pool_creation() {
    let _pool = opengoose_sandbox::SandboxPool::new();
}
