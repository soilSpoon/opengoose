# Workflow Health Manager - Run 2026-03-18T10:32:45Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23240339019
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): entries in `.github/workflows/shared/`

## Compilation Status
- 14/14 lock files present ✅
- Known hash mismatch (accepted/not_planned): Duplicate Code Detector, Schema Consistency Checker

## Workflow Run Health (2026-03-18)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | 2/2 success | 100% | |
| CI Quality Gate | ✅ Healthy | 5/5 success | 100% | |
| Code Simplifier | ✅ Healthy | 1/1 success | 100% | |
| Daily Doc Updater | ✅ Healthy | 13/13 success | 100% | |
| Daily Perf Improver | ✅ Healthy | skipped (no changes) | N/A | |
| Daily Test Improver | ✅ Healthy | skipped (no changes) | N/A | |
| Daily Rust Testing Expert | ✅ HEALTHY | 5 consecutive success (#10-14, Mar 14-18) | ✅ Fully recovered | #318 CLOSED |
| CI Optimization Coach | ❌ Critical | 8/8 failure | 0% | #319 open |
| CLI Consistency Checker | ❌ Critical | 8/9 failure (1 success run #1) | 11% | #320 open |
| Glossary Maintainer | ❌ Critical | 9/9 failure | 0% | #292 open |
| CI Failure Doctor | ✅ Healthy | event-triggered | 100% | |
| Duplicate Code Detector | ⚠️ Accepted | all failure | N/A | not_planned |
| Schema Consistency Checker | ⚠️ Accepted | all failure | N/A | not_planned |
| Workflow Health Manager | ✅ Healthy | self | 100% | self |
| PR Fix | ⚠️ PR-triggered | N/A | N/A | |
| Q | ⚠️ PR-triggered | N/A | N/A | |

## Critical Issues

- **P1** Glossary Maintainer — 9/9 consecutive failures from Mar 6 through Mar 18; never succeeded; #292 open; commented with run #9 update today
- **P1** CI Optimization Coach — 8/8 consecutive failures (5 startup_failure Mar 6-12, 3 failure Mar 13-17); #319 open; last run #8 Mar 17 (no new run today)
- **P1** CLI Consistency Checker — 8 failures after 1 early success; last run #9 Mar 17; #320 open; no new run today
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer: not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer: not_planned

## Actions This Run

- ✅ Commented on #292 (Glossary Maintainer — 9th consecutive failure, run #9 on 2026-03-18)
- No new runs for CI Coach (#319) or CLI Checker (#320) today — no update needed

## Systemic Patterns

- **Startup_failure cluster (Mar 6-12)**: CI Coach and CLI Checker both had startup_failures on exact same dates (Mar 6-12), suggesting a shared infrastructure issue that was partially resolved around Mar 13.
- **Glossary Maintainer**: 9 consecutive failures - agent fails consistently; pattern from day 1; never recovered.
- **Recovery confirmed**: Daily Rust Testing Expert — 5 consecutive successes (Mar 14-18). Issue #318 CLOSED.
- **Healthy cluster**: Daily Doc Updater, Daily Rust Testing Expert, Code Simplifier, Agentic Maintenance, CI Quality Gate — consistently healthy.

## Health Scores (approximate)

- Healthy (≥80): 9 workflows
- Critical (<60): 3 (Glossary Maintainer, CI Optimization Coach, CLI Consistency Checker)
- Accepted/not_planned: 2 (Duplicate Code Detector, Schema Consistency Checker)
- PR-triggered (no health score): 2 (PR Fix, Q)

## Trend

- → Glossary Maintainer (9th failure): no improvement, getting worse
- → CI Coach (last run #8 Mar 17, 8th failure): no improvement
- → CLI Checker (last run #9 Mar 17, 8th failure): no improvement
- ✅ Daily Rust Testing Expert: 5th consecutive success today, fully healthy
- Overall: 9 healthy, 3 critical (same as yesterday)
