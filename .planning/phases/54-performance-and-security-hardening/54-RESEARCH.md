# Phase 54: Performance and Security Hardening - Research

**Researched:** 2026-03-04
**Domain:** Inference performance optimization, UMA memory management, API security hardening (Rust/Axum/MLX/Apple Silicon)
**Confidence:** HIGH

## Summary

Phase 54 is a pure hardening phase -- no new features, only speed and safety improvements on existing surfaces. The codebase already has substantial infrastructure for memory management (`adapteros-memory` with tiered manager, pressure manager, LRU model cache, unified tracker, watchdog), rate limiting (token-bucket per-tenant + tower-http concurrency), auth (JWT/API key/cookie with dev-bypass gating), and security headers (CSP, HSTS, X-Frame-Options). The work is about tuning, closing gaps, and adding measurable verification.

The four requirements split cleanly into two domains: **performance** (PERF-54-01 inference latency, PERF-54-02 memory budget) and **security** (SEC-54-01 endpoint audit, SEC-54-02 secret protection). Both domains can be planned as independent work streams since they share no file contention beyond config.

**Primary recommendation:** Build a reproducible benchmark script (like existing contract checks) to measure TTFT/throughput/peak memory before and after optimizations. For security, create a contract check script that scans for secret leaks, validates auth enforcement on all route tiers, and runs input validation fuzzing. Both become CI-runnable regression tests.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- TTFT under 500ms on cold adapter load (aggressive target)
- Warm adapter throughput must match raw MLX baseline -- orchestration/routing adds zero meaningful overhead
- Reproducible benchmark suite as deliverable: TTFT, tok/s, memory peak (runnable script, not just test assertions)
- Hard UMA ceiling with LRU eviction -- never OOM, evict least-recently-used adapters when approaching limit
- Default ceiling is configurable (sensible default like 75% UMA), operators tune per-machine via config
- Toast notification when adapter is evicted
- Evicted adapters reload transparently on next use
- All attack surfaces audited at equal depth: auth/access control, input validation/injection, secret exposure
- Rate limiting tuned per route group tier (health/public/internal/protected)
- Formal security audit report as deliverable (publishable artifact)
- Dependency vulnerability scanning included (cargo-audit, advisory review)
- Fail closed on all auth ambiguity in production; AOS_DEV_NO_AUTH=1 preserved for dev
- Structured security audit trail: failed auth, rate limit hits, suspicious input logged
- Model weight protection: OS-level file permissions on var/models/ plus API auth
- CI security smoke tests: secrets in logs, auth enforcement, input validation fuzzing

### Claude's Discretion
- Inference concurrency model (serialize vs parallel) -- based on UMA constraints and MLX capabilities
- Adapter hot/cold tiering strategy -- based on what architecture supports
- Exact UMA ceiling default percentage
- Specific optimizations to hit TTFT target (preloading, caching, lazy init)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PERF-54-01 | Inference latency meets or beats comparable local LoRA tools (TTFT, throughput) | Benchmark script infrastructure, MLX FFI optimization patterns, adapter preloading pipeline, KV cache warmup |
| PERF-54-02 | Memory stays within UMA budget -- no OOM on 16GB with reasonable adapter counts | Existing `MemoryPressureManager` + `UnifiedMemoryTracker` + `ModelCache` LRU; needs configurable UMA ceiling and eviction notification |
| SEC-54-01 | All API endpoints pass security audit: auth, input validation, rate limiting, no injection | Existing middleware chain, rate limiting (3 implementations), security headers, route tier architecture; needs audit script and per-tier rate tuning |
| SEC-54-02 | Secrets never logged, exposed in errors, or accessible without auth | Existing `is_dev_bypass_active()` guards, security validation tests; needs systematic log scanning and model weight file permissions |
</phase_requirements>

## Standard Stack

### Core (Already in Codebase)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `adapteros-memory` | internal | UMA tracking, pressure management, LRU cache, tiered memory | Already built: `MemoryPressureManager`, `UnifiedMemoryTracker`, `ModelCache`, `TieredMemoryManager` |
| `adapteros-auth` | internal | JWT/API key/cookie auth with dev-bypass | Already built: `AuthMode`, `AuthConfig`, `AuthState`, compile-time release guards |
| `adapteros-server-api` (rate_limit) | internal | Token-bucket + sliding window rate limiting | Already built: `RateLimiterState` (dashmap-based), `check_rate_limit` (SQLite-based), tower-http layer |
| `adapteros-lora-worker` | internal | Inference pipeline, adapter hot-swap, cache warmup | Already built: `InferencePipeline`, `AdapterHotSwap`, `CacheWarmupManager`, `UmaPressureMonitor` |
| `lru` | crate | LRU cache for model eviction | Already used in `ModelCache` |
| `dashmap` | crate | Concurrent hash map for rate limiter state | Already used in `RateLimiterState` |
| `parking_lot` | crate | Fast rwlocks for memory tracking | Already used throughout memory crate |
| `tower-http` | crate | HTTP middleware (body limit, request limit, CORS) | Already used in middleware chain |

