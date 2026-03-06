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
opengoose profile add <path>
opengoose profile remove <name>
opengoose profile init [--force]

# Teams
opengoose team list
opengoose team show <name>
opengoose team add <path>
opengoose team remove <name>
opengoose team init [--force]
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
- `opengoose-cli`

## Docs

- `AGENTS.md`: repository principles and change policy
- `docs/codebase-review-2026-03.md`: architecture review, refactor priorities, and executable backlog (DoD)

## License

MIT
