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

## Default Profiles

OpenGoose ships five built-in profiles:

| Profile | Description |
|---|---|
| `main` | Default assistant — builds its own identity through an onboarding conversation on first run |
| `developer` | Software developer focused on writing, debugging, and refactoring code |
| `researcher` | Research assistant for investigation, synthesis, and analysis |
| `reviewer` | Code and document reviewer focused on quality and correctness |
| `writer` | Writing assistant for drafts, editing, and communication |

Use `opengoose profile list` to see all available profiles and `opengoose profile show <name>` to inspect one.

## Workspace Identity

Each profile has a workspace directory at `~/.opengoose/workspace-<profile>/` that stores identity and memory files:

| File | Purpose |
|---|---|
| `BOOTSTRAP.md` | First-run onboarding script (auto-deleted after setup) |
| `IDENTITY.md` | Agent name, role, and personality |
| `USER.md` | Learned preferences and context about the user |
| `SOUL.md` | Core values and behavioural principles |
| `MEMORY.md` | Persistent memory across sessions |

**Specialist profiles** (`developer`, `researcher`, `reviewer`, `writer`) come with pre-authored `IDENTITY.md` and `SOUL.md` — no onboarding needed.

**The `main` profile** starts with `BOOTSTRAP.md` and guides the agent through a one-time setup conversation where it learns about the user and writes its own identity files.

Profile YAML files no longer require an `instructions` or `prompt` field; the workspace files supply the system context instead.

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

## License

MIT
