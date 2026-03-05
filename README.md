# OpenGoose

A Discord-to-[Goose](https://github.com/block/goose) AI agent orchestrator built in Rust. OpenGoose bridges Discord messaging with the Goose AI agent framework, enabling users to interact with Goose agents directly through Discord.

## How It Works

OpenGoose implements Goose's `Gateway` trait natively, treating Discord as a transport layer while the Goose agent handles all AI execution.

```
Discord User ──► Discord WebSocket ──► OpenGooseGateway ──► Goose Agent
                                              │
                                         TUI Dashboard
                                    (status, messages, events)
```

**Message flow:**
1. User sends a message in a Discord thread or DM
2. The Discord adapter receives it via WebSocket and converts it into a session
3. The gateway relays the message to Goose's agent handler
4. Goose processes the message and returns a response
5. The response is sent back to Discord (automatically split if over 2000 chars)

**Pairing flow:**
- On first interaction, a 6-character pairing code is generated (expires in 5 minutes)
- The code is displayed in the TUI events panel
- The user enters the code in Discord to authenticate the session

## Architecture

OpenGoose is a modular Rust workspace with 6 crates:

| Crate | Role |
|---|---|
| `opengoose-types` | Core domain types (`SessionKey`, `AppEvent`, `EventBus`) — zero business logic |
| `opengoose-secrets` | Credential management via OS keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service) with env var fallback |
| `opengoose-core` | `Gateway` trait implementation, pairing code generation, message relay |
| `opengoose-discord` | Discord WebSocket adapter built on [Twilight](https://github.com/twilight-rs/twilight) |
| `opengoose-tui` | Terminal UI built on [Ratatui](https://github.com/ratatui/ratatui) — setup wizard + live monitoring dashboard |
| `opengoose-cli` | Binary entrypoint, orchestrates all components |

## Getting Started

### Prerequisites

- Rust (edition 2024)
- A Discord bot token from the [Discord Developer Portal](https://discord.com/developers/applications)
  - Enable the **MESSAGE_CONTENT** intent on your bot
- Goose agent configured (uses `~/.config/goose/` by default)

### Build

```bash
cargo build --release
```

### Run

```bash
# Start the gateway + TUI
cargo run --release -- run

# Or directly after building
./target/release/opengoose run
```

**First launch** — the TUI shows a setup wizard where you enter your Discord bot token. The token is stored securely in your OS keyring.

**Subsequent launches** — the token is loaded from the keyring automatically and the Discord adapter connects immediately.

### Authentication & Credential Management

OpenGoose supports all Goose LLM providers. Credentials are stored securely in your OS keyring.

```bash
# Authenticate with an AI provider (interactive selection if provider omitted)
opengoose auth login anthropic

# Interactive provider selection
opengoose auth login

# List all providers and their authentication status
opengoose auth list        # or: opengoose auth ls

# Remove stored credentials for a provider
opengoose auth logout anthropic
```

**Supported providers:** Anthropic, OpenAI, Google Gemini, OpenRouter, xAI, Venice, GitHub Copilot, Tetrate, LiteLLM, Azure OpenAI, Databricks, Snowflake, AWS Bedrock, GCP Vertex AI, SageMaker TGI, Ollama, Local Inference.

Custom secrets (e.g. Discord bot token) can also be managed:

```bash
opengoose auth set discord_bot_token
opengoose auth remove discord_bot_token
```

Secrets cannot be retrieved or displayed through the CLI — only set, listed, or removed.

### Environment Variables

| Variable | Description |
|---|---|
| `DISCORD_BOT_TOKEN` | Discord bot token (takes precedence over keyring) |
| `ANTHROPIC_API_KEY` | Anthropic API key (takes precedence over keyring) |
| `OPENAI_API_KEY` | OpenAI API key (takes precedence over keyring) |
| `RUST_LOG` | Log level filter (default: `info,opengoose=debug`) |

## TUI

The terminal UI has two modes:

- **Setup mode** — Minimal interface for first-time bot token entry
- **Normal mode** — Full dashboard with:
  - Status bar (connection state, uptime, session count)
  - Message panel (scrollable history, max 1000)
  - Events panel (system events & logs, max 2000)
  - Command palette (`Ctrl+O`) for actions like configuring AI providers, generating pairing codes, or updating the token
  - Keyboard shortcut help bar

## Tech Stack

- **Async runtime**: [Tokio](https://tokio.rs/)
- **Discord**: [Twilight](https://github.com/twilight-rs/twilight) 0.17 (gateway + HTTP + model)
- **AI Agent**: [Goose](https://github.com/block/goose) v1.26.1
- **Secrets**: [keyring](https://crates.io/crates/keyring) 3.x + [zeroize](https://crates.io/crates/zeroize) for secure memory cleanup
- **TUI**: [Ratatui](https://github.com/ratatui/ratatui) 0.30 + [Crossterm](https://crates.io/crates/crossterm) 0.29
- **CLI**: [Clap](https://crates.io/crates/clap) 4.x

## License

MIT
