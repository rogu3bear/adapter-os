# Phase 54: Performance and Security Hardening - Research

**Researched:** 2026-03-04 (updated 2026-03-05)
**Domain:** Inference performance optimization, UMA memory management, API security hardening (Rust/Axum/MLX/Apple Silicon)
**Confidence:** HIGH

## Summary

Phase 54 is a pure hardening phase -- no new features, only speed and safety improvements on existing surfaces. The codebase already has substantial infrastructure for memory management (`adapteros-memory` with tiered manager, pressure manager, LRU model cache, unified tracker, watchdog), rate limiting (token-bucket per-tenant dashmap + SQLite sliding-window + tower-http concurrency layer), auth (JWT/API key/cookie with dev-bypass gating), and security headers (CSP, HSTS, X-Frame-Options). The work is about tuning, closing gaps, and adding measurable verification.

The four requirements split cleanly into two domains: **performance** (PERF-54-01 inference latency, PERF-54-02 memory budget) and **security** (SEC-54-01 endpoint audit, SEC-54-02 secret protection). Both domains can be planned as independent work streams since they share no file contention beyond config types.

A critical finding from codebase investigation: `MemoryConfig` already exists in `adapteros-policy::packs::memory` with `max_memory_usage_pct: 85.0` and `min_headroom_pct: 15.0`. The new config-layer `MemoryConfig` in `adapteros-config::types` must use a distinct name (e.g., `UmaMemoryConfig` or `MemoryCeilingConfig`) to avoid import collisions.

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
| PERF-54-01 | Inference latency meets or beats comparable local LoRA tools (TTFT, throughput) | Benchmark script infrastructure, existing `InferenceMetrics` (tokens_per_second, latency percentiles), `CacheWarmupManager` with `HealthCheckResult`, `preload_adapters_for_inference()` in streaming_infer.rs |
| PERF-54-02 | Memory stays within UMA budget -- no OOM on 16GB with reasonable adapter counts | Existing `MemoryPressureManager` + `UnifiedMemoryTracker` + `ModelCache` LRU + `UmaPressureMonitor` (5s polling with Mach API + sysctl); needs configurable UMA ceiling and eviction notification pipeline |
| SEC-54-01 | All API endpoints pass security audit: auth, input validation, rate limiting, no injection | Existing middleware chain with global rate limiting (SQLite sliding-window), `RATE_LIMIT_EXEMPT_PATHS`, security headers; needs per-tier rate config and audit contract script |
| SEC-54-02 | Secrets never logged, exposed in errors, or accessible without auth | Existing `dev_bypass_status()` with production-mode blocking, `check_release_security_assertions.sh`; needs log scanning, secret exposure scanner, model weight file permissions |
</phase_requirements>

## Standard Stack

### Core (Already in Codebase)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `adapteros-memory` | internal | UMA tracking, pressure management, LRU cache, tiered memory | Already built: `MemoryPressureManager`, `UnifiedMemoryTracker`, `ModelCache`, `TieredMemoryManager` |
| `adapteros-auth` | internal | JWT/API key/cookie auth with dev-bypass | Already built: `AuthMode`, `dev_bypass_status()`, compile-time release guards |
| `adapteros-server-api` (rate_limit) | internal | Token-bucket + SQLite sliding-window rate limiting | Already built: `RateLimiterState` (dashmap), `check_rate_limit` (SQLite), `rate_limiting_middleware` |
| `adapteros-lora-worker` (memory) | internal | UMA pressure monitoring, `sysctl hw.memsize` + Mach `host_statistics64` | Already built: `UmaPressureMonitor` with circuit breaker, `UmaStats`, standalone `get_uma_stats()` |
| `adapteros-policy::packs::memory` | internal | Memory policy with eviction ordering | Already built: `MemoryConfig` (15% headroom, 85% max usage), `MemoryPolicy`, `EvictionOrder` |
| `lru` | crate | LRU cache for model eviction | Already used in `ModelCache` |
| `dashmap` | crate | Concurrent hash map for rate limiter state | Already used in `RateLimiterState` |
| `parking_lot` | crate | Fast rwlocks for memory tracking | Already used throughout memory crate |
| `tower-http` | crate | HTTP middleware (body limit, CORS, compression) | Already used in middleware chain |