### Supporting (May Need)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `cargo-audit` | CLI tool | Dependency vulnerability scanning | Already in `scripts/security_audit.sh`, needs CI integration |
| `sysinfo` | crate | Cross-platform system memory info | If `sysctl hw.memsize` approach needs portability |
| `criterion` | crate | Micro-benchmarks for hot paths | Only if existing benchmark harness insufficient |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Custom benchmark script | `criterion` benches | Script is better here: needs end-to-end TTFT, not micro-benchmarks. Criterion doesn't support inference pipeline timing well |
| SQLite rate limiting | In-memory only (dashmap) | Both exist; keep dashmap for hot path, SQLite for persistence/admin. No change needed |
| Manual log scanning for secrets | `tracing-subscriber` filter | Manual scan catches what filters miss. Both approaches complement each other |

**Installation:**
```bash
# No new dependencies needed. Existing tooling sufficient.
cargo install cargo-audit  # if not already installed (scripts/security_audit.sh handles this)
```

## Architecture Patterns

### Existing Project Structure (Relevant Crates)
```
crates/
├── adapteros-memory/           # UMA tracking, pressure, eviction, tiered manager
│   ├── src/pressure_manager.rs # LRU eviction with pinned adapters, K-reduction
│   ├── src/model_cache.rs      # LRU model cache with eviction scoring
│   ├── src/unified_tracker.rs  # Multi-backend memory accounting
│   └── src/tiered_manager.rs   # Hot/warm/cold tier migration
├── adapteros-auth/             # JWT/API key/cookie, dev-bypass gating
├── adapteros-server-api/       # Axum handlers, middleware chain, rate limiting
│   ├── src/security/rate_limiting.rs  # SQLite sliding-window per-tenant
│   ├── src/rate_limit.rs              # Token-bucket with injected clock
│   ├── src/http/ratelimit.rs          # tower-http concurrency layer
│   ├── src/middleware_security.rs     # Security headers, CSP, HSTS
│   └── src/backpressure.rs            # UMA backpressure guard
├── adapteros-lora-worker/      # Inference pipeline, hot-swap, cache warmup
│   ├── src/inference_pipeline.rs      # Full inference with routing
│   ├── src/adapter_hotswap.rs         # Two-phase swap with rollback
│   ├── src/cache_warmup.rs            # Pre-warm adapter caches
│   ├── src/memory.rs                  # UmaPressureMonitor (polling)
│   └── src/inference_metrics.rs       # TTFT, tok/s, latency percentiles
├── adapteros-config/           # Config types including RateLimitsConfig
│   └── src/types.rs            # ServerConfig, SecurityConfig, RateLimitsConfig
└── adapteros-lora-mlx-ffi/     # MLX C++ FFI bridge
    └── src/memory_management.rs # MLX memory tracking
```

### Pattern 1: Configurable UMA Ceiling with LRU Eviction
**What:** Add a `memory_ceiling_pct` config field that defaults to 75%, reads total UMA via `sysctl hw.memsize`, computes ceiling bytes, and passes to `MemoryPressureManager`.
**When to use:** At boot, when initializing the memory subsystem.
**Existing foundation:**
- `MemoryLimits::new(max_vram, max_system_ram, headroom_pct)` already parameterizes limits
- `MemoryPressureManager` already handles LRU eviction with pinned adapters
- `UmaPressureMonitor` already polls memory and caches pressure level
- `ModelCache` already does LRU eviction with configurable `max_memory_bytes`

**Gap:** No single config knob ties total UMA to a ceiling percentage. The pieces exist but aren't wired to a user-facing config.

