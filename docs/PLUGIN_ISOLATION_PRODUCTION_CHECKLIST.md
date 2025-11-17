# Plugin Isolation & Failure Safety - Production Readiness Checklist

**Status:** ⚠️ **NOT PRODUCTION-READY** - Implementation complete, verification pending

**Last Updated:** 2025-11-17
**PRD Reference:** PRD 7 - Plugin Isolation & Failure Safety

---

## Executive Summary

The plugin isolation system has been **implemented** but **NOT verified**. Core safety mechanisms (timeouts, panic isolation, failure tracking) are in place, but the system requires:
- Build verification
- Test execution
- Metrics implementation
- Circuit breaker pattern
- Telemetry persistence verification

**Estimated work to production: 2-3 days**

---

## Implementation Status Matrix

| Component | Implemented | Compiled | Tested | Production-Ready |
|-----------|-------------|----------|--------|------------------|
| Core types (PluginHealth) | ✅ | ✅ | ⚠️ Manual | ✅ |
| Telemetry events | ✅ | ✅ | ⚠️ Manual | ✅ |
| Timeout wrappers | ✅ | ❌ Unknown | ❌ | ❌ |
| Panic isolation | ✅ | ❌ Unknown | ❌ | ❌ |
| Failure tracking | ✅ | ❌ Unknown | ❌ | ⚠️ Needs metrics |
| Auto-disable (3 failures) | ✅ | ❌ Unknown | ❌ | ⚠️ Needs circuit breaker |
| Telemetry emission | ✅ | ❌ Unknown | ❌ | ⚠️ Needs verification |
| API endpoints | ✅ | ❌ Unknown | ❌ | ❌ |
| Test suite | ✅ | ❌ Unknown | ❌ | ❌ |
| Documentation | ✅ | N/A | N/A | ⚠️ Needs updates |

---

## Critical Blockers (Must Fix Before Production)

### 1. Build Verification ❌ **BLOCKER**

**Status:** Plugin registry changes have NOT been compiled

**Issue:** Pre-existing build errors in dependencies (`adapteros-db`, `adapteros-lora-worker`, `adapteros-lora-kernel-mtl`) prevent verification of:
- `PluginRegistry` compiles with new telemetry integration
- Import paths are correct (`TelemetryEventBuilder`, `EventType`, etc.)
- Type signatures match
- No lifetime/ownership issues

**Action Required:**
```bash
# Fix dependency builds first
cargo build -p adapteros-db 2>&1 | tee db-build-errors.log
cargo build -p adapteros-lora-worker 2>&1 | tee worker-build-errors.log

# Then verify plugin registry
cargo build -p adapteros-server 2>&1 | grep -E "(error|warning.*plugin)"
```

**Owner:** [UNASSIGNED]
**Priority:** P0
**Estimated Effort:** 4-8 hours

---

### 2. Test Execution ❌ **BLOCKER**

**Status:** 500+ line test suite has NEVER been run

**Issue:** `tests/plugin_isolation_safety_tests.rs` is well-structured but completely unverified. Could have:
- Syntax errors
- Logic bugs
- Wrong assertions
- Missing imports
- Type mismatches

**Action Required:**
```bash
# Fix Metal kernel build first (test dependency)
cd crates/adapteros-lora-kernel-mtl && cargo build

# Run plugin tests
cargo test --test plugin_isolation_safety_tests

# Verify all 11 tests pass
cargo test --test plugin_isolation_safety_tests -- --nocapture
```

**Expected Results:**
- ✅ `test_plugin_panic_does_not_crash_supervisor`
- ✅ `test_plugin_timeout_handling`
- ✅ `test_plugin_failure_does_not_affect_core`
- ✅ `test_plugin_enable_disable_no_restart`
- ✅ `test_plugin_health_visibility`
- ✅ `test_plugin_repeated_failures_tracking`
- ✅ `test_plugin_telemetry_emitted`
- ✅ `test_core_inference_independent_of_plugins`
- ✅ `test_plugin_supervisor_recovery`
- ✅ `test_plugin_chaos_concurrent_operations`
- ✅ `test_plugin_full_lifecycle_with_failures`

**Owner:** [UNASSIGNED]
**Priority:** P0
**Estimated Effort:** 2-4 hours (assuming tests pass first try)

---

### 3. Telemetry Persistence Verification ⚠️ **IMPORTANT**

**Status:** Telemetry integration added but not verified

