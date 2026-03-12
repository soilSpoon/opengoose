# Prollytree Reference

Prolly Trees (Probabilistic B-trees) combine the search efficiency of B-trees with the integrity and structural sharing of Merkle trees.

## Core Features

- **History Independence**: The same set of data results in the same tree structure regardless of insertion order.
- **Efficient Diffing**: O(diff) performance for comparing database states.
- **Structural Sharing**: Changes create new nodes while pointing to unchanged existing nodes.
- **3-way Merge**: Built-in support for merging concurrent changes from different agents.

## Implementation in OpenGoose

OpenGoose utilizes the `prollytree` crate (v0.3.2-beta) with the `git` feature enabled.

### Key Operations
- `insert(key, value)`: Content-addressed storage.
- `branch(name)`: Zero-copy database branching.
- `commit(message)`: Atomic snapshot of the current state.
- `merge(target)`: Automated 3-way merge with configurable conflict resolvers.

### Conflict Resolution
OpenGoose supports several resolution strategies:
- `TimestampResolver`: Last write wins based on time.
- `AgentPriorityResolver`: Higher priority agent's change wins.
- `SemanticMergeResolver`: Intelligently merges JSON objects.
