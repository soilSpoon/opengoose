//! Compact algorithm — marks old completed items as compacted.
//!
//! Delegates to [`ProllyBeadsStore`] via the `BeadsMaintenance` trait.

use std::sync::Arc;

use opengoose_types::BeadsMaintenance;

use crate::prolly::ProllyBeadsStore;

/// Compact store backed by a prollytree.
pub struct CompactStore {
    store: Arc<ProllyBeadsStore>,
}

impl CompactStore {
    pub fn new(store: Arc<ProllyBeadsStore>) -> Self {
        Self { store }
    }

    /// Compact completed items older than `older_than_secs` seconds.
    /// Returns the number of items compacted.
    pub fn compact(&self, team_run_id: &str, older_than_secs: u64) -> usize {
        self.store
            .compact(team_run_id, older_than_secs)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (Arc<ProllyBeadsStore>, CompactStore) {
        let store = Arc::new(ProllyBeadsStore::in_memory());
        let compact = CompactStore::new(store.clone());
        (store, compact)
    }

    #[test]
    fn test_compact_completed() {
        let (store, compact) = test_store();
        let a = store.create("s", "run1", "Done A", None);
        let b = store.create("s", "run1", "Done B", None);
        store.create("s", "run1", "Active", None);
        store.set_output(&a, "result");
        store.set_output(&b, "result");

        let count = compact.compact("run1", 0);
        assert_eq!(count, 2);

        assert_eq!(store.get(&a).unwrap().status, "compacted");
        assert_eq!(store.get(&b).unwrap().status, "compacted");
    }

    #[test]
    fn test_compact_skips_non_completed() {
        let (store, compact) = test_store();
        store.create("s", "run1", "Pending", None);
        let count = compact.compact("run1", 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_compact_run_scoping() {
        let (store, compact) = test_store();
        let a = store.create("s", "run1", "A", None);
        let b = store.create("s", "run2", "B", None);
        store.set_output(&a, "done");
        store.set_output(&b, "done");

        let count = compact.compact("run1", 0);
        assert_eq!(count, 1);
        assert_eq!(store.get(&b).unwrap().status, "completed");
    }
}