**Implementation:** `plugin_registry.rs:91-96`
```rust
// Write to telemetry writer for durable storage
if let Some(ref telemetry) = self.telemetry {
    if let Err(e) = telemetry.log_event(event) {
        tracing::warn!(error = %e, "Failed to persist plugin telemetry event");
    }
}
```

**Verification Needed:**
1. Confirm events are actually written to signed bundles
2. Verify bundle rotation works correctly
3. Test bundle integrity with Ed25519 signatures
4. Query persisted events from disk

**Action Required:**
```rust
// Add integration test
#[tokio::test]
async fn test_plugin_telemetry_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let telemetry = Arc::new(TelemetryWriter::new(
        temp_dir.path(),
        100,
        1024 * 1024
    ).unwrap());

    let registry = PluginRegistry::with_telemetry(db, telemetry);
    let plugin = MockPlugin::new("test");

    // Trigger telemetry emission
    registry.register("test", plugin, config).await.unwrap();

    // Verify bundle exists and contains plugin.started event
    let bundles = find_bundles(temp_dir.path());
    assert!(!bundles.is_empty(), "No telemetry bundles created");

    let events = read_bundle(&bundles[0]);
    assert!(events.iter().any(|e| e.event_type == "plugin.started"));
}
```

**Owner:** [UNASSIGNED]
**Priority:** P1
**Estimated Effort:** 2 hours

---

## High-Priority Enhancements (Before Production)

### 4. Prometheus Metrics ⚠️ **IMPORTANT**

**Status:** NOT IMPLEMENTED

**Issue:** No metrics for monitoring plugin health in production. Operators have no visibility into:
- Plugin failure rates
- Health check latency
- Restart counts
- Auto-disable frequency

**Implementation Required:**

**Location:** `crates/adapteros-server/src/plugin_registry.rs`

```rust
use prometheus::{Counter, Gauge, Histogram, register_counter, register_gauge, register_histogram};

// Add to PluginRegistry struct
struct PluginMetrics {
    plugin_failures_total: Counter,
    plugin_health_check_duration: Histogram,
    plugin_restart_total: Counter,
    plugin_auto_disable_total: Counter,
    plugin_uptime_seconds: Gauge,
}

impl PluginRegistry {
    fn new_with_metrics(...) {
        let metrics = PluginMetrics {
            plugin_failures_total: register_counter!(
                "plugin_failures_total",
                "Total number of plugin failures"
            ).unwrap(),
            plugin_health_check_duration: register_histogram!(
                "plugin_health_check_duration_seconds",
                "Plugin health check duration"
            ).unwrap(),
            // ... more metrics
        };
    }

    async fn record_failure(&self, plugin_name: &str, error: &str) {
        self.metrics.plugin_failures_total.inc();
        // ... existing code
    }
}
```

**Grafana Queries:**
```promql
# Alert on high failure rate
rate(plugin_failures_total[5m]) > 0.1

# Dashboard - health check p99
histogram_quantile(0.99, plugin_health_check_duration_seconds)

# Auto-disable events
increase(plugin_auto_disable_total[1h])
```

**Owner:** [UNASSIGNED]
**Priority:** P1
**Estimated Effort:** 4 hours

---

### 5. Circuit Breaker Pattern ⚠️ **IMPORTANT**

**Status:** Simple auto-disable implemented, no exponential backoff

**Current Behavior:**
```
Failure 1: Restart immediately
Failure 2: Restart immediately
Failure 3: Auto-disable permanently
```

**Production Requirement:**
```
Failure 1: Restart immediately
Failure 2: Wait 1s, restart
Failure 3: Wait 2s, restart
Failure 4: Wait 4s, restart
Failure 5: Wait 8s, restart
Failure 6+: Open circuit, manual intervention
```

**Implementation Required:**

**Location:** `crates/adapteros-server/src/plugin_registry.rs:21-27`

```rust
#[derive(Debug, Clone)]
struct PluginMetadata {
    consecutive_failures: u32,
    last_error: Option<String>,
    last_healthy_at: Option<i64>,
    // NEW FIELDS
    circuit_state: CircuitState,
    last_failure_at: Option<i64>,
    backoff_seconds: u64,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,        // Normal operation
    HalfOpen,      // Testing after backoff
    Open,          // Circuit tripped, manual reset required
}

impl PluginMetadata {
    fn should_attempt_restart(&self) -> bool {
        if self.circuit_state == CircuitState::Open {
            return false;
        }

        if let Some(last_failure) = self.last_failure_at {
            let now = chrono::Utc::now().timestamp();
            let elapsed = (now - last_failure) as u64;
            elapsed >= self.backoff_seconds
        } else {
            true
        }
    }

    fn calculate_backoff(&self) -> u64 {
        // Exponential backoff: 1s, 2s, 4s, 8s, 16s
        let base: u64 = 2;
        base.pow(self.consecutive_failures.min(4))
    }
}
```

