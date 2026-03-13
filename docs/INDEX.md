# OpenGoose Documentation Index

Welcome to the OpenGoose documentation. This guide provides topic-based navigation to help you understand the architecture, implementation, and research behind the project.

## Quick Navigation

### [Goose Internals](10-references/goose/README.md)
Goose is the core engine. Learn about the subagent system, MCP dispatch, GooseMode, and key APIs.
- **Primary Doc:** [Goose Deep Dive](10-references/goose/README.md)
- **Key Concepts:** Subagents, Permission Modes, Context Management.

### [Gastown & Multi-Agent Systems](10-references/gastown/README.md)
Explore the Gastown paradigm (Go 75k LOC) for orchestrating 20-30 parallel agents.
- **Primary Doc:** [Gastown Summary](10-references/gastown/README.md)
- **Key Concepts:** Polecat model, GUPP, Role-based orchestration, Mail system.
- **Note:** Gastown uses Dolt; OpenGoose reimplements patterns on prollytree.

### [Goosetown](10-references/goosetown/README.md)
Goose 프레임워크 기반 멀티에이전트 시스템. gtwall 브로드캐스트, Village Map 시각화.
- **Primary Doc:** [Goosetown Summary](10-references/goosetown/README.md)
- **Key Concepts:** gtwall, Skill System, Delegation >> Doing.

### [TinyClaw](10-references/tinyclaw/README.md)
경량 멀티에이전트 오케스트레이션. TinyOffice 대시보드 (정보 밀도 중심).
- **Primary Doc:** [TinyClaw Summary](10-references/tinyclaw/README.md)
- **Key Concepts:** TinyOffice (Office View), SQLite WAL Queue.
- **Note:** Gastown/Goosetown과 별개 프로젝트. Agent Map 설계의 정보 밀도 참조.

### [Wasteland (Federation)](10-references/wasteland/README.md)
분산 에이전트 연합, 평판 시스템, Trust Ladder.
- **Primary Doc:** [Wasteland Summary](10-references/wasteland/README.md)
- **Key Concepts:** Stamps, Trust Ladder, Yearbook Rule, HOP URI.

### [Beads & Task Management](30-implementation/beads-algorithm.md)
The Beads algorithm provides structured, dependency-aware task management for AI agents.
- **Primary Doc:** [Beads Algorithm](30-implementation/beads-algorithm.md)
- **Key Concepts:** Ready/Prime/Compact, Wisp, work_items.

### [Storage Architecture](20-architecture/storage.md)
prollytree 기반 스토리지 아키텍처. 순수 Rust 단일 바이너리.
- **Primary Doc:** [Storage Architecture](20-architecture/storage.md)
- **Key Concepts:** Prolly Trees, Structural Sharing, 3-way Merge, ConflictResolver.
- **Note:** SQLite → prollytree 전면 전환 진행 중. Dolt 미사용.

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
