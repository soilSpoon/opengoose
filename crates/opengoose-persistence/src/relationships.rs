//! Work item relationships backed by prollytree.
//!
//! Provides typed relations between work items (blocks, depends_on, etc.)

use std::sync::Arc;

use crate::db_enum::db_enum;
use crate::prolly::ProllyBeadsStore;

db_enum! {
    /// Type of relationship between work items.
    pub enum RelationType {
        Blocks => "blocks",
        DependsOn => "depends_on",
        RelatesTo => "relates_to",
        Duplicates => "duplicates",
    }
}

/// Relationship operations backed by a prollytree.
pub struct RelationStore {
    store: Arc<ProllyBeadsStore>,
}

impl RelationStore {
    pub fn new(store: Arc<ProllyBeadsStore>) -> Self {
        Self { store }
    }

    /// Add a relationship between two work items.
    pub fn add_relation(&self, from_hash_id: &str, to_hash_id: &str, rel_type: RelationType) {
        self.store
            .insert_relationship(from_hash_id, to_hash_id, rel_type.as_str());
    }

    /// Remove a relationship between two work items.
    pub fn remove_relation(&self, from_hash_id: &str, to_hash_id: &str, _rel_type: RelationType) {
        // Delete the relationship key from the tree
        let mut key = b"rel:".to_vec();
        key.extend_from_slice(from_hash_id.as_bytes());
        key.push(b':');
        key.extend_from_slice(to_hash_id.as_bytes());
        // ProllyBeadsStore doesn't expose direct key deletion for relationships yet,
        // so we insert an empty kind to effectively nullify it.
        self.store.insert_relationship(from_hash_id, to_hash_id, "");
    }

    /// Get blockers for a work item.
    pub fn get_blockers(&self, _hash_id: &str) -> Vec<String> {
        // Delegate to ProllyBeadsStore's internal method
        // For now, use the Beads trait implementation which already handles this
        self.store
            .list_for_run("", None)
            .into_iter()
            .filter(|_| false) // Placeholder - blockers are handled internally by ready/prime
            .map(|item| item.hash_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (Arc<ProllyBeadsStore>, RelationStore) {
        let store = Arc::new(ProllyBeadsStore::in_memory());
        let relations = RelationStore::new(store.clone());
        (store, relations)
    }

    #[test]
    fn test_add_relation() {
        let (store, relations) = test_store();
        let a = store.create("s", "run1", "Blocker", None);
        store.update_status(&a, "in_progress");
        let b = store.create("s", "run1", "Blocked", None);

        relations.add_relation(&b, &a, RelationType::Blocks);

        // Verify via ready — blocked item should not be ready
        use opengoose_types::{BeadsRead, BeadsReadyOptions};
        let opts = BeadsReadyOptions {
            team_run_id: "run1".to_string(),
            ..Default::default()
        };
        let ready = store.ready(&opts).unwrap();
        assert!(ready.iter().all(|i| i.hash_id != b));
    }

    #[test]
    fn test_relation_type_roundtrip() {
        for rt in [
            RelationType::Blocks,
            RelationType::DependsOn,
            RelationType::RelatesTo,
            RelationType::Duplicates,
        ] {
            assert_eq!(RelationType::parse(rt.as_str()).unwrap(), rt);
        }
    }
}
