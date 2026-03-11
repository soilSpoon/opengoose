# Workflow Health Manager - Run 2026-03-11T10:25:36Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22947935511
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): 6

## Compilation Status
- 14/14 lock files present ✅
- All local timestamps identical (fresh checkout, no drift detected)
- Note: Duplicate Code Detector (6/6 failures) and Schema Consistency Checker (3/6 failures) still fail with hash mismatch — #69 was closed as `not_planned` on Mar 10; lock files not recompiled.

## Workflow Run Health

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| CI Optimization Coach | ❌ Critical | 3/3 startup_failure (Mar 6,9,10) | 0% | New issue #aw_cicoach created |
| CI Doctor | ⚠️ Unknown | 0 scheduled | N/A | PR-triggered |
| CLI Consistency Checker | ❌ Critical | 3/4 startup_failure | 25% (dispatch only) | New issue #aw_cicoach |
| Code Simplifier | ✅ Healthy | 5/6 success | 83% | |
| Daily Doc Updater | ✅ Healthy | 5/6 success | 83% | |
| Daily Perf Improver | ✅ Healthy | all skipped (PR-triggered) | N/A | |
| Daily Test Improver | ✅ Healthy | all skipped (PR-triggered) | N/A | |
| Daily Rust Testing Expert | ❌ Critical | 7/7 startup_failure (Mar 5-11) | 0% sched | #70 closed as not_planned (Mar 10) |
| Duplicate Code Detector | ❌ Critical | 6/6 failure (hash mismatch) | 0% | #69 closed as not_planned (Mar 10) |
| Glossary Maintainer | ❌ Critical | 4/4 failure (agent zero output) | 0% | #48 open, comment added today |
| PR Fix | ⚠️ N/A | all skipped | N/A | PR-triggered |
| Q | ⚠️ N/A | all skipped/cancelled | N/A | PR-triggered |
| Schema Consistency Checker | ⚠️ Warning | 3/6 failure (Mar 8-10) | 50% | #69 closed as not_planned |
| Workflow Health Manager | ✅ Healthy | in_progress | N/A | This workflow |

## Critical Issues (P1)
- **P1** CI Optimization Coach — 3 consecutive startup_failure (no scheduled success ever) — new issue created
- **P1** CLI Consistency Checker — 3/4 startup_failure (dispatch worked Mar 5) — same new issue
- **P1** Daily Rust Testing Expert — 7 consecutive startup_failure (Mar 5-11) — #70 closed not_planned, no new issue (respecting maintainer decision)
- **P1** Duplicate Code Detector — 6/6 failure (hash mismatch, lock outdated) — #69 closed not_planned
- **P1** Schema Consistency Checker — 3/6 failure (hash mismatch, same cause as above) — #69 closed not_planned
- **P1** Glossary Maintainer — 4/4 failure (agent zero output) — #48 open, comment added

## Actions This Run
- Added comment to #48 (Glossary Maintainer — 4th consecutive failure, pattern unchanged)
- Created new issue for CI Optimization Coach + CLI Consistency Checker (no prior tracking, P1)

## Systemic Patterns
- **startup_failure cluster**: Daily Rust Testing Expert + CI Coach + CLI Consistency Checker — all fail on schedule with startup_failure; dispatch works. Likely runner quota or scheduler registration issue at specific UTC times.
- **Hash mismatch cluster**: Duplicate Code Detector + Schema Consistency Checker — both need `gh aw compile` after frontmatter changes on Mar 8. #69 closed not_planned, no recompilation occurred.
- **Glossary Maintainer silent failure**: 4th consecutive. Agent fires but produces no safe-output call. Needs agent-stdio.log inspection.

## Health Scores
- Healthy: 4 (Code Simplifier, Daily Doc Updater, Daily Perf Improver*, Daily Test Improver*) = 29%
- Critical: 6 (Daily Rust Testing Expert, Duplicate Code Detector, Schema Consistency Checker, Glossary Maintainer, CI Coach, CLI Consistency Checker) = 43%
- Warning/Unknown: 4 (CI Doctor, PR Fix, Q) = 29%
- Self: Workflow Health Manager = healthy
