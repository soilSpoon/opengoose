# Beads System

The Beads system is the backbone of task tracking in both Gastown and Goosetown. It replaces vague markdown plans with a dependency-aware task graph.

## Core Concepts

### 1. Atomic Work Units (Beads)
- Every task is a "Bead" with a unique hash-based ID (e.g., `bd-a1b2`).
- Hash IDs prevent merge conflicts in multi-agent environments.

### 2. State Machine
Beads move through a lifecycle:
- **Pending**: Created but not yet executable.
- **Ready**: No pending dependencies; available for an agent.
- **In Progress**: Claimed by an agent.
- **Completed**: Work finished and validated.

### 3. Context Management
- **Prime**: AI-optimized project context containing only the most relevant beads.
- **Compact**: Summarizes old, completed beads to save context window tokens.

### 4. Work Items Relationship
Beads are linked via dependency types:
- `blocks` / `depends_on`
- `duplicates`
- `supersedes`

## OpenGoose Implementation
OpenGoose implements the Beads algorithm using the `work_items` table, extending it with UUIDs and materialized paths for hierarchical tracking.
