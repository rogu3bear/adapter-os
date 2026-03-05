---
phase: 54-performance-and-security-hardening
plan: 01
subsystem: infra
tags: [uma, memory, benchmark, apple-silicon, performance, inference]

# Dependency graph
requires: []
provides:
  - "UmaMemoryConfig with configurable ceiling_pct, headroom_pct, eviction_notifications"
  - "MemoryLimits::from_uma_config factory for config-driven pressure limits"
  - "UmaMemorySection in EffectiveConfig (wired from cp.toml [uma_memory])"
  - "Inference benchmark script measuring TTFT, throughput, peak UMA, with MLX baseline comparison"
affects: [inference, memory-management, operator-tooling]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "UMA memory budget as config-driven MemoryLimits rather than hardcoded constants"
    - "Bash benchmark suite following contract-check script pattern with JSON output"

key-files:
  created:
    - "scripts/benchmarks/inference_benchmark.sh"
  modified:
    - "crates/adapteros-config/src/types.rs"
    - "crates/adapteros-config/src/effective.rs"
    - "crates/adapteros-memory/src/unified_tracker.rs"
    - "configs/cp.toml"

key-decisions:
  - "UmaMemoryConfig placed in types.rs (not MemoryConfig) to avoid collision with adapteros-policy::packs::memory::MemoryConfig"
  - "MemoryLimits::from_uma_config sets both max_vram and max_system_ram to the same effective ceiling since Apple Silicon shares one pool"
  - "Boot warmup already wired via inference_warmup module -- no additional wiring needed"
  - "Benchmark script uses python3 for floating-point math (already a macOS system dependency)"
  - "tests/benchmark/src/throughput_benchmarks.rs stub explicitly deferred -- bash script covers E2E requirement"

patterns-established:
  - "UMA memory budget pattern: config ceiling_pct -> MemoryLimits::from_uma_config -> UnifiedMemoryTracker -> MemoryPressureManager"
  - "Inference benchmark script pattern: measure, compare baseline, warn on regression, output JSON"

requirements-completed: [PERF-54-01, PERF-54-02]

# Metrics
duration: 8min
completed: 2026-03-05
---

# Phase 54 Plan 01: UMA Memory Ceiling and Inference Benchmark Summary

**Configurable UMA memory budget (75% default ceiling, 15% headroom) with E2E inference benchmark measuring TTFT, throughput, peak memory, and MLX baseline overhead**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-05T06:05:35Z
- **Completed:** 2026-03-05T06:14:00Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Added UmaMemoryConfig struct with ceiling_pct (75), headroom_pct (15), eviction_notifications (true)
- Added MemoryLimits::from_uma_config factory method that computes effective limits from total UMA bytes and config
- Created 400-line inference benchmark script with cold TTFT, warm throughput, peak memory, and MLX baseline comparison
- Wired UmaMemorySection into EffectiveConfig with builder from DeterministicConfig

## Task Commits

Each task was committed atomically:

1. **Task 1: UmaMemoryConfig and boot warmup wiring** - `c3d36ff69` (feat)
2. **Task 2: Inference benchmark suite script** - `15179511c` (feat)

## Files Created/Modified
- `crates/adapteros-config/src/types.rs` - UmaMemoryConfig struct with defaults and serde(default)
- `crates/adapteros-config/src/effective.rs` - UmaMemorySection, build_uma_memory_section, field in EffectiveConfig
- `crates/adapteros-memory/src/unified_tracker.rs` - MemoryLimits::from_uma_config factory method
- `configs/cp.toml` - [uma_memory] section with documented defaults
- `scripts/benchmarks/inference_benchmark.sh` - E2E inference benchmark suite

## Decisions Made
- Named struct `UmaMemoryConfig` (not `MemoryConfig`) to avoid collision with adapteros-policy
- Set both max_vram and max_system_ram to the same effective ceiling in from_uma_config since Apple Silicon uses unified memory
- Boot warmup was already wired via `run_startup_inference_warmup` in server boot -- no changes needed
- Benchmark uses python3 for precise floating-point division (guaranteed on macOS)
- Throughput benchmark stub in tests/benchmark/src/throughput_benchmarks.rs explicitly deferred per CONTEXT.md

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed parse_bool return type in effective.rs**
- **Found during:** Task 1 (UmaMemorySection builder)
- **Issue:** `parse_bool()` returns `Result<bool, String>`, not `bool` -- direct `.unwrap_or()` caused type mismatch
- **Fix:** Used `.and_then(|v| parse_bool(v).ok())` to unwrap the Result before defaulting
- **Files modified:** crates/adapteros-config/src/effective.rs
- **Verification:** cargo check -p adapteros-config passes
- **Committed in:** c3d36ff69 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Minor type handling fix. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- UMA memory budget ready for consumption by boot/worker code that constructs MemoryLimits
- Benchmark script ready to run against a live instance (requires server + model)
- Plans 02 and 03 can proceed independently

---
*Phase: 54-performance-and-security-hardening*
*Completed: 2026-03-05*
