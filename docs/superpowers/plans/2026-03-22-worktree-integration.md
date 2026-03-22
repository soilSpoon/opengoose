# Worktree Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Worker rig가 work item을 처리할 때 git worktree로 격리된 환경을 만들고, 완료/실패/크래시 시 자동 정리하는 시스템.

**Architecture:** WorktreeGuard (RAII Drop 패턴)가 worktree 생명주기를 관리. 정상 경로는 Drop이 처리하고, 크래시로 인한 고아 worktree는 Worker 시작 시 sweep이 정리. Stuck 상태는 `keep` 플래그로 보존.

**Tech Stack:** Rust, tokio::process (git commands), std::fs, tempfile (tests)

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Create | `crates/opengoose-rig/src/worktree.rs` | WorktreeGuard 구조체, create/remove/sweep, git 명령 실행 |
| Modify | `crates/opengoose-rig/src/lib.rs` (append) | `pub mod worktree;` 추가 |
| Modify | `crates/opengoose-rig/src/rig.rs:216-276` | `process_claimed_item()`에 worktree 통합 |
| Modify | `crates/opengoose-rig/src/rig.rs:156-196` | `Worker.run()`에 sweep 추가 |

---

### Task 1: WorktreeGuard 구조체 + Drop

WorktreeGuard는 worktree 경로와 브랜치를 들고 있다가, 스코프를 벗어나면 자동으로 `git worktree remove` + `git branch -D`를 실행하는 RAII 구조체.

**Files:**
- Create: `crates/opengoose-rig/src/worktree.rs`
- Modify: `crates/opengoose-rig/src/lib.rs`

- [ ] **Step 1: 모듈 등록**

`crates/opengoose-rig/src/lib.rs`에 추가:

```rust
pub mod worktree;
```

- [ ] **Step 2: WorktreeGuard 구조체 + Drop 구현 작성**

`crates/opengoose-rig/src/worktree.rs`:

```rust
use std::path::{Path, PathBuf};
use tracing::warn;

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
```

- [ ] **Step 3: 단위 테스트 — Drop이 keep=false일 때 정리하는지 확인**

실제 git repo를 tempdir에 만들어서 테스트:

```rust
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
        // initial commit 필요 (worktree add에 필요)
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

        // git worktree add
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
        } // guard dropped here

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

        assert!(wt_path.exists()); // 남아있어야 함
    }
}
```

- [ ] **Step 4: 테스트 실행해서 실패 확인**

Run: `cargo test -p opengoose-rig worktree`
Expected: FAIL — `remove_worktree` 함수가 아직 없음

- [ ] **Step 5: remove_worktree 함수 구현**

```rust
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
```

- [ ] **Step 6: 테스트 통과 확인**

Run: `cargo test -p opengoose-rig worktree`
Expected: PASS

- [ ] **Step 7: 커밋**

```bash
git add crates/opengoose-rig/src/worktree.rs crates/opengoose-rig/src/lib.rs
git commit -m "feat(rig): add WorktreeGuard with RAII cleanup"
```

---

### Task 2: worktree 생성 함수

`WorktreeGuard::create()`로 git worktree를 생성하고 guard를 반환.

**Files:**
- Modify: `crates/opengoose-rig/src/worktree.rs`

- [ ] **Step 1: create 테스트 작성**

```rust
#[test]
fn create_worktree_and_guard() {
    let repo = init_test_repo();
    let base = tempfile::tempdir().unwrap();
    let guard = WorktreeGuard::create(
        repo.path(),
        &RigId::new("main"),
        1,
        Some(base.path()),
    ).unwrap();

    assert!(guard.path.exists());
    assert!(guard.path.join(".git").exists()); // worktree는 .git 파일을 가짐
    assert_eq!(guard.branch, "rig/main/1");
    assert!(!guard.keep);
}

#[test]
fn attach_returns_none_for_nonexistent() {
    let repo = init_test_repo();
    let base = tempfile::tempdir().unwrap();
    assert!(WorktreeGuard::attach(repo.path(), &RigId::new("x"), 99, Some(base.path())).is_none());
}

#[test]
fn attach_returns_some_for_existing_worktree() {
    let repo = init_test_repo();
    let base = tempfile::tempdir().unwrap();
    let mut guard = WorktreeGuard::create(repo.path(), &RigId::new("att"), 1, Some(base.path())).unwrap();
    let path = guard.path.clone();
    guard.keep = true; // 삭제하지 않음
    drop(guard);

    let attached = WorktreeGuard::attach(repo.path(), &RigId::new("att"), 1, Some(base.path()));
    assert!(attached.is_some());
    assert_eq!(attached.unwrap().path, path);
}
```

- [ ] **Step 2: 테스트 실행 — 실패 확인**

Run: `cargo test -p opengoose-rig worktree::tests::create`
Expected: FAIL — `create` 메서드 없음

- [ ] **Step 3: WorktreeGuard::create 구현**

