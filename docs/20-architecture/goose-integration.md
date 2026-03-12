# Goose Integration

This document outlines how Goose-native features are integrated into the OpenGoose v2 architecture.

## 1. Core Integration Points

Goose provides the following core capabilities which OpenGoose leverages:
- **Agent Execution**: `Agent::reply()` for interacting with LLMs.
- **Session Management**: Independent sessions for each agent in a team.
- **MCP Dispatch**: Standardized tool access through Model Context Protocol.
- **Permission Management**: Granular control via `GooseMode`.

## 2. Shared vs. Built Features

| Goose-native (Reused) | OpenGoose (Built) |
|-----------------------|-------------------|
| Subagent System | Team Orchestration (Chain/FanOut) |
| Permission Manager | Witness Monitor (Stuck/Zombie detection) |
| Context Management | Multi-channel Adapters (Discord/Slack) |
| Extension Manager | Agent Map Visualization |

## 3. Communication Bridge
While Goose agents are typically isolated, OpenGoose bridges them using specialized MCP Team Tools. This allows agents to:
- Delegate to other team members.
- Broadcast findings to a shared team wall.
- Synchronize state through the persistent storage layer.

---

*Extracted from v2-master.md section 1.2*
