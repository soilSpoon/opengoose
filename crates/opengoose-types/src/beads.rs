//! Beads trait definitions and shared types.
//!
//! These traits define the contract for work item storage operations.
//! Implementations live in `opengoose-persistence`.

use serde::{Deserialize, Serialize};

/// Lightweight work item representation for trait boundaries.
///
/// This is the cross-crate type; persistence crates map to/from their
/// internal representations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeadItem {
    pub hash_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub assigned_to: Option<String>,
    pub parent_hash_id: Option<String>,
    pub is_ephemeral: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Options for the `ready()` query.
#[derive(Debug, Clone)]
pub struct BeadsReadyOptions {
    /// Maximum number of items to return.
    pub batch_size: usize,
    /// If true, include items already assigned to someone.
    pub include_assigned: bool,
    /// Scope to a specific team run.
    pub team_run_id: String,
}

impl Default for BeadsReadyOptions {
    fn default() -> Self {
        Self {
            batch_size: 10,
            include_assigned: false,
            team_run_id: String::new(),
        }
    }
}

/// A single item in a prime snapshot section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeSectionItem {
    pub hash_id: String,
    pub title: String,
    pub status: String,
    pub priority: i32,
    pub updated_at: Option<String>,
}

/// Structured snapshot of work item state for agent context injection.
///
/// Data only — formatting lives in `opengoose-core`.
#[derive(Debug, Clone, Default)]
pub struct PrimeSnapshot {
    pub active: Vec<PrimeSectionItem>,
    pub ready: Vec<PrimeSectionItem>,
    pub recently_completed: Vec<PrimeSectionItem>,
    /// Each entry is (blocked item, list of blocker hash_ids).
    pub blocked: Vec<(PrimeSectionItem, Vec<String>)>,
}

/// Read operations on the Beads work item store.
pub trait BeadsRead: Send + Sync {
    /// Return work items that are ready to be worked on.
    fn ready(&self, opts: &BeadsReadyOptions) -> anyhow::Result<Vec<BeadItem>>;
}

/// Context generation for agent system prompts.
pub trait BeadsPrimeSource: Send + Sync {
    /// Produce a structured snapshot of work item state for the given agent.
    fn prime_snapshot(&self, team_run_id: &str, agent_name: &str)
        -> anyhow::Result<PrimeSnapshot>;
}

/// Maintenance operations (compaction, cleanup).
pub trait BeadsMaintenance: Send + Sync {
    /// Compact completed work items older than `older_than_secs` seconds.
    /// Returns the number of items compacted.
    fn compact(&self, team_run_id: &str, older_than_secs: u64) -> anyhow::Result<usize>;
}
