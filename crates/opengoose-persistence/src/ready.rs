//! Ready query — finds unblocked, pending work items.
//!
//! Delegates to [`ProllyBeadsStore`] via the `BeadsRead` trait.

use std::sync::Arc;

use opengoose_types::{BeadsRead, BeadsReadyOptions};

use crate::prolly::ProllyBeadsStore;
use crate::work_items::WorkItem;

/// Options for the ready query.
pub struct ReadyOptions {
    pub batch_size: usize,
    pub include_assigned: bool,
}

impl Default for ReadyOptions {
    fn default() -> Self {
        Self {
            batch_size: 10,
            include_assigned: false,
        }
    }
}

/// Ready query backed by a prollytree.
pub struct ReadyStore {
    store: Arc<ProllyBeadsStore>,
}

impl ReadyStore {
    pub fn new(store: Arc<ProllyBeadsStore>) -> Self {
        Self { store }
    }

    /// Return work items that are ready to be picked up.
    pub fn ready(
        &self,
        team_run_id: &str,
        options: &ReadyOptions,
    ) -> Vec<WorkItem> {
        let opts = BeadsReadyOptions {
            team_run_id: team_run_id.to_string(),
            batch_size: options.batch_size,
            include_assigned: options.include_assigned,
        };
        self.store
            .ready(&opts)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|bead| self.store.get(&bead.hash_id))
            .map(WorkItem::from_prolly)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (Arc<ProllyBeadsStore>, ReadyStore) {
        let store = Arc::new(ProllyBeadsStore::in_memory());
        let ready = ReadyStore::new(store.clone());
        (store, ready)
    }

    #[test]
    fn test_ready_basic() {
        let (store, ready) = test_store();
        store.create("s", "run1", "Task A", None);
        store.create("s", "run1", "Task B", None);

        let items = ready.ready("run1", &ReadyOptions::default());
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_ready_excludes_in_progress() {
        let (store, ready) = test_store();
        let id = store.create("s", "run1", "A", None);
        store.update_status(&id, "in_progress");
        store.create("s", "run1", "B", None);

        let items = ready.ready("run1", &ReadyOptions::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "B");
    }

    #[test]
    fn test_ready_excludes_blocked() {
        let (store, ready) = test_store();
        let blocker = store.create("s", "run1", "Blocker", None);
        store.update_status(&blocker, "in_progress");
        let blocked = store.create("s", "run1", "Blocked", None);
        store.insert_relationship(&blocked, &blocker, "blocks");

        let items = ready.ready("run1", &ReadyOptions::default());
        assert!(items.iter().all(|i| i.title != "Blocked"));
    }

    #[test]
    fn test_ready_excludes_ephemeral() {
        let (store, ready) = test_store();
        store.create_wisp("s", "run1", "Wisp", "agent");
        store.create("s", "run1", "Normal", None);

        let items = ready.ready("run1", &ReadyOptions::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Normal");
    }

    #[test]
    fn test_ready_batch_size() {
        let (store, ready) = test_store();
        for i in 0..5 {
            store.create("s", "run1", &format!("Task {i}"), None);
        }

        let items = ready.ready(
            "run1",
            &ReadyOptions {
                batch_size: 3,
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_ready_run_scoping() {
        let (store, ready) = test_store();
        store.create("s", "run1", "A", None);
        store.create("s", "run2", "B", None);

        let items = ready.ready("run1", &ReadyOptions::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "A");
    }

    #[test]
    fn test_ready_unblocks_when_blocker_completes() {
        let (store, ready) = test_store();
        let blocker = store.create("s", "run1", "Blocker", None);
        let blocked = store.create("s", "run1", "Blocked", None);
        store.insert_relationship(&blocked, &blocker, "blocks");

        let items = ready.ready("run1", &ReadyOptions::default());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Blocker");

        store.set_output(&blocker, "done");
        let items = ready.ready("run1", &ReadyOptions::default());
        assert!(items.iter().any(|i| i.title == "Blocked"));
    }
}
