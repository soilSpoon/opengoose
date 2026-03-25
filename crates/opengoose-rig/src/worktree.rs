use crate::RigError;
use opengoose_board::Board;
use opengoose_board::work_item::{RigId, Status};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

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

impl WorktreeGuard {
    /// 명시적 async 정리. spawn_blocking으로 tokio 스레드 블로킹을 방지.
    /// 호출 후 self이 소비되므로 Drop은 실행되지 않음 (keep=true로 설정 후 drop).
    pub async fn remove(mut self) {
        self.keep = true; // Drop에서 중복 정리 방지
        let repo = self.repo_dir.clone();
        let path = self.path.clone();
        let branch = self.branch.clone();
        let result =
            tokio::task::spawn_blocking(move || remove_worktree(&repo, &path, &branch)).await;
        match result {
            Ok(Err(e)) => {
                warn!(path = %self.path.display(), error = %e, "failed to remove worktree")
            }
            Err(e) => warn!(path = %self.path.display(), error = %e, "remove task panicked"),
            _ => {}
        }
    }
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        // 안전망: remove()가 호출되지 않았을 때만 실행 (blocking, 최후 수단)
        if let Err(e) = remove_worktree(&self.repo_dir, &self.path, &self.branch) {
            warn!(path = %self.path.display(), error = %e, "failed to remove worktree on drop");
        }
    }
}

/// RigId가 경로/브랜치 이름에 안전한지 검증.
/// `..`, `/`, `\` 포함 시 path traversal 위험.
fn validate_rig_id(rig_id: &RigId) -> Result<(), RigError> {
    let id = &rig_id.0;
    if id.contains("..") || id.contains('/') || id.contains('\\') || id.is_empty() {
        return Err(RigError::WorktreeFailed(format!(
            "invalid rig id for worktree: {id:?} (must not contain '..', '/', '\\' or be empty)"
        )));
    }
    Ok(())
}

impl WorktreeGuard {
    /// 새 worktree 생성.
    /// base_dir이 None이면 DEFAULT_WORKTREE_BASE (/tmp/og-rigs) 사용.
    /// 경로: {base}/{rig_id}/{item_id}, 브랜치: rig/{rig_id}/{item_id}
    pub fn create(
        repo_dir: &Path,
        rig_id: &RigId,
        item_id: i64,
        base_dir: Option<&Path>,
    ) -> Result<Self, RigError> {
        validate_rig_id(rig_id)?;
        let wt_path = worktree_path(base_dir, rig_id, item_id);
        let branch = format!("rig/{}/{}", rig_id.0, item_id);

        if let Some(parent) = wt_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| RigError::WorktreeFailed(format!("create dir: {e}")))?;
        }

        let output = std::process::Command::new("git")
            .args(["worktree", "add"])
            .arg(&wt_path)
            .args(["-b", &branch])
            .current_dir(repo_dir)
            .output()
            .map_err(|e| RigError::WorktreeFailed(format!("git exec: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(RigError::WorktreeFailed(format!(
                "git worktree add failed: {stderr}"
            )));
        }

        Ok(Self {
            path: wt_path,
            branch,
            repo_dir: repo_dir.to_path_buf(),
            keep: false,
        })
    }

    /// 기존 worktree에 대한 guard 생성 (resume 시 사용).
    /// .git 파일 존재로 유효한 worktree인지 검증.
    pub fn attach(
        repo_dir: &Path,
        rig_id: &RigId,
        item_id: i64,
        base_dir: Option<&Path>,
    ) -> Option<Self> {
        validate_rig_id(rig_id).ok()?;
        let wt_path = worktree_path(base_dir, rig_id, item_id);
        let branch = format!("rig/{}/{}", rig_id.0, item_id);

        // .git 파일 존재 확인 (worktree는 .git 디렉토리가 아닌 .git 파일을 가짐)
        if wt_path.join(".git").exists() {
            Some(Self {
                path: wt_path,
                branch,
                repo_dir: repo_dir.to_path_buf(),
                keep: false,
            })
        } else {
            None
        }
    }
}

