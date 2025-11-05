# Implementation Completion Plan
## Real-Time Alert Streaming & Operation Tracking Integration

**Date:** 2025-01-15  
**Status:** In Progress  
**Priority:** High

---

## Overview

This plan documents the remaining work to fully integrate real-time alert streaming and operation tracking features with proper citations and codebase standards compliance.

**Citations Format:** `[source: <path> L<start>-L<end>]` per [CITATIONS.md](CITATIONS.md)

---

## 1. Alert Creation Broadcasting

### Current State
- ✅ Alert updates broadcasted in `acknowledge_process_alert` handler
- ❌ Alert creation not broadcasted when alerts are triggered in the evaluator

### Required Changes

**File:** `crates/adapteros-system-metrics/src/alerting.rs`

**Location:** `trigger_alert` method around line 433

**Citation Reference:**
- [source: crates/adapteros-server-api/src/handlers.rs L4598-L4603] - Alert update broadcasting pattern
- [source: crates/adapteros-server-api/src/state.rs L250-L251] - Alert broadcast channel definition

**Implementation:**
```rust
// In trigger_alert method, after ProcessAlert::create succeeds:
let alert_id = ProcessAlert::create(self.db.pool(), alert_request.clone()).await?;

// Fetch the created alert to broadcast
let created_alert = ProcessAlert::get_by_id(self.db.pool(), &alert_id).await?;
if let Some(alert) = created_alert {
    // Convert to ProcessAlertResponse and broadcast
    let response = map_process_alert_to_response(alert);
    // NOTE: Need to pass alert_tx to AlertEvaluator or use a callback
    // This requires refactoring AlertEvaluator to accept broadcast channel
}
```

**Dependencies:**
1. Pass `alert_tx` broadcast channel to `AlertEvaluator` constructor
2. Update `AlertEvaluator::new()` to accept `Option<broadcast::Sender<ProcessAlertResponse>>`
3. Update all `AlertEvaluator` creation sites

**Files to Modify:**
- `crates/adapteros-system-metrics/src/alerting.rs` - Add broadcasting in `trigger_alert`
- `crates/adapteros-system-metrics/src/lib.rs` - Update `AlertEvaluator::new()` signature
- `crates/adapteros-server/src/main.rs` - Pass `alert_tx` when creating evaluator

**Citations:**
- Alert handler pattern: [source: crates/adapteros-server-api/src/handlers.rs L4598-L4603]
- Broadcast channel setup: [source: crates/adapteros-server-api/src/state.rs L310-L312]
- SSE stream pattern: [source: crates/adapteros-server-api/src/handlers.rs L10570-L10583]

---

## 2. Operation Tracker Integration Citations

### Current State
- ✅ `operation_tracker.rs` module exists
- ✅ Handlers implemented (`get_operation_status_handler`, `operation_progress_stream`)
- ❌ Missing citations in handler implementations

### Required Changes

**File:** `crates/adapteros-server-api/src/handlers.rs`

**Add Citations:**
1. **Line 7131-7160** (`get_operation_status_handler`):
   ```rust
   /// Get operation status by resource ID
   /// 
   /// # Citations
   /// - Operation tracker implementation: [source: crates/adapteros-server-api/src/operation_tracker.rs L50-L200]
   /// - State management: [source: crates/adapteros-server-api/src/state.rs L416-L417]
   /// - Progress event type: [source: crates/adapteros-server-api/src/types.rs L78-L88]
   ```

2. **Line 9677-9719** (`operation_progress_stream`):
   ```rust
   /// SSE stream for operation progress updates
   /// 
   /// # Citations
   /// - SSE pattern reference: [source: crates/adapteros-server-api/src/handlers.rs L10570-L10583]
   /// - Broadcast channel: [source: crates/adapteros-server-api/src/state.rs L414-L415]
   /// - Event type: [source: crates/adapteros-server-api/src/types.rs L78-L88]
   ```

**File:** `crates/adapteros-server-api/src/operation_tracker.rs`

**Add Citations:**
- Reference similar patterns in other trackers:
  - [source: crates/adapteros-policy/src/evidence_tracker.rs L172-L187] - Evidence tracking pattern
  - [source: crates/adapteros-concurrent-fs/src/manager.rs L30-L37] - Operation tracking pattern

---

## 3. Missing Handler Implementations

