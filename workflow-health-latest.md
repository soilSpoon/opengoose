# Workflow Health Manager - Run 2026-03-12T10:26:05Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22997349031
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): 6

## Compilation Status
- 14/14 lock files present ✅
- No timestamp drift (fresh checkout)
- Duplicate Code Detector and Schema Consistency Checker still have hash mismatch (#69 closed not_planned — lock files not recompiled by maintainer's decision)

## Workflow Run Health

| Workflow | Status | Recent Runs | Success Rate | Tracking |
|----------|--------|-------------|--------------|---------|
| CI Optimization Coach | ❌ Critical | 4/4 startup_failure (Mar 6,9,10,11) | 0% | #58 closed not_planned, #155 closed completed; new issue created this run |
| CI Doctor | ⚠️ Unknown | 0 scheduled | N/A | PR-triggered |
| CLI Consistency Checker | ❌ Critical | 4/5 startup_failure | 20% (dispatch only) | Same as CI Coach; new issue created this run |
| Code Simplifier | ✅ Healthy | 6/7 success | 86% | |
| Daily Doc Updater | ✅ Healthy | 6/7 success | 86% | |
| Daily Perf Improver | ✅ Healthy | all skipped (PR-triggered) | N/A | |
| Daily Test Improver | ✅ Healthy | all skipped (PR-triggered) | N/A | |
| Daily Rust Testing Expert | ❌ Critical | 8/8 startup_failure (Mar 5-12) | 0% sched | #70 closed not_planned (Mar 10) — respect maintainer decision |
| Duplicate Code Detector | ❌ Critical | 7/7 failure (hash mismatch) | 0% | #69 closed not_planned (Mar 10) — respect maintainer decision |
| Glossary Maintainer | ❌ Critical | 5/5 failure (agent zero output) | 0% | #48 closed completed (Mar 12), #292 auto-created today — comment added this run |
| PR Fix | ⚠️ N/A | all skipped | N/A | PR-triggered |
| Q | ⚠️ N/A | all skipped/cancelled | N/A | PR-triggered |
| Schema Consistency Checker | ⚠️ Warning | 5/7 failure (Mar 8-11) | 29% | #69 closed not_planned — respect |
| Workflow Health Manager | ✅ Healthy | 6/6 success (runs 1-6) | 100% | This workflow |

## Critical Issues
- **P1** Glossary Maintainer — 5th consecutive failure; #48 closed by soilSpoon but not fixed; comment added to auto-created #292
- **P1** CI Optimization Coach — 4/4 startup_failure since creation; #155 closed "completed" by soilSpoon but still failing; new issue #aw_coach1 created
- **P1** CLI Consistency Checker — 4/5 startup_failure; same as CI Coach; tracked in same new issue
- **P1** Daily Rust Testing Expert — 8/8 startup_failure; #70 closed not_planned — not re-reporting
- **P1** Duplicate Code Detector — 7/7 failure; #69 closed not_planned — not re-reporting
- **⚠️** Schema Consistency Checker — 5/7 failure; #69 closed not_planned — not re-reporting

## Actions This Run
- Added comment to #292 (Glossary Maintainer — 5th failure, pattern context, #48 was closed without fix)
- Created new issue for CI Optimization Coach + CLI Consistency Checker (3rd recurrence: #58, #155 both closed, failures continue)

## Systemic Patterns
- **startup_failure cluster**: Daily Rust Testing Expert + CI Coach + CLI Consistency Checker — all fail on schedule with startup_failure; dispatch works. Likely runner quota or scheduler registration issue at specific UTC times.
- **Hash mismatch cluster**: Duplicate Code Detector + Schema Consistency Checker — maintainer decided not to fix (not_planned)
- **Glossary Maintainer silent failure**: 5th consecutive. Agent fires but produces no safe-output call.

## Maintainer Closure Pattern
- soilSpoon closed 4 workflow issues on Mar 12 as completed/not_planned without observable fixes
- #48 (Glossary Maintainer) — closed "completed", failed again same day
- #155 (CI Coach + CLI Checker) — closed "completed", failures were already known on Mar 11

## Health Scores
- Healthy: 4 workflows (29%): Code Simplifier, Daily Doc Updater, Daily Perf Improver*, Daily Test Improver*
- Critical: 5 (36%): Daily Rust Testing Expert, Duplicate Code Detector, Glossary Maintainer, CI Coach, CLI Consistency Checker
- Warning: 2 (14%): Schema Consistency Checker
- Unknown/PR-only: 4 (29%): CI Doctor, PR Fix, Q, Workflow Health Manager (self=healthy)

*PR-triggered, skipped in recent windows — not truly failing