### Supporting (May Need)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `cargo-audit` | CLI tool | Dependency vulnerability scanning | Already in `scripts/security_audit.sh`, needs CI integration |
| `tokio::sync::broadcast` | tokio | Eviction notification channel | For decoupling memory eviction from SSE transport |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Custom benchmark script | `criterion` benches | Script is better: needs end-to-end TTFT (cold adapter load, SSE stream timing), not micro-benchmarks. Criterion cannot test full API round-trip |
| SQLite rate limiting | In-memory only (dashmap) | Both exist. SQLite sliding-window runs in global `rate_limiting_middleware`, dashmap token-bucket in `RateLimiterState`. Keep both -- different use cases |
| Manual log scanning | `tracing-subscriber` filter | CI-time grep catches hardcoded secrets; runtime redaction is complementary but a code change, not a script |

**Installation:**
```bash
# No new dependencies needed. Existing tooling sufficient.
cargo install cargo-audit  # if not already installed (scripts/security_audit.sh handles this)
```

## Architecture Patterns

### Existing Project Structure (Relevant Crates)
```
crates/
├── adapteros-memory/              # UMA tracking, pressure, eviction, tiered manager
│   ├── src/pressure_manager.rs    # LRU eviction with pinned adapters, K-reduction
│   ├── src/model_cache.rs         # LRU model cache with eviction scoring
│   ├── src/unified_tracker.rs     # Multi-backend memory accounting (MemoryLimits, PressureLevel)
│   └── src/tiered_manager.rs      # Hot/Warm/Cold tier migration with configurable demotion
├── adapteros-auth/                # JWT/API key/cookie, dev-bypass gating
│   ├── src/mode.rs                # AuthMode enum (BearerToken, Cookie, ApiKey, DevBypass, Unauthenticated)
│   └── src/state.rs               # Auth state management
├── adapteros-server-api/          # Axum handlers, middleware chain, rate limiting
│   ├── src/middleware_security.rs  # Security headers, rate_limiting_middleware (SQLite), RATE_LIMIT_EXEMPT_PATHS
│   ├── src/rate_limit.rs          # RateLimiterConfig (dashmap token-bucket, fail-closed, injected clock)
│   ├── src/security/rate_limiting.rs  # SQLite sliding-window per-tenant check_rate_limit()
│   ├── src/backpressure.rs        # UMA backpressure guard (checks cached pressure level)
│   ├── src/auth.rs                # dev_bypass_status() with OnceLock, production-mode blocking
│   ├── src/routes/mod.rs          # Route builder: health_routes, public_routes, internal_routes, protected_routes
│   ├── src/sse/types.rs           # SseStreamType enum (17 stream types), SseEvent struct
│   └── src/handlers/streaming_infer.rs  # SSE inference streaming, preload_adapters_for_inference()
├── adapteros-lora-worker/         # Inference pipeline, hot-swap, cache warmup, memory monitoring
│   ├── src/inference_pipeline.rs  # Full inference with routing
│   ├── src/adapter_hotswap.rs     # Two-phase swap with rollback
│   ├── src/cache_warmup.rs        # CacheWarmupManager, HealthCheckConfig, HealthCheckResult
│   ├── src/memory.rs              # UmaPressureMonitor (5s polling, Mach API + sysctl + vm_stat)
│   └── src/inference_metrics.rs   # InferenceMetrics (total/successful/failed requests, tok/s, p50/p95/p99)
├── adapteros-config/              # Config types including RateLimitsConfig
│   └── src/types.rs               # ServerConfig, SecurityConfig, RateLimitsConfig (3 fields), WorkerSafetyConfig
├── adapteros-policy/              # Policy packs including memory
│   └── src/packs/memory.rs        # MemoryConfig (ALREADY EXISTS: min_headroom_pct, max_memory_usage_pct, evict_order)
├── adapteros-transport-types/     # Worker-CP transport contract
│   └── src/lib.rs                 # WorkerInferenceRequest, NO SSE event types here
├── adapteros-ui/                  # Leptos 0.7 WASM frontend
│   ├── src/sse.rs                 # InferenceEvent enum (Token, Done, Error, Paused, Other)
│   ├── src/signals/notifications.rs  # Toast infrastructure (ToastSeverity, auto-dismiss timers)
│   └── src/components/toast.rs    # ToastItem component (ALREADY EXISTS with Liquid Glass styling)
└── adapteros-lora-mlx-ffi/        # MLX C++ FFI bridge
    └── src/memory_management.rs   # MemoryTracker (peak_memory, allocation tracking)
```

