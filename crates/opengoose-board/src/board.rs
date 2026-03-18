// Wanted Board — pull 기반 작업 분배
//
// 모든 작업은 여기를 통과한다.
// post → claim → submit → merge. 이것이 전부.

use crate::beads;
use crate::branch::{Branch, CommitEntry};
use crate::merge;
use crate::relations::{RelationGraph, RelationType};
use crate::stamps::StampStore;
use crate::store::CowStore;
use crate::work_item::{BoardError, PostWorkItem, RigId, Status, WorkItem};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;

pub struct Board {
    main: CowStore,
    branches: HashMap<String, Branch>,
    next_id: i64,
    relations: RelationGraph,
    stamps: StampStore,
    commit_log: Vec<CommitEntry>,
    notify: Arc<Notify>,
}

impl Board {
    pub fn new() -> Self {
        Self {
            main: CowStore::new(),
            branches: HashMap::new(),
            next_id: 1,
            relations: RelationGraph::new(),
            stamps: StampStore::new(),
            commit_log: Vec::new(),
            notify: Arc::new(Notify::new()),
        }
    }

    // ── 기본 API ─────────────────────────────────────────────

    pub fn post(&mut self, req: PostWorkItem) -> WorkItem {
        let now = Utc::now();
        let id = self.next_id;
        self.next_id += 1;

        let item = WorkItem {
            id,
            title: req.title,
            description: req.description,
            created_by: req.created_by,
            created_at: now,
            status: Status::Open,
            priority: req.priority,
            tags: req.tags,
            claimed_by: None,
            updated_at: now,
        };

        self.main.insert(id, item.clone());
        self.notify.notify_waiters();
        item
    }

    pub fn claim(&mut self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.main.get_mut_or(item_id)?;

        if item.status == Status::Claimed {
            return Err(BoardError::AlreadyClaimed {
                id: item_id,
                claimed_by: item.claimed_by.clone().unwrap_or_else(|| RigId::new("unknown")),
            });
        }

        item.status.validate_transition(Status::Claimed)?;
        item.status = Status::Claimed;
        item.claimed_by = Some(rig_id.clone());
        item.updated_at = Utc::now();

        Ok(item.clone())
    }

    pub fn submit(&mut self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.main.get_mut_or(item_id)?;
        Self::verify_claimed_by(item, rig_id)?;
        item.status.validate_transition(Status::Done)?;

        item.status = Status::Done;
        item.updated_at = Utc::now();

        Ok(item.clone())
    }

    pub fn unclaim(&mut self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.main.get_mut_or(item_id)?;
        Self::verify_claimed_by(item, rig_id)?;
        item.status.validate_transition(Status::Open)?;

        item.status = Status::Open;
        item.claimed_by = None;
        item.updated_at = Utc::now();

        self.notify.notify_waiters();
        Ok(item.clone())
    }

    pub fn mark_stuck(&mut self, item_id: i64, rig_id: &RigId) -> Result<WorkItem, BoardError> {
        let item = self.main.get_mut_or(item_id)?;

        if let Some(claimed) = &item.claimed_by
            && claimed != rig_id
        {
            return Err(BoardError::NotClaimedBy {
                id: item_id,
                claimed_by: claimed.clone(),
                attempted_by: rig_id.clone(),
            });
        }

        item.status.validate_transition(Status::Stuck)?;
        item.status = Status::Stuck;
        item.updated_at = Utc::now();

        Ok(item.clone())
    }

    pub fn retry(&mut self, item_id: i64) -> Result<WorkItem, BoardError> {
        let item = self.main.get_mut_or(item_id)?;
        item.status.validate_transition(Status::Open)?;

        item.status = Status::Open;
        item.claimed_by = None;
        item.updated_at = Utc::now();

        self.notify.notify_waiters();
        Ok(item.clone())
    }

