//! ProllyTree-backed storage for work items.
//!
//! This module provides a content-addressed, version-aware storage layer
//! built on prollytree. It runs alongside the existing SQLite/Diesel layer
//! and will eventually replace it.
//!
//! Key properties:
//! - O(diff) time complexity between snapshots
//! - Structural sharing (branches share unchanged subtrees)
//! - Cryptographic proofs of data integrity

use std::path::PathBuf;

use prollytree::config::TreeConfig;
use prollytree::diff::{ConflictResolver, DiffResult, MergeConflict, MergeResult};
use prollytree::storage::{FileNodeStorage, InMemoryNodeStorage, NodeStorage};
use prollytree::tree::{ProllyTree, Tree};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Work item stored in the prolly tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProllyWorkItem {
    pub hash_id: String,
    pub session_key: String,
    pub team_run_id: String,
    pub parent_hash_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assigned_to: Option<String>,
    pub priority: i32,
    pub is_ephemeral: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Key prefix for work items in the prolly tree.
const WORK_ITEM_PREFIX: &[u8] = b"wi:";

/// Key prefix for relationships.
const REL_PREFIX: &[u8] = b"rel:";

/// Build a work item key from a hash_id.
fn work_item_key(hash_id: &str) -> Vec<u8> {
    let mut key = WORK_ITEM_PREFIX.to_vec();
    key.extend_from_slice(hash_id.as_bytes());
    key
}

/// Build a relationship key.
fn rel_key(child_hash_id: &str, parent_hash_id: &str) -> Vec<u8> {
    let mut key = REL_PREFIX.to_vec();
    key.extend_from_slice(child_hash_id.as_bytes());
    key.push(b':');
    key.extend_from_slice(parent_hash_id.as_bytes());
    key
}

/// Conflict resolver that prefers the most recently updated work item.
pub struct WorkItemStatusResolver;

impl ConflictResolver for WorkItemStatusResolver {
    fn resolve_conflict(&self, conflict: &MergeConflict) -> Option<MergeResult> {
        // If source has a value, try to pick the one with higher status priority
        // completed > in_progress > pending
        match (&conflict.source_value, &conflict.destination_value) {
            (Some(src), Some(dst)) => {
                let src_item: Option<ProllyWorkItem> = serde_json::from_slice(src).ok();
                let dst_item: Option<ProllyWorkItem> = serde_json::from_slice(dst).ok();

                match (src_item, dst_item) {
                    (Some(s), Some(d)) => {
                        let winner = if status_priority(&s.status) >= status_priority(&d.status) {
                            src
                        } else {
                            dst
                        };
                        Some(MergeResult::Modified(conflict.key.clone(), winner.clone()))
                    }
                    _ => Some(MergeResult::Modified(conflict.key.clone(), src.clone())),
                }
            }
            (Some(src), None) => {
                Some(MergeResult::Modified(conflict.key.clone(), src.clone()))
            }
            (None, _) => Some(MergeResult::Removed(conflict.key.clone())),
        }
    }
}

fn status_priority(status: &str) -> u8 {
    match status {
        "completed" => 4,
        "failed" => 3,
        "in_progress" => 2,
        "cancelled" => 1,
        _ => 0, // pending, compacted
    }
}

/// ProllyTree-backed work item store.
///
/// Generic over the storage backend (`InMemoryNodeStorage` for tests,
/// `FileNodeStorage` for production).
pub struct ProllyStore<const N: usize, S: NodeStorage<N>> {
    tree: ProllyTree<N, S>,
}

impl<const N: usize, S: NodeStorage<N>> ProllyStore<N, S> {
    /// Create a new store with the given storage backend.
    pub fn new(storage: S) -> Self {
        let config = TreeConfig::<N>::default();
        Self {
            tree: ProllyTree::new(storage, config),
        }
    }

    /// Insert a work item. Returns the key used.
    pub fn insert_work_item(&mut self, item: &ProllyWorkItem) -> Vec<u8> {
        let key = work_item_key(&item.hash_id);
        let value = serde_json::to_vec(item).expect("WorkItem serialization cannot fail");
        self.tree.insert(key.clone(), value);
        debug!(hash_id = %item.hash_id, title = %item.title, "prolly: work item inserted");
        key
    }

    /// Get a work item by hash_id.
    pub fn get_work_item(&self, hash_id: &str) -> Option<ProllyWorkItem> {
        let key = work_item_key(hash_id);
        let node = self.tree.find(&key)?;
        let idx = node.keys.iter().position(|k| k == &key)?;
        serde_json::from_slice(&node.values[idx]).ok()
    }

    /// Update a work item. Returns true if it existed.
    pub fn update_work_item(&mut self, item: &ProllyWorkItem) -> bool {
        let key = work_item_key(&item.hash_id);
        let value = serde_json::to_vec(item).expect("WorkItem serialization cannot fail");
        self.tree.update(key, value)
    }

    /// Delete a work item by hash_id. Returns true if it existed.
    pub fn delete_work_item(&mut self, hash_id: &str) -> bool {
        let key = work_item_key(hash_id);
        self.tree.delete(&key)
    }

    /// List all work items (full scan, deduplicated).
    pub fn list_work_items(&self) -> Vec<ProllyWorkItem> {
        let keys = self.tree.collect_keys();
        let mut seen = std::collections::HashSet::new();
        keys.iter()
            .filter(|k| k.starts_with(WORK_ITEM_PREFIX))
            .filter(|k| seen.insert((*k).clone()))
            .filter_map(|key| {
                let node = self.tree.find(key)?;
                let idx = node.keys.iter().position(|k| k == key)?;
                serde_json::from_slice(&node.values[idx]).ok()
            })
            .collect()
    }

    /// List work items filtered by status.
    pub fn list_by_status(&self, status: &str) -> Vec<ProllyWorkItem> {
        self.list_work_items()
            .into_iter()
            .filter(|item| item.status == status)
            .collect()
    }

    /// List work items for a specific team run.
    pub fn list_for_run(&self, team_run_id: &str) -> Vec<ProllyWorkItem> {
        self.list_work_items()
            .into_iter()
            .filter(|item| item.team_run_id == team_run_id)
            .collect()
    }

    /// Insert a relationship between two work items.
    pub fn insert_relationship(
        &mut self,
        child_hash_id: &str,
        parent_hash_id: &str,
        kind: &str,
    ) {
        let key = rel_key(child_hash_id, parent_hash_id);
        self.tree.insert(key, kind.as_bytes().to_vec());
    }

    /// Check if a work item is blocked (has a "blocks" relationship
    /// where the blocker is not completed).
    pub fn is_blocked(&self, hash_id: &str) -> bool {
        let prefix = format!("rel:{}:", hash_id);
        let keys = self.tree.collect_keys();
        keys.iter()
            .filter(|k| k.starts_with(prefix.as_bytes()))
            .any(|key| {
                let node = self.tree.find(key);
                node.and_then(|n| {
                    let idx = n.keys.iter().position(|k| k == key)?;
                    let kind = std::str::from_utf8(&n.values[idx]).ok()?;
                    if kind == "blocks" {
                        // Extract parent hash_id from key
                        let key_str = std::str::from_utf8(key).ok()?;
                        let parent = key_str.strip_prefix(&prefix)?;
                        let parent_item = self.get_work_item(parent)?;
                        if parent_item.status != "completed" {
                            return Some(true);
                        }
                    }
                    None
                })
                .unwrap_or(false)
            })
    }

    /// Get the root hash (content-addressed snapshot identifier).
    pub fn root_hash(&self) -> Option<prollytree::digest::ValueDigest<N>> {
        self.tree.get_root_hash()
    }

    /// Get tree statistics.
    pub fn stats(&self) -> prollytree::tree::TreeStats {
        self.tree.stats()
    }

    /// Diff this store against another store.
    pub fn diff(&self, other: &Self) -> Vec<DiffResult> {
        self.tree.diff(&other.tree)
    }

    /// Total number of key-value pairs.
    pub fn size(&self) -> usize {
        self.tree.size()
    }
}

/// Convenience type alias for in-memory store (tests).
pub type InMemoryProllyStore = ProllyStore<32, InMemoryNodeStorage<32>>;

/// Convenience type alias for file-backed store (production).
pub type FileProllyStore = ProllyStore<32, FileNodeStorage<32>>;

/// Create a new in-memory prolly store.
pub fn in_memory_store() -> InMemoryProllyStore {
    ProllyStore::new(InMemoryNodeStorage::<32>::default())
}

/// Create a new file-backed prolly store.
pub fn file_store(dir: PathBuf) -> FileProllyStore {
    ProllyStore::new(FileNodeStorage::<32>::new(dir))
}

/// Git-versioned work item store with branch/commit/merge support.
pub mod versioned {
    use std::path::Path;

    use prollytree::git::versioned_store::InMemoryVersionedKvStore;
    use prollytree::git::types::GitKvError;

    use super::ProllyWorkItem;

    const WORK_ITEM_PREFIX: &[u8] = b"wi:";

    fn work_item_key(hash_id: &str) -> Vec<u8> {
        let mut key = WORK_ITEM_PREFIX.to_vec();
        key.extend_from_slice(hash_id.as_bytes());
        key
    }

    /// A versioned work item store backed by prollytree + git.
    ///
    /// Supports branching, committing, and merging work item snapshots.
    pub struct VersionedWorkItemStore {
        inner: InMemoryVersionedKvStore<32>,
    }

    impl VersionedWorkItemStore {
        /// Initialize a new versioned store in a directory that contains a git repo.
        pub fn init(path: &Path) -> Result<Self, GitKvError> {
            let inner = InMemoryVersionedKvStore::<32>::init(path)?;
            Ok(Self { inner })
        }

        /// Open an existing versioned store.
        pub fn open(path: &Path) -> Result<Self, GitKvError> {
            let inner = InMemoryVersionedKvStore::<32>::open(path)?;
            Ok(Self { inner })
        }

        /// Insert a work item (staged, not yet committed).
        pub fn insert(&mut self, item: &ProllyWorkItem) -> Result<(), GitKvError> {
            let key = work_item_key(&item.hash_id);
            let value = serde_json::to_vec(item).map_err(|e| {
                GitKvError::GitObjectError(format!("serialization: {e}"))
            })?;
            self.inner.insert(key, value)
        }

        /// Get a work item by hash_id.
        pub fn get(&self, hash_id: &str) -> Option<ProllyWorkItem> {
            let key = work_item_key(hash_id);
            let value = self.inner.get(&key)?;
            serde_json::from_slice(&value).ok()
        }

        /// Update a work item (staged).
        pub fn update(&mut self, item: &ProllyWorkItem) -> Result<bool, GitKvError> {
            let key = work_item_key(&item.hash_id);
            let value = serde_json::to_vec(item).map_err(|e| {
                GitKvError::GitObjectError(format!("serialization: {e}"))
            })?;
            self.inner.update(key, value)
        }

        /// Delete a work item (staged).
        pub fn delete(&mut self, hash_id: &str) -> Result<bool, GitKvError> {
            let key = work_item_key(hash_id);
            self.inner.delete(&key)
        }

        /// Commit all staged changes. Returns commit ID as hex string.
        pub fn commit(&mut self, message: &str) -> Result<String, GitKvError> {
            let oid = self.inner.commit(message)?;
            Ok(oid.to_string())
        }

        /// Create a new branch from current HEAD.
        pub fn create_branch(&mut self, name: &str) -> Result<(), GitKvError> {
            self.inner.create_branch(name)
        }

        /// Get the current branch name.
        pub fn current_branch(&self) -> &str {
            self.inner.current_branch()
        }

        /// List all branches.
        pub fn list_branches(&self) -> Result<Vec<String>, GitKvError> {
            self.inner.list_branches()
        }

        /// Get commit history.
        pub fn log(&self) -> Result<Vec<prollytree::git::types::CommitInfo>, GitKvError> {
            self.inner.log()
        }

        /// Show staged changes status.
        pub fn status(&self) -> Vec<(Vec<u8>, String)> {
            self.inner.status()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(hash_id: &str, title: &str, status: &str) -> ProllyWorkItem {
        ProllyWorkItem {
            hash_id: hash_id.to_string(),
            session_key: "sess1".to_string(),
            team_run_id: "run1".to_string(),
            parent_hash_id: None,
            title: title.to_string(),
            description: None,
            status: status.to_string(),
            assigned_to: None,
            priority: 3,
            is_ephemeral: false,
            created_at: "2026-03-12T00:00:00Z".to_string(),
            updated_at: "2026-03-12T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_insert_and_get() {
        let mut store = in_memory_store();
        let item = make_item("bd-abc123", "Fix auth bug", "pending");
        store.insert_work_item(&item);

        let retrieved = store.get_work_item("bd-abc123").unwrap();
        assert_eq!(retrieved.title, "Fix auth bug");
        assert_eq!(retrieved.status, "pending");
    }

    #[test]
    fn test_update() {
        let mut store = in_memory_store();
        let mut item = make_item("bd-abc123", "Fix auth bug", "pending");
        store.insert_work_item(&item);

        item.status = "in_progress".to_string();
        item.assigned_to = Some("coder".to_string());
        assert!(store.update_work_item(&item));

        let retrieved = store.get_work_item("bd-abc123").unwrap();
        assert_eq!(retrieved.status, "in_progress");
        assert_eq!(retrieved.assigned_to.as_deref(), Some("coder"));
    }

    #[test]
    fn test_delete() {
        let mut store = in_memory_store();
        let item = make_item("bd-abc123", "Fix auth bug", "pending");
        store.insert_work_item(&item);
        assert!(store.delete_work_item("bd-abc123"));
        assert!(store.get_work_item("bd-abc123").is_none());
    }

    #[test]
    fn test_get_nonexistent() {
        let store = in_memory_store();
        assert!(store.get_work_item("bd-nope").is_none());
    }

    #[test]
    fn test_list_work_items() {
        let mut store = in_memory_store();
        store.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        store.insert_work_item(&make_item("bd-002", "Task B", "in_progress"));
        store.insert_work_item(&make_item("bd-003", "Task C", "completed"));

        let all = store.list_work_items();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_list_by_status() {
        let mut store = in_memory_store();
        store.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        store.insert_work_item(&make_item("bd-002", "Task B", "pending"));
        store.insert_work_item(&make_item("bd-003", "Task C", "completed"));

        let all = store.list_work_items();
        assert_eq!(all.len(), 3);
        let pending = store.list_by_status("pending");
        assert_eq!(pending.len(), 2);
        let completed = store.list_by_status("completed");
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_list_for_run() {
        let mut store = in_memory_store();
        store.insert_work_item(&make_item("bd-001", "Task A", "pending"));

        let mut item_b = make_item("bd-002", "Task B", "pending");
        item_b.team_run_id = "run2".to_string();
        store.insert_work_item(&item_b);

        let run1 = store.list_for_run("run1");
        assert_eq!(run1.len(), 1);
        assert_eq!(run1[0].hash_id, "bd-001");
    }

    #[test]
    fn test_root_hash_changes() {
        let mut store = in_memory_store();
        let hash_before = store.root_hash();

        store.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        let hash_after = store.root_hash();

        // Root hash should change after insertion
        assert_ne!(hash_before, hash_after);
    }

    #[test]
    fn test_same_data_same_hash() {
        let mut store1 = in_memory_store();
        let mut store2 = in_memory_store();

        let item = make_item("bd-001", "Task A", "pending");
        store1.insert_work_item(&item);
        store2.insert_work_item(&item);

        // History-independent: same data → same root hash
        assert_eq!(store1.root_hash(), store2.root_hash());
    }

    #[test]
    fn test_diff_between_stores() {
        let mut store1 = in_memory_store();
        let mut store2 = in_memory_store();

        // Both have Task A
        store1.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        store2.insert_work_item(&make_item("bd-001", "Task A", "pending"));

        // Only store2 has Task B
        store2.insert_work_item(&make_item("bd-002", "Task B", "pending"));

        // store1 has Task C, store2 doesn't
        store1.insert_work_item(&make_item("bd-003", "Task C", "pending"));

        let diff = store1.diff(&store2);
        // Should detect added (bd-002) and removed (bd-003)
        assert!(!diff.is_empty());
    }

    #[test]
    fn test_stats() {
        let mut store = in_memory_store();
        store.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        store.insert_work_item(&make_item("bd-002", "Task B", "pending"));

        let stats = store.stats();
        assert_eq!(stats.total_key_value_pairs, 2);
    }

    #[test]
    fn test_relationship_and_blocking() {
        let mut store = in_memory_store();
        store.insert_work_item(&make_item("bd-001", "Blocker", "in_progress"));
        store.insert_work_item(&make_item("bd-002", "Blocked", "pending"));

        // bd-002 is blocked by bd-001
        store.insert_relationship("bd-002", "bd-001", "blocks");
        assert!(store.is_blocked("bd-002"));

        // Complete the blocker
        let mut blocker = store.get_work_item("bd-001").unwrap();
        blocker.status = "completed".to_string();
        store.update_work_item(&blocker);

        // bd-002 should no longer be blocked
        assert!(!store.is_blocked("bd-002"));
    }

    #[test]
    fn test_conflict_resolver() {
        let resolver = WorkItemStatusResolver;

        let conflict = MergeConflict {
            key: b"wi:bd-001".to_vec(),
            base_value: None,
            source_value: Some(
                serde_json::to_vec(&make_item("bd-001", "Task", "completed")).unwrap(),
            ),
            destination_value: Some(
                serde_json::to_vec(&make_item("bd-001", "Task", "in_progress")).unwrap(),
            ),
        };

        let result = resolver.resolve_conflict(&conflict);
        match result {
            Some(MergeResult::Modified(_, value)) => {
                let item: ProllyWorkItem = serde_json::from_slice(&value).unwrap();
                // completed has higher priority than in_progress
                assert_eq!(item.status, "completed");
            }
            _ => panic!("Expected Modified result"),
        }
    }

    #[test]
    fn test_file_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        // Write
        {
            let mut store = file_store(path.clone());
            store.insert_work_item(&make_item("bd-001", "Persistent Task", "pending"));
            // Persist root so it can be reloaded
            store.tree.persist_root();
            store.tree.save_config().unwrap();
        }

        // Read back
        {
            let storage = FileNodeStorage::<32>::new(path);
            let config = ProllyTree::<32, FileNodeStorage<32>>::load_config(&storage).unwrap();
            let tree = ProllyTree::load_from_storage(storage, config);
            assert!(tree.is_some(), "Should reload tree from disk");
            let tree = tree.unwrap();
            let store = ProllyStore { tree };
            let item = store.get_work_item("bd-001").unwrap();
            assert_eq!(item.title, "Persistent Task");
        }
    }

    #[test]
    fn test_batch_insert() {
        let mut store = in_memory_store();
        let items: Vec<_> = (0..100)
            .map(|i| make_item(&format!("bd-{i:04}"), &format!("Task {i}"), "pending"))
            .collect();

        for item in &items {
            store.insert_work_item(item);
        }

        assert_eq!(store.list_work_items().len(), 100);

        // Verify random access
        let item50 = store.get_work_item("bd-0050").unwrap();
        assert_eq!(item50.title, "Task 50");
    }

    mod versioned_tests {
        use super::*;
        use crate::prolly::versioned::VersionedWorkItemStore;
        use std::process::Command;

        fn init_git_repo(dir: &std::path::Path) {
            Command::new("git")
                .args(["init", "--initial-branch=main"])
                .current_dir(dir)
                .output()
                .expect("git init failed");
            Command::new("git")
                .args(["config", "user.email", "test@opengoose.dev"])
                .current_dir(dir)
                .output()
                .expect("git config failed");
            Command::new("git")
                .args(["config", "user.name", "Test"])
                .current_dir(dir)
                .output()
                .expect("git config failed");
            // Need an initial commit for HEAD to exist
            Command::new("git")
                .args(["commit", "--allow-empty", "-m", "init"])
                .current_dir(dir)
                .output()
                .expect("git commit failed");
        }

        #[test]
        fn test_versioned_insert_commit() {
            let dir = tempfile::tempdir().unwrap();
            init_git_repo(dir.path());

            let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();
            let item = make_item("bd-v001", "Versioned task", "pending");
            store.insert(&item).unwrap();

            // Should be visible before commit (from staging area)
            let retrieved = store.get("bd-v001").unwrap();
            assert_eq!(retrieved.title, "Versioned task");

            // Commit
            let commit_id = store.commit("Add first task").unwrap();
            assert!(!commit_id.is_empty());

            // Still visible after commit
            let retrieved = store.get("bd-v001").unwrap();
            assert_eq!(retrieved.title, "Versioned task");
        }

        #[test]
        fn test_versioned_branch_and_commit() {
            let dir = tempfile::tempdir().unwrap();
            init_git_repo(dir.path());

            let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

            // Insert on main
            let item = make_item("bd-v001", "Main task", "pending");
            store.insert(&item).unwrap();
            store.commit("Add main task").unwrap();

            // Create feature branch
            store.create_branch("feature/new-work").unwrap();
            assert_eq!(store.current_branch(), "feature/new-work");

            // Insert on feature branch
            let item2 = make_item("bd-v002", "Feature task", "pending");
            store.insert(&item2).unwrap();
            store.commit("Add feature task").unwrap();

            // Both should be visible on feature branch
            assert!(store.get("bd-v001").is_some());
            assert!(store.get("bd-v002").is_some());

            // Verify branch list
            let branches = store.list_branches().unwrap();
            assert!(branches.contains(&"main".to_string()));
            assert!(branches.contains(&"feature/new-work".to_string()));
        }

        #[test]
        fn test_versioned_commit_log() {
            let dir = tempfile::tempdir().unwrap();
            init_git_repo(dir.path());

            let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

            store.insert(&make_item("bd-v001", "Task 1", "pending")).unwrap();
            store.commit("First commit").unwrap();

            store.insert(&make_item("bd-v002", "Task 2", "pending")).unwrap();
            store.commit("Second commit").unwrap();

            let log = store.log().unwrap();
            // Should have at least our 2 commits + initial commit from init()
            assert!(log.len() >= 2);
        }

        #[test]
        fn test_versioned_update_and_delete() {
            let dir = tempfile::tempdir().unwrap();
            init_git_repo(dir.path());

            let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

            let item = make_item("bd-v001", "Task", "pending");
            store.insert(&item).unwrap();
            store.commit("Add task").unwrap();

            // Update
            let mut updated = item.clone();
            updated.status = "in_progress".to_string();
            store.update(&updated).unwrap();
            store.commit("Update status").unwrap();

            let retrieved = store.get("bd-v001").unwrap();
            assert_eq!(retrieved.status, "in_progress");

            // Delete
            store.delete("bd-v001").unwrap();
            store.commit("Delete task").unwrap();

            assert!(store.get("bd-v001").is_none());
        }

        #[test]
        fn test_versioned_status() {
            let dir = tempfile::tempdir().unwrap();
            init_git_repo(dir.path());

            let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

            store.insert(&make_item("bd-v001", "Task 1", "pending")).unwrap();
            store.insert(&make_item("bd-v002", "Task 2", "pending")).unwrap();

            let status = store.status();
            // Should have 2 staged additions
            assert_eq!(status.len(), 2);
            assert!(status.iter().all(|(_, s)| s == "added"));
        }
    }

    #[test]
    fn test_size_and_depth() {
        let mut store = in_memory_store();
        assert_eq!(store.size(), 0);

        for i in 0..50 {
            store.insert_work_item(&make_item(
                &format!("bd-{i:04}"),
                &format!("Task {i}"),
                "pending",
            ));
        }

        assert_eq!(store.size(), 50);
        // Tree should have some depth with 50 items
        assert!(store.tree.depth() >= 1);
    }
}
