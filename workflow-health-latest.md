# Workflow Health Manager - Run 2026-03-10T10:26:13Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22897958877
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): 6

## Compilation Status
- 14/14 lock files present ✅
- All timestamps OK (no file modified after lock in working copy)
- Note: Duplicate Code Detector and Schema Consistency Checker lock files are outdated per GitHub API (source commits newer than lock commits). These must be fixed via `gh aw compile`.

## Workflow Run Health

| Workflow | Status | Sched Runs (Total) | Success Rate | Tracking |
|----------|--------|--------------------|--------------|---------|
| CI Optimization Coach | ❌ Critical | 2 | 0% (startup_failure) | no issue yet |
| CI Doctor | ⚠️ Unknown | 0 recent | N/A | PR-triggered |
| CLI Consistency Checker | ❌ Critical | 2/3 sched | 33% (sched 0%) | no issue yet |
| Code Simplifier | ✅ Healthy | 5 | 80% (1 early failure) | |
| Daily Doc Updater | ✅ Healthy | 5 | 80% (1 early failure) | |
| Daily Perf Improver | ✅ Healthy | 0 sched, all skipped | N/A (PR-triggered) | |
| Daily Test Improver | ✅ Healthy | 0 sched, all skipped | N/A (PR-triggered) | |
| Daily Rust Testing Expert | ❌ Critical | 5 sched / 1 dispatch | 0% sched (startup_failure 6 days) | #70 open, comment added |
| Duplicate Code Detector | ❌ Critical | 5 | 0% (hash mismatch) | #69 open, comment added |
| Glossary Maintainer | ❌ Critical | 3 | 0% (agent no output) | #48 open, comment added |
| PR Fix | ⚠️ N/A | 0 | N/A | PR-triggered |
| Q | ⚠️ N/A | 0 | N/A | PR-triggered |
| Schema Consistency Checker | ❌ Critical | 5 | 60% (hash mismatch last 2) | #69 open, comment added |
| Workflow Health Manager | ✅ Healthy | 5 | in_progress (this run) | This workflow |

## Critical Issues (P1)
- **P1** Daily Rust Testing Expert — startup_failure 6 consecutive scheduled days (Mar 5-10) — #70 open, comment added
- **P1** Duplicate Code Detector — hash mismatch 5/5 runs since Mar 5 (lock outdated since Mar 8 commit) — #69 open, comment added
- **P1** Schema Consistency Checker — hash mismatch 2 recent runs (Mar 8-9) — same issue #69, comment added
- **P1** Glossary Maintainer — agent produces zero output 3/3 runs (Mar 6, 9, 10) — #48 open, comment added
- **P2** CI Coach — startup_failure 2/2 scheduled runs (Mar 6, 9) — no issue yet
- **P2** CLI Consistency Checker — startup_failure 2/3 scheduled runs (Mar 6, 9) — no issue yet

## Actions This Run
- Added comment to #48 (Glossary Maintainer — 3rd consecutive failure, agent zero output analysis)
- Added comment to #70 (Daily Rust Testing Expert — 6th consecutive scheduled startup_failure)
- Added comment to #69 (Duplicate Code Detector + Schema Consistency Checker — still unresolved)

## Systemic Patterns
- **Hash mismatch cluster**: Duplicate Code Detector + Schema Consistency Checker both updated on Mar 8 without recompiling lock files. Fix: run `gh aw compile` after every frontmatter change.
- **startup_failure cluster**: Daily Rust Testing Expert + CI Coach + CLI Consistency Checker all fail on schedule with startup_failure. Dispatch works (at least for Rust Expert). Likely runner quota/scheduling issue at specific UTC times.
- **Glossary Maintainer silent failure**: Agent fires 1 LLM call, then exits with 0 outputs. Distinct from startup_failure. Agent-stdio.log needed to diagnose.

## Health Scores
- Healthy: 4 (Code Simplifier, Daily Doc Updater, Daily Perf Improver*, Daily Test Improver*) = 29%
- Critical: 5 (Daily Rust Testing Expert, Duplicate Code Detector, Schema Consistency Checker, Glossary Maintainer, CI Coach) = 36%
- Warning/Unknown: 4 (CLI Consistency Checker, CI Doctor, PR Fix, Q) = 29%
- Self: Workflow Health Manager = 1 (healthy)
