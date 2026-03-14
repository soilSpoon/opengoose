# Workflow Health Manager - Run 2026-03-14T10:21:35Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23086013454
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): entries in `.github/workflows/shared/`

## Compilation Status
- 14/14 lock files present ✅
- Known hash mismatch (accepted/not_planned): Duplicate Code Detector, Schema Consistency Checker

## Workflow Run Health (past 7 days, as of 10:21 UTC 2026-03-14)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | 5/5 success | 100% | |
| CI Quality Gate | ✅ Healthy | recent: success | ~75% | |
| Claude Code Review | ✅ Healthy | 1/1 success | 100% | |
| Code Simplifier | ✅ Healthy | 1/1 success | 100% | |
| Daily Doc Updater | ✅ Healthy | 1/1 success | 100% | |
| Daily Perf Improver | ✅ Healthy | 1/1 success | 100% | #254 monthly |
| Daily Test Improver | ✅ Healthy | 1/1 success | 100% | #310 monthly |
| Daily Rust Testing Expert | ⚠️ Warning | recovered today | 1 success, 1 failure | #318 open (recovered today) |
| CI Optimization Coach | ❌ Critical | 1/1 failure | 0% | #319 open (clippy pre-check fails) |
| CLI Consistency Checker | ❌ Critical | 1/1 failure | 0% | #320 open (agent exec failure) |
| Glossary Maintainer | ❌ Critical | 1/1 failure | 0% (7th consec.) | #292 open; comment added this run |
| Duplicate Code Detector | ⚠️ Accepted | failure | N/A | maintainer: not_planned |
| Schema Consistency Checker | ⚠️ Accepted | failure | N/A | maintainer: not_planned |
| Workflow Health Manager | ✅ Healthy | self | 100% | This workflow |
| PR Fix | ⚠️ PR-triggered | N/A | N/A | |
| Q | ⚠️ PR-triggered | N/A | N/A | |

## Critical Issues

- **P1** Glossary Maintainer — 7th consecutive failure; #292 open; comment added this run
- **P1** CI Optimization Coach — pre-agent clippy step fails; #319 auto-created; root cause: clippy lint errors in repo
- **P1** CLI Consistency Checker — agent execution failure (no safe-output produced); #320 auto-created
- **Recovered** Daily Rust Testing Expert — latest run (2026-03-14 07:58) succeeded; #318 still open; commented to notify of recovery
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer: not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer: not_planned

## Actions This Run

- Added comment to #292 (Glossary Maintainer — 7th consecutive failure, ongoing P1)
- Added comment to #318 (Daily Rust Testing Expert — recovered, suggest close)

## Systemic Patterns

- **Pre-agent clippy failure**: CI Optimization Coach fails because clippy runs on the repo *before* the agent starts. This is unusual — clippy would need to pass on the codebase first. Likely a real lint regression in the repo.
- **Agent safe-output failure cluster**: Glossary Maintainer + CLI Consistency Checker both fail in agent execution phase (no safe-output call). Likely agent invocation or tool misconfiguration.
- **Healthy cluster**: Daily Perf Improver, Daily Test Improver, Code Simplifier, Agentic Maintenance — consistently producing good results.

## Health Scores (approximate)

- Healthy (≥80): 6 workflows
- Warning/Recovering: 1 (Daily Rust Testing Expert)
- Critical (<60): 3 (Glossary Maintainer, CI Optimization Coach, CLI Consistency Checker)
- Accepted/not_planned: 2