```rust
use opengoose_board::work_item::RigId;

impl WorktreeGuard {
    /// 새 worktree 생성.
    /// base_dir이 None이면 DEFAULT_WORKTREE_BASE (/tmp/og-rigs) 사용.
    /// 경로: {base}/{rig_id}/{item_id}, 브랜치: rig/{rig_id}/{item_id}
    pub fn create(
        repo_dir: &Path,
        rig_id: &RigId,
        item_id: i64,
        base_dir: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let wt_path = worktree_path(base_dir, rig_id, item_id);
        let branch = format!("rig/{}/{}", rig_id.0, item_id);

        // 부모 디렉토리 생성
        if let Some(parent) = wt_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = std::process::Command::new("git")
            .args(["worktree", "add"])
            .arg(&wt_path)
            .args(["-b", &branch])
            .current_dir(repo_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree add failed: {stderr}");
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
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p opengoose-rig worktree`
Expected: PASS

- [ ] **Step 5: 커밋**

```bash
git add crates/opengoose-rig/src/worktree.rs
git commit -m "feat(rig): add WorktreeGuard::create and attach"
```

---

### Task 3: sweep_orphaned_worktrees

Worker 시작 시 고아 worktree를 정리하는 함수.

**Files:**
- Modify: `crates/opengoose-rig/src/worktree.rs`

- [ ] **Step 1: sweep 테스트 작성**

```rust
#[tokio::test]
async fn sweep_removes_orphaned_worktrees() {
    let repo = init_test_repo();
    let base = tempfile::tempdir().unwrap();
    let rig_id = RigId::new("sweep-rig");

    // worktree를 만들되 guard 없이 남겨둠 (고아 시뮬레이션)
    let wt_path = base.path().join(&rig_id.0).join("999");

    std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
    std::process::Command::new("git")
        .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", "rig/sweep-rig/999"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(wt_path.exists());

    // Board에 해당 item이 없으므로 → 고아로 판단 → 삭제
    let board = Arc::new(Board::in_memory().await.unwrap());
    sweep_orphaned_worktrees(repo.path(), &rig_id, &board, Some(base.path())).await;

    assert!(!wt_path.exists());
}

#[tokio::test]
async fn sweep_preserves_claimed_worktrees() {
    let repo = init_test_repo();
    let base = tempfile::tempdir().unwrap();
    let rig_id = RigId::new("keep-rig");

    let wt_path = base.path().join(&rig_id.0).join("1");
    std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
    std::process::Command::new("git")
        .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", "rig/keep-rig/1"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    // Board에 item #1이 Claimed 상태로 존재
    let board = Arc::new(Board::in_memory().await.unwrap());
    use opengoose_board::work_item::{PostWorkItem, Priority};
    board.post(PostWorkItem {
        title: "claimed".into(),
        description: String::new(),
        created_by: RigId::new("user"),
        priority: Priority::P1,
        tags: vec![],
    }).await.unwrap();
    board.claim(1, &rig_id).await.unwrap();

    sweep_orphaned_worktrees(repo.path(), &rig_id, &board, Some(base.path())).await;

    assert!(wt_path.exists()); // Claimed → 유지
}
```

- [ ] **Step 2: 테스트 실행 — 실패 확인**

Run: `cargo test -p opengoose-rig worktree::tests::sweep`
Expected: FAIL

- [ ] **Step 3: sweep 구현**

```rust
use opengoose_board::Board;
use opengoose_board::work_item::Status;
use std::sync::Arc;
use tracing::info;

/// Worker 시작 시 고아 worktree 정리.
/// {base_dir}/{rig_id}/ 아래를 스캔하여:
/// - Board에 해당 item이 없거나 Done/Abandoned → 삭제
/// - Claimed/Stuck → 유지 (resume에서 처리)
/// base_dir이 None이면 DEFAULT_WORKTREE_BASE 사용.
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
        Err(_) => return, // 디렉토리 없으면 할 일 없음
    };

    // 디렉토리 엔트리를 먼저 수집 (blocking I/O를 최소화)
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
            Ok(Some(item)) => matches!(item.status, Status::Done | Status::Abandoned),
            Ok(None) => true,   // Board에 없으면 고아
            Err(_) => false,    // DB 에러 시 보수적으로 유지
        };

        if should_remove {
            let branch = format!("rig/{}/{}", rig_id.0, item_id);
            info!(item_id, "sweeping orphaned worktree");
            let repo = repo_dir.to_path_buf();
            let path = wt_path.clone();
            // git 명령은 blocking → spawn_blocking으로 격리
            let result = tokio::task::spawn_blocking(move || {
                remove_worktree(&repo, &path, &branch)
            }).await;
            match result {
                Ok(Err(e)) => warn!(item_id, error = %e, "failed to sweep orphaned worktree"),
                Err(e) => warn!(item_id, error = %e, "sweep task panicked"),
                _ => {}
            }
        }
    }
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cargo test -p opengoose-rig worktree`
Expected: PASS

- [ ] **Step 5: 커밋**

