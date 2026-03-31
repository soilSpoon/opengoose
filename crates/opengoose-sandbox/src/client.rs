//! High-level sandbox client for Worker integration.
//!
//! SandboxClient wraps SandboxPool and provides a task-oriented API:
//! - Mount a host worktree (read-only + overlay)
//! - Execute commands in the sandbox
//! - Extract a unified diff of changes
//! - Apply changes back to host

#[cfg(target_os = "macos")]
use crate::error::{Result, SandboxError};
#[cfg(target_os = "macos")]
use crate::pool::SandboxPool;
#[cfg(target_os = "macos")]
use crate::vm::{ExecResult, MicroVm};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::Duration;

/// Default timeout for sandbox commands.
#[cfg(target_os = "macos")]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// High-level client for executing work in an isolated sandbox VM.
///
/// Usage:
/// ```ignore
/// let client = SandboxClient::new();
/// let mut session = client.start("/path/to/worktree")?;
/// let result = session.exec("cargo", &["test"])?;
/// let diff = session.unified_diff()?;
/// session.apply_to_host()?;
/// ```
#[cfg(target_os = "macos")]
pub struct SandboxClient {
    pool: SandboxPool,
}

#[cfg(target_os = "macos")]
impl SandboxClient {
    pub fn new() -> Self {
        SandboxClient {
            pool: SandboxPool::new(),
        }
    }

    /// Start a sandbox session for the given host worktree directory.
    /// The worktree is mounted read-only via virtio-fs with an overlay for writes.
    pub fn start(&self, worktree: &Path) -> Result<SandboxSession> {
        start_session(&self.pool, worktree)
    }

}

/// Shared session-start logic used by both `SandboxClient` and `SandboxClientRef`.
#[cfg(target_os = "macos")]
fn start_session(pool: &SandboxPool, worktree: &Path) -> Result<SandboxSession> {
    let mut vm = pool.acquire()?;
    vm.mount_virtio_fs(worktree);

    let mount_result = vm.exec_raw("mount_workspace", &[], DEFAULT_TIMEOUT)?;
    if mount_result.status != 0 {
        return Err(SandboxError::Exec(format!(
            "workspace mount failed: {}",
            mount_result.stderr
        )));
    }

    Ok(SandboxSession {
        vm,
        worktree: worktree.to_path_buf(),
    })
}

/// A lightweight sandbox client that borrows an external `SandboxPool`.
///
/// Use this when a pool is shared across multiple components (e.g. `Arc<SandboxPool>`
/// in the Worker runtime) so VMs are reused instead of creating a new pool per client.
#[cfg(target_os = "macos")]
pub struct SandboxClientRef<'a> {
    pool: &'a SandboxPool,
}

#[cfg(target_os = "macos")]
impl<'a> SandboxClientRef<'a> {
    /// Create a client that borrows an external pool.
    /// The pool's VM is reused across calls (sub-ms fork).
    pub fn new(pool: &'a SandboxPool) -> Self {
        Self { pool }
    }

    /// Start a sandbox session, same as `SandboxClient::start()`.
    pub fn start(&self, worktree: &Path) -> Result<SandboxSession> {
        start_session(self.pool, worktree)
    }
}

#[cfg(target_os = "macos")]
impl Default for SandboxClient {
    fn default() -> Self {
        Self::new()
    }
}

/// An active sandbox session with a mounted worktree.
/// The VM is running and ready to execute commands.
#[cfg(target_os = "macos")]
pub struct SandboxSession {
    vm: MicroVm,
    worktree: PathBuf,
}

#[cfg(target_os = "macos")]
impl SandboxSession {
    /// Execute a command in the sandbox. Working directory is /workspace (the overlay).
    pub fn exec(&mut self, cmd: &str, args: &[&str]) -> Result<ExecResult> {
        self.exec_with_timeout(cmd, args, DEFAULT_TIMEOUT)
    }

