# Goose Engine Reference

Goose is the core agentic engine that OpenGoose orchestrates. This document provides a summary of its key internal systems.

## Key Concepts

### 1. Subagent System
Goose allows a main agent to delegate sub-tasks to independent agent instances.
- **TaskConfig**: Encapsulates dependencies like LLM provider, parent session ID, and extensions.
- **SubagentRunParams**: Collects all execution parameters, including cancellation tokens and message callbacks.
- **Cancellation**: Uses `tokio_util::sync::CancellationToken` for cooperative cancellation between parent and subagent.

### 2. Permission Modes (GooseMode)
Determines how tools are authorized during tool calls:
- **Auto**: Automatically allow all tools.
- **Approve**: Require user approval for every tool call.
- **SmartApprove**: Automatically allow read-only tools; require approval for write tools.
- **Chat**: Disable all tool usage (conversation only).

### 3. MCP Dispatch
Goose uses the Model Context Protocol (MCP) to manage tool extensions.
- **ExtensionManager**: Handles loading, caching, and dispatching tool calls to MCP clients.
- **Tool Prefixes**: Tools are typically prefixed by their extension name (e.g., `developer__shell`) unless explicitly marked as unprefixed.

### 4. Context Management
Goose manages the LLM context window through a series of processors:
- **fix_conversation**: A 7-step pipeline to normalize messages before sending to the LLM.
- **Compaction**: Automatically summarizes the conversation when it exceeds a threshold (default 80% of context limit).
- **Progressive Tool Removal**: If context is still too large, it removes tool responses starting from the middle of the conversation.

## OpenGoose Integration
OpenGoose leverages these Goose-native features and extends them for multi-agent teams.
- **Reuse**: Agent, Session, Recipe, MCP, Permission, and Context Management.
- **Build**: Witness, Team Tools, Agent Map, and Git worktree isolation.

---

*For detailed analysis, see the [Goose Deep Dive](../../architecture/goose-deep-dive.md).*