    pub fn abandon(&mut self, item_id: i64) -> Result<WorkItem, BoardError> {
        let item = self.main.get_mut_or(item_id)?;
        item.status.validate_transition(Status::Abandoned)?;

        item.status = Status::Abandoned;
        item.updated_at = Utc::now();

        Ok(item.clone())
    }

    /// claimed_by 검증 헬퍼: 올바른 rig이 claim 중인지 확인.
    fn verify_claimed_by(item: &WorkItem, rig_id: &RigId) -> Result<(), BoardError> {
        match &item.claimed_by {
            Some(claimed) if claimed != rig_id => Err(BoardError::NotClaimedBy {
                id: item.id,
                claimed_by: claimed.clone(),
                attempted_by: rig_id.clone(),
            }),
            None => Err(BoardError::NotClaimed { id: item.id }),
            _ => Ok(()),
        }
    }

    pub fn get(&self, item_id: i64) -> Option<&WorkItem> {
        self.main.get(item_id)
    }

    pub fn list(&self) -> Vec<&WorkItem> {
        self.main.values().collect()
    }

    // ── 브랜치 API ───────────────────────────────────────────

    pub fn branch(&mut self, name: impl Into<String>) -> String {
        let name = name.into();
        let branch = Branch::from_main(&name, &self.main);
        self.branches.insert(name.clone(), branch);
        name
    }

    pub fn branch_store(&self, name: &str) -> Result<&CowStore, BoardError> {
        self.branches
            .get(name)
            .map(|b| &b.store)
            .ok_or_else(|| BoardError::BranchNotFound(name.to_string()))
    }

    pub fn branch_store_mut(&mut self, name: &str) -> Result<&mut CowStore, BoardError> {
        self.branches
            .get_mut(name)
            .map(|b| &mut b.store)
            .ok_or_else(|| BoardError::BranchNotFound(name.to_string()))
    }

    pub fn commit(&mut self, branch_name: &str, message: impl Into<String>) -> Result<[u8; 32], BoardError> {
        let branch = self
            .branches
            .get_mut(branch_name)
            .ok_or_else(|| BoardError::BranchNotFound(branch_name.to_string()))?;

        let hash = branch.store.root_hash();
        let parent_hash = self.commit_log.last().map(|e| e.root_hash);

        self.commit_log.push(CommitEntry {
            branch: branch_name.to_string(),
            message: message.into(),
            root_hash: hash,
            parent_hash,
            timestamp: Utc::now(),
        });

        Ok(hash)
    }

    pub fn merge_branch(&mut self, branch_name: &str) -> Result<(), BoardError> {
        let branch = self
            .branches
            .get(branch_name)
            .ok_or_else(|| BoardError::BranchNotFound(branch_name.to_string()))?;

        let changed_keys = branch.store.diff_keys(&branch.base);

        for id in changed_keys {
            match (branch.base.get(id), branch.store.get(id), self.main.get(id)) {
                (None, Some(source), None) => {
                    self.main.insert(id, source.clone());
                }
                (Some(base), Some(source), Some(dest)) => {
                    let merged = merge::merge_work_item(base, source, dest);
                    self.main.insert(id, merged);
                }
                (Some(_), Some(source), _) => {
                    self.main.insert(id, source.clone());
                }
                _ => {}
            }
        }

        self.branches.remove(branch_name);
        Ok(())
    }

    pub fn drop_branch(&mut self, branch_name: &str) {
        self.branches.remove(branch_name);
    }

    // ── Relations API ────────────────────────────────────────

    pub fn add_dependency(&mut self, blocker: i64, blocked: i64) -> Result<(), BoardError> {
        self.relations.add(blocker, blocked, RelationType::Blocks)
    }

    pub fn remove_dependency(&mut self, blocker: i64, blocked: i64) {
        self.relations.remove(blocker, blocked);
    }

    pub fn relations(&self) -> &RelationGraph {
        &self.relations
    }

    // ── Stamps API ───────────────────────────────────────────

    pub fn stamps(&self) -> &StampStore {
        &self.stamps
    }

    pub fn stamps_mut(&mut self) -> &mut StampStore {
        &mut self.stamps
    }

