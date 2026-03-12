# Glossary

Core terminology used across the OpenGoose project and related paradigms.

### Goose & Internal Concepts
- **Goose**: The core agentic engine developed by Block. OpenGoose is a native orchestrator on top of it.
- **GooseMode**: Global behavior modes (Auto, Approve, SmartApprove, Chat) determining tool permissions.
- **MCP (Model Context Protocol)**: The standard for agents to interact with tools and resources.
- **Subagent**: An independent agent instance created by a parent agent to perform a specific sub-task.
- **Recipe**: A reusable configuration for an agent, including instructions, tools, and retry logic.

### Gastown & Orchestration
- **Gastown**: A multi-agent orchestration paradigm for running 20-30 agents in parallel.
- **Polecat**: A short-lived, grunt worker agent assigned to a single task and disposed of after.
- **Landing the Plane**: The protocol for merging parallel work back into the main branch, often involving "re-imagining" conflicts.
- **Mayor**: A human-concierge role that orchestrates but never writes code.
- **Witness**: A monitoring role that ensures agents are not stuck or becoming "zombies."
- **Deacon**: A background daemon for continuous maintenance and autonomous tasks.

### Beads & Storage
- **Beads**: A dependency-aware task management system. An atomic unit of work.
- **Wisp**: A lightweight, ephemeral bead used for transient communication or sub-tasks.
- **Ready**: A state for beads that have no pending dependencies and are eligible for execution.
- **Prime**: The process of generating an optimized context summary from beads for the AI agent.
- **Compact**: The process of summarizing completed beads to save context tokens.
- **Prolly Tree**: A Probabilistic B-tree combining B-tree efficiency with Merkle tree integrity.
- **Dolt**: A SQL database with Git-like versioning (branch, merge, etc.).
- **Molecule**: A structured unit of data or context, often used in repository-native knowledge bases.
- **Proto**: A prototype or early-stage definition of a role or task.