### Pattern 1: Configurable UMA Ceiling
**What:** Add a `UmaMemoryConfig` (NOT `MemoryConfig` -- that name is taken by `adapteros-policy`) to `adapteros-config::types` with `ceiling_pct: u8` (default 75).
**When to use:** At boot, when initializing the memory subsystem.
**Existing foundation:**
- `MemoryLimits::new(max_vram, max_system_ram, headroom_pct)` already parameterizes limits
- `UmaPressureMonitor` already polls via `sysctl hw.memsize` + Mach `host_statistics64` at 5s interval
- `ModelCacheConfig` already has `max_memory_bytes: u64` and `headroom_threshold: f64`
- `MemoryConfig` in `adapteros-policy` already has `max_memory_usage_pct: 85.0` -- the new config should align

**Gap:** No user-facing config knob ties total UMA to a ceiling percentage. The pieces exist but the wiring from config TOML -> memory limits is missing.

**CRITICAL NAMING:** Must use `UmaMemoryConfig` or `MemoryCeilingConfig`, not `MemoryConfig`, because `adapteros-policy::packs::memory::MemoryConfig` already exists and is publicly exported.

### Pattern 2: Rate Limiting Architecture (Current State)
**What:** Rate limiting currently works as follows:
- **Global middleware** (`rate_limiting_middleware` in `middleware_security.rs`): Applied to ALL routes via the global middleware stack (line 2849-2852 in routes/mod.rs). Uses `check_rate_limit()` (SQLite sliding-window). Has `RATE_LIMIT_EXEMPT_PATHS` for bypass.
- **Health routes bypass**: Health routes (`/healthz`, `/readyz`, `/version`) are merged separately and do NOT pass through the global middleware stack (line 2889).
- **Token-bucket** (`RateLimiterState` in `rate_limit.rs`): DashMap-based per-tenant, but currently appears to be unused in the middleware chain (the global middleware calls `check_rate_limit` from `security/rate_limiting.rs`, not `RateLimiterState`).
- **Config** (`RateLimitsConfig` in `configs/cp.toml`): Only 3 fields: `requests_per_minute=300`, `burst_size=60`, `inference_per_minute=150`. No per-tier differentiation.

**Gap:** Per-tier rate limiting needs to add tier-specific fields to `RateLimitsConfig` and modify `rate_limiting_middleware` to apply different limits based on route tier.

**Approach:** Since the middleware is global, the per-tier logic must inspect the request path to determine which tier it belongs to and apply the corresponding limit. Alternatively, apply different rate limiters per route group (more invasive but cleaner).

### Pattern 3: Eviction Notification Pipeline
**What:** When `MemoryPressureManager` evicts an adapter, emit an event through a `tokio::sync::broadcast` channel that the SSE infrastructure picks up and delivers to connected UI clients.
**Existing foundation:**
- `SseStreamType::Alerts` already exists (capacity 200) for system notifications
- `ToastItem` component exists with full Liquid Glass styling, severity levels, auto-dismiss
- `NotificationAction` / toast context infrastructure exists in `signals/notifications.rs`
- `InferenceEvent` enum in `sse.rs` has `#[serde(other)]` catch-all variant `Other`

**Key decisions:**
- Eviction events go through `SseStreamType::Alerts` (not inference stream)
- UI subscribes to alerts stream and shows toast via existing notification infrastructure
- Transport type for eviction event can live in `adapteros-core` or `adapteros-transport-types`

### Pattern 4: Security Audit Contract Check
**What:** A bash script in `scripts/contracts/` that checks auth enforcement, secret exposure, input validation.
**Existing foundation:**
- `check_release_security_assertions.sh` already checks dev bypass flags + tenant guard
- `check_api_route_tiers.py` verifies route-to-tier mapping
- `check_middleware_chain.py` verifies middleware chain completeness
- `security_audit.sh` runs cargo-audit + SBOM generation
- `check_all.sh` orchestrates 16+ contract checks (would add new ones)