    // ── Beads API ────────────────────────────────────────────

    pub fn ready(&self) -> Vec<WorkItem> {
        // 의존성이 걸린 아이템만 검사 — 대부분의 아이템은 의존성 없음
        let mut blocked_ids = std::collections::HashSet::new();

        for &id in self.relations.blocked_item_ids() {
            let blockers = self.relations.blockers_of(id);
            let has_open_blocker = blockers.iter().any(|&bid| {
                self.main.get(bid).is_none_or(|b| b.status != Status::Done)
            });
            if has_open_blocker {
                blocked_ids.insert(id);
            }
        }

        beads::filter_ready(self.main.values().cloned(), &blocked_ids)
    }

    pub fn prime(&self, rig_id: &RigId) -> String {
        let items: Vec<_> = self.main.values().cloned().collect();
        beads::prime_summary(&items, rig_id)
    }

    // ── 알림 API ─────────────────────────────────────────────

    pub async fn wait_for_claimable(&self) {
        self.notify.notified().await;
    }

    pub fn notify_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    // ── 내부 ─────────────────────────────────────────────────

    pub fn main_store(&self) -> &CowStore {
        &self.main
    }

    pub fn commit_log(&self) -> &[CommitEntry] {
        &self.commit_log
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stamps::{Dimension, Severity, Stamp};
    use crate::work_item::Priority;

    fn post_item(board: &mut Board, title: &str) -> WorkItem {
        board.post(PostWorkItem {
            title: title.to_string(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P1,
            tags: vec![],
        })
    }

    // ── 기본 수명주기 ────────────────────────────────────────

    #[test]
    fn post_creates_open_item() {
        let mut board = Board::new();
        let item = post_item(&mut board, "test");
        assert_eq!(item.id, 1);
        assert_eq!(item.status, Status::Open);
        assert!(item.claimed_by.is_none());
    }

    #[test]
    fn auto_increment_ids() {
        let mut board = Board::new();
        let a = post_item(&mut board, "a");
        let b = post_item(&mut board, "b");
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
    }

    #[test]
    fn claim_transitions_to_claimed() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let rig = RigId::new("dev");
        let claimed = board.claim(1, &rig).unwrap();
        assert_eq!(claimed.status, Status::Claimed);
        assert_eq!(claimed.claimed_by, Some(rig));
    }

    #[test]
    fn claim_already_claimed_fails() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        board.claim(1, &RigId::new("dev")).unwrap();
        assert!(board.claim(1, &RigId::new("other")).is_err());
    }

    #[test]
    fn submit_completes_work() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let rig = RigId::new("dev");
        board.claim(1, &rig).unwrap();

