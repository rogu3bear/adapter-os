# PR-003: Seed Fallback Hardening

## Summary

Prevent dev/test fallback seeds from silently entering production inference paths by removing the hardcoded fallback in `derive_seed_contextual()` and enforcing seed context presence in production.

## Problem Statement

In `crates/adapteros-core/src/seed_override.rs:384-387`:

```rust
pub fn derive_seed_contextual(label: &str) -> Result<[u8; 32]> {
    if let Some(mut ctx) = get_thread_seed_context() {
        // ... proper derivation ...
    } else {
        // VULNERABILITY: Hardcoded fallback used when no context set
        let fallback = B3Hash::hash(b"adapteros-fallback-no-context");
        let effective = get_effective_global_seed(&fallback);
        Ok(derive_seed(&effective, label))
    }
}
```

**Attack/Bug Scenarios**:

1. **Missing middleware**: If request middleware fails to set seed context, all requests use the same deterministic seed
2. **Thread-local leak**: If context leaks to wrong request, seed isolation breaks
3. **Silent degradation**: No alert when fallback is used in production
4. **Replay attack enabler**: Known fallback seed allows pre-computed attacks

The `assert_thread_local_clean()` function only panics in debug builds (line 329-344), providing no production protection.

## Solution

1. Remove hardcoded fallback - return error when no context in strict mode
2. Add `determinism.require_seed_context` config flag (default true in release)
3. Emit `strict_mode_failure_event()` when fallback would be used
4. Add production-safe logging for leaked state detection
5. Enforce context assertion at request boundaries in middleware

---

## Implementation Details

### File Changes

#### 1. `crates/adapteros-core/src/seed_override.rs`

**Replace fallback logic with strict enforcement**:

```rust
/// Derive a seed using the thread-local context if available.
///
/// # Errors
///
/// Returns `AosError::DeterminismViolation` if:
/// - No seed context is set AND strict mode is enabled
/// - The config requires seed context (`require_seed_context = true`)
///
/// # Fallback Behavior
///
/// In non-strict mode with `require_seed_context = false`, falls back to
/// a deterministic but request-independent seed. This is logged as a warning
/// and increments the `seed_fallback_total` metric.
pub fn derive_seed_contextual(label: &str) -> Result<[u8; 32]> {
    if let Some(mut ctx) = get_thread_seed_context() {
        let seed = ctx.derive(label)?;
        set_thread_seed_context(ctx);
        return Ok(seed);
    }

    // No context available - check configuration
    let config = get_determinism_config();
    let require_context = require_seed_context_enabled();

    if config.strict_mode || require_context {
        // Emit observability event
        let event = strict_mode_failure_event(
            "derive_seed_contextual called without seed context",
            Some("seed_derivation".to_string()),
            true, // fallback_used would be true if we allowed it
            None,
            None,
        );
        emit_observability_event(&event);

        // Increment metric
        metrics::counter!("seed_context_missing_total", "strict" => "true").increment(1);

        return Err(AosError::DeterminismViolation(
            "Seed context required but not set. Ensure request middleware sets SeedContextGuard."
                .to_string(),
        ));
    }

    // Non-strict fallback path (dev/test only)
    tracing::warn!(
        label = label,
        "Using fallback seed - no context set (non-strict mode)"
    );
    metrics::counter!("seed_fallback_used_total").increment(1);

    // Use a fallback, but make it obvious in logs
    let fallback = B3Hash::hash(b"adapteros-fallback-no-context-DEV-ONLY");
    let effective = get_effective_global_seed(&fallback);
    Ok(derive_seed(&effective, label))
}

/// Check if seed context is required based on configuration.
///
/// Reads from environment variable `AOS_REQUIRE_SEED_CONTEXT` or
/// config file setting `determinism.require_seed_context`.
///
/// Default: `true` in release builds, `false` in debug builds.
pub fn require_seed_context_enabled() -> bool {
    static REQUIRE_CONTEXT: OnceLock<bool> = OnceLock::new();

    *REQUIRE_CONTEXT.get_or_init(|| {
        // Check environment variable first
        if let Ok(val) = std::env::var("AOS_REQUIRE_SEED_CONTEXT") {
            return matches!(val.to_lowercase().as_str(), "1" | "true" | "yes");
        }

        // Default based on build type
        if cfg!(debug_assertions) {
            false // Allow fallback in debug builds for easier development
        } else {
            true // Require context in release builds
        }
    })
}
```

**Update `assert_thread_local_clean()` for production safety**:

```rust
/// Assert that thread-local seed state is clean.
///
/// In debug builds: panics if state is not clean (catches bugs immediately)
/// In release builds: logs error and emits metric (for production monitoring)
///
/// Returns `true` if state was clean, `false` if it was dirty (and cleaned).
#[inline]
pub fn assert_thread_local_clean() -> bool {
    if is_thread_local_clean() {
        return true;
    }

    let info = get_leaked_state_info();

    #[cfg(debug_assertions)]
    {
        panic!(
            "DETERMINISM BUG: Thread-local seed state leaked from previous request! \
             tenant_id={:?}, request_id={:?}, nonce_counter={:?}",
            info.as_ref().and_then(|i| i.tenant_id.as_ref()),
            info.as_ref().and_then(|i| i.request_id.as_ref()),
            info.as_ref().and_then(|i| i.nonce_counter),
        );
    }

    #[cfg(not(debug_assertions))]
    {
        // Production: log error, emit metric, clean up
        tracing::error!(
            tenant_id = ?info.as_ref().and_then(|i| i.tenant_id.as_ref()),
            request_id = ?info.as_ref().and_then(|i| i.request_id.as_ref()),
            nonce_counter = ?info.as_ref().and_then(|i| i.nonce_counter),
            "DETERMINISM BUG: Thread-local seed state leaked - cleaning up"
        );

        metrics::counter!("seed_context_leaked_total").increment(1);

        // Clean up the leaked state
        clear_thread_seed_context();

        false
    }
}

/// Assert thread-local state is clean, with explicit cleanup on failure.
///
/// Use this at request boundaries to ensure isolation.
/// Returns the cleanup status for logging/metrics.
pub fn ensure_thread_local_clean() -> ThreadLocalCleanupResult {
    if is_thread_local_clean() {
        return ThreadLocalCleanupResult::AlreadyClean;
    }

    let info = get_leaked_state_info();
    clear_thread_seed_context();

    ThreadLocalCleanupResult::CleanedUp {
        tenant_id: info.as_ref().and_then(|i| i.tenant_id.clone()),
        request_id: info.as_ref().and_then(|i| i.request_id.clone()),
        nonce_counter: info.as_ref().and_then(|i| i.nonce_counter),
    }
}

#[derive(Debug, Clone)]
pub enum ThreadLocalCleanupResult {
    AlreadyClean,
    CleanedUp {
        tenant_id: Option<String>,
        request_id: Option<String>,
        nonce_counter: Option<u64>,
    },
}
```

#### 2. `crates/adapteros-server-api/src/middleware/observability.rs`

**Add seed context enforcement middleware**:

```rust
use adapteros_core::seed_override::{
    assert_thread_local_clean, ensure_thread_local_clean,
    SeedContext, SeedContextGuard, ThreadLocalCleanupResult,
};

/// Middleware layer that ensures seed context isolation between requests.
///
/// # Behavior
///
/// 1. **Pre-request**: Asserts thread-local state is clean (logs/metrics if not)
/// 2. **Request processing**: Sets up `SeedContextGuard` for the request
/// 3. **Post-request**: Guard drops and clears context (via RAII)
///
/// This middleware MUST be applied to all inference endpoints.
pub async fn seed_isolation_middleware<B>(
    State(state): State<AppState>,
    request: Request<B>,
    next: Next<B>,
) -> Response {
    // Pre-request: Ensure clean state
    let pre_cleanup = ensure_thread_local_clean();
    if let ThreadLocalCleanupResult::CleanedUp { tenant_id, request_id, .. } = &pre_cleanup {
        tracing::warn!(
            leaked_tenant_id = ?tenant_id,
            leaked_request_id = ?request_id,
            "Cleaned leaked seed context before request"
        );
    }

    // Extract request context for seed derivation
    let tenant_id = extract_tenant_id(&request);
    let request_id = extract_request_id(&request);
    let manifest_hash = state.manifest_cache.current_hash();

    // Create seed context for this request
    let seed_ctx = SeedContext::new(
        state.global_seed,
        manifest_hash,
        state.seed_mode,
        state.worker_id,
        tenant_id.clone(),
    ).with_request_id(request_id.clone());

    // RAII guard ensures cleanup on all exit paths (including panics)
    let _guard = SeedContextGuard::new(seed_ctx);

    // Process request
    let response = next.run(request).await;

    // Post-request: Guard drops here, clearing context
    // Double-check in case of any bugs
    let post_cleanup = ensure_thread_local_clean();
    if let ThreadLocalCleanupResult::CleanedUp { .. } = post_cleanup {
        tracing::error!("Seed context still present after guard drop - bug in SeedContextGuard");
        metrics::counter!("seed_guard_cleanup_failed_total").increment(1);
    }

    response
}
```

**Register middleware in router**:

```rust
// In router setup
pub fn inference_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/inference", post(inference_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        // ... other routes ...
        .layer(middleware::from_fn_with_state(
            state.clone(),
            seed_isolation_middleware,
        ))
        .with_state(state)
}
```