### Anti-Patterns to Avoid
- **Name collision:** Do NOT create `MemoryConfig` in `adapteros-config` -- `adapteros-policy` already exports `MemoryConfig`. Use `UmaMemoryConfig` or `MemoryCeilingConfig`.
- **Custom OOM killer:** Never implement custom process killing. Use existing LRU eviction in `MemoryPressureManager` with pinned adapters and K-reduction protocol.
- **Blocking sysctl in hot path:** The existing `UmaPressureMonitor` polls every 5s via `spawn_blocking`. Never call `sysctl` synchronously during inference.
- **Fourth rate limiter:** Three rate limiting implementations exist (SQLite sliding-window, DashMap token-bucket, tower-http). Wire per-tier config into the EXISTING `rate_limiting_middleware` (which uses `check_rate_limit` from SQLite). Do not add a fourth implementation.
- **Secret grep at runtime:** Secret scanning should be CI-time (grep source for patterns), not runtime. Runtime uses `tracing` redaction.
- **`println!` in UI:** Use `leptos::logging::log!` (WASM has no stdout).
- **`.get()` / `.set()` on signals:** Use `try_get()` / `try_set()` in ALL reactive contexts (signals dispose on component unmount).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| UMA total detection | Custom memory reader | `sysctl hw.memsize` (already used in `memory.rs::get_total_memory_bytes()`) | OS API; runs once at boot |
| Memory pressure detection | Custom poller | `UmaPressureMonitor` (5s interval, Mach API, circuit breaker) | Already handles fallbacks, backoff, ANE metrics |
| LRU eviction | Custom eviction logic | `MemoryPressureManager` + `ModelCache` | Already supports pinned adapters, K-reduction, cross-backend ordering |
| Token-bucket rate limiting | Custom implementation | `RateLimiterState` (DashMap, injected clock, fail-closed, TTL eviction) | Already production-ready |
| Security headers | Manual header injection | `security_headers_middleware` | Already has CSP, HSTS, X-Frame-Options, Permissions-Policy, cache control |
| Dependency scanning | Custom vulnerability checker | `cargo-audit` via `scripts/security_audit.sh` | Industry standard, RustSec advisory database |
| Inference metrics | Custom timing | `InferenceMetrics` (tok/s, p50/p95/p99, adapter selections, stop reasons) | Already tracks TTFT implicitly via latency percentiles |
| Toast notifications | Custom toast from scratch | `ToastItem` component + `NotificationAction` + `use_notification_context` | Already built with Liquid Glass, auto-dismiss, severity levels |

**Key insight:** This phase is about wiring existing components together and filling gaps, not building new subsystems. Every major subsystem (memory, auth, rate limiting, metrics, toasts) already exists.

## Common Pitfalls

### Pitfall 1: MemoryConfig Name Collision
**What goes wrong:** Creating `pub struct MemoryConfig` in `adapteros-config::types` collides with `adapteros_policy::packs::memory::MemoryConfig`.
**Why it happens:** Both crates are commonly imported together. Even if they're in different modules, re-exports and glob imports cause confusion.
**How to avoid:** Name the new config struct `UmaMemoryConfig` or `MemoryCeilingConfig`. Document clearly which one is the policy-layer config vs the infrastructure config.
**Warning signs:** Ambiguous import errors, wrong `MemoryConfig` used silently.

### Pitfall 2: UMA Ceiling Races with MLX
**What goes wrong:** Setting a UMA ceiling in Rust while MLX allocates memory via its own C++ runtime leads to accounting disagreements. Our tracker says 8GB used but MLX has allocated 10GB.
**Why it happens:** MLX manages its own memory pool. `MemoryTracker` in `memory_management.rs` tracks what it observes (peak via atomic CAS), but MLX allocations bypass the Rust tracker.
**How to avoid:** Use `UmaPressureMonitor::get_uma_stats()` (Mach `host_statistics64` API) as ground truth -- it reads actual OS-level memory pressure, not our accounting. The ceiling check should compare against OS-reported usage, not tracker-reported usage.
**Warning signs:** `MemoryTracker.peak_memory()` diverges from `UmaStats.used_mb * 1024 * 1024`.

### Pitfall 3: Rate Limit Path Matching Fragility
**What goes wrong:** `RATE_LIMIT_EXEMPT_PATHS` uses prefix matching (`path.starts_with(exempt)`). Entries like `/v1/system/` and `/v1/models/` exempt ALL routes under those prefixes, including protected model mutation endpoints.
**Why it happens:** The exempt list was designed for bootstrap/health scenarios but inadvertently exempts too many routes.
**How to avoid:** When implementing per-tier rate limits, review and tighten the exempt list. Model mutation routes (POST `/v1/models/import`, DELETE `/v1/models/{id}`) should not be exempt. Only GET status routes should be exempt.
**Warning signs:** `check_middleware_chain.py` reports rate limit bypass on protected routes.