```bash
git add crates/opengoose-rig/src/worktree.rs
git commit -m "feat(rig): add sweep_orphaned_worktrees for crash recovery"
```

---

### Task 4: Worker.run()에 sweep 통합

Worker 시작 Phase 0으로 sweep 호출.

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:156-196`

- [ ] **Step 1: `Worker.run()`에 sweep 호출 추가**

`rig.rs`의 `run()` 메서드, `info!("worker started")` 직후 (line 161과 163 사이):

```rust
// Phase 0: Sweep — 크래시로 남은 고아 worktree 정리
let repo_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
crate::worktree::sweep_orphaned_worktrees(&repo_dir, &self.id, board, None).await;
```

- [ ] **Step 2: 빌드 확인**

Run: `cargo check -p opengoose-rig`
Expected: success

- [ ] **Step 3: 커밋**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "feat(rig): integrate sweep into Worker.run() Phase 0"
```

---

### Task 5: process_claimed_item()에 WorktreeGuard 통합

claim된 아이템 처리 시 worktree를 생성하고, 결과에 따라 guard가 정리.

**Files:**
- Modify: `crates/opengoose-rig/src/rig.rs:216-276`

- [ ] **Step 1: process_claimed_item에 worktree 통합**

현재 `process_claimed_item` (line 216-276)을 다음과 같이 수정:

```rust
async fn process_claimed_item(&self, item: &WorkItem, board: &Arc<Board>) {
    let repo_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let session_name = format!("task-{}", item.id);

    // Worktree 생성 또는 기존 것에 attach (resume 시)
    let mut guard = match crate::worktree::WorktreeGuard::attach(&repo_dir, &self.id, item.id, None) {
        Some(guard) => {
            info!(rig = %self.id, item_id = item.id, "attached to existing worktree");
            guard
        }
        None => match crate::worktree::WorktreeGuard::create(&repo_dir, &self.id, item.id, None) {
            Ok(guard) => {
                info!(rig = %self.id, item_id = item.id, path = %guard.path.display(), "created worktree");
                guard
            }
            Err(e) => {
                warn!(rig = %self.id, item_id = item.id, error = %e, "failed to create worktree, abandoning");
                board.abandon(item.id).await.ok();
                return;
            }
        },
    };

    // 기존 세션 조회 → 없으면 새로 생성
    let (session_id, resuming) = match self.find_session_by_name(&session_name).await {
        Some(id) => {
            info!(rig = %self.id, item_id = item.id, "resuming existing session");
            (id, true)
        }
        None => {
            match self
                .agent
                .config
                .session_manager
                .create_session(
                    guard.path.clone(), // worktree 경로를 cwd로 사용
                    session_name,
                    goose::session::session_manager::SessionType::User,
                    goose::config::goose_mode::GooseMode::Auto,
                )
                .await
            {
                Ok(s) => (s.id, false),
                Err(e) => {
                    warn!(rig = %self.id, item_id = item.id, error = %e, "failed to create session, abandoning");
                    board.abandon(item.id).await.ok();
                    return; // guard drops → worktree removed
                }
            }
        }
    };

    let prompt = if resuming {
        format!("Continue working on item #{}: {}", item.id, item.title)
    } else {
        format!(
            "Work item #{}: {}\n\n{}",
            item.id, item.title, item.description
        )
    };

    let input = WorkInput::task(prompt, item.id).with_session_id(session_id);

    let result = self.process(input).await;
    match result {
        Ok(()) => {
            // guard.keep은 false → drop 시 worktree 자동 삭제
            if let Err(e) = board.submit(item.id, &self.id).await {
                warn!(rig = %self.id, item_id = item.id, error = %e, "submit failed");
            } else {
                info!(rig = %self.id, item_id = item.id, "submitted work item");
            }
        }
        Err(e) => {
            warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
            // guard.keep은 false → drop 시 worktree 자동 삭제
            if let Err(e) = board.abandon(item.id).await {
                warn!(rig = %self.id, item_id = item.id, error = %e, "abandon failed");
            }
        }
    }
    // guard drops here → if keep==false, worktree removed
}
```

**참고:** Stuck 상태 통합 (guard.keep = true 설정)은 Phase 후반에 검증 루프(clippy/test 2라운드)가 구현될 때 추가. 현재는 성공/실패 두 경로만 존재.

- [ ] **Step 2: 빌드 확인**

Run: `cargo check -p opengoose-rig`
Expected: success

- [ ] **Step 3: 기존 테스트 통과 확인**

Run: `cargo test -p opengoose-rig`
Expected: all pass

- [ ] **Step 4: 커밋**

```bash
git add crates/opengoose-rig/src/rig.rs
git commit -m "feat(rig): integrate WorktreeGuard into process_claimed_item"
```

---

### Task 6: 최종 검증

**Files:** (없음 — 검증만)

- [ ] **Step 1: 전체 빌드**

Run: `cargo check --workspace`
Expected: success

- [ ] **Step 2: 전체 테스트**

Run: `cargo test --workspace`
Expected: all pass

- [ ] **Step 3: clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: no warnings
