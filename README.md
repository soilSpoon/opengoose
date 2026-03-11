# OpenGoose

OpenGoose is a Goose-native, multi-channel AI orchestrator written in Rust.

## Quick Start

```bash
# Build
cargo build --release

# Run (default command is also `run`)
cargo run --release -- run
# or
opengoose
```

## CLI Commands

```bash
# Runtime
opengoose run
opengoose web --host 0.0.0.0 --port 8080

# Machine-readable output
opengoose --json auth list
opengoose --json db cleanup --profile main
opengoose --json event history --filter kind:message_received
opengoose --json profile show developer
opengoose --json team list

# Database maintenance
opengoose db cleanup [--profile <name>]
opengoose db cleanup --retention-days <days> [--event-retention-days <days>]

# Event history
opengoose event history [--limit <n>]
opengoose event history --filter gateway:discord --since 24h

# Alert rules
opengoose alert list
opengoose alert create high-backlog --metric queue_backlog --condition gt --threshold 100
opengoose alert test
opengoose alert test --rule high-backlog --dry-run
opengoose alert history [--limit <n>]
opengoose alert history --rule high-backlog --since 24h

# Provider auth / secrets
opengoose auth login [provider]
opengoose auth list        # alias: opengoose auth ls
opengoose auth models <provider>
opengoose auth logout <provider>
opengoose auth set <key>
opengoose auth remove <key>

# Profiles
opengoose profile list
opengoose profile show <name>
opengoose profile set <name> --message-retention-days <days>
opengoose profile set <name> --event-retention-days <days>
opengoose profile add <path>
opengoose profile remove <name>
opengoose profile init [--force]

# Teams
opengoose team list
opengoose team show <name>
opengoose team add <path>
opengoose team remove <name>
opengoose team init [--force]

# Shell completions
opengoose completion bash
opengoose completion zsh
```

## Workspace Crates

- `opengoose-types`
- `opengoose-core`
- `opengoose-discord`
- `opengoose-telegram`
- `opengoose-slack`
- `opengoose-tui`
- `opengoose-secrets`
- `opengoose-profiles`
- `opengoose-teams`
- `opengoose-persistence`
- `opengoose-provider-bridge`
- `opengoose-web`
- `opengoose-cli`

## Platform Support

Built-in adapters: Discord, Slack, Telegram.

Custom platforms are supported via `Platform::Custom(String)` — add a new
adapter crate without modifying `opengoose-core` or `opengoose-types`.
See the [Adding a New Channel Platform][new-platform] guide.

[new-platform]: docs/codebase-review-2026-03.md#adding-a-new-channel-platform

## Docs

- `AGENTS.md`: repository principles and change policy
- `docs/codebase-review-2026-03.md`: architecture, dependency graph, and backlog

## Container Deployment

```bash
# Build the production image
docker build -t opengoose .

# Run the web dashboard with persisted state
docker run --rm \
  -p 8080:8080 \
  -e OPENGOOSE_HOST=0.0.0.0 \
  -e OPENGOOSE_PORT=8080 \
  -e OPENGOOSE_DB_PATH=/var/lib/opengoose/.opengoose/sessions.db \
  -v opengoose-data:/var/lib/opengoose \
  opengoose

# Or use the bundled compose file
docker compose up --build
```

The image defaults to `opengoose web`, exposes the health endpoints under
`/api/health`, `/api/health/ready`, and `/api/health/live`, and preserves the
SQLite database on a mounted volume.

### Runtime Environment

- `OPENGOOSE_HOST`: bind address for `opengoose web`. Defaults to `127.0.0.1` on the CLI and `0.0.0.0` in the container image.
- `OPENGOOSE_PORT`: web port for `opengoose web`. Defaults to `8080`.
- `OPENGOOSE_DB_PATH`: SQLite database path. Defaults to `~/.opengoose/sessions.db`; the container image sets it to `/var/lib/opengoose/.opengoose/sessions.db`.
- `HOME`: optional root for persisted OpenGoose state such as profiles, teams, plugins, and config metadata. The compose file mounts a named volume at `/var/lib/opengoose`.

For containerized deployments, prefer environment variables for credentials instead
of the host keyring. OpenGoose already resolves secrets from env first, including
provider keys such as `OPENAI_API_KEY` and `ANTHROPIC_API_KEY`, plus channel
credentials such as `DISCORD_BOT_TOKEN`, `TELEGRAM_BOT_TOKEN`, `SLACK_BOT_TOKEN`,
`SLACK_APP_TOKEN`, `MATRIX_HOMESERVER_URL`, and `MATRIX_ACCESS_TOKEN`.

## License

MIT
