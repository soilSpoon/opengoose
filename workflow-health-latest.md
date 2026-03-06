# Workflow Health Manager - Run 2026-03-06T10:22:49Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22759235789
- Total agentic workflows discovered: 14 (all with lock files)
- Shared include files (excluded): 6

## Compilation Status
All 14 executable .md workflows have corresponding .lock.yml files. ✅

## Workflow Run Health

| Workflow | Status | Runs | Success Rate |
|----------|--------|------|-------------|
| CI Quality Gate | ⚠️ Warning | 5 | 40% (2/5) |
| Claude Code Review | ✅ Healthy | 4 | 100% (4/4) |
| Glossary Maintainer | ❌ Critical | 1 | 0% (0/1) — issue #48 exists |
| Daily Perf Improver | ⏭️ Skipped | 3 | N/A (skip conditions) |
| Daily Test Improver | ⏭️ Skipped | 3 | N/A (skip conditions) |
| PR Fix | ⏭️ Skipped | 3 | N/A (skip conditions) |
| Q | ⏭️ Skipped | 3 | N/A (skip conditions) |
| Claude Code | ⏭️ Skipped | 2 | N/A (skip conditions) |
| Workflow Health Manager | 🔄 Running | 1 | In Progress |

Newly added workflows (no runs yet, not a health concern):
- CI Optimization Coach, CI Failure Doctor, CLI Consistency Checker, Code Simplifier, Daily Documentation Updater, Daily Rust Testing Expert, Duplicate Code Detector, Schema Consistency Checker, Agentic Maintenance

## Critical Issues
- **P1**: CI Quality Gate failing 3/5 runs due to `cargo +nightly fmt` formatting errors in `session_manager.rs` — issue created
- **P2**: Glossary Maintainer pre-agent failure — issue #48 already exists

## Issues Created This Run
- Created P1 issue for CI Quality Gate formatting failures

## Issues Already Tracked
- Issue #48: Glossary Maintainer failed (pre-agent)
