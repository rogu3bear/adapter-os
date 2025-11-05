# Hallucination Patch Plan
## Fixing Critical Implementation Gaps

**Date:** 2025-01-15
**Status:** Critical - Implementation Hallucinations Detected
**Priority:** IMMEDIATE

---

## Executive Summary

Hallucination audit revealed that while the real-time streaming infrastructure is **fully implemented and working**, the core business logic contains **severe hallucinations**:

- ✅ Broadcasting channels work perfectly
- ✅ SSE streams work perfectly
- ❌ **Model loading returns mock responses**
- ❌ **Alert evaluation never runs**
- ❌ **Progress updates never sent during operations**
- ❌ **Tests validate isolated components, not end-to-end flows**

**Result**: Convincing UI shows "real-time" data that's completely fake.

---

## Table of Contents

1. [Model Loading Hallucination](#1-model-loading-hallucination)
2. [Alert Evaluation Hallucination](#2-alert-evaluation-hallucination)
3. [Progress Update Hallucination](#3-progress-update-hallucination)
4. [Test Hallucination](#4-test-hallucination)
5. [Implementation Checklist](#5-implementation-checklist)

---

## 1. Model Loading Hallucination

### Problem Statement
**Severity**: 🚨 CRITICAL

The `load_model_with_retry` function:
- ✅ Creates operation tracker entry
- ✅ Starts retry loop with cancellation checks
- ❌ **Calls `load_model_internal` which returns mock responses**
- ❌ **Never calls `update_progress` during loading**
- ✅ Completes operation (shows 100% instantly)

**Evidence**: `crates/adapteros-server-api/src/handlers.rs` lines 7110-7121

### Solution Architecture

#### Phase 1A: Real Model Loading Logic
**File:** `crates/adapteros-server-api/src/handlers.rs`

**Replace mock implementation with real loading:**

```rust
// Replace lines 7110-7121
// TODO: Implement actual model loading logic here
// For now, return a mock response

// Real implementation:
let mut runtime = state.model_runtime.as_ref()
    .ok_or_else(|| anyhow::anyhow!("Model runtime not available"))?
    .lock().await;

// Validate model exists in database first
let model_record = sqlx::query!(
    "SELECT id, name, model_path, model_type FROM models WHERE id = ?",
    model_id
)
.fetch_optional(state.db.pool())
.await?
.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

// Perform actual model loading with progress callbacks
let model_path = model_record.model_path
    .ok_or_else(|| anyhow::anyhow!("Model path not configured"))?;

// Progress callback closure
let progress_callback = |progress_pct: f64, message: String| {
    // This closure will be called during loading to update progress
    let tracker_clone = tracker.clone();
    let model_id_clone = model_id.clone();
    let tenant_id_clone = tenant_id.clone();
    tokio::spawn(async move {
        let _ = tracker_clone.update_model_progress(
            &model_id_clone,
            &tenant_id_clone,
            progress_pct,
            Some(message),
        ).await;
    });
};

// Load model with progress tracking
runtime.load_model_async_with_progress(
    tenant_id,
    &model_record.id,
    &model_path,
    progress_callback,
    Duration::from_secs(request.timeout_secs.unwrap_or(300)),
).await?;
```

**Dependencies:**
1. Add `load_model_async_with_progress` method to `ModelRuntime`
2. Update `load_model_internal` signature to accept progress callback
3. Add model validation logic

#### Phase 1B: Progress Integration
**File:** `crates/adapteros-server-api/src/handlers.rs`

**Add progress updates in retry loop:**

```rust
// Add progress updates during retry attempts
let load_result = retry_executor
    .execute_with_progress(|attempt, max_attempts| async {
        // Update progress for retry attempt
        let progress_pct = (attempt as f64 / max_attempts as f64) * 50.0; // 0-50% for retries
        let _ = tracker.update_model_progress(
            &model_id,
            tenant_id.as_str(),
            progress_pct,
            Some(format!("Retry attempt {}/{}", attempt, max_attempts)),
        ).await;

        // Check if operation was cancelled
        if tracker.is_operation_cancelled(&model_id, tenant_id.as_str()).await.unwrap_or(false) {
            return Err(anyhow::anyhow!("Operation cancelled by user"));
        }

        // Perform the actual load operation with progress callback
        load_model_internal_with_progress(
            &state,
            &model_id,
            &request,
            tenant_id.as_str(),
            |progress_pct, message| {
                // Convert 0-100 progress to 50-100 range (after retries complete)
                let adjusted_progress = 50.0 + (progress_pct * 0.5);
                let tracker_clone = tracker.clone();
                let model_id_clone = model_id.clone();
                let tenant_id_clone = tenant_id.clone();
                tokio::spawn(async move {
                    let _ = tracker_clone.update_model_progress(
                        &model_id_clone,
                        &tenant_id_clone,
                        adjusted_progress,
                        Some(message),
                    ).await;
                });
            }
        ).await
    })
    .await;
```

**Dependencies:**
1. Update `RetryExecutor` to support progress callbacks
2. Create `load_model_internal_with_progress` function

#### Phase 1C: ModelRuntime Progress Support
**File:** `crates/adapteros-server-api/src/model_runtime.rs`

**Add progress callback support:**

```rust
pub async fn load_model_async_with_progress<F>(
    &mut self,
    tenant_id: &str,
    model_id: &str,
    model_path: &str,
    progress_callback: F,
    timeout: Duration,
) -> Result<(), String>
where
    F: Fn(f64, String) + Send + Sync + 'static,
{
    // Send initial progress
    progress_callback(0.0, "Starting model validation".to_string());

    // Validate model files (10% progress)
    self.validate_model_files(model_path)?;
    progress_callback(10.0, "Model files validated".to_string());

    // Load model weights (20-80% progress)
    // ... actual loading logic with progress updates ...

    // Finalize loading (80-100% progress)
    progress_callback(100.0, "Model loaded successfully".to_string());

    Ok(())
}
```

---

## 2. Alert Evaluation Hallucination

### Problem Statement
**Severity**: 🚨 CRITICAL

The AlertEvaluator is instantiated in AppState but **never actually evaluates alerts**.

**Evidence**: No code calls `evaluate_all_tenants()` or starts evaluation loops.

### Solution Architecture

#### Phase 2A: Alert Evaluation Service
**File:** `crates/adapteros-server/src/main.rs`

**Add alert evaluation service to main server startup:**

```rust
// After state initialization, start alert evaluation service
let alert_evaluator = state.alert_evaluator.clone();
let _ = spawn_deterministic("Alert evaluator".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(30)); // Evaluate every 30 seconds

    loop {
        interval.tick().await;

        if let Err(e) = alert_evaluator.evaluate_all_tenants().await {
            error!("Alert evaluation failed: {}", e);
            // Continue running despite errors
        }
    }
});
```

**Citation**: [source: crates/adapteros-system-metrics/src/alerting.rs L152-173] - AlertEvaluator::start method exists but unused

#### Phase 2B: Alert Evaluator Startup
**File:** `crates/adapteros-system-metrics/src/alerting.rs`

**Update AlertEvaluator to support background evaluation:**

```rust
impl AlertEvaluator {
    /// Start the alert evaluation service in the background
    pub fn start_background_service(self: Arc<Self>) -> Result<(), anyhow::Error> {
        spawn_deterministic("Alert evaluator".to_string(), async move {
            let evaluator = self;
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                interval.tick().await;

                if let Err(e) = evaluator.evaluate_all_tenants().await {
                    error!("Alert evaluation failed: {}", e);
                    // Continue running despite errors
                }
            }
        })
        .map_err(|e| anyhow::anyhow!("Failed to start alert evaluator: {}", e))?;

        Ok(())
    }
}
```

---

## 3. Progress Update Hallucination

### Problem Statement
**Severity**: 🚨 MODERATE

Operations are tracked but `update_progress` is **never called during actual operations**.

### Solution Architecture

#### Phase 3A: Progress Updates in Model Loading
**File:** `crates/adapteros-server-api/src/handlers.rs`

**Add progress updates throughout loading process:**

```rust
// In load_model_internal_with_progress
pub async fn load_model_internal_with_progress<F>(
    state: &AppState,
    model_id: &str,
    request: &LoadModelRequest,
    tenant_id: &str,
    progress_callback: F,
) -> Result<ModelResponse>
where
    F: Fn(f64, String) + Send + Sync + 'static,
{
    progress_callback(0.0, "Starting model load".to_string());

    // Validate model exists (10%)
    let model_record = sqlx::query!(
        "SELECT id, name, model_path, model_type FROM models WHERE id = ?",
        model_id
    )
    .fetch_optional(state.db.pool())
    .await?
    .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

    progress_callback(10.0, "Model record validated".to_string());

    // Get model runtime (20%)
    let mut runtime = state.model_runtime.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Model runtime not available"))?
        .lock().await;

    progress_callback(20.0, "Model runtime acquired".to_string());

    // Load model with progress (20-90%)
    let model_path = model_record.model_path
        .ok_or_else(|| anyhow::anyhow!("Model path not configured"))?;

    runtime.load_model_async_with_progress(
        tenant_id,
        &model_record.id,
        &model_path,
        |pct, msg| {
            // Convert 0-100 to 20-90 range
            let adjusted_pct = 20.0 + (pct * 0.7);
            progress_callback(adjusted_pct, msg);
        },
        Duration::from_secs(request.timeout_secs.unwrap_or(300)),
    ).await?;

    progress_callback(90.0, "Model loaded, finalizing".to_string());

    // Create response (100%)
    let response = ModelResponse {
        id: model_record.id,
        name: model_record.name,
        model_type: model_record.model_type,
        status: "loaded".to_string(),
        loaded_at: Some(chrono::Utc::now()),
        memory_usage: Some(1024 * 1024 * 1024), // TODO: Get real usage from runtime
    };

    progress_callback(100.0, "Model load completed".to_string());

    Ok(response)
}
```

#### Phase 3B: Progress Updates in Unload Operations
**File:** `crates/adapteros-server-api/src/handlers.rs`

**Add similar progress tracking for unload operations.**

---

## 4. Test Hallucination

### Problem Statement
**Severity**: 🚨 MODERATE

Integration tests test components in isolation but **don't validate end-to-end user flows**.

### Solution Architecture

#### Phase 4A: End-to-End Alert Streaming Test
**File:** `tests/integration/alert_streaming.rs`

**Add real end-to-end test:**

```rust
#[tokio::test]
async fn test_alert_creation_end_to_end_sse() {
    // Setup full server with routes
    let app = create_test_server_with_alerts().await;

    // Create a monitoring rule via API
    let rule_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/monitoring/rules")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{
                    "name": "Test High CPU",
                    "tenant_id": "test-tenant",
                    "metric_name": "cpu_usage",
                    "threshold_value": 80.0,
                    "severity": "warning"
                }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(rule_response.status(), StatusCode::CREATED);

    // Trigger alert creation via metrics API
    let metrics_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/metrics")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{
                    "cpu_usage": 85.0,
                    "tenant_id": "test-tenant"
                }"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Verify alert appears in SSE stream
    // (This requires more complex test setup with SSE client)
}
```

#### Phase 4B: End-to-End Model Loading Test
**File:** `tests/integration/model_operations.rs`

**Add real model loading test with progress verification.**

---

## 5. Implementation Checklist

### Phase 1: Model Loading Reality (Priority: CRITICAL)
- [ ] **1A**: Replace `load_model_internal` mock with real implementation
- [ ] **1B**: Add progress updates in retry loop  
- [ ] **1C**: Add progress callback support to ModelRuntime
- [ ] **Test**: Verify progress updates appear in SSE stream

### Phase 2: Alert Evaluation Reality (Priority: CRITICAL)
- [ ] **2A**: Start alert evaluation service in main.rs
- [ ] **2B**: Add AlertEvaluator::start_background_service method
- [ ] **Test**: Verify alerts are created and broadcast

### Phase 3: Progress Update Reality (Priority: HIGH)
- [ ] **3A**: Add progress updates in model loading
- [ ] **3B**: Add progress updates in model unloading
- [ ] **Test**: Verify progress bars update in real-time

### Phase 4: Test Reality (Priority: MEDIUM)
- [ ] **4A**: Add end-to-end alert streaming test
- [ ] **4B**: Add end-to-end model loading test
- [ ] **Verify**: Tests validate actual user-facing functionality

### Validation Steps
- [ ] SSE streams show real data, not empty/fake
- [ ] Progress bars update during operations
- [ ] Alerts appear when conditions are met
- [ ] Operations complete with real results
- [ ] Tests fail when features are broken

---

## Citations Reference

### Existing Infrastructure (Working)
- Broadcast channels: [source: crates/adapteros-server-api/src/state.rs L427-428]
- SSE handlers: [source: crates/adapteros-server-api/src/handlers.rs L12929-12935]
- Operation tracking: [source: crates/adapteros-server-api/src/operation_tracker.rs L1-L50]
- EventSource frontend: [source: ui/src/components/AlertsPage.tsx L196-240]

### Implementation References
- Alert evaluation: [source: crates/adapteros-system-metrics/src/alerting.rs L223-277]
- Model runtime: [source: crates/adapteros-server-api/src/model_runtime.rs L349-520]
- Progress tracking: [source: crates/adapteros-server-api/src/operation_tracker.rs L315-340]
- Retry logic: [source: crates/adapteros-server-api/src/retry.rs]

---

## Risk Assessment

### High Risk Issues
1. **Model Loading**: Currently returns mock responses - **breaks core functionality**
2. **Alert Evaluation**: Never runs - **breaks monitoring**
3. **Progress Updates**: Never sent - **breaks UX**

### Mitigation Strategies
1. **Incremental Deployment**: Deploy fixes one at a time with feature flags
2. **Comprehensive Testing**: Add end-to-end tests before deployment
3. **Rollback Plan**: Keep mock implementations as fallback
4. **Monitoring**: Add metrics to detect fake vs real responses

---

## Success Criteria

### Before Fix
- ✅ SSE streams work (empty/fake data)
- ✅ Progress bars work (static values)
- ✅ Alert UI works (no alerts shown)
- ❌ Model loading works (returns mocks)
- ❌ Alert creation works (never runs)
- ❌ Progress updates work (never sent)

### After Fix
- ✅ SSE streams work (real data)
- ✅ Progress bars work (dynamic updates)
- ✅ Alert UI works (shows real alerts)
- ✅ Model loading works (loads real models)
- ✅ Alert creation works (evaluates metrics)
- ✅ Progress updates work (real-time updates)

---

**Bottom Line**: The streaming infrastructure is production-ready. The hallucinations are in the business logic layer. Fix the core logic, and the real-time features will work perfectly.</contents>
</xai:function_call<?>><xai:function_call name="read_file">
<parameter name="target_file">crates/adapteros-server-api/src/model_runtime.rs
