//! Git-versioned work item store with branch/commit/merge support.

use std::path::Path;

use prollytree::git::types::GitKvError;
use prollytree::git::versioned_store::InMemoryVersionedKvStore;

use crate::store::{ProllyWorkItem, work_item_key};

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
        let value = serde_json::to_vec(item)
            .map_err(|e| GitKvError::GitObjectError(format!("serialization: {e}")))?;
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
        let value = serde_json::to_vec(item)
            .map_err(|e| GitKvError::GitObjectError(format!("serialization: {e}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ProllyWorkItem;
    use std::process::Command;

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

        let retrieved = store.get("bd-v001").unwrap();
        assert_eq!(retrieved.title, "Versioned task");

        let commit_id = store.commit("Add first task").unwrap();
        assert!(!commit_id.is_empty());

        let retrieved = store.get("bd-v001").unwrap();
        assert_eq!(retrieved.title, "Versioned task");
    }

    #[test]
    fn test_versioned_branch_and_commit() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());

        let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

        let item = make_item("bd-v001", "Main task", "pending");
        store.insert(&item).unwrap();
        store.commit("Add main task").unwrap();

        store.create_branch("feature/new-work").unwrap();
        assert_eq!(store.current_branch(), "feature/new-work");

        let item2 = make_item("bd-v002", "Feature task", "pending");
        store.insert(&item2).unwrap();
        store.commit("Add feature task").unwrap();

        assert!(store.get("bd-v001").is_some());
        assert!(store.get("bd-v002").is_some());

        let branches = store.list_branches().unwrap();
        assert!(branches.contains(&"main".to_string()));
        assert!(branches.contains(&"feature/new-work".to_string()));
    }

    #[test]
    fn test_versioned_commit_log() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());

        let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

        store
            .insert(&make_item("bd-v001", "Task 1", "pending"))
            .unwrap();
        store.commit("First commit").unwrap();

        store
            .insert(&make_item("bd-v002", "Task 2", "pending"))
            .unwrap();
        store.commit("Second commit").unwrap();

        let log = store.log().unwrap();
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

        let mut updated = item.clone();
        updated.status = "in_progress".to_string();
        store.update(&updated).unwrap();
        store.commit("Update status").unwrap();

        let retrieved = store.get("bd-v001").unwrap();
        assert_eq!(retrieved.status, "in_progress");

        store.delete("bd-v001").unwrap();
        store.commit("Delete task").unwrap();

        assert!(store.get("bd-v001").is_none());
    }

    #[test]
    fn test_versioned_status() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());

        let mut store = VersionedWorkItemStore::init(dir.path()).unwrap();

        store
            .insert(&make_item("bd-v001", "Task 1", "pending"))
            .unwrap();
        store
            .insert(&make_item("bd-v002", "Task 2", "pending"))
            .unwrap();

        let status = store.status();
        assert_eq!(status.len(), 2);
        assert!(status.iter().all(|(_, s)| s == "added"));
    }
}
