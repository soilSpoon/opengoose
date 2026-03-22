use std::path::{Path, PathBuf};
use tracing::warn;
use opengoose_board::work_item::RigId;

/// Worktree 기본 경로.
const DEFAULT_WORKTREE_BASE: &str = "/tmp/og-rigs";

/// Git worktree의 RAII 가드.
/// Drop 시 `keep == false`이면 worktree와 브랜치를 자동 삭제.
pub struct WorktreeGuard {
    /// worktree 디렉토리 경로 (e.g. /tmp/og-rigs/main/1)
    pub path: PathBuf,
    /// 브랜치 이름 (e.g. rig/main/1)
    pub branch: String,
    /// 원본 repo 경로 (git 명령 실행 위치)
    repo_dir: PathBuf,
    /// true면 Drop 시 정리하지 않음 (Stuck 상태용)
    pub keep: bool,
}

/// worktree 경로 계산. base_dir이 None이면 DEFAULT_WORKTREE_BASE 사용.
fn worktree_path(base_dir: Option<&Path>, rig_id: &RigId, item_id: i64) -> PathBuf {
    let base = base_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WORKTREE_BASE));
    base.join(&rig_id.0).join(item_id.to_string())
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        if let Err(e) = remove_worktree(&self.repo_dir, &self.path, &self.branch) {
            warn!(path = %self.path.display(), error = %e, "failed to remove worktree on drop");
        }
    }
}

/// worktree 삭제 + 브랜치 삭제.
fn remove_worktree(repo_dir: &Path, wt_path: &Path, branch: &str) -> anyhow::Result<()> {
    // git worktree remove --force <path>
    let output = std::process::Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(wt_path)
        .current_dir(repo_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 이미 삭제된 경우 무시
        if !stderr.contains("is not a working tree") {
            anyhow::bail!("git worktree remove failed: {stderr}");
        }
    }

    // git branch -D <branch>
    let output = std::process::Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("not found") {
            anyhow::bail!("git branch -D failed: {stderr}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_test_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::fs::write(tmp.path().join("README.md"), "init").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        tmp
    }

    #[test]
    fn drop_removes_worktree_when_keep_is_false() {
        let repo = init_test_repo();
        let wt_path = repo.path().join("wt-test");
        let branch = "rig/test/1".to_string();

        std::process::Command::new("git")
            .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", &branch])
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(wt_path.exists());

        {
            let _guard = WorktreeGuard {
                path: wt_path.clone(),
                branch: branch.clone(),
                repo_dir: repo.path().to_path_buf(),
                keep: false,
            };
        }

        assert!(!wt_path.exists());
    }

    #[test]
    fn drop_preserves_worktree_when_keep_is_true() {
        let repo = init_test_repo();
        let wt_path = repo.path().join("wt-keep");
        let branch = "rig/keep/1".to_string();

        std::process::Command::new("git")
            .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", &branch])
            .current_dir(repo.path())
            .output()
            .unwrap();

        {
            let _guard = WorktreeGuard {
                path: wt_path.clone(),
                branch: branch.clone(),
                repo_dir: repo.path().to_path_buf(),
                keep: true,
            };
        }

        assert!(wt_path.exists());
    }
}