### Pitfall 4: Secret Leakage via Debug Derive
**What goes wrong:** `SecurityConfig` in `configs/types.rs` has `#[derive(Debug)]` and contains `jwt_secret: String`. Any `tracing::debug!("{:?}", config)` or error that includes the config struct leaks the JWT secret.
**Why it happens:** `derive(Debug)` is automatic and includes all fields. The config struct doesn't have a custom `Debug` implementation.
**How to avoid:** Implement a custom `Debug` for `SecurityConfig` that redacts `jwt_secret`, or use a newtype wrapper like `Secret<String>` that redacts on `Debug`/`Display`. The secret scanner script should grep for `Debug` derives on config structs containing sensitive field names.
**Warning signs:** JWT secret appears in test output or log files.

### Pitfall 5: Eviction Toast Without SSE Subscription
**What goes wrong:** Backend evicts an adapter and emits an SSE event, but the UI only subscribes to inference and training SSE streams, not the alerts stream, so no toast appears.
**Why it happens:** The SSE infrastructure supports 17 stream types (`SseStreamType`), but the UI may not subscribe to `Alerts`.
**How to avoid:** Verify the UI subscribes to the `Alerts` SSE stream. Add the subscription in `sse.rs` or the main app setup. Test by triggering an eviction and verifying the toast appears.
**Warning signs:** Backend logs "adapter evicted" but UI shows nothing. No `EventSource` connection to alerts endpoint in browser devtools.

### Pitfall 6: Benchmark Flakiness on Shared Machines
**What goes wrong:** TTFT benchmarks are noisy: macOS thermal throttling, Spotlight indexing, iCloud sync compete for UMA.
**Why it happens:** Apple Silicon dynamic clock management adjusts frequencies based on thermal state.
**How to avoid:** Run 3+ iterations, report median not mean. Include a warmup phase (first iteration discarded). Document "quiet machine" requirements. Compare against baseline ratio, not absolute numbers.
**Warning signs:** >20% variance between runs.

## Code Examples

### Existing: UMA Stats Collection (from memory.rs)
```rust
// Source: crates/adapteros-lora-worker/src/memory.rs:297-302
fn get_total_memory_bytes(&self) -> Option<u64> {
    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}
```

### Existing: Memory Pressure Detection (from memory.rs)
```rust
// Source: crates/adapteros-lora-worker/src/memory.rs:59-77
// UmaPressureMonitor polls every 5s via spawn_blocking
match tokio::task::spawn_blocking(|| {
    std::panic::catch_unwind(get_uma_stats)
}).await {
    Ok(Ok(stats)) => {
        let pressure = determine_pressure(&stats, min_headroom as f32);
        *pressure_cache.write() = pressure;
        // ...
    }
}
```

### Existing: Rate Limiting Middleware (from middleware_security.rs)
```rust
// Source: crates/adapteros-server-api/src/middleware_security.rs:134-161
pub async fn rate_limiting_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();
    if is_rate_limit_exempt(path) {
        return next.run(req).await;
    }
    // Uses check_rate_limit (SQLite sliding-window)
    match check_rate_limit(&state.db, &tenant_id).await {
        Ok(result) if result.allowed => { /* add X-RateLimit-* headers */ }
        Ok(result) => { /* 429 Too Many Requests */ }
        Err(_) => { /* fail-closed: 503 */ }
    }
}
```

### Existing: Token Bucket Config (from rate_limit.rs)
```rust
// Source: crates/adapteros-server-api/src/rate_limit.rs:46-60
pub struct RateLimiterConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub bucket_ttl_secs: u64,  // default: 3600
    pub max_buckets: usize,    // default: 10000
    pub fail_closed: bool,     // default: true
}
```

### Existing: Config RateLimitsConfig (from types.rs)
```rust
// Source: crates/adapteros-config/src/types.rs:172-176
pub struct RateLimitsConfig {
    pub requests_per_minute: u32,  // 300 in cp.toml
    pub burst_size: u32,           // 60 in cp.toml
    pub inference_per_minute: u32, // 150 in cp.toml
}
```

