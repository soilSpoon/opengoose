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
opengoose run --model gpt-5-mini
opengoose web --port 8080
./scripts/web-smoke.sh http://127.0.0.1:8080

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

# Projects (agent-native project context)
opengoose project list
opengoose project show <name>
opengoose project add <path>
opengoose project remove <name>
opengoose project init [--force]
opengoose project run <name> <input> [--team <team>]

# Teams
opengoose team list
opengoose team show <name>
opengoose team add <path>
opengoose team remove <name>
opengoose team init [--force]
opengoose team run <team> "<input>"
opengoose team run <team> "<input>" --model gpt-5-mini

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
- `opengoose-projects`
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
- `docs/web-dashboard.md`: dashboard behavior, live update model, and smoke checks

## License

MIT