    /// Execute with a custom timeout.
    pub fn exec_with_timeout(
        &mut self,
        cmd: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<ExecResult> {
        self.vm.exec(cmd, args, timeout)
    }

    /// Read a file from the sandbox overlay (guest path, e.g., "/workspace/src/main.rs").
    pub fn read_file(&mut self, guest_path: &str) -> Result<String> {
        let result = self.exec("cat", &[guest_path])?;
        if result.status != 0 {
            return Err(SandboxError::Exec(format!(
                "cat {guest_path}: {}",
                result.stderr
            )));
        }
        Ok(result.stdout)
    }

    /// Write a file in the sandbox overlay (goes to overlay, not host).
    pub fn write_file(&mut self, guest_path: &str, content: &str) -> Result<()> {
        // Reject paths containing shell metacharacters to prevent injection.
        const SHELL_META: &[char] = &[
            '`', '$', '(', ')', '{', '}', ';', '&', '|', '<', '>', '\n', '\r', '\0', '!', '#',
        ];
        if guest_path.chars().any(|c| SHELL_META.contains(&c)) {
            return Err(SandboxError::Exec(format!(
                "guest_path contains shell metacharacters: {guest_path}"
            )));
        }
        // Use sh -c with heredoc-style to handle multi-line content safely
        let escaped = content.replace('\'', "'\\''");
        let cmd = format!("printf '%s' '{escaped}' > '{guest_path}'");
        let result = self.exec("sh", &["-c", &cmd])?;
        if result.status != 0 {
            return Err(SandboxError::Exec(format!(
                "write {guest_path}: {}",
                result.stderr
            )));
        }
        Ok(())
    }

    /// Get a unified diff of all changes made in the overlay relative to the original worktree.
    /// Returns the diff as a string suitable for `git apply`.
    pub fn unified_diff(&mut self) -> Result<String> {
        // Use diff -ruN to compare the original (mounted read-only at /mnt/host)
        // with the overlay workspace. This gives us a unified diff without needing git.
        let result = self.exec_with_timeout(
            "diff",
            &["-ruN", "/mnt/host", "/workspace"],
            Duration::from_secs(60),
        )?;
        // diff returns 0 = no differences, 1 = differences found, 2 = error
        if result.status == 2 {
            return Err(SandboxError::Exec(format!(
                "diff failed: {}",
                result.stderr
            )));
        }
        Ok(result.stdout)
    }

    /// Backward-compatible alias for callers still using the old name.
    pub fn git_diff(&mut self) -> Result<String> {
        self.unified_diff()
    }

    /// Apply the overlay changes back to the host worktree.
    /// Uses diff to extract changes, then applies them to the host filesystem.
    pub fn apply_to_host(&mut self) -> Result<ApplyResult> {
        let diff = self.unified_diff()?;
        if diff.is_empty() {
            return Ok(ApplyResult {
                files_changed: 0,
                diff: String::new(),
            });
        }

        // Parse diff to count changed files and apply
        let files_changed = diff.lines().filter(|l| l.starts_with("diff ")).count();

        // Write diff to a secure temp file outside the worktree, then apply with patch.
        let mut tmp_diff = tempfile::Builder::new()
            .prefix("opengoose-sandbox-")
            .suffix(".patch")
            .tempfile()
            .map_err(|e| SandboxError::Exec(format!("create temp diff: {e}")))?;
        use std::io::Write as _;
        tmp_diff
            .write_all(diff.as_bytes())
            .map_err(|e| SandboxError::Exec(format!("write diff: {e}")))?;
        tmp_diff
            .flush()
            .map_err(|e| SandboxError::Exec(format!("flush diff: {e}")))?;

        let status = std::process::Command::new("patch")
            .args(["-p1", "-d"])
            .arg(&self.worktree)
            .arg("-i")
            .arg(tmp_diff.path())
            .status()
            .map_err(|e| SandboxError::Exec(format!("patch: {e}")))?;

        if !status.success() {
            return Err(SandboxError::Exec(format!(
                "patch failed (exit {})",
                status.code().unwrap_or(-1)
            )));
        }

        Ok(ApplyResult {
            files_changed,
            diff,
        })
    }

    /// Get the host worktree path.
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }
}

/// Result of applying sandbox changes to the host.
#[derive(Debug)]
pub struct ApplyResult {
    pub files_changed: usize,
    pub diff: String,
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn client_ref_new_creates_client() {
        let pool = SandboxPool::new();
        let client = SandboxClientRef::new(&pool);
        let _ = client;
    }
}
