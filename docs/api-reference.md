# OpenGoose Web API Reference

The `opengoose-web` crate exposes a lightweight REST API for monitoring and
inspecting an OpenGoose runtime. All endpoints are read-only (GET). Responses
are JSON. Errors follow the standard [error format](#error-responses).

---

## Base URL

By default the server listens on **port 3000**. Set a different port via the
`--port` flag when starting the OpenGoose CLI.

```
http://localhost:3000
```

---

## Endpoints

### GET /api/health

Returns a simple liveness check. Use this to verify the server is running.

#### Response

```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

| Field     | Type   | Description                      |
|-----------|--------|----------------------------------|
| `status`  | string | Always `"ok"` when healthy       |
| `version` | string | Crate version of `opengoose-web` |

#### Example

```bash
curl http://localhost:3000/api/health
```

---

### GET /api/metrics

Returns detailed runtime counters: session, queue, and run statistics.

#### Response

```json
{
  "sessions": {
    "total": 12,
    "messages": 340
  },
  "queue": {
    "pending": 3,
    "processing": 1,
    "completed": 120,
    "failed": 2,
    "dead": 0
  },
  "runs": {
    "running": 2,
    "completed": 18,
    "failed": 1,
    "suspended": 0
  }
}
```

| Field               | Type   | Description                                       |
|---------------------|--------|---------------------------------------------------|
| `sessions.total`    | number | Total conversation sessions in the database       |
| `sessions.messages` | number | Total messages across all sessions                |
| `queue.pending`     | number | Messages waiting to be picked up                  |
| `queue.processing`  | number | Messages currently being processed                |
| `queue.completed`   | number | Successfully processed messages                   |
| `queue.failed`      | number | Messages that failed processing (retryable)       |
| `queue.dead`        | number | Messages that exhausted all retries               |
| `runs.running`      | number | Orchestration runs currently active               |
| `runs.completed`    | number | Successfully completed runs                       |
| `runs.failed`       | number | Runs that terminated with an error                |
| `runs.suspended`    | number | Runs paused and waiting to resume                 |

#### Example

```bash
curl http://localhost:3000/api/metrics
```

---

### GET /api/dashboard

Returns aggregate runtime statistics.

#### Response

```json
{
  "session_count": 12,
  "message_count": 340,
  "run_count": 8,
  "agent_count": 4,
  "team_count": 2
}
```

| Field           | Type   | Description                              |
|-----------------|--------|------------------------------------------|
| `session_count` | number | Total conversation sessions in the DB    |
| `message_count` | number | Total messages across all sessions       |
| `run_count`     | number | Total orchestration runs (all statuses)  |
| `agent_count`   | number | Number of configured agent profiles      |
| `team_count`    | number | Number of configured team workflows      |

#### Example

```bash
curl http://localhost:3000/api/dashboard
```

---

### GET /api/sessions

Returns a paginated list of conversation sessions.

#### Query Parameters

| Parameter | Type   | Default | Description                        |
|-----------|--------|---------|------------------------------------|
| `limit`   | number | `50`    | Maximum number of sessions to return |

#### Response

```json
[
  {
    "session_key": "discord:guild123:channel456",
    "active_team": "feature-dev",
    "created_at": "2026-03-10T12:00:00Z",
    "updated_at": "2026-03-10T12:34:00Z"
  }
]
```

| Field         | Type            | Description                                     |
|---------------|-----------------|-------------------------------------------------|
| `session_key` | string          | Stable platform session identifier              |
| `active_team` | string or null  | Name of the active team workflow, if any        |
| `created_at`  | string (ISO 8601) | When the session was first created            |
| `updated_at`  | string (ISO 8601) | When the session was last active              |

#### Examples

```bash
# List the 50 most recent sessions (default)
curl http://localhost:3000/api/sessions

# List up to 10 sessions
curl "http://localhost:3000/api/sessions?limit=10"
```

---

### GET /api/sessions/{session_key}/messages

Returns the message history for a specific session.

#### Path Parameters

| Parameter     | Description                                                   |
|---------------|---------------------------------------------------------------|
| `session_key` | The stable session identifier (e.g. `discord:guild:channel`) |

#### Query Parameters

| Parameter | Type   | Default | Description                          |
|-----------|--------|---------|--------------------------------------|
| `limit`   | number | `100`   | Maximum number of messages to return |

#### Response

```json
[
  {
    "role": "user",
    "content": "Hello, can you help me review this PR?",
    "author": "alice",
    "created_at": "2026-03-10T12:00:00Z"
  },
  {
    "role": "assistant",
    "content": "Sure! Please share the PR link.",
    "author": null,
    "created_at": "2026-03-10T12:00:03Z"
  }
]
```

| Field        | Type            | Description                              |
|--------------|-----------------|------------------------------------------|
| `role`       | string          | `"user"` or `"assistant"`                |
| `content`    | string          | Message text                             |
| `author`     | string or null  | Display name of the sender, if available |
| `created_at` | string (ISO 8601) | When the message was recorded          |

#### Examples

```bash
# Get messages for a Discord session
curl "http://localhost:3000/api/sessions/discord:guild123:channel456/messages"

# Get the last 20 messages
curl "http://localhost:3000/api/sessions/discord:guild123:channel456/messages?limit=20"
```

---

### GET /api/runs

Returns a paginated list of orchestration runs.

#### Query Parameters

| Parameter | Type   | Default | Description                                         |
|-----------|--------|---------|-----------------------------------------------------|
| `status`  | string | (all)   | Filter by run status: `running`, `completed`, `failed` |
| `limit`   | number | `50`    | Maximum number of runs to return                    |

#### Response

```json
[
  {
    "team_run_id": "run-abc123",
    "session_key": "discord:guild123:channel456",
    "team_name": "feature-dev",
    "workflow": "chain",
    "status": "running",
    "current_step": 2,
    "total_steps": 4,
    "result": null,
    "created_at": "2026-03-10T12:00:00Z",
    "updated_at": "2026-03-10T12:05:00Z"
  }
]
```

| Field          | Type            | Description                                  |
|----------------|-----------------|----------------------------------------------|
| `team_run_id`  | string          | Unique run identifier                        |
| `session_key`  | string          | Associated session                           |
| `team_name`    | string          | Team workflow that was executed              |
| `workflow`     | string          | Workflow type (e.g. `chain`)                 |
| `status`       | string          | `running`, `completed`, or `failed`          |
| `current_step` | number          | Steps completed so far                       |
| `total_steps`  | number          | Total steps in the workflow                  |
| `result`       | string or null  | Final output when `status` is `completed`    |
| `created_at`   | string (ISO 8601) | When the run started                       |
| `updated_at`   | string (ISO 8601) | Last step transition time                  |

#### Examples

```bash
# List all runs (up to 50)
curl http://localhost:3000/api/runs

# List only running workflows
curl "http://localhost:3000/api/runs?status=running"

# List the last 5 completed runs
curl "http://localhost:3000/api/runs?status=completed&limit=5"
```

---

### GET /api/agents

Returns the list of configured agent profiles.

#### Response

```json
[
  { "name": "main" },
  { "name": "architect" },
  { "name": "tester" }
]
```

| Field  | Type   | Description             |
|--------|--------|-------------------------|
| `name` | string | Agent profile name      |

#### Example

```bash
curl http://localhost:3000/api/agents
```

---

### GET /api/teams

Returns the list of configured team workflows.

#### Response

```json
[
  { "name": "feature-dev" },
  { "name": "security-audit" }
]
```

| Field  | Type   | Description            |
|--------|--------|------------------------|
| `name` | string | Team workflow name     |

#### Example

```bash
curl http://localhost:3000/api/teams
```

### GET /api/alerts/history

Returns the most recent alert trigger events.

#### Query Parameters

| Parameter | Type   | Default | Description                                                         |
|-----------|--------|---------|---------------------------------------------------------------------|
| `limit`   | number | `50`    | Maximum number of events to return (`1` to `1000`)                   |
| `offset`  | number | `0`     | Number of records to skip before returning results                  |
| `rule`    | string | (all)   | Optional exact alert rule name to filter by                          |
| `since`   | string | (none)  | Optional cutoff time (`24h`, `7d`, RFC3339, or `YYYY-MM-DD HH:MM:SS`) |

#### Response

```json
[
  {
    "id": 9,
    "rule_id": "b2f...",
    "rule_name": "high-backlog",
    "metric": "queue_backlog",
    "value": 120.5,
    "triggered_at": "2026-03-11 09:12:03"
  }
]
```

| Field        | Type   | Description                                 |
|--------------|--------|---------------------------------------------|
| `id`         | number | Alert history row identifier                 |
| `rule_id`    | string | Rule UUID                                   |
| `rule_name`  | string | Rule name                                    |
| `metric`     | string | Triggered metric                              |
| `value`      | number | Metric value at trigger time                  |
| `triggered_at` | string | Trigger timestamp (`YYYY-MM-DD HH:MM:SS`) |

#### Examples

```bash
# Last 50 alert events
curl "http://localhost:3000/api/alerts/history"

# Filter by rule and include events from the last 24 hours
curl "http://localhost:3000/api/alerts/history?rule=high-backlog&since=24h"
```

### POST /api/alerts/test

Evaluates alert rules against current metrics immediately.

#### Query Parameters

| Parameter | Type   | Default | Description                                      |
|-----------|--------|---------|--------------------------------------------------|
| `rule`    | string | (all)   | Optional exact rule name to evaluate               |
| `dry_run` | bool   | `false` | If `true`, do not write triggered alerts to history |

#### Response

```json
{
  "metrics": {
    "queue_backlog": 0,
    "failed_runs": 2,
    "error_rate": 1
  },
  "triggered": ["high-backlog"]
}
```

#### Examples

```bash
# Evaluate all enabled rules
curl -X POST "http://localhost:3000/api/alerts/test"

# Evaluate a single rule without recording events
curl -X POST "http://localhost:3000/api/alerts/test?rule=high-backlog&dry_run=true"
```

Invalid rule values:
- returns `404 Not Found` for unknown rule names
- returns `422 Unprocessable Entity` when the rule exists but is disabled

--- 

## Error Responses

All errors use a consistent JSON body with an HTTP status code that reflects the
failure kind.

### Error Body

```json
{
  "error": "not found: session discord:guild:channel"
}
```

| Field   | Type   | Description              |
|---------|--------|--------------------------|
| `error` | string | Human-readable error message |

### HTTP Status Codes

| Code | Meaning              | When                                             |
|------|----------------------|--------------------------------------------------|
| 400  | Bad Request          | Invalid query parameters or request format       |
| 404  | Not Found            | Requested resource does not exist                |
| 409  | Conflict             | Resource already exists (e.g. duplicate profile) |
| 500  | Internal Server Error | Unexpected database or template error           |

### Example Error Response

```bash
$ curl -i "http://localhost:3000/api/sessions/nonexistent:key/messages"
HTTP/1.1 404 Not Found
content-type: application/json

{"error":"not found: session nonexistent:key"}
```

---

## Common Patterns

### Polling for active runs

```bash
# Poll every 5 seconds until no runs are active
while true; do
  ACTIVE=$(curl -s "http://localhost:3000/api/runs?status=running" | jq length)
  echo "Active runs: $ACTIVE"
  [ "$ACTIVE" -eq 0 ] && break
  sleep 5
done
```

### Dump all sessions to JSON

```bash
curl -s http://localhost:3000/api/sessions | jq .
```

### Get dashboard stats as a one-liner

```bash
curl -s http://localhost:3000/api/dashboard | \
  jq '"Sessions: \(.session_count) | Runs: \(.run_count) | Agents: \(.agent_count)"'
```
