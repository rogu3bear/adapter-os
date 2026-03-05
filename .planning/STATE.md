# Session State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-04)

**Core value:** Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable.
**Current focus:** System stabilization — fix training worker spawn, clean stale runtime state, commit dirty tree.

## Position

**Milestone:** v1.1.18 System Stabilization (active)
**Current phase:** Phase 49 - Training Worker Spawn Fix (complete)
**Status:** Phase 49 complete (2 plans executed)

## Session Log

- 2026-03-04: Completed v1.1.17 Production Cut Closure (phase 47).
- 2026-03-04: Documentation architecture audit — fixed drift in CLAUDE.md (crate table 28->85, migration path, MLX filenames, Leptos API, middleware chain, env vars), ROADMAP.md, PROJECT.md, STATE.md.
- 2026-03-04: Initialized v1.1.18 System Stabilization milestone targeting training worker spawn fix, stale runtime state cleanup, and dirty tree commit.
- 2026-03-05: Completed Phase 49 (Training Worker Spawn Fix) — binary resolution fix, preflight boot gate, circuit breaker, crash job cleanup.

## Decisions

- Training worker binary resolution uses 5-tier priority: env > config > sibling exe > workspace target > Err (never bare PATH lookup)
- Circuit breaker: 3 crashes in 5 minutes = permanently degraded, stops restart attempts
- Graceful worker exits (exit code 0) do not count as crashes for circuit breaker
- In-flight training jobs marked failed on any worker exit, not just crashes

## Session

**Last Date:** 2026-03-05T04:23:46Z
**Stopped At:** Completed Phase 49 (49-01 and 49-02)
**Resume File:** .planning/phases/49-training-worker-spawn-fix/49-02-SUMMARY.md
