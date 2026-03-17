# Workflow Health Manager - Run 2026-03-17T10:31:15Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23189792201
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): entries in `.github/workflows/shared/`

## Compilation Status
- 14/14 lock files present ✅
- Known hash mismatch (accepted/not_planned): Duplicate Code Detector, Schema Consistency Checker

## Workflow Run Health (2026-03-17)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | 6/6 success | ~100% | |
| CI Quality Gate | ✅ Healthy | 5/7 success | ~80% | |
| Code Simplifier | ✅ Healthy | 1/1 success | 100% | |
| Daily Doc Updater | ✅ Healthy | 12/12 success | 100% | |
| Daily Perf Improver | ✅ Healthy | recent success | ~100% | #254 monthly |
| Daily Test Improver | ✅ Healthy | recent success | ~100% | #310 monthly |
| Daily Rust Testing Expert | ✅ FULLY RECOVERED | 4 consecutive success (Mar 14-17) | 4/13 overall but trend ✅ | #318 CLOSED ✅ |
| CI Optimization Coach | ❌ Critical | 7/7 failure | 0% | #319 open |
| CLI Consistency Checker | ❌ Critical | 7/8 failure | 12.5% | #320 open |
| Glossary Maintainer | ❌ Critical | 8/8 failure | 0% | #292 open |
| CI Failure Doctor | ✅ Healthy | event-triggered | 100% | |
| Duplicate Code Detector | ⚠️ Accepted | all failure | N/A | not_planned |
| Schema Consistency Checker | ⚠️ Accepted | all failure | N/A | not_planned |
| Workflow Health Manager | ✅ Healthy | self | 100% | self |
| PR Fix | ⚠️ PR-triggered | N/A | N/A | |
| Q | ⚠️ PR-triggered | N/A | N/A | |

## Critical Issues

- **P1** Glossary Maintainer — 8/8 consecutive failures since Mar 6 (11 days); run #8 failed today; #292 open
- **P1** CI Optimization Coach — 7/7 consecutive failures; last run Mar 16 (run #7); #319 open
- **P1** CLI Consistency Checker — 7/8 failures (1 success on first run Mar 5); last run Mar 16 (run #8); #320 open
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer: not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer: not_planned

## Actions This Run

- ✅ Closed #318 (Daily Rust Testing Expert — 4 consecutive successes Mar 14-17, fully recovered)
- ✅ Commented on #292 (Glossary Maintainer — 8th consecutive failure)

## Systemic Patterns

- **Startup_failure cluster (Mar 6-12)**: CI Coach and CLI Checker both had startup_failures on exact same dates (Mar 6-12), suggesting a shared infrastructure issue that was partially resolved around Mar 13 (switched to regular `failure` from `startup_failure`).
- **Glossary Maintainer**: 8 consecutive failures - agent fails to complete safe-output call; persistent pattern from day 1.
- **CI Coach + CLI Checker**: No new runs since Mar 16 (today is Mar 17 at 10:31 UTC — runs may not have fired yet).
- **Recovery confirmed**: Daily Rust Testing Expert — 4 consecutive successes. Issue #318 CLOSED.
- **Healthy cluster**: Daily Doc Updater, Daily Perf Improver, Daily Test Improver, Code Simplifier, Agentic Maintenance — consistently healthy.

## Health Scores (approximate)

- Healthy (≥80): 9 workflows
- Critical (<60): 3 (Glossary Maintainer, CI Optimization Coach, CLI Consistency Checker)
- Accepted/not_planned: 2 (Duplicate Code Detector, Schema Consistency Checker)
- PR-triggered (no health score): 2 (PR Fix, Q)

## Trend

- ✅ Daily Rust Testing Expert #318 CLOSED — 4 consecutive successes confirmed
- → Glossary Maintainer (8th failure), CI Coach (7th), CLI Checker (7th): no improvement
- Overall: 9 healthy (+1 from last run since #318 confirmed healthy), 3 critical (same)
