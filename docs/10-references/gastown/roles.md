# Gastown Roles & Responsibilities

Gastown utilizes a hierarchical set of roles to manage agents and humans effectively.

## Leadership Roles

### Mayor 🎩
- **Responsibility**: Human concierge and orchestrator.
- **Rule**: Never writes code. Only directs, delegates, and coordinates.

### Witness
- **Responsibility**: Health monitoring.
- **Task**: Patrols agent status, detects "stuck" or "zombie" agents, and provides a "nudge."

### Deacon
- **Responsibility**: Autonomous maintenance.
- **Task**: Runs background daemon tasks, surveillance cycles, and continuous integration checks.

## Worker Roles

### Polecat 🦨
- **Responsibility**: Grunt worker.
- **Lifecycle**: Temporary session created for a single task. Disposed of after completion.

### Crew
- **Responsibility**: Human developer workspace.
- **Task**: Manual intervention or high-level architecture decisions.

### Refinery
- **Responsibility**: Merge queue manager.
- **Task**: Batch-then-bisect testing and resolving merge conflicts via "re-imagination."

## Support Roles

### Boot
- **Responsibility**: Watchdog for the Deacon.
- **Task**: Ensures the Deacon process is alive every 5 minutes.

### Dogs
- **Responsibility**: Maintenance helpers.
- **Task**: Handle compression, health checks, and archiving of old data.