### Existing: InferenceEvent enum in UI (from sse.rs)
```rust
// Source: crates/adapteros-ui/src/sse.rs:43-101
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
pub enum InferenceEvent {
    Token { text: String },
    Done { total_tokens: usize, latency_ms: u64, trace_id: Option<String>, ... },
    Error { message: String },
    Paused { pause_id: String, inference_id: String, ... },
    #[serde(other)]
    Other,
}
```

### Existing: Toast Component (from toast.rs)
```rust
// Source: crates/adapteros-ui/src/components/toast.rs
#[component]
pub fn ToastItem(
    toast: ToastData,
    #[prop(optional)] on_dismiss: Option<Callback<String>>,
) -> impl IntoView {
    // Full implementation exists with Liquid Glass styling,
    // severity classes, auto-dismiss, expandable details
}
```

### Pattern: UMA Ceiling Config (new -- distinct from policy MemoryConfig)
```rust
// Extends crates/adapteros-config/src/types.rs
// IMPORTANT: Do NOT name this MemoryConfig -- conflicts with adapteros-policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UmaMemoryConfig {
    /// Maximum percentage of total UMA to use (default: 75)
    #[serde(default = "default_uma_ceiling_pct")]
    pub ceiling_pct: u8,
    /// Headroom percentage within the ceiling for eviction trigger (default: 15)
    #[serde(default = "default_headroom_pct")]
    pub headroom_pct: u8,
    /// Enable eviction notifications via SSE (default: true)
    #[serde(default = "default_true")]
    pub eviction_notifications: bool,
}
fn default_uma_ceiling_pct() -> u8 { 75 }
fn default_headroom_pct() -> u8 { 15 }
```

### Pattern: Per-Tier Rate Config (extends existing)
```rust
// Extends crates/adapteros-config/src/types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsConfig {
    pub requests_per_minute: u32,       // existing -- default/protected tier
    pub burst_size: u32,                // existing
    pub inference_per_minute: u32,      // existing
    // New: per-tier overrides (None = use `requests_per_minute`)
    #[serde(default)]
    pub health_rpm: Option<u32>,        // None = unlimited (health probes bypass middleware)
    #[serde(default)]
    pub public_rpm: Option<u32>,        // e.g., 600 for login/status
    #[serde(default)]
    pub internal_rpm: Option<u32>,      // e.g., 1000 for worker heartbeats
    #[serde(default)]
    pub protected_rpm: Option<u32>,     // e.g., 300 for all protected routes
}
```