### Pattern 2: Benchmark Script (Contract Check Pattern)
**What:** A bash script in `scripts/benchmarks/` that runs TTFT/throughput/memory measurements and outputs JSON results. Follows the existing `scripts/contracts/check_*.sh` pattern.
**When to use:** Pre/post optimization, CI regression detection.
**Existing foundation:**
- `tests/benchmark/` already has a benchmark harness (though `throughput_benchmarks.rs` is a stub)
- `InferenceMetrics` tracks tokens_per_second, latency percentiles
- `MemoryManagementStats` tracks peak memory
- `CacheWarmupManager` already has warmup-and-measure logic (`HealthCheckResult` with latency_ms, tokens_per_second)

### Pattern 3: Per-Tier Rate Limiting
**What:** Different rate limits for health/public/internal/protected route tiers.
**When to use:** At middleware configuration in route builder.
**Existing foundation:**
- Route tiers are already defined in `routes/mod.rs` (health, public, internal, protected)
- `RateLimiterConfig` has `requests_per_minute` and `burst_size` but is single-valued
- `RateLimitConfig` (tower-http) also single-valued at 300 rpm
- `RATE_LIMIT_EXEMPT_PATHS` exists for bypass

**Gap:** Rate limits are uniform across all tiers. Need tier-specific configs.

### Pattern 4: Security Audit Contract Check
**What:** A bash script in `scripts/contracts/` that checks auth enforcement, secret exposure, input validation.
**When to use:** CI, pre-release gate.
**Existing foundation:**
- `scripts/contracts/check_release_security_assertions.sh` already checks dev bypass flags
- `scripts/security_audit.sh` already runs cargo-audit + SBOM generation
- `tests/security/` has access control, audit trail, isolation tests

### Anti-Patterns to Avoid
- **Custom OOM killer:** Never implement custom process killing. Use the existing LRU eviction in `MemoryPressureManager` -- it already handles pinned adapters and cross-backend eviction ordering.
- **Blocking memory checks in hot path:** The existing `UmaPressureMonitor` polls every 5s and caches the result. Never call `sysctl` synchronously during inference.
- **Multiple rate limit implementations:** Three already exist (SQLite, dashmap token-bucket, tower-http). Don't add a fourth. Wire the per-tier config into the existing `RateLimiterConfig`.
- **Secret grep at runtime:** Secret scanning should be CI-time (grep source for patterns), not runtime. Runtime uses `tracing` redaction.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| UMA ceiling detection | Custom memory reader | `sysctl hw.memsize` (macOS) or `sysinfo` crate | OS provides this; parsing `/proc/meminfo` is error-prone and unnecessary on macOS |
| LRU eviction | Custom eviction logic | `adapteros-memory::MemoryPressureManager` | Already built with pinned adapters, cross-backend ordering, K-reduction protocol |
| Token-bucket rate limiting | Custom implementation | `adapteros-server-api::rate_limit::RateLimiterState` | Already has injected clock, fail-closed, TTL eviction, DashMap concurrency |
| Security headers | Manual header injection | `middleware_security.rs` | Already has CSP, HSTS, X-Frame-Options, Permissions-Policy, cache control |
| Dependency scanning | Custom vulnerability checker | `cargo-audit` via `scripts/security_audit.sh` | Industry standard, advisory database maintained by RustSec |
| Inference metrics | Custom timing | `InferenceMetrics` in `inference_metrics.rs` | Already tracks TTFT, tok/s, p50/p95/p99, adapter selections |

**Key insight:** This phase is about wiring existing components together and filling gaps, not building new subsystems. The memory management, auth, rate limiting, and metrics infrastructure is already comprehensive.

## Common Pitfalls

