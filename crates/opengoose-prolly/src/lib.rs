//! ProllyTree-backed storage for work items.
//!
//! This crate provides a content-addressed, version-aware storage layer
//! built on prollytree. It is isolated from the SQLite/Diesel persistence
//! layer to keep the heavy prollytree dependency tree (Arrow, gitoxide)
//! from propagating to crates that don't need it.
//!
//! Key properties:
//! - O(diff) time complexity between snapshots
//! - Structural sharing (branches share unchanged subtrees)
//! - Cryptographic proofs of data integrity

pub mod store;
pub mod versioned;

pub use store::{
    FileProllyStore, InMemoryProllyStore, ProllyStore, ProllyWorkItem, WorkItemStatusResolver,
    file_store, in_memory_store,
};
pub use versioned::VersionedWorkItemStore;
