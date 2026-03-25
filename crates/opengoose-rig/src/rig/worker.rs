// Worker-specific impl: Board pull loop, claim -> execute -> submit.

use super::Worker;
use crate::pipeline::PipelineContext;
use crate::work_mode::{WorkInput, task_session_id};
use anyhow::Context;
use opengoose_board::Board;
use opengoose_board::work_item::WorkItem;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

impl Worker {
    /// Pull loop. Waits for work on Board, then claim -> execute -> submit.
    /// Operator lacks this method -- compile-time guarantee.
    /// Returns immediately if board is None (Worker always has a board).
    pub async fn run(&self) {
        let Some(board) = &self.board else {
            warn!(rig = %self.id, "worker has no board, exiting");
            return;
        };
        info!(rig = %self.id, "worker started, waiting for work");

        // Phase 0: Sweep -- clean up orphaned worktrees from crashes
        let repo_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
        crate::worktree::sweep_orphaned_worktrees(&repo_dir, &self.id, board, None).await;

        // Phase 1: Resume -- process previously claimed items
        let stale = board.claimed_by(&self.id).await.unwrap_or_default();
        if !stale.is_empty() {
            info!(rig = %self.id, count = stale.len(), "resuming previously claimed items");
        }
        for item in &stale {
            if self.cancel.is_cancelled() {
                break;
            }
            self.process_claimed_item(item, board, &repo_dir).await;
        }

        loop {
            // 1. Register interest first -- captures all notifications after this point
            let notify = board.notify_handle();
            let notified = notify.notified();

            // 2. Check for ready items + execute
            match self.try_claim_and_execute(&repo_dir).await {
                Ok(true) => continue, // work found, immediately check for more
                Ok(false) => {}       // no work, proceed to wait
                Err(e) => warn!(rig = %self.id, error = %e, "execution failed"),
            }

            // 3. No work -- wait for notification (loss-proof)
            tokio::select! {
                _ = notified => {}
                _ = self.cancel.cancelled() => {
                    info!(rig = %self.id, "worker cancelled");
                    break;
                }
            }
        }
    }

    /// Claim highest-priority work item from Board and execute.
    /// Uses SQLite transaction for atomicity and AlreadyClaimed validation.
    pub(crate) async fn try_claim_and_execute(&self, repo_dir: &Path) -> anyhow::Result<bool> {
        let board = self.board().context("Worker must have a board")?;

        let ready = board.ready().await?;
        if ready.is_empty() {
            return Ok(false);
        }

        // Find first claimable candidate and execute
        let claimed = self.try_claim_first(board, &ready).await?;
        let Some(item) = claimed else {
            return Ok(true); // all preempted -- retry immediately
        };

        info!(rig = %self.id, item_id = item.id, title = %item.title, "claimed work item");
        self.process_claimed_item(&item, board, repo_dir).await;
        Ok(true)
    }