### Pitfall 1: UMA Ceiling Races with MLX
**What goes wrong:** Setting a UMA ceiling in Rust while MLX allocates memory via its own C++ runtime leads to accounting disagreements. Our tracker says 8GB used but MLX has allocated 10GB.
**Why it happens:** MLX manages its own memory pool (unified memory, no explicit GPU transfers). Our `MemoryTracker` in `memory_management.rs` tracks what it observes, but MLX allocations bypass Rust.
**How to avoid:** Use `mlx_metal_get_active_memory()` and `mlx_metal_get_peak_memory()` FFI calls (from MLX's C API) as the ground truth, not our own accounting. The ceiling check should query MLX directly.
**Warning signs:** Memory reported by our tracker diverges from `vm_stat` output.

### Pitfall 2: Benchmark Flakiness on Shared Machines
**What goes wrong:** TTFT benchmarks are noisy because macOS schedules other processes on the same UMA, thermal throttling kicks in, or the first run warms the model.
**Why it happens:** Apple Silicon thermal management dynamically adjusts clock speeds. Spotlight indexing, iCloud sync, etc. compete for resources.
**How to avoid:** Run 3+ iterations, report median not mean. Include a warmup phase. Document "quiet machine" requirements. Compare against baseline, not absolute numbers.
**Warning signs:** >20% variance between runs.

### Pitfall 3: Rate Limit Bypass via Missing Middleware
**What goes wrong:** A new endpoint is added to the `protected` tier but the rate limiting middleware wasn't in its middleware chain, allowing unlimited requests.
**Why it happens:** Axum's middleware is applied per-route-group. If a route is added to the wrong group or a middleware layer is omitted, it silently bypasses protection.
**How to avoid:** The security audit contract check should verify that ALL routes have the expected middleware chain. Use `check_api_route_tiers.py` (already exists) and extend it.
**Warning signs:** `check_middleware_chain.py` (already exists) reports gaps.

### Pitfall 4: Secret Leakage via Error Messages
**What goes wrong:** An auth error includes the JWT secret in the error detail, or a database error leaks the connection string.
**Why it happens:** `AosError::from(sqlx::Error)` or `format!("{:?}", config)` includes the full config struct with secrets.
**How to avoid:** Implement `Display` for config types that redacts sensitive fields. Use `#[serde(skip)]` or custom Debug implementations. The CI secret scanner should grep for patterns like `jwt_secret`, `api_key` in error message strings.
**Warning signs:** `cargo test` output includes real secret values.

### Pitfall 5: Eviction Toast Without UI Coordination
**What goes wrong:** The backend evicts an adapter and emits an SSE event, but the UI doesn't handle it, so the user sees no notification.
**Why it happens:** SSE event contract between server and UI must be kept in sync (documented in CLAUDE.md). New event types need `#[serde(default)]` for backward compatibility.
**How to avoid:** Define the eviction event type in `adapteros-transport-types`, implement handler in UI `signals/chat.rs`, use `<Show>` for the toast component.
**Warning signs:** Backend logs "adapter evicted" but UI shows nothing.

## Code Examples

### Existing: Check Memory Pressure (from backpressure.rs)
```rust
// Source: crates/adapteros-server-api/src/backpressure.rs
pub fn check_uma_backpressure(state: &AppState) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let pressure = state.uma_monitor.get_current_pressure();
    if matches!(pressure, MemoryPressureLevel::High | MemoryPressureLevel::Critical) {
        let err = UmaBackpressureError::new(pressure.to_string());
        return Err((StatusCode::SERVICE_UNAVAILABLE, Json(err.into())));
    }
    Ok(())
}
```

### Existing: LRU Eviction with Pinned Adapters (from pressure_manager.rs)
```rust
// Source: crates/adapteros-memory/src/pressure_manager.rs
fn evict_low_priority(&self, target_bytes: u64) -> Result<MemoryPressureReport> {
    let pinned: Vec<u32> = self.pinned_adapters.read().iter().copied().collect();
    let candidates = self.tracker.get_eviction_candidates(&pinned);
    // ... evicts unpinned adapters LRU-first until target_bytes freed
}
```

### Existing: Rate Limiter Config (from rate_limit.rs)
```rust
// Source: crates/adapteros-server-api/src/rate_limit.rs
pub struct RateLimiterConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub bucket_ttl_secs: u64,
    pub max_buckets: usize,
    pub fail_closed: bool,
}
```

### Pattern: Per-Tier Rate Config (new, extends existing)
```rust
// Extends crates/adapteros-config/src/types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsConfig {
    pub requests_per_minute: u32,       // default for protected tier
    pub burst_size: u32,
    pub inference_per_minute: u32,
    // New: per-tier overrides
    pub health_rpm: Option<u32>,        // None = unlimited (health checks)
    pub public_rpm: Option<u32>,        // e.g., 600 for login/status
    pub internal_rpm: Option<u32>,      // e.g., 1000 for worker heartbeats
    pub protected_rpm: Option<u32>,     // e.g., 300 for all protected routes
}
```

### Pattern: UMA Ceiling Config (new)
```rust
// New field in config types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum percentage of total UMA to use (default: 75)
    #[serde(default = "default_memory_ceiling_pct")]
    pub ceiling_pct: u8,
    /// Headroom percentage within the ceiling (default: 15)
    #[serde(default = "default_headroom_pct")]
    pub headroom_pct: u8,
    /// Enable eviction notifications (default: true)
    #[serde(default = "default_true")]
    pub eviction_notifications: bool,
}
fn default_memory_ceiling_pct() -> u8 { 75 }
fn default_headroom_pct() -> u8 { 15 }
```

### Pattern: Benchmark Script Output
```bash
# scripts/benchmarks/inference_benchmark.sh (follows contract check pattern)
#!/usr/bin/env bash
set -euo pipefail

echo "=== adapterOS Inference Benchmark Suite ==="
echo "Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "Machine: $(sysctl -n hw.model)"
echo "UMA: $(( $(sysctl -n hw.memsize) / 1024 / 1024 / 1024 ))GB"

# Run TTFT test (cold adapter load)
# Run throughput test (warm adapter, sustained generation)
# Run memory peak test (load N adapters, measure peak)
# Output JSON: { ttft_ms, tokens_per_sec, peak_memory_mb, adapter_count }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fixed memory limits | Configurable UMA ceiling % | Phase 54 | Operators tune per-machine |
| Uniform rate limits | Per-tier rate limits | Phase 54 | Health checks not throttled, inference path tuned separately |
| Manual security review | Automated contract checks | Partial (Phase 47) | CI prevents regression |
| No benchmark suite | Reproducible benchmark script | Phase 54 | Proves optimizations, catches regressions |

**Deprecated/outdated:**
- The `check_release_security_assertions.sh` is a good start but only checks dev bypass flags. Phase 54 expands to comprehensive audit.
- The `scripts/security_audit.sh` does cargo-audit + SBOM but doesn't check runtime security (auth enforcement, secret exposure).

## Claude's Discretion Recommendations

### Inference Concurrency: Serialize
**Recommendation:** Serialize inference requests per-worker. MLX on Apple Silicon operates on unified memory where GPU and CPU share the same physical memory. Running concurrent inference requests would cause memory contention on the same UMA pool, reducing throughput rather than improving it. The existing `UmaPressureMonitor` + backpressure guard enforces this pattern.
**Confidence:** HIGH -- this matches MLX's design philosophy (single-stream execution on unified memory).

### Adapter Hot/Cold Tiering: Use Existing TieredMemoryManager
**Recommendation:** Wire the existing `TieredMemoryManager` (Hot=GPU active, Warm=unified, Cold=evicted to disk). The infrastructure is fully built (`tiered_manager.rs`) with automatic migration based on access patterns and idle timeout. Configure: Hot = currently-used adapter, Warm = recently-used (last 60s), Cold = evicted (reloads transparently).
**Confidence:** HIGH -- the code exists, just needs integration with the adapter lifecycle.

### UMA Ceiling Default: 75%
**Recommendation:** Default `ceiling_pct = 75`. On a 16GB machine, this leaves 4GB for macOS + apps. On 48GB (M4 Max), this allows 36GB for adapters/models. The existing `MemoryWatchdogConfig` uses 85% warning / 95% critical thresholds, so 75% ceiling leaves headroom before warnings.
**Confidence:** MEDIUM -- depends on typical adapter sizes and OS memory pressure. 75% is conservative; 80% might work on machines dedicated to AdapterOS.

### TTFT Optimization Strategy
**Recommendation:** Three optimizations to hit <500ms cold TTFT:
1. **Adapter preloading:** `preload_adapters_for_inference()` already exists in `streaming_infer.rs`. Ensure it runs before SSE stream starts (currently does).
2. **Model cache warming at boot:** `CacheWarmupManager` exists. Run one warmup iteration at boot to prime the KV cache and tokenizer.
3. **Lazy initialization bypass:** Ensure tokenizer and model weights are loaded during boot, not on first request. Check `InferencePipeline::new()` initialization path.
**Confidence:** MEDIUM -- 500ms cold TTFT is aggressive for 7B models. May need to define "cold" as "adapter not loaded but base model warm" rather than "everything cold from disk."

## Open Questions

1. **What constitutes "cold" for TTFT measurement?**
   - What we know: The context says "cold adapter load" for <500ms target
   - What's unclear: Does "cold" mean base model is warm but adapter is not loaded, or completely cold from disk?
   - Recommendation: Define as "base model warm, adapter not in memory." Full cold start (model from disk) on a 7B model will exceed 500ms on any hardware.

2. **How large are typical LoRA adapters in memory?**
   - What we know: LoRA adapters are small relative to base models (typically 1-50MB vs 14GB for 7B base)
   - What's unclear: Exact memory footprint of quantized adapters in the AdapterOS format
   - Recommendation: Measure during benchmark implementation. Even on 16GB, dozens of adapters should fit within 75% ceiling.

3. **Should rate limits persist across restarts?**
   - What we know: SQLite rate limiting (`security/rate_limiting.rs`) persists; DashMap rate limiting (`rate_limit.rs`) is in-memory only
   - What's unclear: Whether the user expects rate limit state to survive restarts
   - Recommendation: In-memory (DashMap) is fine -- rate limiting is about protecting the running system, not accounting.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test + cargo nextest |
| Config file | `Cargo.toml` (workspace test config) |
| Quick run command | `cargo test -p adapteros-server-api -- --test-threads=1` |
| Full suite command | `cargo nt` (nextest) |
| Estimated runtime | ~30-60 seconds per crate |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PERF-54-01 | Inference TTFT < 500ms, throughput matches MLX baseline | integration/benchmark | `bash scripts/benchmarks/inference_benchmark.sh` | No -- Wave 0 gap |
| PERF-54-02 | Memory within UMA budget, LRU eviction works, no OOM on 16GB | unit + integration | `cargo test -p adapteros-memory -- pressure eviction` | Partial -- pressure_manager tests exist |
| SEC-54-01 | Auth enforcement on all tiers, rate limiting per tier, no injection | contract + integration | `bash scripts/contracts/check_security_audit.sh` | No -- Wave 0 gap |
| SEC-54-02 | No secrets in logs/errors, model weight auth, audit trail | contract + unit | `bash scripts/contracts/check_secret_exposure.sh` | No -- Wave 0 gap |

### Nyquist Sampling Rate
- **Minimum sample interval:** After every committed task -> run: `cargo test -p adapteros-server-api -- --test-threads=1`
- **Full suite trigger:** Before merging final task of any plan wave
- **Phase-complete gate:** Full suite green + contract checks pass before `/gsd:verify-work`
- **Estimated feedback latency per task:** ~30 seconds

### Wave 0 Gaps (must be created before implementation)
- [ ] `scripts/benchmarks/inference_benchmark.sh` -- TTFT/throughput/memory benchmark script
- [ ] `scripts/contracts/check_security_audit.sh` -- comprehensive security contract check
- [ ] `scripts/contracts/check_secret_exposure.sh` -- secret/credential leak scanner
- [ ] `tests/benchmark/src/throughput_benchmarks.rs` -- currently a stub, needs implementation
- [ ] Config types for `MemoryConfig` with `ceiling_pct` field
- [ ] Per-tier rate limit config fields in `RateLimitsConfig`

## Sources

### Primary (HIGH confidence)
- Codebase exploration: `adapteros-memory/src/` (pressure_manager.rs, model_cache.rs, unified_tracker.rs, tiered_manager.rs, watchdog.rs)
- Codebase exploration: `adapteros-server-api/src/` (rate_limit.rs, security/rate_limiting.rs, http/ratelimit.rs, middleware_security.rs, backpressure.rs)
- Codebase exploration: `adapteros-auth/src/` (lib.rs, mode.rs, config.rs)
- Codebase exploration: `adapteros-lora-worker/src/` (inference_pipeline.rs, adapter_hotswap.rs, cache_warmup.rs, memory.rs, inference_metrics.rs)
- Codebase exploration: `adapteros-config/src/types.rs` (RateLimitsConfig, SecurityConfig, WorkerSafetyConfig)
- Codebase exploration: `scripts/contracts/` (check_release_security_assertions.sh, check_all.sh)
- Codebase exploration: `scripts/security_audit.sh` (cargo-audit, SBOM)
- Codebase exploration: `configs/cp.toml` (rate_limits, security sections)

### Secondary (MEDIUM confidence)
- MLX unified memory model inference from CLAUDE.md documentation and FFI wrapper code
- Apple Silicon UMA behavior from codebase comments and memory management design

### Tertiary (LOW confidence)
- 500ms cold TTFT target feasibility -- depends on model size, adapter size, and hardware. May need redefinition of "cold."

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all components exist in codebase, well-documented
- Architecture: HIGH -- patterns follow existing contract check and middleware patterns
- Pitfalls: HIGH -- derived from actual codebase analysis (MLX memory, rate limit gaps, SSE contract)
- Performance targets: MEDIUM -- 500ms TTFT depends on definition of "cold" and hardware
- Security completeness: MEDIUM -- existing tests are mostly documentation-style; real audit may surface issues

**Research date:** 2026-03-04
**Valid until:** 2026-04-04 (stable domain, internal codebase)
