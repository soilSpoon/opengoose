//! Work item persistence for orchestration and team execution flows.
//!
//! Provides [`WorkItemStore`] backed by a prollytree via [`ProllyBeadsStore`].

#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;

use tracing::debug;

use crate::prolly::ProllyBeadsStore;

pub use types::{WorkItem, WorkStatus};

/// Work item operations backed by a prollytree.
pub struct WorkItemStore {
    store: Arc<ProllyBeadsStore>,
}

impl WorkItemStore {
    pub fn new(store: Arc<ProllyBeadsStore>) -> Self {
        Self { store }
    }

    /// Create a new work item. Returns the generated hash_id.
    pub fn create(
        &self,
        session_key: &str,
        team_run_id: &str,
        title: &str,
        parent_hash_id: Option<&str>,
    ) -> String {
        let hash_id = self.store.create(session_key, team_run_id, title, parent_hash_id);
        debug!(hash_id = %hash_id, title, "work item created");
        hash_id
    }

    /// Create an ephemeral wisp. Returns the generated hash_id.
    pub fn create_wisp(
        &self,
        session_key: &str,
        team_run_id: &str,
        title: &str,
        agent: &str,
    ) -> String {
        let hash_id = self.store.create_wisp(session_key, team_run_id, title, agent);
        debug!(hash_id = %hash_id, title, agent, "wisp created");
        hash_id
    }

    /// Update the status of a work item.
    pub fn update_status(&self, hash_id: &str, status: WorkStatus) {
        self.store.update_status(hash_id, status.as_str());
    }

    /// Assign a work item to an agent at a specific workflow step.
    pub fn assign(&self, hash_id: &str, agent: &str, step: Option<i32>) {
        self.store.assign(hash_id, agent, step);
    }

    /// Set the input for a work item.
    pub fn set_input(&self, hash_id: &str, input: &str) {
        self.store.set_input(hash_id, input);
    }

    /// Set the output (result) for a work item and mark it completed.
    pub fn set_output(&self, hash_id: &str, output: &str) {
        self.store.set_output(hash_id, output);
    }

    /// Set the error message and mark the work item as failed.
    pub fn set_error(&self, hash_id: &str, error: &str) {
        self.store.set_error(hash_id, error);
    }

    /// Get a work item by hash_id.
    pub fn get(&self, hash_id: &str) -> Option<WorkItem> {
        self.store.get(hash_id).map(WorkItem::from_prolly)
    }

    /// List work items for a team run, optionally filtered by status.
    pub fn list_for_run(
        &self,
        team_run_id: &str,
        status: Option<&WorkStatus>,
    ) -> Vec<WorkItem> {
        self.store
            .list_for_run(team_run_id, status.map(|s| s.as_str()))
            .into_iter()
            .map(WorkItem::from_prolly)
            .collect()
    }

    /// Get children of a parent work item.
    pub fn get_children(&self, parent_hash_id: &str) -> Vec<WorkItem> {
        self.store
            .get_children(parent_hash_id)
            .into_iter()
            .map(WorkItem::from_prolly)
            .collect()
    }

    /// Find the resume point for a chain workflow: returns (next_step, last_output).
    pub fn find_resume_point(&self, parent_hash_id: &str) -> Option<(i32, String)> {
        self.store.find_resume_point(parent_hash_id)
    }

    /// Delete completed ephemeral wisps for a given team run.
    pub fn purge_ephemeral(&self, team_run_id: &str) -> usize {
        let count = self.store.purge_ephemeral(team_run_id);
        if count > 0 {
            debug!(count, team_run_id, "purged ephemeral wisps");
        }
        count
    }

    /// Access the underlying ProllyBeadsStore.
    pub fn inner(&self) -> &ProllyBeadsStore {
        &self.store
    }
}
