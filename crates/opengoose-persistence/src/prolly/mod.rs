//! ProllyTree-backed Beads store.
//!
//! Provides [`ProllyBeadsStore`] which wraps a prollytree and implements the
//! Beads traits from `opengoose-types` (`BeadsRead`, `BeadsPrimeSource`,
//! `BeadsMaintenance`).

pub mod hash_id;

use std::sync::Mutex;

use prollytree::config::TreeConfig;
use prollytree::storage::{InMemoryNodeStorage, NodeStorage};
use prollytree::tree::{ProllyTree, Tree};
use serde::{Deserialize, Serialize};
use tracing::debug;

use opengoose_types::{
    BeadItem, BeadsMaintenance, BeadsPrimeSource, BeadsRead, BeadsReadyOptions, PrimeSectionItem,
    PrimeSnapshot,
};

pub use hash_id::generate_hash_id;

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

/// Work item stored in the prolly tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProllyWorkItem {
    pub hash_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub assigned_to: Option<String>,
    pub parent_hash_id: Option<String>,
    pub is_ephemeral: bool,
    pub team_run_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ProllyWorkItem {
    fn to_bead_item(&self) -> BeadItem {
        BeadItem {
            hash_id: self.hash_id.clone(),
            title: self.title.clone(),
            status: self.status.clone(),
            priority: self.priority,
            assigned_to: self.assigned_to.clone(),
            parent_hash_id: self.parent_hash_id.clone(),
            is_ephemeral: self.is_ephemeral,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

/// ProllyTree-backed Beads store implementing the Beads traits.
///
/// Thread-safe via internal `Mutex`. Uses in-memory storage by default;
/// can be extended to file-backed storage.
pub struct ProllyBeadsStore<const N: usize = 32, S: NodeStorage<N> = InMemoryNodeStorage<32>> {
    tree: Mutex<ProllyTree<N, S>>,
}

impl ProllyBeadsStore<32, InMemoryNodeStorage<32>> {
    /// Create a new in-memory store.
    pub fn in_memory() -> Self {
        let storage = InMemoryNodeStorage::<32>::default();
        let config = TreeConfig::<32>::default();
        Self {
            tree: Mutex::new(ProllyTree::new(storage, config)),
        }
    }
}

impl<const N: usize, S: NodeStorage<N>> ProllyBeadsStore<N, S> {
    /// Create a new store with the given storage backend.
    pub fn new(storage: S) -> Self {
        let config = TreeConfig::<N>::default();
        Self {
            tree: Mutex::new(ProllyTree::new(storage, config)),
        }
    }

    /// Insert a work item.
    pub fn insert(&self, item: &ProllyWorkItem) {
        let key = work_item_key(&item.hash_id);
        let value = serde_json::to_vec(item).expect("ProllyWorkItem serialization cannot fail");
        let mut tree = self.tree.lock().unwrap();
        tree.insert(key, value);
        debug!(hash_id = %item.hash_id, title = %item.title, "prolly beads: inserted");
    }

    /// Get a work item by hash_id.
    pub fn get(&self, hash_id: &str) -> Option<ProllyWorkItem> {
        let key = work_item_key(hash_id);
        let tree = self.tree.lock().unwrap();
        let node = tree.find(&key)?;
        let idx = node.keys.iter().position(|k| k == &key)?;
        serde_json::from_slice(&node.values[idx]).ok()
    }

    /// Update a work item. Returns true if it existed.
    pub fn update(&self, item: &ProllyWorkItem) -> bool {
        let key = work_item_key(&item.hash_id);
        let value = serde_json::to_vec(item).expect("ProllyWorkItem serialization cannot fail");
        let mut tree = self.tree.lock().unwrap();
        tree.update(key, value)
    }

    /// Delete a work item by hash_id. Returns true if it existed.
    pub fn delete(&self, hash_id: &str) -> bool {
        let key = work_item_key(hash_id);
        let mut tree = self.tree.lock().unwrap();
        tree.delete(&key)
    }

    /// List all work items.
    fn list_all(&self) -> Vec<ProllyWorkItem> {
        let tree = self.tree.lock().unwrap();
        let keys = tree.collect_keys();
        let mut seen = std::collections::HashSet::new();
        keys.iter()
            .filter(|k| k.starts_with(WORK_ITEM_PREFIX))
            .filter(|k| seen.insert((*k).clone()))
            .filter_map(|key| {
                let node = tree.find(key)?;
                let idx = node.keys.iter().position(|k| k == key)?;
                serde_json::from_slice(&node.values[idx]).ok()
            })
            .collect()
    }

    /// Insert a relationship between two work items.
    pub fn insert_relationship(&self, from_id: &str, to_id: &str, kind: &str) {
        let mut key = REL_PREFIX.to_vec();
        key.extend_from_slice(from_id.as_bytes());
        key.push(b':');
        key.extend_from_slice(to_id.as_bytes());
        let mut tree = self.tree.lock().unwrap();
        tree.insert(key, kind.as_bytes().to_vec());
    }

    /// Get blockers for a work item (items that block it via "blocks" relation).
    fn get_blockers(&self, hash_id: &str) -> Vec<String> {
        let tree = self.tree.lock().unwrap();
        let prefix = format!("rel:{hash_id}:");
        let keys = tree.collect_keys();
        keys.iter()
            .filter(|k| k.starts_with(prefix.as_bytes()))
            .filter_map(|key| {
                let node = tree.find(key)?;
                let idx = node.keys.iter().position(|k| k == key)?;
                let kind = std::str::from_utf8(&node.values[idx]).ok()?;
                if kind == "blocks" {
                    let key_str = std::str::from_utf8(key).ok()?;
                    let blocker_id = key_str.strip_prefix(&prefix)?;
                    // Check if blocker is not completed
                    let blocker_key = work_item_key(blocker_id);
                    let blocker_node = tree.find(&blocker_key)?;
                    let blocker_idx = blocker_node.keys.iter().position(|k| k == &blocker_key)?;
                    let blocker: ProllyWorkItem =
                        serde_json::from_slice(&blocker_node.values[blocker_idx]).ok()?;
                    if blocker.status != "completed" && blocker.status != "cancelled" {
                        return Some(blocker.hash_id);
                    }
                }
                None
            })
            .collect()
    }

    /// Total number of items.
    pub fn size(&self) -> usize {
        let tree = self.tree.lock().unwrap();
        tree.size()
    }
}

impl<const N: usize, S: NodeStorage<N> + Send + Sync> BeadsRead for ProllyBeadsStore<N, S> {
    fn ready(&self, opts: &BeadsReadyOptions) -> anyhow::Result<Vec<BeadItem>> {
        let all = self.list_all();
        let items: Vec<BeadItem> = all
            .iter()
            .filter(|item| item.team_run_id == opts.team_run_id)
            .filter(|item| !item.is_ephemeral)
            .filter(|item| item.status == "pending")
            .filter(|item| opts.include_assigned || item.assigned_to.is_none())
            .filter(|item| self.get_blockers(&item.hash_id).is_empty())
            .take(opts.batch_size)
            .map(|item| item.to_bead_item())
            .collect();
        Ok(items)
    }
}

impl<const N: usize, S: NodeStorage<N> + Send + Sync> BeadsPrimeSource
    for ProllyBeadsStore<N, S>
{
    fn prime_snapshot(
        &self,
        team_run_id: &str,
        agent_name: &str,
    ) -> anyhow::Result<PrimeSnapshot> {
        let all = self.list_all();
        let run_items: Vec<_> = all
            .iter()
            .filter(|item| item.team_run_id == team_run_id)
            .filter(|item| !item.is_ephemeral)
            .collect();

        let active: Vec<PrimeSectionItem> = run_items
            .iter()
            .filter(|item| item.status == "in_progress" && item.assigned_to.as_deref() == Some(agent_name))
            .map(|item| PrimeSectionItem {
                hash_id: item.hash_id.clone(),
                title: item.title.clone(),
                status: item.status.clone(),
                priority: item.priority,
                updated_at: Some(item.updated_at.clone()),
            })
            .collect();

        let ready: Vec<PrimeSectionItem> = run_items
            .iter()
            .filter(|item| item.status == "pending" && item.assigned_to.is_none())
            .filter(|item| self.get_blockers(&item.hash_id).is_empty())
            .take(10)
            .map(|item| PrimeSectionItem {
                hash_id: item.hash_id.clone(),
                title: item.title.clone(),
                status: item.status.clone(),
                priority: item.priority,
                updated_at: Some(item.updated_at.clone()),
            })
            .collect();

        let mut recently_completed: Vec<PrimeSectionItem> = run_items
            .iter()
            .filter(|item| item.status == "completed")
            .map(|item| PrimeSectionItem {
                hash_id: item.hash_id.clone(),
                title: item.title.clone(),
                status: item.status.clone(),
                priority: item.priority,
                updated_at: Some(item.updated_at.clone()),
            })
            .collect();
        recently_completed.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        recently_completed.truncate(5);

        let blocked: Vec<(PrimeSectionItem, Vec<String>)> = run_items
            .iter()
            .filter(|item| item.status == "pending")
            .filter_map(|item| {
                let blockers = self.get_blockers(&item.hash_id);
                if blockers.is_empty() {
                    None
                } else {
                    Some((
                        PrimeSectionItem {
                            hash_id: item.hash_id.clone(),
                            title: item.title.clone(),
                            status: item.status.clone(),
                            priority: item.priority,
                            updated_at: Some(item.updated_at.clone()),
                        },
                        blockers,
                    ))
                }
            })
            .collect();

        Ok(PrimeSnapshot {
            active,
            ready,
            recently_completed,
            blocked,
        })
    }
}

impl<const N: usize, S: NodeStorage<N> + Send + Sync> BeadsMaintenance
    for ProllyBeadsStore<N, S>
{
    fn compact(&self, team_run_id: &str, _older_than_secs: u64) -> anyhow::Result<usize> {
        let all = self.list_all();
        let mut count = 0;
        for item in &all {
            if item.team_run_id == team_run_id
                && item.status == "completed"
                && !item.is_ephemeral
            {
                let mut updated = item.clone();
                updated.status = "compacted".to_string();
                self.update(&updated);
                count += 1;
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(hash_id: &str, title: &str, status: &str) -> ProllyWorkItem {
        ProllyWorkItem {
            hash_id: hash_id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            priority: 3,
            assigned_to: None,
            parent_hash_id: None,
            is_ephemeral: false,
            team_run_id: "run1".to_string(),
            created_at: "2026-03-13T00:00:00Z".to_string(),
            updated_at: "2026-03-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_insert_and_get() {
        let store = ProllyBeadsStore::in_memory();
        let item = make_item("bd-abc1", "Fix bug", "pending");
        store.insert(&item);

        let retrieved = store.get("bd-abc1").unwrap();
        assert_eq!(retrieved.title, "Fix bug");
    }

    #[test]
    fn test_update_and_delete() {
        let store = ProllyBeadsStore::in_memory();
        let mut item = make_item("bd-abc1", "Fix bug", "pending");
        store.insert(&item);

        item.status = "in_progress".to_string();
        assert!(store.update(&item));
        assert_eq!(store.get("bd-abc1").unwrap().status, "in_progress");

        assert!(store.delete("bd-abc1"));
        assert!(store.get("bd-abc1").is_none());
    }

    #[test]
    fn test_ready_basic() {
        let store = ProllyBeadsStore::in_memory();
        store.insert(&make_item("bd-001", "Task A", "pending"));
        store.insert(&make_item("bd-002", "Task B", "in_progress"));

        let opts = BeadsReadyOptions {
            team_run_id: "run1".to_string(),
            ..Default::default()
        };
        let items = store.ready(&opts).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].hash_id, "bd-001");
    }

    #[test]
    fn test_ready_excludes_blocked() {
        let store = ProllyBeadsStore::in_memory();
        store.insert(&make_item("bd-001", "Blocker", "in_progress"));
        store.insert(&make_item("bd-002", "Blocked", "pending"));
        store.insert_relationship("bd-002", "bd-001", "blocks");

        let opts = BeadsReadyOptions {
            team_run_id: "run1".to_string(),
            ..Default::default()
        };
        let items = store.ready(&opts).unwrap();
        assert!(items.iter().all(|i| i.hash_id != "bd-002"));
    }

    #[test]
    fn test_ready_excludes_ephemeral() {
        let store = ProllyBeadsStore::in_memory();
        let mut wisp = make_item("bd-001", "Wisp", "pending");
        wisp.is_ephemeral = true;
        store.insert(&wisp);

        let opts = BeadsReadyOptions {
            team_run_id: "run1".to_string(),
            ..Default::default()
        };
        let items = store.ready(&opts).unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_prime_snapshot() {
        let store = ProllyBeadsStore::in_memory();
        let mut active = make_item("bd-001", "Active task", "in_progress");
        active.assigned_to = Some("coder".to_string());
        store.insert(&active);
        store.insert(&make_item("bd-002", "Ready task", "pending"));
        store.insert(&make_item("bd-003", "Done task", "completed"));

        let snap = store.prime_snapshot("run1", "coder").unwrap();
        assert_eq!(snap.active.len(), 1);
        assert_eq!(snap.active[0].title, "Active task");
        assert_eq!(snap.ready.len(), 1);
        assert_eq!(snap.recently_completed.len(), 1);
    }

    #[test]
    fn test_prime_snapshot_blocked() {
        let store = ProllyBeadsStore::in_memory();
        store.insert(&make_item("bd-001", "Blocker", "in_progress"));
        store.insert(&make_item("bd-002", "Blocked", "pending"));
        store.insert_relationship("bd-002", "bd-001", "blocks");

        let snap = store.prime_snapshot("run1", "agent").unwrap();
        assert_eq!(snap.blocked.len(), 1);
        assert_eq!(snap.blocked[0].0.hash_id, "bd-002");
        assert_eq!(snap.blocked[0].1, vec!["bd-001"]);
    }

    #[test]
    fn test_compact() {
        let store = ProllyBeadsStore::in_memory();
        store.insert(&make_item("bd-001", "Done A", "completed"));
        store.insert(&make_item("bd-002", "Done B", "completed"));
        store.insert(&make_item("bd-003", "Active", "in_progress"));

        let count = store.compact("run1", 0).unwrap();
        assert_eq!(count, 2);

        assert_eq!(store.get("bd-001").unwrap().status, "compacted");
        assert_eq!(store.get("bd-003").unwrap().status, "in_progress");
    }

    #[test]
    fn test_hash_id_generation() {
        let id = generate_hash_id("Fix auth bug", 1, 10);
        assert!(id.starts_with("bd-"));
        assert_eq!(id.len(), 7); // bd- + 4
    }
}