/// worktree 삭제 + 브랜치 삭제.
fn remove_worktree(repo_dir: &Path, wt_path: &Path, branch: &str) -> Result<(), RigError> {
    // git worktree remove --force <path>
    let output = std::process::Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(wt_path)
        .current_dir(repo_dir)
        .output()
        .map_err(|e| RigError::WorktreeFailed(format!("git exec: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // 이미 삭제된 경우 무시
        if !stderr.contains("is not a working tree") {
            return Err(RigError::WorktreeFailed(format!(
                "git worktree remove failed: {stderr}"
            )));
        }
    }

    // git branch -D <branch>
    let output = std::process::Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| RigError::WorktreeFailed(format!("git exec: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("not found") {
            return Err(RigError::WorktreeFailed(format!(
                "git branch -D failed: {stderr}"
            )));
        }
    }

    Ok(())
}

/// Worker 시작 시 고아 worktree 정리.
/// {base_dir}/{rig_id}/ 아래를 스캔하여:
/// - Board에 해당 item이 없거나 Done/Abandoned → 삭제
/// - Claimed/Stuck → 유지 (resume에서 처리)
///
/// `base_dir`이 `None`이면 `DEFAULT_WORKTREE_BASE` 사용.
pub async fn sweep_orphaned_worktrees(
    repo_dir: &Path,
    rig_id: &RigId,
    board: &Arc<Board>,
    base_dir: Option<&Path>,
) {
    let base = base_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WORKTREE_BASE));
    let rig_dir = base.join(&rig_id.0);

    let entries = match std::fs::read_dir(&rig_dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let dirs: Vec<_> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name();
            let id = name.to_str()?.parse::<i64>().ok()?;
            Some((id, e.path()))
        })
        .collect();

    for (item_id, wt_path) in dirs {
        let should_remove = match board.get(item_id).await {
            Ok(Some(item)) => {
                matches!(item.status, Status::Done | Status::Abandoned | Status::Open)
            }
            Ok(None) => true,
            Err(_) => false,
        };

        if should_remove {
            let branch = format!("rig/{}/{}", rig_id.0, item_id);
            info!(item_id, "sweeping orphaned worktree");
            let repo = repo_dir.to_path_buf();
            let path = wt_path.clone();
            let result =
                tokio::task::spawn_blocking(move || remove_worktree(&repo, &path, &branch)).await;
            match result {
                Ok(Err(e)) => warn!(item_id, error = %e, "failed to sweep orphaned worktree"),
                Err(e) => warn!(item_id, error = %e, "sweep task panicked"),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(tmp: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@test.com"])
            .args(args)
            .current_dir(tmp)
            .output()
            .expect("output should succeed");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_test_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("temp dir creation should succeed");
        git(tmp.path(), &["init"]);
        std::fs::write(tmp.path().join("README.md"), "init")
            .expect("test fixture write should succeed");
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "-m", "init"]);
        tmp
    }

    #[test]
    fn drop_removes_worktree_when_keep_is_false() {
        let repo = init_test_repo();
        let wt_path = repo.path().join("wt-test");
        let branch = "rig/test/1".to_string();

        git(
            repo.path(),
            &[
                "worktree",
                "add",
                wt_path.to_str().expect("path should be valid UTF-8"),
                "-b",
                &branch,
            ],
        );
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

        git(
            repo.path(),
            &[
                "worktree",
                "add",
                wt_path.to_str().expect("path should be valid UTF-8"),
                "-b",
                &branch,
            ],
        );

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

    #[test]
    fn create_worktree_and_guard() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");
        let guard = WorktreeGuard::create(repo.path(), &RigId::new("main"), 1, Some(base.path()))
            .expect("worktree creation should succeed");

        assert!(guard.path.exists());
        assert!(guard.path.join(".git").exists()); // worktree는 .git 파일을 가짐
        assert_eq!(guard.branch, "rig/main/1");
        assert!(!guard.keep);
    }

    #[test]
    fn attach_returns_none_for_nonexistent() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");
        assert!(
            WorktreeGuard::attach(repo.path(), &RigId::new("x"), 99, Some(base.path())).is_none()
        );
    }

    #[test]
    fn attach_returns_some_for_existing_worktree() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");
        let mut guard =
            WorktreeGuard::create(repo.path(), &RigId::new("att"), 1, Some(base.path()))
                .expect("worktree creation should succeed");
        let path = guard.path.clone();
        guard.keep = true; // 삭제하지 않음
        drop(guard);

        let attached = WorktreeGuard::attach(repo.path(), &RigId::new("att"), 1, Some(base.path()));
        assert!(attached.is_some());
        assert_eq!(attached.expect("is_some should succeed").path, path);
    }

    #[tokio::test]
    async fn sweep_removes_orphaned_worktrees() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");
        let rig_id = RigId::new("sweep-rig");

        // worktree를 만들되 guard 없이 남겨둠 (고아 시뮬레이션)
        let wt_path = base.path().join(&rig_id.0).join("999");

        std::fs::create_dir_all(wt_path.parent().expect("directory creation should succeed"))
            .expect("parent should succeed");
        git(
            repo.path(),
            &[
                "worktree",
                "add",
                wt_path.to_str().expect("path should be valid UTF-8"),
                "-b",
                "rig/sweep-rig/999",
            ],
        );
        assert!(wt_path.exists());

        // Board에 해당 item이 없으므로 → 고아로 판단 → 삭제
        let board = Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        sweep_orphaned_worktrees(repo.path(), &rig_id, &board, Some(base.path())).await;

        assert!(!wt_path.exists());
    }

    #[tokio::test]
    async fn sweep_preserves_claimed_worktrees() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");
        let rig_id = RigId::new("keep-rig");

        let wt_path = base.path().join(&rig_id.0).join("1");
        std::fs::create_dir_all(wt_path.parent().expect("directory creation should succeed"))
            .expect("parent should succeed");
        git(
            repo.path(),
            &[
                "worktree",
                "add",
                wt_path.to_str().expect("path should be valid UTF-8"),
                "-b",
                "rig/keep-rig/1",
            ],
        );

        // Board에 item #1이 Claimed 상태로 존재
        let board = Arc::new(
            Board::in_memory()
                .await
                .expect("in-memory board should initialize"),
        );
        use opengoose_board::work_item::{PostWorkItem, Priority};
        board
            .post(PostWorkItem {
                title: "claimed".into(),
                description: String::new(),
                created_by: RigId::new("user"),
                priority: Priority::P1,
                tags: vec![],
            })
            .await
            .expect("board operation should succeed");
        board.claim(1, &rig_id).await.expect("claim should succeed");

        sweep_orphaned_worktrees(repo.path(), &rig_id, &board, Some(base.path())).await;

        assert!(wt_path.exists()); // Claimed → 유지
    }

    #[test]
    fn create_rejects_path_traversal_rig_id() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");

        assert!(
            WorktreeGuard::create(repo.path(), &RigId::new("../../etc"), 1, Some(base.path()))
                .is_err()
        );
        assert!(
            WorktreeGuard::create(repo.path(), &RigId::new("a/b"), 1, Some(base.path())).is_err()
        );
        assert!(
            WorktreeGuard::create(repo.path(), &RigId::new("a\\b"), 1, Some(base.path())).is_err()
        );
        assert!(WorktreeGuard::create(repo.path(), &RigId::new(""), 1, Some(base.path())).is_err());
    }

    #[tokio::test]
    async fn remove_cleans_up_without_blocking_drop() {
        let repo = init_test_repo();
        let base = tempfile::tempdir().expect("temp dir creation should succeed");
        let guard =
            WorktreeGuard::create(repo.path(), &RigId::new("rm-test"), 1, Some(base.path()))
                .expect("worktree creation should succeed");
        let path = guard.path.clone();
        assert!(path.exists());

        guard.remove().await;
        assert!(!path.exists());
    }
}
