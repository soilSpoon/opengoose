//! Prime snapshot — structured work item state for agent context injection.
//!
//! Delegates to [`ProllyBeadsStore`] via the `BeadsPrimeSource` trait.

use std::sync::Arc;

use opengoose_types::{BeadsPrimeSource, PrimeSnapshot};

use crate::prolly::ProllyBeadsStore;

/// Prime snapshot generator backed by a prollytree.
pub struct PrimeStore {
    store: Arc<ProllyBeadsStore>,
}

impl PrimeStore {
    pub fn new(store: Arc<ProllyBeadsStore>) -> Self {
        Self { store }
    }

    /// Generate a markdown-formatted prime snapshot.
    pub fn prime(&self, team_run_id: &str, agent_name: &str) -> String {
        let snap = self
            .store
            .prime_snapshot(team_run_id, agent_name)
            .unwrap_or_default();
        format_snapshot(&snap)
    }

    /// Generate a structured prime snapshot.
    pub fn snapshot(&self, team_run_id: &str, agent_name: &str) -> PrimeSnapshot {
        self.store
            .prime_snapshot(team_run_id, agent_name)
            .unwrap_or_default()
    }
}

fn format_snapshot(snap: &PrimeSnapshot) -> String {
    let mut out = String::new();

    if !snap.active.is_empty() {
        out.push_str("# Active Tasks (assigned to you)\n");
        for item in &snap.active {
            out.push_str(&format!("- [{}] {} ({})\n", item.hash_id, item.title, item.status));
        }
        out.push('\n');
    }

    if !snap.ready.is_empty() {
        out.push_str("# Ready Tasks (available)\n");
        for item in &snap.ready {
            out.push_str(&format!(
                "- [{}] {} (pending, priority: {})\n",
                item.hash_id, item.title, item.priority
            ));
        }
        out.push('\n');
    }

    if !snap.recently_completed.is_empty() {
        out.push_str("# Recently Completed\n");
        for item in &snap.recently_completed {
            let ts = item.updated_at.as_deref().unwrap_or("—");
            out.push_str(&format!(
                "- [{}] {} (completed, {})\n",
                item.hash_id, item.title, ts
            ));
        }
        out.push('\n');
    }

    if !snap.blocked.is_empty() {
        out.push_str("# Blocked\n");
        for (item, blockers) in &snap.blocked {
            out.push_str(&format!(
                "- [{}] {} (blocked by: {})\n",
                item.hash_id,
                item.title,
                blockers.join(", ")
            ));
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (Arc<ProllyBeadsStore>, PrimeStore) {
        let store = Arc::new(ProllyBeadsStore::in_memory());
        let prime = PrimeStore::new(store.clone());
        (store, prime)
    }

    #[test]
    fn test_prime_empty() {
        let (_store, prime) = test_store();
        let snap = prime.snapshot("run1", "agent");
        assert!(snap.active.is_empty());
        assert!(snap.ready.is_empty());
    }

    #[test]
    fn test_prime_with_items() {
        let (store, prime) = test_store();
        let active = store.create("s", "run1", "Active task", None);
        store.assign(&active, "coder", None);
        store.create("s", "run1", "Ready task", None);
        let done = store.create("s", "run1", "Done task", None);
        store.set_output(&done, "result");

        let snap = prime.snapshot("run1", "coder");
        assert_eq!(snap.active.len(), 1);
        assert_eq!(snap.ready.len(), 1);
        assert_eq!(snap.recently_completed.len(), 1);
    }

    #[test]
    fn test_prime_blocked() {
        let (store, prime) = test_store();
        let blocker = store.create("s", "run1", "Blocker", None);
        store.update_status(&blocker, "in_progress");
        let blocked = store.create("s", "run1", "Blocked", None);
        store.insert_relationship(&blocked, &blocker, "blocks");

        let snap = prime.snapshot("run1", "agent");
        assert_eq!(snap.blocked.len(), 1);
        assert_eq!(snap.blocked[0].0.title, "Blocked");
    }

    #[test]
    fn test_prime_markdown() {
        let (store, prime) = test_store();
        store.create("s", "run1", "Ready A", None);
        let md = prime.prime("run1", "agent");
        assert!(md.contains("# Ready Tasks"));
        assert!(md.contains("Ready A"));
    }
}
