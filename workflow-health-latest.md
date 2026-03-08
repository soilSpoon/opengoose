# Workflow Health Manager - Run 2026-03-08T10:20:39Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22819070991
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): 6

## Compilation Status
- 14/14 lock files present ✅
- All lock files up-to-date by local filesystem timestamps ✅
- NOTE: daily-perf-improver still failing via GitHub API hash check (frontmatter mismatch persists in git history)

## Workflow Run Health

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|-------------|---------|
| Agentic Maintenance | ✅ Healthy | 10 | 100% | |
| CI Optimization Coach | ❌ Critical | 1 | 0% (startup_failure) | #58 |
| CI Doctor | ⚠️ Unknown | 0 recent | N/A | PR-triggered |
| CLI Consistency Checker | ❌ Critical | 1 | 0% (startup_failure) | #58 |
| Code Simplifier | ✅ Healthy | 2 | 100% | |
| Daily Doc Updater | ✅ Healthy | 1 | 100% | |
| Daily Perf Improver | ❌ Critical | 2 | 0% (hash mismatch) | #57 |
| Daily Test Improver | ✅ Healthy | 1 | 100% | |
| Daily Rust Testing Expert | ❌ Critical | 2 | 0% (startup_failure) | #58 |
| Duplicate Code Detector | ⚠️ Warning | 1 | 0% safe_outputs fail | partial success |
| Glossary Maintainer | ❌ Critical | 1 | 0% (pre-agent) | #48 |
| PR Fix | ⚠️ N/A | 0 | N/A | PR-triggered |
| Q | ⚠️ N/A | 0 | N/A | PR-triggered |
| Schema Consistency Checker | ✅ Healthy | 1 | 100% | #55 minor config |
| Workflow Health Manager | ✅ Healthy | 3 | 100% | This workflow |

## Critical Issues
- **P1** Daily Perf Improver — lock file hash mismatch, 2nd consecutive failure — issue #57 (updated comment)
- **P1** Daily Rust Testing Expert — startup_failure 2x — issue #58 (updated comment)
- **P2** CI Coach + CLI Consistency Checker — startup_failure — issue #58 (same ticket)
- **P2** Glossary Maintainer — pre-agent failure, 1 run ever — issue #48 (no new run to report)

## Actions This Run
- Added status update comment to #57 (Daily Perf Improver, 2nd day failing)
- Added status update comment to #58 (Daily Rust Testing Expert, now 2 consecutive startup_failures)
- No new issues needed — all problems already tracked

## Systemic Patterns
- `startup_failure` cluster: 3 newer workflows (CI Coach, CLI Checker, Daily Rust Testing Expert) all fail with startup_failure; older workflows succeed → possible runner quota/seat issue for new workflows
- Daily Perf Improver hash mismatch: persists across 2 days, requires manual `gh aw compile` + commit

## Health Scores
- Healthy: 5 workflows (Agentic Maintenance, Code Simplifier, Daily Doc Updater, Daily Test Improver, Schema Consistency Checker, Workflow Health Manager) → ~43%
- Critical: 5 workflows (CI Coach, CLI Checker, Daily Perf Improver, Daily Rust Testing Expert, Glossary Maintainer) → ~36%
- Warning/Unknown: 4 workflows (CI Doctor, Duplicate Code Detector, PR Fix, Q)
