# OpenGoose Documentation Index

Welcome to the OpenGoose documentation. This guide provides topic-based navigation to help you understand the architecture, implementation, and research behind the project.

## Quick Navigation

### [Goose Internals](10-references/goose/README.md)
Goose is the core engine. Learn about the subagent system, MCP dispatch, GooseMode, and key APIs.
- **Primary Doc:** [Goose Deep Dive](10-references/goose/README.md)
- **Key Concepts:** Subagents, Permission Modes, Context Management.

### [Gastown & Multi-Agent Systems](10-references/gastown/README.md)
Explore the Gastown paradigm for orchestrating 20-30 parallel agents.
- **Primary Doc:** [Gastown Summary](10-references/gastown/README.md)
- **Key Concepts:** Polecat model, Landing the Plane, Role-based orchestration.

### [Beads & Task Management](30-implementation/beads-algorithm.md)
The Beads algorithm provides structured, dependency-aware task management for AI agents.
- **Primary Doc:** [Beads Algorithm](30-implementation/beads-algorithm.md)
- **Key Concepts:** Ready/Prime/Compact, Wisp, work_items.

### [Storage Architecture](20-architecture/storage.md)
Details on why we chose Prolly Trees over SQLite or Dolt for our single-binary requirement.
- **Primary Doc:** [Storage Architecture](20-architecture/storage.md)
- **Key Concepts:** Prolly Trees, Structural Sharing, 3-way Merge.

### [OpenGoose v2 Architecture](20-architecture/v2-master.md)
The master blueprint for OpenGoose v2, aligning Goose-native features with Gastown principles.
- **Primary Doc:** [v2 Master Architecture](20-architecture/v2-master.md)

---

## Getting Started for Developers

1. **Understand the Core:** Read the [v2 Master Architecture](20-architecture/v2-master.md) to see how components fit together.
2. **Explore Goose:** Dive into [Goose References](10-references/goose/README.md) to understand the underlying engine.
3. **Task Management:** Learn how tasks are managed via the [Beads Algorithm](30-implementation/beads-algorithm.md).
4. **Codebase Overview:** Check the latest [Codebase Review](40-operations/codebase-review-2026-03.md) for current status and backlog.

---

## Quick Reference

| Goal | Primary Document |
|------|------------------|
| Understanding Subagents | [Subagent System](10-references/goose/subagent-system.md) |
| Permission & Security | [Permission Modes](10-references/goose/permission-modes.md) |
| Gastown Roles | [Roles & Responsibilities](10-references/gastown/roles.md) |
| Prolly Tree Details | [Prollytree Reference](10-references/storage/prollytree.md) |
| API Integration | [API Reference](30-implementation/api-reference.md) |
| Web Dashboard | [Web Dashboard](40-operations/web-dashboard.md) |
