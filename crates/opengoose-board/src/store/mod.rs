mod merge;
mod persist;

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::branch::Branch;
use crate::work_item::{RigId, WorkItem};

// ── Commit types ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommitId(pub u64);

#[derive(Debug, Clone)]
pub struct Commit {
    pub id: CommitId,
    pub parent: Option<CommitId>,
    pub root_hash: [u8; 32],
    pub branch: RigId,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

// ── CowStore ──────────────────────────────────────────

#[derive(Clone)]
pub struct CowStore {
    pub(crate) main: Arc<BTreeMap<i64, WorkItem>>,
    pub(crate) commits: Vec<Commit>,
    next_commit_id: u64,
}

impl Default for CowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CowStore {
    pub fn new() -> Self {
        Self {
            main: Arc::new(BTreeMap::new()),
            commits: Vec::new(),
            next_commit_id: 0,
        }
    }

    pub fn from_items(items: BTreeMap<i64, WorkItem>, commits: Vec<Commit>) -> Self {
        let next_commit_id = commits.last().map(|c| c.id.0 + 1).unwrap_or(0);
        Self {
            main: Arc::new(items),
            commits,
            next_commit_id,
        }
    }

    /// Insert an item directly into main (used by post()).
    pub fn insert_to_main(&mut self, item: WorkItem) {
        Arc::make_mut(&mut self.main).insert(item.id, item);
    }

    /// Update an item directly in main (used for non-branched transitions).
    pub fn update_in_main(&mut self, id: i64, f: impl FnOnce(&mut WorkItem)) {
        if let Some(item) = Arc::make_mut(&mut self.main).get_mut(&id) {
            f(item);
        }
    }

    /// Remove an item from main.
    pub fn remove_from_main(&mut self, id: i64) {
        Arc::make_mut(&mut self.main).remove(&id);
    }

    /// Create a snapshot branch. O(1) via Arc::clone.
    pub fn branch(&mut self, rig_id: &RigId) -> Branch {
        let base_commit = self.commits.last().map(|c| c.id.0).unwrap_or(0);
        Branch::new(rig_id.clone(), Arc::clone(&self.main), base_commit)
    }

    /// Discard a branch without merging.
    pub fn discard(&mut self, branch: Branch) {
        let _ = branch;
    }

    /// Read-only access to main.
    pub fn main_snapshot(&self) -> Arc<BTreeMap<i64, WorkItem>> {
        Arc::clone(&self.main)
    }

    /// Get an item from main.
    pub fn get(&self, id: i64) -> Option<&WorkItem> {
        self.main.get(&id)
    }

    /// List all items in main.
    pub fn list_main(&self) -> impl Iterator<Item = &WorkItem> {
        self.main.values()
    }

    /// Current commits.
    pub fn commits(&self) -> &[Commit] {
        &self.commits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_item::{Priority, Status};
    use chrono::TimeZone;

    fn make_item(id: i64) -> WorkItem {
        WorkItem {
            id,
            title: format!("Item {id}"),
            description: String::new(),
            created_by: RigId::new("test"),
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            status: Status::Open,
            priority: Priority::P1,
            tags: vec![],
            claimed_by: None,
            updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    fn seeded_store() -> CowStore {
        let mut store = CowStore::new();
        store.insert_to_main(make_item(1));
        store.insert_to_main(make_item(2));
        store.insert_to_main(make_item(3));
        store
    }

    #[test]
    fn branch_creates_snapshot() {
        let mut store = seeded_store();
        let branch = store.branch(&RigId::new("alice"));
        assert_eq!(branch.list().count(), 3);
    }

    #[test]
    fn branch_snapshot_isolated_from_main() {
        let mut store = seeded_store();
        let branch = store.branch(&RigId::new("alice"));
        store.insert_to_main(make_item(4));
        assert_eq!(branch.list().count(), 3);
        assert_eq!(store.main.len(), 4);
    }

    #[test]
    fn merge_applies_branch_changes_to_main() {
        let mut store = seeded_store();
        let mut branch = store.branch(&RigId::new("alice"));

        branch.update(1, |item| {
            item.status = Status::Claimed;
            item.claimed_by = Some(RigId::new("alice"));
            item.updated_at = Utc::now();
        });

        let result = store.merge(branch).expect("merging branch should succeed");
        assert_eq!(
            store.main.get(&1).expect("item 1 should exist").status,
            Status::Claimed
        );
        assert_eq!(result.merged_items.len(), 1);
    }

    #[test]
    fn merge_appends_commit() {
        let mut store = seeded_store();
        assert_eq!(store.commits.len(), 0);

        let branch = store.branch(&RigId::new("alice"));
        store.merge(branch).expect("merging branch should succeed");

        assert_eq!(store.commits.len(), 1);
        assert_eq!(store.commits[0].branch, RigId::new("alice"));
        assert!(store.commits[0].parent.is_none());
    }

    #[test]
    fn second_merge_chains_commits() {
        let mut store = seeded_store();

        let branch1 = store.branch(&RigId::new("alice"));
        store
            .merge(branch1)
            .expect("merging alice's branch should succeed");

        let branch2 = store.branch(&RigId::new("bob"));
        store
            .merge(branch2)
            .expect("merging bob's branch should succeed");

        assert_eq!(store.commits.len(), 2);
        assert_eq!(store.commits[1].parent, Some(store.commits[0].id));
    }

    #[test]
    fn commit_root_hash_changes_with_data() {
        let mut store = seeded_store();

        let branch1 = store.branch(&RigId::new("alice"));
        store
            .merge(branch1)
            .expect("merging alice's branch should succeed");
        let hash1 = store.commits[0].root_hash;

        let mut branch2 = store.branch(&RigId::new("bob"));
        branch2.update(1, |item| {
            item.status = Status::Claimed;
            item.updated_at = Utc::now();
        });
        store
            .merge(branch2)
            .expect("merging bob's branch should succeed");
        let hash2 = store.commits[1].root_hash;

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn discard_branch_does_not_affect_main() {
        let mut store = seeded_store();
        let mut branch = store.branch(&RigId::new("alice"));

        branch.update(1, |item| item.status = Status::Claimed);
        store.discard(branch);

        assert_eq!(
            store.main.get(&1).expect("get should succeed").status,
            Status::Open
        );
        assert_eq!(store.commits.len(), 0);
    }

    #[test]
    fn main_snapshot_returns_arc_clone() {
        let store = seeded_store();
        let snap = store.main_snapshot();
        assert_eq!(snap.len(), 3);
    }

    #[tokio::test]
    async fn persist_and_restore_roundtrip() {
        use sea_orm::Database;
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite connection should succeed");
        crate::board::Board::create_tables(&db)
            .await
            .expect("table creation should succeed");

        let mut store = seeded_store();
        let mut branch = store.branch(&RigId::new("alice"));
        branch.update(1, |item| {
            item.status = Status::Claimed;
            item.claimed_by = Some(RigId::new("alice"));
            item.updated_at = Utc::now();
        });
        store
            .merge(branch)
            .expect("merging alice's branch should succeed");
        store.persist(&db).await.expect("persist should succeed");

        let restored = CowStore::restore(&db)
            .await
            .expect("restore should succeed");
        assert_eq!(restored.main.len(), 3);
        assert_eq!(
            restored.main.get(&1).expect("get should succeed").status,
            Status::Claimed
        );
        assert_eq!(restored.commits.len(), 1);
        assert_eq!(restored.commits[0].branch, RigId::new("alice"));
    }
}
