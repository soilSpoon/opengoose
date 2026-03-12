# AGENTS.md (repo guide)

This repository is a Goose-native orchestrator with a minimal-core philosophy.

## Principles

1. Prefer Goose-native reuse over custom engine reimplementation.
2. Keep core behavior small and explicit.
3. Keep transport/platform specifics inside adapter crates.
4. Preserve testability by separating policy logic from I/O plumbing.

## Documentation policy

- Keep `README.md` as the current-project overview and command reference.
- Keep deep analysis/refactor notes in `docs/codebase-review-2026-03.md`.
- Remove stale architecture docs instead of letting them drift.

## Change policy

- When adding a channel-specific behavior, ask first if it can be shared via `opengoose-core`.
- When changing CLI surface, update `README.md` command examples in the same change.
- When changing architectural boundaries, update `docs/codebase-review-2026-03.md`.

## CI

- Single workflow (`ci-quality-gate.yml`) to avoid duplication and simplify maintenance.
- Change detection skips CI when no Rust files are modified, saving time and cost.
- Use nightly for fmt (some rustfmt options require it), stable for clippy/test (matches production).
- No matrix for stable/nightly — stable-only testing is sufficient for most projects; nightly is only needed for fmt.
- Use `mozilla-actions/sccache-action` for compiler-level caching; sccache is enforced project-wide via `.cargo/config.toml` (`rustc-wrapper = "sccache"`).