#### 3. `crates/adapteros-lora-worker/src/services/determinism_policy.rs`

**Add seed context validation**:

```rust
use adapteros_core::seed_override::{
    get_thread_seed_context, require_seed_context_enabled,
};

/// Validate determinism prerequisites before inference.
///
/// # Checks
///
/// 1. Seed context is set (if required)
/// 2. Seed mode matches request profile
/// 3. Backend supports requested determinism level
///
/// # Errors
///
/// Returns error if any prerequisite fails and strict mode is enabled.
pub fn validate_determinism_prerequisites(
    profile: &ExecutionProfile,
) -> Result<DeterminismPrerequisites> {
    let mut warnings = Vec::new();

    // Check seed context
    let ctx = get_thread_seed_context();
    if ctx.is_none() {
        if require_seed_context_enabled() || profile.seed_mode == SeedMode::Strict {
            return Err(AosError::DeterminismViolation(
                "Seed context not set. Inference requires seed context for determinism."
                    .to_string(),
            ));
        } else {
            warnings.push("No seed context set - using fallback seed".to_string());
        }
    }

    // Validate seed mode compatibility
    if let Some(ref ctx) = ctx {
        if profile.seed_mode == SeedMode::Strict && ctx.seed_mode != SeedMode::Strict {
            return Err(AosError::DeterminismViolation(
                format!(
                    "Request requires strict seed mode but context has {:?}",
                    ctx.seed_mode
                ),
            ));
        }
    }

    Ok(DeterminismPrerequisites {
        seed_context_present: ctx.is_some(),
        seed_mode: ctx.as_ref().map(|c| c.seed_mode).unwrap_or(profile.seed_mode),
        warnings,
    })
}

#[derive(Debug)]
pub struct DeterminismPrerequisites {
    pub seed_context_present: bool,
    pub seed_mode: SeedMode,
    pub warnings: Vec<String>,
}
```

#### 4. `configs/cp.toml`

**Add configuration options**:

```toml
[determinism]
# Require seed context for all inference requests.
# When true, requests without seed context will fail.
# Default: true in release builds, false in debug builds.
require_seed_context = true

# Seed mode for production inference.
# Options: strict, best_effort, non_deterministic (debug only)
default_seed_mode = "strict"

# Enable strict determinism enforcement.
# When true, any determinism violation is an error.
strict_mode = true

# Log seed derivation details (verbose, for debugging)
trace_seeds = false
```

#### 5. Environment Variables

Document in `.env.example`:

```bash
# Seed context enforcement
# Set to "false" only for development/testing
AOS_REQUIRE_SEED_CONTEXT=true

# Debug determinism (logs all seed derivations)
AOS_DEBUG_DETERMINISM=0
```

---

## Acceptance Criteria

- [ ] `derive_seed_contextual()` returns error when no context and `require_seed_context = true`
- [ ] Hardcoded fallback only used in debug builds with explicit config
- [ ] Fallback usage emits `strict_mode_failure_event()` in production
- [ ] `seed_context_missing_total` metric incremented on missing context
- [ ] `seed_fallback_used_total` metric incremented when fallback used (dev)
- [ ] `seed_context_leaked_total` metric incremented on leaked state detection
- [ ] Request middleware sets `SeedContextGuard` for all inference routes
- [ ] `assert_thread_local_clean()` logs and cleans in release (panic in debug)
- [ ] Configuration flag `determinism.require_seed_context` works correctly
- [ ] Environment variable `AOS_REQUIRE_SEED_CONTEXT` overrides config

---

## Test Plan

### Unit Tests

**File**: `crates/adapteros-core/tests/seed_fallback_tests.rs`