**Supervisor Integration:**
```rust
// In start_plugin supervisor loop
if health.status == PluginStatus::Dead(_) {
    let should_restart = {
        let metadata = metadata_clone.read().await;
        metadata.get(&name_clone)
            .map(|m| m.should_attempt_restart())
            .unwrap_or(false)
    };

    if should_restart {
        // Attempt reload with backoff
        let backoff = metadata.get(&name_clone).map(|m| m.calculate_backoff());
        if let Some(delay) = backoff {
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
        // ... reload logic
    }
}
```

**Owner:** [UNASSIGNED]
**Priority:** P1
**Estimated Effort:** 6 hours

---

### 6. Graceful Shutdown ⚠️ **IMPORTANT**

**Status:** Crude `abort()` implementation

**Current Code:** `plugin_registry.rs:400-402`
```rust
let mut tasks = self.tasks.write().await;
if let Some(handle) = tasks.remove(name) {
    handle.abort();  // CRUDE: Just kills the task
}
```

**Production Requirement:**
```rust
pub async fn stop_plugin(&self, name: &str) -> Result<()> {
    // 1. Stop health checks
    // 2. Call plugin.stop() with timeout
    // 3. Wait for task to finish gracefully (5s)
    // 4. If still running, abort

    let plugins = self.plugins.read().await;
    if let Some(plugin) = plugins.get(name) {
        // Graceful stop with timeout
        match timeout(PLUGIN_OPERATION_TIMEOUT, plugin.stop()).await {
            Ok(Ok(_)) => info!("Plugin {} stopped gracefully", name),
            Ok(Err(e)) => warn!("Plugin {} stop failed: {}", name, e),
            Err(_) => warn!("Plugin {} stop timeout", name),
        }
    }

    // Wait for supervisor task to exit
    let mut tasks = self.tasks.write().await;
    if let Some(handle) = tasks.remove(name) {
        // Give task 5 seconds to finish
        match tokio::time::timeout(Duration::from_secs(5), async {
            handle.await
        }).await {
            Ok(_) => info!("Plugin {} supervisor exited cleanly", name),
            Err(_) => {
                warn!("Plugin {} supervisor did not exit, aborting", name);
                // Abort only as last resort
            }
        }
    }

    Ok(())
}
```

**Owner:** [UNASSIGNED]
**Priority:** P2
**Estimated Effort:** 2 hours

---

## Medium-Priority Enhancements

### 7. Real Chaos Test ⚠️

**Status:** Mock chaos test exists, but not real

**Current Test:** `test_plugin_chaos_concurrent_operations` simulates concurrent operations on a mock plugin

**Missing:** Actual chaos test that:
1. Starts real AdapterOS server
2. Starts real inference traffic (100 req/s)
3. Kills Git plugin process mid-traffic (SIGKILL)
4. Verifies inference endpoints stay healthy (0 errors)
5. Verifies plugin auto-recovery

**Implementation:**
```rust
#[tokio::test]
async fn test_real_chaos_plugin_kill() {
    // 1. Start server with Git plugin
    let server = start_test_server().await;

    // 2. Start inference traffic generator
    let traffic_handle = tokio::spawn(async move {
        for i in 0..1000 {
            let resp = server.post("/v1/chat/completions")
                .json(&request)
                .send()
                .await;
            assert!(resp.is_ok(), "Inference failed at req {}", i);
        }
    });

    // 3. Wait 2 seconds, then kill Git plugin
    tokio::time::sleep(Duration::from_secs(2)).await;
    kill_plugin_process("git");

    // 4. Traffic should continue without errors
    traffic_handle.await.unwrap();

    // 5. Verify plugin marked as Dead
    let status = server.get("/v1/plugins/git/status").send().await;
    assert_eq!(status.health.status, "Dead");
}
```

**Owner:** [UNASSIGNED]
**Priority:** P2
**Estimated Effort:** 4 hours

---

### 8. Integration Test with Real Server ⚠️

**Status:** Unit tests only, no integration tests

**Required:**
```rust
#[tokio::test]
async fn test_plugin_enable_disable_api() {
    let server = start_test_server().await;

    // Disable Git plugin
    let resp = server.post("/v1/plugins/git/disable")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status, "disabled");

    // Verify status reflects disabled
    let status = server.get("/v1/plugins/git/status")
        .send()
        .await
        .unwrap();
    assert_eq!(status.enabled, false);

    // Re-enable
    let resp = server.post("/v1/plugins/git/enable")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status, "enabled");
}
```

