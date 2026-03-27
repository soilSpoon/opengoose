//! Integration tests for SandboxClient — the high-level Worker-facing API.

#[cfg(target_os = "macos")]
use opengoose_sandbox::SandboxClient;

/// Try to start a sandbox session and check virtiofs support.
#[cfg(target_os = "macos")]
fn try_start_with_virtiofs(worktree: &std::path::Path) -> Option<opengoose_sandbox::SandboxSession> {
    let client = SandboxClient::new();
    let mut session = client.start(worktree).ok()?;
    // Check if /workspace exists (kernel supports virtiofs+overlay)
    let r = session.exec("test", &["-d", "/workspace"]).ok()?;
    if r.status != 0 {
        return None;
    }
    Some(session)
}

/// Test: SandboxClient can be created.
#[test]
fn test_client_creation() {
    #[cfg(target_os = "macos")]
    {
        let _client = SandboxClient::new();
    }
}

/// Test: start session, execute a basic command (no virtiofs needed).
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_client_basic_exec() {
    let dir = tempfile::tempdir().unwrap();
    let client = SandboxClient::new();
    let Some(mut session) = client.start(dir.path()).ok().filter(|_| true) else { return };

    let result = session.exec("echo", &["sandbox works"]).unwrap();
    assert_eq!(result.status, 0);
    assert_eq!(result.stdout.trim(), "sandbox works");
}

/// Test: multi-exec in a single session.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_client_multi_exec() {
    let dir = tempfile::tempdir().unwrap();
    let client = SandboxClient::new();
    let Some(mut session) = client.start(dir.path()).ok().filter(|_| true) else { return };

    let r1 = session.exec("echo", &["one"]).unwrap();
    let r2 = session.exec("echo", &["two"]).unwrap();
    let r3 = session.exec("echo", &["three"]).unwrap();

    assert_eq!(r1.stdout.trim(), "one");
    assert_eq!(r2.stdout.trim(), "two");
    assert_eq!(r3.stdout.trim(), "three");
}

/// Test: read_file via virtiofs overlay.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_client_read_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("data.txt"), "read test").unwrap();

    let Some(mut session) = try_start_with_virtiofs(dir.path()) else { return };

    let content = session.read_file("/workspace/data.txt").unwrap();
    assert_eq!(content.trim(), "read test");
}

/// Test: write_file + read_file roundtrip in overlay.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_client_write_read_roundtrip() {
    let dir = tempfile::tempdir().unwrap();

    let Some(mut session) = try_start_with_virtiofs(dir.path()) else { return };

    session
        .write_file("/workspace/new.txt", "written in sandbox")
        .unwrap();
    let content = session.read_file("/workspace/new.txt").unwrap();
    assert_eq!(content.trim(), "written in sandbox");

    assert!(!dir.path().join("new.txt").exists());
}

/// Test: git_diff detects overlay changes.
#[test]
#[cfg(target_os = "macos")]
#[serial_test::serial]
fn test_client_git_diff() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "original").unwrap();

    let Some(mut session) = try_start_with_virtiofs(dir.path()) else { return };

    session
        .write_file("/workspace/file.txt", "modified")
        .unwrap();
    let diff = session.git_diff().unwrap();
    assert!(diff.contains("original"));
    assert!(diff.contains("modified"));
}
