//! Core ProllyTree-backed work item store.

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
pub(crate) fn work_item_key(hash_id: &str) -> Vec<u8> {
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
    pub(crate) tree: ProllyTree<N, S>,
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

        assert_ne!(hash_before, hash_after);
    }

    #[test]
    fn test_same_data_same_hash() {
        let mut store1 = in_memory_store();
        let mut store2 = in_memory_store();

        let item = make_item("bd-001", "Task A", "pending");
        store1.insert_work_item(&item);
        store2.insert_work_item(&item);

        assert_eq!(store1.root_hash(), store2.root_hash());
    }

    #[test]
    fn test_diff_between_stores() {
        let mut store1 = in_memory_store();
        let mut store2 = in_memory_store();

        store1.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        store2.insert_work_item(&make_item("bd-001", "Task A", "pending"));
        store2.insert_work_item(&make_item("bd-002", "Task B", "pending"));
        store1.insert_work_item(&make_item("bd-003", "Task C", "pending"));

        let diff = store1.diff(&store2);
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

        store.insert_relationship("bd-002", "bd-001", "blocks");
        assert!(store.is_blocked("bd-002"));

        let mut blocker = store.get_work_item("bd-001").unwrap();
        blocker.status = "completed".to_string();
        store.update_work_item(&blocker);

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
                assert_eq!(item.status, "completed");
            }
            _ => panic!("Expected Modified result"),
        }
    }

    #[test]
    fn test_file_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();

        {
            let mut store = file_store(path.clone());
            store.insert_work_item(&make_item("bd-001", "Persistent Task", "pending"));
            store.tree.persist_root();
            store.tree.save_config().unwrap();
        }

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

        let item50 = store.get_work_item("bd-0050").unwrap();
        assert_eq!(item50.title, "Task 50");
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
        assert!(store.tree.depth() >= 1);
    }
}