        let done = board.submit(1, &rig).unwrap();
        assert_eq!(done.status, Status::Done);
    }

    #[test]
    fn submit_wrong_rig_fails() {
        let mut board = Board::new();
        post_item(&mut board, "test");
        board.claim(1, &RigId::new("dev")).unwrap();

        assert!(board.submit(1, &RigId::new("other")).is_err());
    }

    #[test]
    fn unclaim_returns_to_open() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let rig = RigId::new("dev");
        board.claim(1, &rig).unwrap();
        let unclaimed = board.unclaim(1, &rig).unwrap();

        assert_eq!(unclaimed.status, Status::Open);
        assert!(unclaimed.claimed_by.is_none());
    }

    #[test]
    fn stuck_and_retry() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let rig = RigId::new("dev");
        board.claim(1, &rig).unwrap();
        board.mark_stuck(1, &rig).unwrap();
        assert_eq!(board.get(1).unwrap().status, Status::Stuck);

        board.retry(1).unwrap();
        assert_eq!(board.get(1).unwrap().status, Status::Open);
    }

    #[test]
    fn abandon_from_open() {
        let mut board = Board::new();
        post_item(&mut board, "test");
        board.abandon(1).unwrap();
        assert_eq!(board.get(1).unwrap().status, Status::Abandoned);
    }

    #[test]
    fn abandon_from_stuck() {
        let mut board = Board::new();
        post_item(&mut board, "test");
        let rig = RigId::new("dev");
        board.claim(1, &rig).unwrap();
        board.mark_stuck(1, &rig).unwrap();
        board.abandon(1).unwrap();
        assert_eq!(board.get(1).unwrap().status, Status::Abandoned);
    }

    #[test]
    fn invalid_transition_fails() {
        let mut board = Board::new();
        post_item(&mut board, "test");
        // Open → Done (invalid — must claim first)
        assert!(board.submit(1, &RigId::new("dev")).is_err());
    }

    // ── 브랜치 + 머지 ───────────────────────────────────────

    #[test]
    fn branch_is_isolated() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let br = board.branch("dev-branch");

        let store = board.branch_store_mut(&br).unwrap();
        let item = store.get_mut(1).unwrap();
        item.status = Status::Claimed;
        item.claimed_by = Some(RigId::new("dev"));
        item.updated_at = Utc::now();

        assert_eq!(board.get(1).unwrap().status, Status::Open);
    }

    #[test]
    fn merge_applies_branch_changes() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let br = board.branch("dev-branch");

        {
            let store = board.branch_store_mut(&br).unwrap();
            let item = store.get_mut(1).unwrap();
            item.status = Status::Claimed;
            item.claimed_by = Some(RigId::new("dev"));
            item.updated_at = Utc::now();
        }

        board.merge_branch(&br).unwrap();
        assert_eq!(board.get(1).unwrap().status, Status::Claimed);
    }

    #[test]
    fn merge_status_higher_wins() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let br = board.branch("dev-branch");

        {
            let store = board.branch_store_mut(&br).unwrap();
            let item = store.get_mut(1).unwrap();
            item.status = Status::Claimed;
            item.updated_at = Utc::now();
        }

        {
            let item = board.main.get_mut(1).unwrap();
            item.status = Status::Done;
            item.updated_at = Utc::now();
        }

        board.merge_branch(&br).unwrap();
        assert_eq!(board.get(1).unwrap().status, Status::Done);
    }

    #[test]
    fn branch_new_items_merged() {
        let mut board = Board::new();
        let br = board.branch("dev-branch");

        {
            let store = board.branch_store_mut(&br).unwrap();
            let now = Utc::now();
            store.insert(100, WorkItem {
                id: 100,
                title: "from branch".into(),
                description: String::new(),
                created_by: RigId::new("dev"),
                created_at: now,
                status: Status::Open,
                priority: Priority::P1,
                tags: vec![],
                claimed_by: None,
                updated_at: now,
            });
        }

        board.merge_branch(&br).unwrap();
        assert_eq!(board.get(100).unwrap().title, "from branch");
    }

    #[test]
    fn drop_branch_no_effect() {
        let mut board = Board::new();
        post_item(&mut board, "test");
        let br = board.branch("dev-branch");

        {
            let store = board.branch_store_mut(&br).unwrap();
            store.get_mut(1).unwrap().status = Status::Done;
        }

        board.drop_branch(&br);
        assert_eq!(board.get(1).unwrap().status, Status::Open);
    }

    // ── Relations + ready() ──────────────────────────────────

    #[test]
    fn ready_excludes_blocked() {
        let mut board = Board::new();
        let a = post_item(&mut board, "blocker");
        let b = post_item(&mut board, "blocked");

        board.add_dependency(a.id, b.id).unwrap();

        let ready = board.ready();
        let ids: Vec<i64> = ready.iter().map(|i| i.id).collect();
        assert!(ids.contains(&a.id));
        assert!(!ids.contains(&b.id));
    }

    #[test]
    fn ready_unblocks_when_done() {
        let mut board = Board::new();
        let a = post_item(&mut board, "blocker");
        let b = post_item(&mut board, "blocked");

        board.add_dependency(a.id, b.id).unwrap();
        board.claim(a.id, &RigId::new("dev")).unwrap();
        board.submit(a.id, &RigId::new("dev")).unwrap();

        let ids: Vec<i64> = board.ready().iter().map(|i| i.id).collect();
        assert!(ids.contains(&b.id));
    }

    #[test]
    fn ready_priority_sorted() {
        let mut board = Board::new();
        board.post(PostWorkItem {
            title: "low".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P2,
            tags: vec![],
        });
        board.post(PostWorkItem {
            title: "urgent".into(),
            description: String::new(),
            created_by: RigId::new("user"),
            priority: Priority::P0,
            tags: vec![],
        });

        let ready = board.ready();
        assert_eq!(ready[0].priority, Priority::P0);
        assert_eq!(ready[1].priority, Priority::P2);
    }

    // ── Stamps ───────────────────────────────────────────────

    #[test]
    fn stamp_yearbook_rule() {
        let mut board = Board::new();
        let result = board.stamps_mut().add(Stamp {
            target_rig: RigId::new("dev"),
            work_item: 1,
            dimension: Dimension::Quality,
            score: 1.0,
            severity: Severity::Leaf,
            stamped_by: RigId::new("dev"),
            timestamp: Utc::now(),
        });
        assert!(result.is_err());
    }

    // ── wait_for_claimable ───────────────────────────────────

    #[tokio::test]
    async fn wait_for_claimable_wakes_on_post() {
        let mut board = Board::new();
        let notify = board.notify_handle();

        let handle = tokio::spawn(async move {
            notify.notified().await;
            true
        });

        tokio::task::yield_now().await;
        post_item(&mut board, "wake up");

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
        assert!(result.is_ok());
    }

    // ── Commit log ───────────────────────────────────────────

    #[test]
    fn commit_creates_log_entry() {
        let mut board = Board::new();
        post_item(&mut board, "test");

        let br = board.branch("dev");
        board.commit(&br, "initial work").unwrap();

        assert_eq!(board.commit_log().len(), 1);
        assert_eq!(board.commit_log()[0].branch, "dev");
    }

    // ── prime() ──────────────────────────────────────────────

    #[test]
    fn prime_returns_summary() {
        let mut board = Board::new();
        post_item(&mut board, "task a");
        post_item(&mut board, "task b");

        let summary = board.prime(&RigId::new("dev"));
        assert!(summary.contains("2 open"));
    }

    // ── Edge cases ────────────────────────────────────────────

    #[test]
    fn merge_already_dropped_branch_fails() {
        let mut board = Board::new();
        post_item(&mut board, "test");
        let br = board.branch("dev-branch");
        board.merge_branch(&br).unwrap();

        // 이미 제거된 브랜치 재머지 시도
        assert!(matches!(
            board.merge_branch(&br),
            Err(BoardError::BranchNotFound(_))
        ));
    }

    #[test]
    fn ready_filters_mixed_statuses() {
        let mut board = Board::new();
        let rig = RigId::new("dev");

        let a = post_item(&mut board, "open");
        let b = post_item(&mut board, "claimed");
        let c = post_item(&mut board, "done");
        let d = post_item(&mut board, "stuck");

        board.claim(b.id, &rig).unwrap();
        board.claim(c.id, &rig).unwrap();
        board.submit(c.id, &rig).unwrap();
        board.claim(d.id, &rig).unwrap();
        board.mark_stuck(d.id, &rig).unwrap();

        let ready = board.ready();
        let ids: Vec<i64> = ready.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![a.id]); // open만
    }

    #[test]
    fn stamp_invalid_score_rejected() {
        let mut board = Board::new();
        let result = board.stamps_mut().add(Stamp {
            target_rig: RigId::new("dev"),
            work_item: 1,
            dimension: Dimension::Quality,
            score: 2.0, // 범위 초과
            severity: Severity::Leaf,
            stamped_by: RigId::new("reviewer"),
            timestamp: Utc::now(),
        });
        assert!(matches!(result, Err(BoardError::InvalidScore(_))));
    }
}