**Owner:** [UNASSIGNED]
**Priority:** P2
**Estimated Effort:** 3 hours

---

## Low-Priority Nice-to-Haves

### 9. Configurable Timeouts

**Current:** All operations use 30s timeout (hardcoded)

**Enhancement:**
```toml
# configs/cp.toml
[plugins]
operation_timeout_secs = 30
health_check_interval_secs = 30
max_consecutive_failures = 3
```

**Owner:** [UNASSIGNED]
**Priority:** P3
**Estimated Effort:** 1 hour

---

### 10. Health Check Adaptive Intervals

**Current:** Fixed 30s health check interval

**Enhancement:** Adjust interval based on stability
- Healthy (no failures): 60s interval
- Degraded (1-2 failures): 30s interval
- Unstable (3+ failures): 10s interval

**Owner:** [UNASSIGNED]
**Priority:** P3
**Estimated Effort:** 2 hours

---

## Documentation Gaps

### 11. PRD Architecture Mismatch

**Issue:** Implementation diverges from PRD 7 specification

**PRD Specifies:**
```rust
enum PluginState { Disabled, Starting, Running, Degraded, Stopped }

struct PluginStatus {
    name: String,
    state: PluginState,
    last_error: Option<String>,
    last_healthy_at: Option<i64>,
}
```

**Implementation Uses:**
```rust
enum PluginStatus { Loaded, Started, Stopped, Degraded(String), Dead(String) }

struct PluginHealth {
    status: PluginStatus,
    last_error: Option<String>,
    last_healthy_at: Option<i64>,
}
```

**Resolution Options:**
1. **Update PRD** to match implementation (recommended)
2. **Refactor code** to match PRD (breaking change)

**Owner:** [UNASSIGNED]
**Priority:** P3
**Estimated Effort:** 1 hour (documentation update)

---

## Verification Checklist

Before marking as production-ready, verify:

### Build & Test
- [ ] `cargo build -p adapteros-server` succeeds
- [ ] `cargo test --test plugin_isolation_safety_tests` passes (all 11 tests)
- [ ] No clippy warnings in plugin code
- [ ] No unused imports or dead code

### Functionality
- [ ] Plugin enable/disable works without server restart
- [ ] Plugin health visible at `/v1/plugins/{name}/status`
- [ ] Timeout enforced on all plugin operations
- [ ] Panics caught and don't crash supervisor
- [ ] Auto-disable after 3 failures
- [ ] Telemetry events persisted to signed bundles

### Integration
- [ ] Real chaos test passes (kill plugin mid-traffic)
- [ ] Integration test with real server passes
- [ ] Metrics visible in Prometheus/Grafana
- [ ] Circuit breaker prevents thundering herd

### Documentation
- [ ] PLUGIN_ISOLATION_SAFETY.md updated with limitations
- [ ] PRD reconciliation documented
- [ ] Runbook created for operators
- [ ] Alert playbook documented

---

## Rollout Plan

### Phase 1: Verification (Week 1)
1. Fix dependency builds
2. Run test suite, fix failures
3. Verify telemetry persistence
4. Add integration tests

### Phase 2: Enhancements (Week 2)
1. Add Prometheus metrics
2. Implement circuit breaker
3. Add graceful shutdown
4. Real chaos test

### Phase 3: Production Deploy (Week 3)
1. Deploy to staging
2. Run chaos tests in staging
3. Gradual rollout to production (10% → 50% → 100%)
4. Monitor metrics and alerts

---

## Sign-Off Criteria

**DO NOT DEPLOY TO PRODUCTION** until:

- ✅ All P0 blockers resolved
- ✅ All P1 items complete
- ✅ Test suite passes (11/11 tests)
- ✅ Chaos test demonstrates resilience
- ✅ Metrics implemented and dashboards created
- ✅ Runbook reviewed by SRE team
- ✅ Security review complete

**Current Status:** ⚠️ **~40% production-ready**

---

## Contact & Ownership

**Implementation:** Claude (Anthropic)
**Date:** 2025-11-17
**Commit:** 9a15cd0 (to be amended)
**Branch:** `claude/plugin-isolation-safety-016mRAjLNr6zoYPTgPRnXiKr`

**Next Owner:** [UNASSIGNED]
**Escalation:** @rogu3bear

---

**This checklist is a living document. Update as work progresses.**
