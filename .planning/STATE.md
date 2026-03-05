# Session State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-04)

**Core value:** Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable.
**Current focus:** System stabilization — fix training worker spawn, clean stale runtime state, commit dirty tree, activate adapter inference end-to-end.

## Position

**Milestone:** v1.1.18 System Stabilization (active)
**Current phase:** Phase 53 — UI Harmony and Visual Polish
**Current Plan:** 1 of 3 in Phase
**Status:** Executing

## Session Log

- 2026-03-04: Completed v1.1.17 Production Cut Closure (phase 47).
- 2026-03-04: Documentation architecture audit — fixed drift in CLAUDE.md (crate table 28->85, migration path, MLX filenames, Leptos API, middleware chain, env vars), ROADMAP.md, PROJECT.md, STATE.md.
- 2026-03-04: Initialized v1.1.18 System Stabilization milestone targeting training worker spawn fix, stale runtime state cleanup, and dirty tree commit.
- 2026-03-04: Phase 51 added: Adapter Inference End-to-End Activation — hot-swap stability, adapter influence, training round-trip.
- 2026-03-04: Phase 52 added: Full Portability — cross-platform builds, relocatable paths, environment-independent config.
- 2026-03-04: Phase 53 added: UI Harmony and Visual Polish — strip bloat, Liquid Glass consistency, Apple-themed minimalism.
- 2026-03-04: Phase 54 added: Performance and Security Hardening — speed optimization, memory budget, attack surface hardening.
- 2026-03-05: Phase 49 complete: binary resolution fix (5-tier priority, never bare PATH), preflight boot gate, circuit breaker (3/5min), crash job cleanup.
- 2026-03-05: Phase 50 complete: runtime_cleanup module (stale socket probing, marker removal) + supervision_state module (JSON format, crash-vs-rebuild detection) + shell script migration.
- 2026-03-05: Phase 53 plan 01 complete: design token foundation — system font stack, duration tokens, transition standardization (87 replacements), flat surface policy, CSS lint.

## Decisions

- System font stack (-apple-system/SF Pro) over bundled web fonts — saves ~80KB, renders native
- Three-tier duration tokens (fast/normal/slow) for all transitions — single point of change
- Flat surface policy: cards/tables use border only, no box-shadow
- Sidebar separation via background contrast, not border-right
- Socket liveness check uses UnixStream::connect — simple, no external dependencies
- Supervision state placed after logging init so rebuild detection is logged
- Legacy key=value format migrated gracefully to JSON with backward compat
- Used `mlx` formula instead of deprecated `ml-explore/mlx/mlx` tap in bootstrap.sh
- Simple for-loop arg parsing in bootstrap.sh -- only two flags, no need for getopts

## Session

**Last Date:** 2026-03-05T05:49:01Z
**Stopped At:** Completed 53-01-PLAN.md (design token foundation)
**Resume File:** .planning/phases/53-ui-harmony-and-visual-polish/53-02-PLAN.md