### 3.1 Model Load with Retry Handler

**File:** `crates/adapteros-server-api/src/handlers.rs`

**Current:** Referenced in routes but implementation may be incomplete

**Citation Reference:**
- Retry pattern: [source: crates/adapteros-server-api/src/retry.rs] (if exists)
- Operation tracker: [source: crates/adapteros-server-api/src/operation_tracker.rs L200-L300]
- Config structure: [source: crates/adapteros-server-api/src/state.rs L100-L135]

**Required:**
1. Verify `load_model_with_retry` handler exists and is complete
2. Add citations referencing:
   - Retry configuration: [source: crates/adapteros-server-api/src/state.rs L100-L135]
   - Operation tracking: [source: crates/adapteros-server-api/src/operation_tracker.rs]

### 3.2 Cancel Model Operation Handler

**File:** `crates/adapteros-server-api/src/handlers.rs`

**Current:** Implementation exists at line 6832

**Required:**
- Add citation to operation tracker cancel pattern
- Reference: [source: crates/adapteros-server-api/src/operation_tracker.rs L300-L400] (verify exact line range)

---

## 4. State Management Citations

### File: `crates/adapteros-server-api/src/state.rs`

**Add Citations for New Fields:**

1. **Router Field (Line ~410):**
   ```rust
   /// Router for K-sparse LoRA adapter selection
   /// 
   /// # Citations
   /// - Router implementation: [source: crates/adapteros-lora-router/src/lib.rs]
   /// - K-sparse routing: [source: docs/ARCHITECTURE_INDEX.md] (verify path)
   /// - Deterministic routing: [source: crates/adapteros-lora-worker/src/inference_pipeline.rs]
   ```

2. **Operation Tracker (Line 416):**
   ```rust
   /// Tracker for ongoing adapter operations
   /// 
   /// # Citations
   /// - Implementation: [source: crates/adapteros-server-api/src/operation_tracker.rs L1-L50]
   /// - Progress broadcasting: [source: crates/adapteros-server-api/src/state.rs L414-L415]
   ```

3. **MLX Configuration (Lines ~45-65):**
   ```rust
   /// MLX-specific configuration
   /// 
   /// # Citations
   /// - MLX FFI backend: [source: crates/adapteros-lora-mlx-ffi/src/lib.rs]
   /// - Model runtime integration: [source: crates/adapteros-server-api/src/model_runtime.rs]
   ```

---

## 5. Frontend Integration Citations

### File: `ui/src/components/AlertsPage.tsx`

**Current:** EventSource implementation added but missing citations

**Add Citations:**
1. **SSE Implementation (Line 196-308):**
   ```typescript
   // Real-time alert streaming using EventSource
   // 
   // Citations:
   // - SSE pattern: [source: ui/src/hooks/useActivityFeed.ts L350-L437]
   // - Backend endpoint: [source: crates/adapteros-server-api/src/handlers.rs L10533-L10589]
   // - Event format: [source: crates/adapteros-server-api/src/types.rs L1742-L1760]
   ```

2. **API Client Method:**
   ```typescript
   // In ui/src/api/client.ts, add method if missing:
   // async listAlerts(params?: { limit?: number }): Promise<Alert[]>
   // Citation: [source: crates/adapteros-server-api/src/handlers.rs L4435-L4514]
   ```

---

## 6. Route Registration Citations

### File: `crates/adapteros-server-api/src/routes.rs`

**Add Citations for New Routes:**

1. **Alert Stream Route (Line 402-405):**
   ```rust
   // Alert streaming endpoint
   // Citation: [source: crates/adapteros-server-api/src/handlers.rs L10533-L10589]
   .route(
       "/v1/monitoring/alerts/stream",
       get(handlers::alerts_stream),
   )
   ```

2. **Operation Progress Stream (Line 921):**
   ```rust
   // Operation progress streaming
   // Citation: [source: crates/adapteros-server-api/src/handlers.rs L9677-L9719]
   .route(
       "/v1/stream/operations/progress",
       get(handlers::operation_progress_stream),
   )
   ```

3. **Operation Status Endpoint (Line 927-930):**
   ```rust
   // Operation status query
   // Citation: [source: crates/adapteros-server-api/src/handlers.rs L7131-L7160]
   .route(
       "/v1/operations/:resource_id/status",
       get(handlers::get_operation_status_handler),
   )
   ```