    /// Claim the first available item from the ready list.
    /// AlreadyClaimed is skipped, other errors are propagated.
    async fn try_claim_first(
        &self,
        board: &Arc<Board>,
        candidates: &[WorkItem],
    ) -> anyhow::Result<Option<WorkItem>> {
        for candidate in candidates {
            match board.claim(candidate.id, &self.id).await {
                Ok(claimed) => return Ok(Some(claimed)),
                Err(opengoose_board::BoardError::AlreadyClaimed { .. }) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(None)
    }

    /// Process a claimed item. Resolve/create session -> process -> submit or abandon.
    /// Errors are handled internally and not propagated to the caller.
    async fn process_claimed_item(&self, item: &WorkItem, board: &Arc<Board>, repo_dir: &Path) {
        // Phase 1: Acquire worktree
        let guard = match self.acquire_worktree(repo_dir, item.id) {
            Ok(guard) => guard,
            Err(e) => {
                warn!(rig = %self.id, item_id = item.id, error = %e, "failed to acquire worktree, abandoning");
                if let Err(e) = board.abandon(item.id).await {
                    warn!(error = %e, item_id = item.id, "failed to abandon work item after worktree acquisition failure");
                }
                return;
            }
        };

        // Phase 2: Middleware hydration
        let pipeline_ctx = PipelineContext {
            agent: &self.agent,
            work_dir: &guard.path,
            rig_id: &self.id,
            board: board.as_ref(),
            item,
        };
        if let Err(e) = pipeline_ctx.run_on_start(&self.middleware).await {
            warn!(rig = %self.id, item_id = item.id, error = %e, "middleware on_start failed, abandoning");
            if let Err(e) = board.abandon(item.id).await {
                warn!(error = %e, item_id = item.id, "failed to abandon work item after middleware on_start failure");
            }
            guard.remove().await;
            return;
        }

        // Phase 3: Resolve session
        let session_name = task_session_id(item.id);
        let (session_id, resuming) = match self.resolve_session(&session_name, &guard.path).await {
            Ok(result) => result,
            Err(e) => {
                warn!(rig = %self.id, item_id = item.id, error = %e, "failed to resolve session, abandoning");
                if let Err(e) = board.abandon(item.id).await {
                    warn!(error = %e, item_id = item.id, "failed to abandon work item after session resolution failure");
                }
                guard.remove().await;
                return;
            }
        };

        // Phase 4: Execute with bounded retry
        let prompt = if resuming {
            format!("Continue working on item #{}: {}", item.id, item.title)
        } else {
            format!(
                "Work item #{}: {}\n\n{}",
                item.id, item.title, item.description
            )
        };

        let keep = self
            .execute_with_retry(item, board, &pipeline_ctx, &session_id, &prompt)
            .await;

        // Phase 5: Cleanup -- keep worktree if stuck (for debugging)
        if !keep {
            guard.remove().await;
        }
    }

    /// Acquire worktree: attach (existing) -> create (new).
    fn acquire_worktree(
        &self,
        repo_dir: &Path,
        item_id: i64,
    ) -> anyhow::Result<crate::worktree::WorktreeGuard> {
        if let Some(guard) =
            crate::worktree::WorktreeGuard::attach(repo_dir, &self.id, item_id, None)
        {
            info!(rig = %self.id, item_id, "attached to existing worktree");
            return Ok(guard);
        }
        let guard = crate::worktree::WorktreeGuard::create(repo_dir, &self.id, item_id, None)?;
        info!(rig = %self.id, item_id, path = %guard.path.display(), "created worktree");
        Ok(guard)
    }

    /// Look up existing session or create new one. Returns (session_id, resuming).
    async fn resolve_session(
        &self,
        session_name: &str,
        work_dir: &Path,
    ) -> anyhow::Result<(String, bool)> {
        if let Some(id) = self.find_session_by_name(session_name).await {
            info!(rig = %self.id, "resuming existing session");
            return Ok((id, true));
        }
        let session = self
            .agent
            .config
            .session_manager
            .create_session(
                work_dir.to_path_buf(),
                session_name.to_string(),
                goose::session::session_manager::SessionType::User,
                goose::config::goose_mode::GooseMode::Auto,
            )
            .await?;
        Ok((session.id, false))
    }

    /// Bounded retry loop: process -> validate -> (submit | retry | stuck).
    /// Returns `true` if the worktree should be kept (e.g. stuck items for debugging).
    async fn execute_with_retry(
        &self,
        item: &WorkItem,
        board: &Arc<Board>,
        pipeline_ctx: &PipelineContext<'_>,
        session_id: &str,
        initial_prompt: &str,
    ) -> bool {
        const MAX_RETRIES: u32 = 2;

        let input =
            WorkInput::task(initial_prompt, item.id).with_session_id(session_id.to_string());
        let mut last_result = self.process(input).await;

        for attempt in 0..=MAX_RETRIES {
            // LLM failure -> immediate abort (not retryable)
            if let Err(ref e) = last_result {
                warn!(rig = %self.id, item_id = item.id, error = %e, "execution failed, abandoning");
                if let Err(e) = board.abandon(item.id).await {
                    warn!(error = %e, item_id = item.id, "failed to abandon work item after LLM execution failure");
                }
                return false;
            }

            // LLM success -> validate
            let validation = match pipeline_ctx.run_validate(&self.middleware).await {
                Ok(v) => v,
                Err(e) => {
                    warn!(rig = %self.id, item_id = item.id, error = %e, "validation infra failed, abandoning");
                    if let Err(e) = board.abandon(item.id).await {
                        warn!(error = %e, item_id = item.id, "failed to abandon work item after validation infrastructure failure");
                    }
                    return false;
                }
            };

            match validation {
                None => {
                    // Validation passed -> submit
                    if let Err(e) = board.submit(item.id, &self.id).await {
                        warn!(rig = %self.id, item_id = item.id, error = %e, "submit failed");
                    } else {
                        info!(rig = %self.id, item_id = item.id, "submitted work item");
                    }
                    return false;
                }
                Some(ref validation_error) if attempt < MAX_RETRIES => {
                    warn!(
                        rig = %self.id, item_id = item.id,
                        attempt = attempt + 1, max = MAX_RETRIES,
                        "validation failed, retrying"
                    );
                    let fix_prompt = format!(
                        "The previous implementation failed validation. Please fix the errors:\n\n{}",
                        validation_error
                    );
                    let retry_input = WorkInput::task(fix_prompt, item.id)
                        .with_session_id(session_id.to_string());
                    last_result = self.process(retry_input).await;
                }
                Some(validation_error) => {
                    warn!(
                        rig = %self.id, item_id = item.id,
                        error = %validation_error,
                        "validation failed after {MAX_RETRIES} retries, marking stuck"
                    );
                    if let Err(e) = board.mark_stuck(item.id, &self.id).await {
                        warn!(error = %e, item_id = item.id, "failed to mark work item stuck after max retries exceeded");
                    }
                    return true;
                }
            }
        }

        false
    }

    /// Look up session by name from goose session_manager. Returns last (newest) match.
    async fn find_session_by_name(&self, name: &str) -> Option<String> {
        let sessions = match self.agent.config.session_manager.list_sessions().await {
            Ok(s) => s,
            Err(e) => {
                debug!(error = %e, name, "failed to list sessions for lookup");
                return None;
            }
        };
        sessions
            .iter()
            .rev()
            .find(|s| s.name == name)
            .map(|s| s.id.clone())
    }
}