```rust
#[test]
fn test_derive_seed_contextual_fails_without_context_strict() {
    clear_thread_seed_context();

    // Set strict mode
    let config = DeterminismConfig::builder().strict_mode(true).build();
    let result = with_determinism_config(config, || {
        derive_seed_contextual("test")
    });

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Seed context required"));
}

#[test]
fn test_derive_seed_contextual_succeeds_with_context() {
    let global = B3Hash::hash(b"test");
    let ctx = SeedContext::new(global, None, SeedMode::Strict, 1, "tenant".to_string());
    let _guard = SeedContextGuard::new(ctx);

    let result = derive_seed_contextual("test");
    assert!(result.is_ok());
}

#[test]
fn test_derive_seed_contextual_fallback_in_dev_mode() {
    clear_thread_seed_context();

    // Non-strict, non-require-context mode
    std::env::set_var("AOS_REQUIRE_SEED_CONTEXT", "false");
    let config = DeterminismConfig::builder().strict_mode(false).build();

    let result = with_determinism_config(config, || {
        derive_seed_contextual("test")
    });

    // Should succeed with fallback in dev mode
    if cfg!(debug_assertions) {
        assert!(result.is_ok());
    } else {
        // Release builds still require context by default
        assert!(result.is_err());
    }

    std::env::remove_var("AOS_REQUIRE_SEED_CONTEXT");
}

#[test]
fn test_ensure_thread_local_clean_cleans_leaked_state() {
    let global = B3Hash::hash(b"test");
    let ctx = SeedContext::new(global, None, SeedMode::Strict, 1, "leaked-tenant".to_string());
    set_thread_seed_context(ctx);

    // Don't use guard - simulate leak
    let result = ensure_thread_local_clean();

    match result {
        ThreadLocalCleanupResult::CleanedUp { tenant_id, .. } => {
            assert_eq!(tenant_id, Some("leaked-tenant".to_string()));
        }
        _ => panic!("Expected CleanedUp result"),
    }

    assert!(is_thread_local_clean());
}

#[test]
fn test_seed_context_guard_raii_cleanup() {
    let global = B3Hash::hash(b"test");

    {
        let ctx = SeedContext::new(global, None, SeedMode::Strict, 1, "tenant".to_string());
        let _guard = SeedContextGuard::new(ctx);

        assert!(get_thread_seed_context().is_some());
    }

    // Guard dropped - context should be cleared
    assert!(get_thread_seed_context().is_none());
}
```

### Integration Tests

**File**: `tests/seed_isolation_integration.rs`

```rust
#[tokio::test]
async fn test_inference_without_middleware_fails() {
    // Create server WITHOUT seed isolation middleware
    let app = Router::new()
        .route("/v1/inference", post(inference_handler))
        .with_state(state_without_middleware());

    let response = app
        .oneshot(Request::post("/v1/inference").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Should fail due to missing seed context
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_inference_with_middleware_succeeds() {
    let app = inference_router(test_state());

    let response = app
        .oneshot(Request::post("/v1/inference")
            .header("X-Tenant-ID", "test-tenant")
            .body(inference_body())
            .unwrap())
        .await
        .unwrap();

    assert!(response.status().is_success());
}

#[tokio::test]
async fn test_concurrent_requests_isolated() {
    let app = inference_router(test_state());

    // Run multiple concurrent requests
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let app = app.clone();
            tokio::spawn(async move {
                let response = app
                    .oneshot(Request::post("/v1/inference")
                        .header("X-Tenant-ID", format!("tenant-{}", i))
                        .body(inference_body())
                        .unwrap())
                    .await
                    .unwrap();

                // Extract tenant from response to verify isolation
                (i, response)
            })
        })
        .collect();

    for handle in handles {
        let (i, response) = handle.await.unwrap();
        assert!(response.status().is_success(), "Request {} failed", i);
    }
}
```

### E2E Tests

**File**: `tests/e2e/seed_enforcement.rs`

```rust
#[tokio::test]
async fn test_production_config_requires_seed_context() {
    // Start server with production config
    let server = start_server_with_config(r#"
        [determinism]
        require_seed_context = true
        strict_mode = true
    "#).await;

    // Metrics should show no fallback usage
    let metrics = fetch_metrics(&server).await;
    assert!(metrics.contains("seed_fallback_used_total 0"));

    // Run some inference requests
    for _ in 0..5 {
        let response = server.post("/v1/inference")
            .json(&inference_request())
            .send()
            .await;
        assert!(response.status().is_success());
    }

    // Still no fallback usage
    let metrics = fetch_metrics(&server).await;
    assert!(metrics.contains("seed_fallback_used_total 0"));
    assert!(metrics.contains("seed_context_leaked_total 0"));
}
```

---

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `seed_context_missing_total` | Counter | Requests where seed context was required but missing |
| `seed_fallback_used_total` | Counter | Times fallback seed was used (should be 0 in prod) |
| `seed_context_leaked_total` | Counter | Detected leaked seed contexts from previous requests |
| `seed_guard_cleanup_failed_total` | Counter | SeedContextGuard failed to clean up (bug indicator) |

---

## Rollout Plan

1. **Week 1**: Merge with `require_seed_context = false` (no behavior change)
2. **Week 2**: Enable metrics collection, monitor `seed_fallback_used_total`
3. **Week 3**: Enable `require_seed_context = true` on staging
4. **Week 4**: Enable on production after confirming zero fallback usage

---

## Security Considerations

- Hardcoded fallback seed is predictable - removes it from production path
- Leaked context could allow cross-tenant seed correlation - now detected and logged
- Missing context in strict mode is now a hard failure, not silent degradation