---

## 7. Type Definitions Citations

### File: `crates/adapteros-server-api/src/types.rs`

**Add Citations:**

1. **OperationProgressEvent (Line 78-88):**
   ```rust
   /// Progress event for ongoing operations
   /// 
   /// # Citations
   /// - Usage in SSE stream: [source: crates/adapteros-server-api/src/handlers.rs L9677-L9719]
   /// - Operation tracker: [source: crates/adapteros-server-api/src/operation_tracker.rs L37-L46]
   ```

2. **ProcessAlertResponse (Line 1742):**
   ```rust
   /// Process alert response type
   /// 
   /// # Citations
   /// - Alert streaming: [source: crates/adapteros-server-api/src/handlers.rs L10533-L10589]
   /// - Broadcast channel: [source: crates/adapteros-server-api/src/state.rs L250-L251]
   ```

---

## 8. Testing & Validation

### Required Tests

1. **Alert Broadcasting Test:**
   - File: `tests/integration/alert_streaming.rs` (create if needed)
   - Test alert creation triggers SSE event
   - Citation: [source: crates/adapteros-server-api/src/handlers.rs L10533-L10589]

2. **Operation Progress Test:**
   - File: `tests/integration/operation_tracking.rs` (create if needed)
   - Test operation progress events broadcast correctly
   - Citation: [source: crates/adapteros-server-api/src/handlers.rs L9677-L9719]

3. **Operation Status Query Test:**
   - Verify status endpoint returns correct status
   - Citation: [source: crates/adapteros-server-api/src/handlers.rs L7131-L7160]

---

## 9. Documentation Updates

### Files to Update

1. **API Documentation:**
   - Add SSE endpoint documentation
   - Citation: [source: crates/adapteros-server-api/src/handlers.rs L10533-L10589]

2. **Developer Guide:**
   - Document alert streaming pattern
   - Document operation tracking pattern
   - Reference: [docs/DEVELOPER_GUIDE.md]

---

## Implementation Checklist

### Phase 1: Alert Broadcasting (High Priority)
- [ ] Refactor `AlertEvaluator` to accept `alert_tx` channel
- [ ] Update `trigger_alert` to broadcast new alerts
- [ ] Update all `AlertEvaluator` creation sites
- [ ] Add citations to alert broadcasting code
- [ ] Test alert creation triggers SSE events

### Phase 2: Citations & Documentation (Medium Priority)
- [ ] Add citations to `get_operation_status_handler`
- [ ] Add citations to `operation_progress_stream`
- [ ] Add citations to `operation_tracker.rs`
- [ ] Add citations to state management fields
- [ ] Add citations to route registrations
- [ ] Add citations to type definitions
- [ ] Add citations to frontend SSE implementation

### Phase 3: Validation & Testing (Medium Priority)
- [ ] Create alert streaming integration test
- [ ] Create operation tracking integration test
- [ ] Verify all handlers compile and run
- [ ] Update API documentation

### Phase 4: Code Review (Low Priority)
- [ ] Review all citations for accuracy
- [ ] Verify citation format matches standards
- [ ] Ensure no TODO comments without citations
- [ ] Check for deprecated patterns

---

## Citation Standards Reference

Per workspace rules:
- Format: `[source: <path> L<start>-L<end>]`
- All code references must use deterministic citations
- Line numbers should reference stable code locations
- Module-level citations acceptable for stable APIs

**Reference:**
- [CITATIONS.md](CITATIONS.md) - Citation format standards
- [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) - Code standards
- [docs/ARCHITECTURE_INDEX.md](docs/ARCHITECTURE_INDEX.md) - Architecture patterns

---

## Notes

1. **Alert Evaluator Refactoring:** The biggest change is passing the broadcast channel to `AlertEvaluator`. This may require updating the evaluator's constructor and all instantiation sites.

2. **Operation Tracker:** The module exists and handlers are implemented, but citations need to be added for maintainability.

3. **Frontend:** The EventSource implementation follows existing patterns but needs citations for consistency.

4. **Testing:** Integration tests should verify the full flow from alert creation → broadcast → SSE → frontend update.

---

**Status:** Ready for implementation  
**Next Steps:** Begin Phase 1 (Alert Broadcasting)

