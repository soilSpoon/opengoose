# Document Map

This map outlines the relationships between key documents in the OpenGoose documentation.

```mermaid
graph TD
    Index[INDEX.md] --> Meta[00-meta/]
    Index --> Refs[10-references/]
    Index --> Arch[20-architecture/]
    Index --> Impl[30-implementation/]
    Index --> Ops[40-operations/]

    Refs --> Goose[goose/README.md]
    Refs --> Gastown[gastown/README.md]
    Refs --> Storage[storage/README.md]

    Arch --> V2[v2-master.md]
    Arch --> StorageArch[storage.md]

    Impl --> Beads[beads-algorithm.md]
    Impl --> API[api-reference.md]

    Goose --> Subagents[subagent-system.md]
    Goose --> Permissions[permission-modes.md]

    Gastown --> BeadsSystem[beads-system.md]
    Gastown --> Roles[roles.md]
    Gastown --> GastownArchive[90-archive/gastown-full-research.md]

    Storage --> Prollytree[prollytree.md]
    Storage --> Dolt[dolt.md]
    Storage --> Comparison[comparison.md]
```

## Key Flows

1. **Architecture Flow**: `v2-master.md` defines the high-level goals, which are implemented via `beads-algorithm.md` and supported by `storage.md`.
2. **Goose Integration**: `goose-integration.md` (in architecture) links the core `goose/README.md` concepts to OpenGoose v2.
3. **Research to Implementation**: `gastown-full-research.md` provided the inspiration for the roles in `roles.md` and the system in `beads-system.md`.
