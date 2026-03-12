# Storage Architecture Summary

OpenGoose uses a single-binary, pure-Rust storage architecture based on Prolly Trees to manage agent state, work items, and versioned data.

## Why Prolly Trees?

### 1. Structural Sharing
Prolly Trees allow for extreme efficiency when branching and versioning data. If you create 100 branches of the database, only the actual changes are stored.

### 2. O(diff) Complexity
Comparing two versions of the database takes time proportional to the number of changes, not the total size of the database. This is critical for fast agent context generation.

### 3. Content-Addressed
Identical data results in the same hash, providing automatic deduplication and data integrity verification.

## Technology Stack

- **Primary Engine**: `prollytree` crate (pure Rust implementation).
- **VCS Layer**: Built-in 3-way merge and conflict resolution strategies.
- **SQL Interface**: GlueSQL integration for structured queries.

---

*For detailed comparisons, see [Storage Comparison](comparison.md).*
