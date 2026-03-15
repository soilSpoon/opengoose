# Workflow Health Manager - Run 2026-03-15T10:21:18Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23108392871
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): entries in `.github/workflows/shared/`

## Compilation Status
- 14/14 lock files present ✅
- Known hash mismatch (accepted/not_planned): Duplicate Code Detector, Schema Consistency Checker

## Workflow Run Health (2026-03-15)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | 110 runs, recent all success | 100% | |
| CI Quality Gate | ✅ Healthy | push runs succeed | ~80% | |
| Claude Code Review | ✅ Healthy | on-demand | 100% | |
| Code Simplifier | ✅ Healthy | 10/10 success | 100% | |
| Daily Doc Updater | ✅ Healthy | 10/10 success | 100% | |
| Daily Perf Improver | ✅ Healthy | recent success | 100% | #254 monthly |
| Daily Test Improver | ✅ Healthy | recent success | 100% | #310 monthly |
| Daily Rust Testing Expert | ✅ RECOVERED | 2 consec. success (Mar 14 + Mar 15) | recovering | #318 open (closing recommended) |
| CI Optimization Coach | ❌ Critical | 6/6 failure | 0% | #319 open |
| CLI Consistency Checker | ❌ Critical | 6/7 failure | 14% | #320 open |
| Glossary Maintainer | ❌ Critical | 6/6 failure | 0% (no run since Mar 13) | #292 open |
| CI Failure Doctor | ✅ Healthy | 2/2 success | 100% | triggered by CI failures |
| Duplicate Code Detector | ⚠️ Accepted | all failure | N/A | maintainer: not_planned |
| Schema Consistency Checker | ⚠️ Accepted | all failure | N/A | maintainer: not_planned |
| Workflow Health Manager | ✅ Healthy | 10/10 success | 100% | self |
| PR Fix | ⚠️ PR-triggered | N/A | N/A | |
| Q | ⚠️ PR-triggered | N/A | N/A | |

## Critical Issues

- **P1** Glossary Maintainer — 6 consecutive failures since Mar 6; last run Mar 13; #292 open
- **P1** CI Optimization Coach — 6 consecutive failures; all failure/startup_failure; #319 open  
- **P1** CLI Consistency Checker — 6 consecutive failures (1 initial success on Mar 5); #320 open
- **Recovered** Daily Rust Testing Expert — 2 consecutive successes (Mar 14 + Mar 15); #318 closing recommended
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer: not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer: not_planned

## Actions This Run

- Commented on #318 (Daily Rust Testing Expert — 2 consecutive successes confirmed, recommend close)
- No new Glossary Maintainer run to report (no new data since Mar 13 comment)

## Systemic Patterns

- **Agent safe-output failure cluster**: Glossary Maintainer + CLI Consistency Checker + CI Coach fail in agent execution/pre-agent phase (no safe-output call). Shared infrastructure issue possible.
- **Startup failure pattern**: CLI Checker and CI Coach had 5 consecutive startup_failures before switching to regular failures — may indicate a config change in the runner or pre-agent setup.
- **Recovery validation**: Daily Rust Testing Expert now has 2 consecutive successes, confirming recovery.
- **Healthy cluster**: Daily Perf Improver, Daily Test Improver, Code Simplifier, Agentic Maintenance — consistently healthy.

## Health Scores (approximate)

- Healthy (≥80): 8 workflows (added Daily Rust Testing Expert back)
- Critical (<60): 3 (Glossary Maintainer, CI Optimization Coach, CLI Consistency Checker)
- Accepted/not_planned: 2 (Duplicate Code Detector, Schema Consistency Checker)
- PR-triggered (no health score): 2 (PR Fix, Q)

## Trend

- ↑ Daily Rust Testing Expert fully recovered (was Warning last run)
- → Glossary Maintainer, CI Coach, CLI Checker: no improvement
- Overall: 8 healthy, 3 critical (same as yesterday minus recovery)
