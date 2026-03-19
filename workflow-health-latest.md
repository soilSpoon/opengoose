# Workflow Health Manager - Run 2026-03-19T10:26:22Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23290439262
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): entries in `.github/workflows/shared/`

## Compilation Status
- 14/14 lock files present ✅
- Known hash mismatch (accepted/not_planned): Duplicate Code Detector, Schema Consistency Checker

## Workflow Run Health (2026-03-19)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | N/A | N/A | |
| CI Quality Gate | ✅ Healthy | 5/5 success | 100% | |
| Code Simplifier | ✅ Healthy | prior runs success | 100% | |
| Daily Doc Updater | ✅ Healthy | 14/14 success | 100% | |
| Daily Perf Improver | ✅ Healthy | skipped (no changes) | N/A | |
| Daily Test Improver | ✅ Healthy | skipped (no changes) | N/A | |
| Daily Rust Testing Expert | ✅ HEALTHY | run #15 today success | 100% | #318 CLOSED |
| CI Optimization Coach | ❌ Critical | 9/9 failure | 0% | #319 open |
| CLI Consistency Checker | ❌ Critical | 9/10 failure (1 success run #1) | 10% | #320 open |
| Glossary Maintainer | ❌ Critical | 10/10 failure | 0% | #292 open |
| CI Failure Doctor | ✅ Healthy | event-triggered | 100% | |
| Duplicate Code Detector | ⚠️ Accepted | all failure | N/A | not_planned |
| Schema Consistency Checker | ⚠️ Accepted | all failure | N/A | not_planned |
| Workflow Health Manager | ✅ Healthy | self | 100% | self |
| PR Fix | ⚠️ PR-triggered | N/A | N/A | |
| Q | ⚠️ PR-triggered | N/A | N/A | |

## Critical Issues

- **P1** Glossary Maintainer — 10/10 consecutive failures from Mar 6 through Mar 19; never succeeded; #292 open; commented with run #10 update today
- **P1** CI Optimization Coach — 9/9 consecutive failures (5 startup_failure Mar 6-12, 4 failure Mar 13-18); #319 open; last run #9 Mar 18; commented today
- **P1** CLI Consistency Checker — 9 failures after 1 early success; last run #10 Mar 18; #320 open; commented today
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer: not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer: not_planned

## Actions This Run

- ✅ Commented on #292 (Glossary Maintainer — 10th consecutive failure, run #10 on 2026-03-19)
- ✅ Commented on #319 (CI Coach — 9th consecutive failure, run #9 on 2026-03-18)
- ✅ Commented on #320 (CLI Checker — 10th run/9 consecutive failures, run #10 on 2026-03-18)

## Systemic Patterns

- **Startup_failure cluster (Mar 6-12)**: CI Coach and CLI Checker both had startup_failures on exact same dates (Mar 6-12), suggesting a shared infrastructure issue that was partially resolved around Mar 13.
- **Post-fix execution failures (Mar 13+)**: Both CI Coach and CLI Checker shifted to `failure` after Mar 13 — agent starts but fails during execution.
- **Glossary Maintainer**: 10 consecutive failures - agent fails consistently; pattern from day 1; never recovered.
- **CLI Checker unique**: Run #1 succeeded (Mar 5) — only workflow to ever succeed, confirming it can work. Regression began Mar 6.
- **Recovery confirmed**: Daily Rust Testing Expert — run #15 today (Mar 19), 6th consecutive success. Issue #318 CLOSED.
- **Healthy cluster**: Daily Doc Updater, Daily Rust Testing Expert, CI Quality Gate — consistently healthy.

## Health Scores (approximate)

- Healthy (≥80): 9 workflows
- Critical (<60): 3 (Glossary Maintainer, CI Optimization Coach, CLI Consistency Checker)
- Accepted/not_planned: 2 (Duplicate Code Detector, Schema Consistency Checker)
- PR-triggered (no health score): 2 (PR Fix, Q)

## Trend

- → Glossary Maintainer (10th failure): no improvement, 13 days of failure
- → CI Coach (run #9 Mar 18, 9th failure): no improvement, 13 days of failure
- → CLI Checker (run #10 Mar 18, 9th consecutive failure): no improvement, 13 days since regression
- ✅ Daily Rust Testing Expert: 6th consecutive success (run #15), fully healthy
- Overall: 9 healthy, 3 critical (unchanged)
