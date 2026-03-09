# Workflow Health Manager - Run 2026-03-09T10:30:13Z

## Summary
- Run: https://github.com/soilSpoon/opengoose/actions/runs/22849122643
- Total agentic workflows: 14 (all with lock files ✅)
- Shared include files (excluded): 6

## Compilation Status
- 14/14 lock files present ✅
- Duplicate Code Detector: lock outdated (md changed Mar 8, lock from Mar 6) ❌
- Schema Consistency Checker: lock outdated (md changed Mar 8, lock from Mar 6) ❌

## Workflow Run Health

| Workflow | Status | Recent Schedule Runs | Success Rate | Tracking |
|----------|--------|---------------------|--------------|---------|
| CI Optimization Coach | ❌ Critical | 1 | 0% (startup_failure) | new issue created |
| CI Doctor | ⚠️ Unknown | 0 recent | N/A | PR-triggered |
| CLI Consistency Checker | ❌ Critical | 1 sched / 1 dispatch | 0% sched | closed #58 |
| Code Simplifier | ✅ Healthy | 3 | 100% | |
| Daily Doc Updater | ✅ Healthy | 3 | 100% | |
| Daily Perf Improver | ✅ RECOVERED | 1 | 100% | #57 closed ✅ |
| Daily Test Improver | ✅ Healthy | 1 sched | 100% | |
| Daily Rust Testing Expert | ❌ Critical | 4 | 0% (startup_failure) | new issue #aw_rust59 |
| Duplicate Code Detector | ❌ Critical | 4 | 0% (hash mismatch) | new issue #aw_hash60 |
| Glossary Maintainer | ❌ Critical | 2 | 0% (agent exec fail) | #48 open, comment added |
| PR Fix | ⚠️ N/A | 0 | N/A | PR-triggered |
| Q | ⚠️ N/A | 0 | N/A | PR-triggered |
| Schema Consistency Checker | ⚠️ Warning | 4 | 50% (hash mismatch Mar 8) | new issue #aw_hash60 |
| Workflow Health Manager | ✅ Healthy | 4+ | 100% | This workflow |

## Critical Issues
- **P1** Daily Rust Testing Expert — startup_failure 4 consecutive days (Mar 6-9) — new issue created
- **P1** Duplicate Code Detector — hash mismatch 4 consecutive failures — new issue created (with schema checker)
- **P1** Glossary Maintainer — agent exec failure 2 runs — #48 open, comment added with analysis
- **P1** Schema Consistency Checker — hash mismatch regression Mar 8 — tracked in same issue as duplicate-code-detector

## Actions This Run
- Added health manager analysis comment to #48 (Glossary Maintainer, 2nd failure)
- Created new issue for Daily Rust Testing Expert (4 consecutive startup_failures, #58 was closed as not_planned)
- Created new issue for Duplicate Code Detector + Schema Consistency Checker (both need `gh aw compile`)
- Daily Perf Improver RECOVERED — issue #57 already closed correctly

## Systemic Patterns
- **Hash mismatch cluster**: Duplicate Code Detector + Schema Consistency Checker both updated on Mar 8 (commit `a3ed0ca`/`5a9bf90`) without recompiling lock files. Same root cause as #57. Fix: run `gh aw compile` after every frontmatter change.
- **startup_failure**: Daily Rust Testing Expert continues failing on schedule (4 days) despite manual workflow_dispatch succeeding. Likely runner quota/scheduling issue at 08:00 UTC.
- **Glossary Maintainer**: Agent exits after 24s with no output — distinct from startup_failure. Needs log inspection.

## Health Scores
- Healthy: 4 (Code Simplifier, Daily Doc Updater, Daily Test Improver, Workflow Health Manager) + 1 recovered (Daily Perf Improver) = 36%
- Critical: 4 (Daily Rust Testing Expert, Duplicate Code Detector, Glossary Maintainer, Schema Consistency Checker) = 29%
- Warning/Unknown: 4 (CI Coach, CI Doctor, CLI Consistency Checker, PR Fix, Q) = 36%
