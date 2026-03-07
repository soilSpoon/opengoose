# Workflow Health Manager - Run 2026-03-07T10:20:21Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22797203893
- Total agentic workflows discovered: 14 (all with lock files)
- Shared include files (excluded): 6

## Compilation Status
- 13/14 lock files up-to-date ✅
- 1/14 lock file OUTDATED ❌: `daily-perf-improver.lock.yml` (frontmatter hash mismatch since 2026-03-06T05:15Z)

## Workflow Run Health

| Workflow | Status | Runs | Success Rate | Notes |
|----------|--------|------|-------------|-------|
| Agentic Maintenance | ✅ Healthy | 12 | 100% | Scheduled, reliable |
| CI Quality Gate | ⚠️ Warning | 2 | 50% (1/2) | Issue #49 open; latest run succeeded |
| Claude Code Review | ✅ Healthy | 1 | 100% | |
| Code Simplifier | ✅ Healthy | 1 | 100% | |
| Schema Consistency Checker | ⚠️ Warning | 1 | 100% | No discussion categories (issue #55) |
| Daily Documentation Updater | ✅ Healthy | 1 | 100% | |
| Daily Test Improver | ✅ Healthy | 1 | 100% | |
| Duplicate Code Detector | ⚠️ Warning | 1 | 0% | Safe_outputs failed (Copilot SWE not available); content succeeded (created issue #51) |
| Glossary Maintainer | ❌ Critical | 1 | 0% | Pre-agent failure, issue #48 |
| Daily Perf Improver | ❌ Critical | 1 | 0% | Lock file outdated, issue created this run |
| CLI Consistency Checker | ❌ Critical | 1 | 0% | startup_failure (0 jobs), issue created this run |
| CI Optimization Coach | ❌ Critical | 1 | 0% | startup_failure (0 jobs), issue created this run |
| Daily Rust Testing Expert | ❌ Critical | 1 | 0% | startup_failure (0 jobs), issue created this run |
| Workflow Health Manager | ✅ Healthy | 2 | 100% | This workflow |

## Critical Issues
- **P1**: Daily Perf Improver — lock file outdated (frontmatter hash mismatch) — new issue created
- **P2**: 3 workflows with startup_failure (CLI Consistency Checker, CI Optimization Coach, Daily Rust Testing Expert) — new issue created
- **P2**: Glossary Maintainer pre-agent failure — issue #48 (existing)
- **P2**: Schema Consistency Checker — no discussion categories — issue #55 (existing)
- **P2**: CI Quality Gate — nightly rustfmt failures — issue #49 (updated with comment)

## Issues Created This Run
- Created P1 issue (parent: #50): Daily Perf Improver lock file outdated
- Created P2 issue (parent: #50): Startup failures on 3 workflows
- Added comment to #49: CI Quality Gate status update

## Systemic Patterns
- `startup_failure` on 3 newly-added workflows suggests possible runner quota/license issue for new workflows
- Copilot SWE agent unavailable for assignment in safe_outputs (Duplicate Code Detector)
