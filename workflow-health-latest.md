# Workflow Health Manager - Run 2026-03-16T10:36:10Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/23139383664
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): entries in `.github/workflows/shared/`

## Compilation Status
- 14/14 lock files present ✅
- Known hash mismatch (accepted/not_planned): Duplicate Code Detector, Schema Consistency Checker

## Workflow Run Health (2026-03-16)

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| Agentic Maintenance | ✅ Healthy | 122 runs, recent all success | ~100% | |
| CI Quality Gate | ✅ Healthy | push runs succeed | ~80% | |
| Claude Code Review | ✅ Healthy | on-demand | 100% | |
| Code Simplifier | ✅ Healthy | 11/11 success | 100% | |
| Daily Doc Updater | ✅ Healthy | 11/11 success | 100% | |
| Daily Perf Improver | ✅ Healthy | 1252+ runs, recent success | ~100% | #254 monthly |
| Daily Test Improver | ✅ Healthy | 1211+ runs, recent success | ~100% | #310 monthly |
| Daily Rust Testing Expert | ✅ FULLY RECOVERED | 3 consecutive success (Mar 14/15/16) | 25% (3/12 but trending ✅) | #318 CLOSED ✅ |
| CI Optimization Coach | ❌ Critical | 6/6 failure; last run Mar 13 | 0% | #319 open |
| CLI Consistency Checker | ❌ Critical | 6/7 failure; last run Mar 13 | 14% | #320 open |
| Glossary Maintainer | ❌ Critical | 7/7 failure; new failure today Mar 16 | 0% | #292 open |
| CI Failure Doctor | ✅ Healthy | event-triggered | 100% | |
| Duplicate Code Detector | ⚠️ Accepted | all failure | N/A | not_planned |
| Schema Consistency Checker | ⚠️ Accepted | all failure | N/A | not_planned |
| Workflow Health Manager | ✅ Healthy | 11/11 success | 100% | self |
| PR Fix | ⚠️ PR-triggered | N/A | N/A | |
| Q | ⚠️ PR-triggered | N/A | N/A | |

## Critical Issues

- **P1** Glossary Maintainer — 7/7 consecutive failures since Mar 6 (10 days); new failure today #292 open; commented with escalation
- **P1** CI Optimization Coach — 6/6 consecutive failures; last run Mar 13 (no new run); #319 open
- **P1** CLI Consistency Checker — 6/7 failures; last run Mar 13; #320 open
- **Accepted** Duplicate Code Detector — hash mismatch; maintainer: not_planned
- **Accepted** Schema Consistency Checker — hash mismatch; maintainer: not_planned

## Actions This Run

- ✅ Closed #318 (Daily Rust Testing Expert — 3 consecutive successes Mar 14/15/16 confirmed)
- ✅ Commented on #292 (Glossary Maintainer — 7th consecutive failure, escalated to P1)

## Systemic Patterns

- **Agent safe-output failure cluster**: Glossary Maintainer fails in agent execution phase with no safe-output call. Persistent pattern since Mar 6.
- **CI Coach + CLI Checker**: No new runs since Mar 13 (both at run 6 and 7 respectively). May have stopped scheduling or reached run limits.
- **Recovery validated**: Daily Rust Testing Expert — 3 consecutive successes, fully stable, issue closed.
- **Healthy cluster**: Daily Perf Improver, Daily Test Improver, Code Simplifier, Agentic Maintenance, Daily Doc Updater — consistently healthy.

## Health Scores (approximate)

- Healthy (≥80): 9 workflows (Daily Rust Testing Expert now fully healthy)
- Critical (<60): 3 (Glossary Maintainer, CI Optimization Coach, CLI Consistency Checker)
- Accepted/not_planned: 2 (Duplicate Code Detector, Schema Consistency Checker)
- PR-triggered (no health score): 2 (PR Fix, Q)

## Trend

- ✅ Daily Rust Testing Expert issue #318 CLOSED — 3 consecutive successes
- → Glossary Maintainer, CI Coach, CLI Checker: no improvement, persistent failures
- Overall: 9 healthy (+1), 3 critical (same as yesterday)
