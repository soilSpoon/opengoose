# Workflow Health Manager - Run 2026-03-13T10:23:04Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23046483434
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): 6

## Compilation Status
- 14/14 lock files present ✅
- No missing lock files
- Known hash mismatch: Duplicate Code Detector + Schema Consistency Checker (maintainer decision: not_planned)

## Workflow Run Health (past 7 days, as of 10:23 UTC)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | 2/2 success | 100% | |
| CI Quality Gate | ✅ Healthy | 3/4 success | 75% | |
| Claude Code Review | ✅ Healthy | 1/1 success | 100% | |
| Code Simplifier | ✅ Healthy | 1/1 success | 100% | |
| CI Optimization Coach | ⚠️ No runs | 0 runs today (scheduled 13:00 UTC) | N/A | No open issue |
| CLI Consistency Checker | ⚠️ No runs | 0 runs today (scheduled 13:15 UTC) | N/A | No open issue |
| CI Doctor | ⚠️ PR-triggered | N/A | N/A | |
| Daily Perf Improver | ⚠️ PR-triggered | 3/3 skipped | N/A | |
| Daily Test Improver | ⚠️ PR-triggered | 3/3 skipped | N/A | |
| Daily Rust Testing Expert | ❌ Critical | 1/1 failure | 0% | #318 open (auto-created today) |
| Duplicate Code Detector | ❌ Critical | no scheduled runs | N/A | maintainer: not_planned |
| Glossary Maintainer | ❌ Critical | 1/1 failure | 0% (6th consec.) | #292 open; comment added this run |
| PR Fix | ⚠️ PR-triggered | 3/3 skipped | N/A | |
| Q | ⚠️ PR-triggered | 3/3 skipped | N/A | |
| Schema Consistency Checker | ⚠️ Warning | no recent runs | N/A | maintainer: not_planned |
| Workflow Health Manager | ✅ Healthy | self | 100% | This workflow |

## Critical Issues
- **P1** Glossary Maintainer — 6th consecutive failure; #292 open; comment added this run
- **P1** Daily Rust Testing Expert — continued failure (run 23041752136); #318 auto-created today
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer closed as not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer closed as not_planned

## Actions This Run
- Added comment to #292 (Glossary Maintainer — 6th consecutive failure)
- Issue #318 already auto-created for Daily Rust Testing Expert

## Systemic Patterns
- **Safe-output failure cluster**: Glossary Maintainer + Daily Rust Testing Expert both fail in the agent execution phase (no safe-output call produced). Detection step completes normally. Likely agent invocation or tool misconfiguration.
- **Healthy cluster**: Code Simplifier, Daily Perf Improver, Daily Test Improver are working well when triggered.

## Health Scores
- Healthy: 4 workflows (Code Simplifier, Agentic Maintenance, CI Quality Gate, Claude Code Review)
- Critical: 2 actively failing (Glossary Maintainer, Daily Rust Testing Expert)
- Accepted/not_planned: 2 (Duplicate Code Detector, Schema Consistency Checker)
- PR-triggered/N/A: 6 (CI Doctor, Daily Perf Improver, Daily Test Improver, PR Fix, Q + CI Coach/CLI Checker not yet run today)
