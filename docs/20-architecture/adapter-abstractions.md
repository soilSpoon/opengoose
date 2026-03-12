# Adapter Abstractions

OpenGoose supports a variety of channel platforms and storage backends through a unified adapter abstraction layer.

## Channel Adapters
Built-in adapters provide a consistent interface for:
- **Discord**
- **Slack**
- **Telegram**
- **Terminal (TUI)**

Custom platforms can be added via `Platform::Custom(String)` without modifying the core orchestrator.

## Storage Adapters
The storage layer abstraction allows OpenGoose to operate as a single binary while supporting different levels of data persistence and versioning.
- **Prolly Tree (Default)**: High-efficiency, versioned storage.
- **SQLite (Legacy/Fallback)**: Traditional relational storage.
- **InMemory**: Used for testing and transient sessions.

---

*Merged content from v2-architecture.md*