### Pattern: Benchmark Script (contract check pattern)
```bash
# scripts/benchmarks/inference_benchmark.sh
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

## Claude's Discretion Recommendations

### Inference Concurrency: Serialize
**Recommendation:** Serialize inference requests per-worker. MLX on Apple Silicon uses single-stream execution on unified memory. Running concurrent inference would cause UMA contention, reducing throughput. The existing `UmaPressureMonitor` + `check_uma_backpressure()` enforces this pattern by rejecting new requests under high/critical memory pressure.
**Confidence:** HIGH -- Apple Silicon UMA is shared between CPU and GPU; concurrent GPU kernels compete for the same memory bandwidth.

### Adapter Hot/Cold Tiering: Use Existing TieredMemoryManager
**Recommendation:** Wire the existing `TieredMemoryManager` into the adapter lifecycle. The infrastructure is fully built in `tiered_manager.rs`:
- **Hot (GPU)**: Actively-used adapter tensors (in GPU compute pipeline)
- **Warm (Unified)**: Recently-used, shared CPU/GPU (default 60s idle timeout)
- **Cold (CPU)**: Evicted from active use, reloads transparently on next inference

Config defaults: `tier_capacities = [4GB GPU, 8GB Unified, 32GB CPU]`, `demotion_timeout = 60s`.
**Confidence:** HIGH -- the code exists with full migration policy, just needs integration with adapter lifecycle events.

### UMA Ceiling Default: 75%
**Recommendation:** Default `ceiling_pct = 75`. On a 16GB machine, this leaves 4GB for macOS + apps. On 48GB M4 Max, this allows 36GB. The existing `MemoryConfig` in policy uses `max_memory_usage_pct = 85.0` with `min_headroom_pct = 15.0`, so 75% ceiling provides additional safety margin before the policy layer's warning thresholds trigger.
**Confidence:** MEDIUM -- depends on typical adapter sizes and macOS baseline memory usage. 75% is conservative; could be 80% on dedicated machines.

### TTFT Optimization Strategy
**Recommendation:** Three optimizations to hit <500ms cold TTFT:
1. **Adapter preloading:** `preload_adapters_for_inference()` exists in `streaming_infer.rs`. Ensure it runs before SSE stream starts (it currently does).
2. **Model cache warming at boot:** `CacheWarmupManager` exists with configurable warmup queries. Wire `run_warmup()` into boot sequence after model loading.
3. **Lazy init bypass:** Check that tokenizer and model weights are loaded during boot, not on first request. The `HealthCheckConfig` warmup (1 iteration, 5 tokens, 30s timeout) is designed for exactly this.

**Important caveat:** "Cold adapter load" must mean "base model warm, adapter not in memory." Full cold start (loading a 7B model from disk) will exceed 500ms on any hardware. The benchmark should define and document this distinction.
**Confidence:** MEDIUM -- 500ms cold TTFT is aggressive for adapter loading. Achievable if "cold" = adapter-cold-only.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fixed memory limits (hardcoded 4GB GPU, 8GB unified) | Configurable UMA ceiling % from TOML | Phase 54 | Operators tune per-machine |
| Uniform rate limits (300 rpm for everything) | Per-tier rate limits | Phase 54 | Health checks not throttled, inference path tuned separately |
| Manual security review only | Automated contract checks (6 existing) | Partial (Phase 47) | CI prevents regression |
| No benchmark suite | Reproducible benchmark script | Phase 54 | Proves optimizations, catches regressions |
| Silent adapter eviction | SSE notification + UI toast | Phase 54 | Operator awareness |

**Deprecated/outdated:**
- `check_release_security_assertions.sh` only checks dev bypass flags and tenant guard count. Phase 54 expands to comprehensive multi-vector audit.
- `scripts/security_audit.sh` does cargo-audit + SBOM but doesn't check runtime security (auth enforcement, secret exposure, input validation).
- `throughput_benchmarks.rs` in `tests/benchmark/src/` is a stub (1 line: `//! Placeholder module`).

## Open Questions

1. **What constitutes "cold" for TTFT measurement?**
   - What we know: CONTEXT.md says "cold adapter load" for <500ms target
   - What's unclear: Does "cold" mean base model warm + adapter not loaded, or everything cold from disk?
   - Recommendation: Define as "base model warm, adapter not in memory." Full cold start on a 7B model exceeds 500ms on any hardware. Document this definition in the benchmark script.

2. **How large are typical LoRA adapters in memory?**
   - What we know: LoRA adapters are small relative to base models (typically 1-50MB vs 14GB for 7B base). `ModelCacheConfig` defaults to 4GB max.
   - What's unclear: Exact memory footprint per adapter in AdapterOS format (`.aos` archive).
   - Recommendation: Measure during benchmark implementation. Even on 16GB, dozens of adapters should fit within 75% ceiling.

3. **Should the security audit script fail CI on warnings or only on errors?**
   - What we know: Existing contract checks use `set -euo pipefail` and exit 1 on any failure.
   - What's unclear: Whether the user wants input validation warnings (no `#[validate]`) to be hard failures.
   - Recommendation: Split into hard failures (auth bypass, secrets detected) and warnings (missing validation annotations). Only hard failures exit 1.

4. **Which SQLite rate limiter does the global middleware actually use?**
   - What we know: `rate_limiting_middleware` calls `check_rate_limit(&state.db, &tenant_id)` from `security/rate_limiting.rs` (SQLite-based). The `RateLimiterState` (DashMap token-bucket) exists but may not be wired into the global middleware.
   - What's unclear: Whether the per-tier config should modify the SQLite-based limiter, the DashMap-based one, or both.
   - Recommendation: Modify `rate_limiting_middleware` to use per-tier RPM from config when calling `check_rate_limit`. The path-based tier detection is straightforward since route prefixes are well-defined.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test + cargo nextest |
