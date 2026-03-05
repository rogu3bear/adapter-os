# Session State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-04)

**Core value:** Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable.
**Current focus:** System stabilization — fix training worker spawn, clean stale runtime state, commit dirty tree, activate adapter inference end-to-end.

## Position

**Milestone:** v1.1.18 System Stabilization (active)
**Current phase:** Phase 54 — Performance and Security Hardening
**Current Plan:** Not started
**Status:** Ready to plan

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
- 2026-03-05: Phase 52 plan 01 complete: portable path resolution — find_project_root with .adapteros-root marker, AOS_ROOT override, fixed DEV_MODEL_PATH absolute path bug, layered model discovery.
- 2026-03-05: Phase 53 plan 02 complete: chat and dashboard polish — removed 8 redundant/dead elements, 7 interaction fixes, focus-visible states, glass tier 3 overlay.
- 2026-03-05: Phase 53 plan 03 complete: secondary surface polish — sidebar glass T2, skeleton loading on 5 pages, EmptyState on 6 views.
- 2026-03-05: Phase 54 plan 01 complete: UMA memory ceiling (75% default, 15% headroom) + inference benchmark script (TTFT/throughput/peak memory/MLX baseline comparison).
- 2026-03-05: Phase 52 plan 02 complete: bootstrap.sh idempotent dependency installer + .adapteros-root project root marker.
- 2026-03-05: Phase 52 plan 03 complete: start script first-run detection, pre-flight dep checks, model-missing fail-fast, migration count logging, zero-touch config defaults validated.
- 2026-03-05: Phase 52 complete: Full Portability — all 3 plans delivered (path resolution, bootstrap, fresh-clone experience).

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
- AOS_ROOT env var takes absolute priority over marker walk for project root detection
- Marker check order: .adapteros-root > Cargo.lock > .git (most specific first)
- Layered model discovery: AOS_MODEL_PATH > var/models/{id} > ~/.cache/adapteros/models/{id}
- Remove header target selector, keep Context drawer as canonical config location
- Contextual controls pattern: hide controls until relevant state exists (Create Adapter hidden until selection > 0)
- Glass tier 3 for overlays via inline style with var(--glass-bg-3) + backdrop-filter
- Sidebar uses Tier 2 glass (12px blur) to match navigation surface spec
- SkeletonTable/SkeletonCard for all loading states; EmptyState component for all empty data views
- First-run detection uses var/ existence (not separate marker) — warn before ensure_var_dirs creates it
- check_build_deps exits hard on failure — fail fast over confusing cargo errors
- Migration count filters out down migrations to show forward schema count only
- require_manifest=false is the portability-safe loader option for zero-config boot
- UmaMemoryConfig named to avoid collision with adapteros-policy::packs::memory::MemoryConfig
- MemoryLimits::from_uma_config sets both max_vram and max_system_ram to same effective ceiling (unified memory)
- Boot warmup already wired via inference_warmup module -- no additional plumbing needed

## Session

**Last Date:** 2026-03-05T06:14:00Z
**Stopped At:** Completed 54-01-PLAN.md
**Resume File:** None