| Config file | `Cargo.toml` workspace |
| Quick run command | `cargo test -p adapteros-server-api -- --test-threads=1` |
| Full suite command | `cargo nt` (nextest) or `bash scripts/ci/local_required_checks.sh` |
| Estimated runtime | ~30-60 seconds per crate, ~120s full suite |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PERF-54-01 | TTFT < 500ms, throughput matches MLX baseline | benchmark script (E2E) | `bash scripts/benchmarks/inference_benchmark.sh` | No -- Wave 0 gap |
| PERF-54-02 | Memory within UMA budget, LRU eviction works, no OOM on 16GB | unit + integration | `cargo test -p adapteros-memory -- pressure` | Partial -- pressure_manager tests exist |
| SEC-54-01 | Auth enforcement on all tiers, rate limiting per tier, no injection | contract check | `bash scripts/contracts/check_security_audit.sh` | No -- Wave 0 gap |
| SEC-54-02 | No secrets in logs/errors, model weight auth, audit trail | contract check | `bash scripts/contracts/check_secret_exposure.sh` | No -- Wave 0 gap |

### Nyquist Sampling Rate
- **Minimum sample interval:** After every committed task -> run: `cargo test -p adapteros-server-api -- --test-threads=1`
- **Full suite trigger:** Before merging final task of any plan wave
- **Phase-complete gate:** Full suite green + all contract checks pass before `/gsd:verify-work`
- **Estimated feedback latency per task:** ~30 seconds

### Wave 0 Gaps (must be created before implementation)
- [ ] `scripts/benchmarks/inference_benchmark.sh` -- TTFT/throughput/memory benchmark script
- [ ] `scripts/contracts/check_security_audit.sh` -- comprehensive security contract check
- [ ] `scripts/contracts/check_secret_exposure.sh` -- secret/credential leak scanner
- [ ] `tests/benchmark/src/throughput_benchmarks.rs` -- currently a 1-line stub, needs implementation
- [ ] `UmaMemoryConfig` config type with `ceiling_pct` field (NOT `MemoryConfig` -- name collision)
- [ ] Per-tier rate limit config fields in `RateLimitsConfig`

## Sources

### Primary (HIGH confidence)
- Codebase: `adapteros-memory/src/` -- pressure_manager.rs, model_cache.rs, unified_tracker.rs, tiered_manager.rs (MemoryLimits, PressureLevel, TieredConfig, ModelCacheConfig)
- Codebase: `adapteros-server-api/src/` -- rate_limit.rs (RateLimiterConfig), middleware_security.rs (rate_limiting_middleware, RATE_LIMIT_EXEMPT_PATHS), routes/mod.rs (route builder with 4 tiers + global middleware)
- Codebase: `adapteros-auth/src/` -- mode.rs (AuthMode), auth.rs (dev_bypass_status with OnceLock)
- Codebase: `adapteros-lora-worker/src/` -- memory.rs (UmaPressureMonitor, UmaStats, get_uma_stats with sysctl+Mach), inference_metrics.rs (InferenceMetrics), cache_warmup.rs (CacheWarmupManager)
- Codebase: `adapteros-config/src/types.rs` -- RateLimitsConfig (3 fields), SecurityConfig (jwt_secret with #[derive(Debug)])
- Codebase: `adapteros-policy/src/packs/memory.rs` -- MemoryConfig (ALREADY EXISTS: 15% headroom, 85% max usage)
- Codebase: `adapteros-ui/src/` -- sse.rs (InferenceEvent), components/toast.rs (ToastItem), signals/notifications.rs (toast infrastructure)
- Codebase: `scripts/contracts/` -- 16+ contract check scripts, check_all.sh orchestration
- Codebase: `configs/cp.toml` -- rate_limits section (300 rpm, 60 burst, 150 inference)

### Secondary (MEDIUM confidence)
- MLX unified memory model inference from CLAUDE.md documentation and FFI wrapper code
- Apple Silicon UMA behavior from codebase comments and Mach API usage in memory.rs

### Tertiary (LOW confidence)
- 500ms cold TTFT target feasibility -- depends on model size, adapter size, and hardware. May need redefinition of "cold."

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all components exist in codebase, well-documented with tests
- Architecture: HIGH -- patterns follow existing contract check and middleware patterns
- Pitfalls: HIGH -- derived from actual codebase analysis (MemoryConfig name collision, rate limit exempt path overshoot, SecurityConfig Debug derive)
- Performance targets: MEDIUM -- 500ms TTFT depends on definition of "cold" and hardware
- Security completeness: MEDIUM -- existing checks are partial; comprehensive audit may surface issues

**Research date:** 2026-03-05
**Valid until:** 2026-04-05 (stable domain, internal codebase)
