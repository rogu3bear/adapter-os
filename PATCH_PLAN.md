# Patch Plan: Fix AdapterOS API Compilation Errors

**Date:** 2025-01-15  
**Status:** Pending Implementation  
**Goal:** Resolve all compilation errors in `adapteros-api` crate while maintaining codebase standards

---

## Current State

### Compilation Errors

1. **Handler Trait Error** (Line 425)
   - Error: `the trait bound fn(...) {inference_handler_metal}: Handler<_, _> is not satisfied`
   - Location: `crates/adapteros-api/src/lib.rs:425`
   - Root Cause: Concrete handlers don't satisfy Axum's `Handler` trait requirements

2. **Router Service Conversion** (Line 179)
   - Error: `no method named 'call' found for struct 'axum::Router'`
   - Location: `crates/adapteros-api/src/lib.rs:179`
   - Root Cause: Router needs conversion to Tower service for hyper connections

3. **Send/Sync Violation** (Line 275)
   - Error: `*mut () cannot be sent between threads safely`
   - Location: `crates/adapteros-api/src/lib.rs:275`
   - Root Cause: Generic closures capture non-Send types

---

## Implementation Plan

### Phase 1: Fix Router Service Conversion

**Objective:** Convert Axum Router to Tower Service for hyper compatibility

**Reference Patterns:**
- [source: crates/adapteros-server/src/main.rs L1512-L1575] - Pattern for UDS serving with Router
- [source: crates/adapteros-service-supervisor/src/server.rs L66-L71] - Using `axum::serve()` pattern

**Changes Required:**

1. **Fix Router Service Pattern** (Line 158-179)
   ```rust
   // Current (broken):
   let app = build_router_metal(state);
   // ... later ...
   let tower_service = app.clone();  // ❌ Router type may not match
   
   // Fix: In Axum 0.7, Router with state implements Service
   // But the Router type must match exactly. Check:
   // - Router type from build_router_metal matches app variable type
   // - State type matches between Router and handlers
   // Reference: adapteros-server/src/main.rs L1512 uses app.clone() directly
   ```

2. **Verify Router Type Consistency** (Line 432-459)
   ```rust
   // Ensure build_router_metal returns Router with correct state type
   // And app variable type matches
   fn build_router_metal(state: Arc<ApiState<...>>) -> Router<Arc<ApiState<...>>>
   // Must match exactly, no type aliases
   ```

3. **Update Service Usage** (Line 179)
   ```rust
   // Current pattern from adapteros-server/src/main.rs L1562-L1571:
   let hyper_svc = hyper::service::service_fn(move |req| {
       let mut svc_clone = svc.clone();
       async move {
           svc_clone.call(req).await.map_err(|e| {
               // error handling
           })
       }
   });
   ```

**Citation:** [source: crates/adapteros-server/src/main.rs L1512-L1575] - Correct UDS serving pattern

---

### Phase 2: Fix Concrete Handler Trait Implementation

**Objective:** Make concrete MetalKernels handlers satisfy Axum's Handler trait

**Reference Patterns:**
- [source: crates/adapteros-service-supervisor/src/server.rs L77-L87] - Simple handler pattern
- [source: crates/adapteros-server-api/src/handlers/journeys.rs L35-L40] - Handler with State extractor

**Changes Required:**

1. **Use Handler Pattern Matching Existing Codebase** (Line 357-429)
   ```rust
   // Current (broken):
   async fn inference_handler_metal(
       State(state): State<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>,
       Json(request): Json<InferenceRequest>,
   ) -> Result<Json<InferenceResponse>, ApiError> {
       inference_handler(State(state), Json(request)).await
   }
   
   // Fix: Ensure handlers match exact signature pattern
   // Option A: Use closures (like generic implementation)
   // Option B: Ensure State extractor pattern matches exactly
   ```

2. **Verify Handler Registration** (Line 432-459)
   ```rust
   // Ensure Router state type matches handler State type exactly
   fn build_router_metal(state: Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>) 
       -> Router<Arc<ApiState<adapteros_lora_kernel_mtl::MetalKernels>>>
   ```

**Citation:** [source: crates/adapteros-service-supervisor/src/server.rs L42-L56] - Router with handlers pattern

---

### Phase 3: Fix Generic Implementation Send/Sync Issues

**Objective:** Resolve `*mut ()` Send/Sync violation in generic closures

**Reference Patterns:**
- [source: crates/adapteros-api/src/lib.rs L61-L62] - Unsafe Send/Sync impls
- [source: crates/adapteros-lora-kernel-api/src/lib.rs] - FusedKernels trait definition

**Changes Required:**

1. **Verify FusedKernels Trait Bounds** (Check trait definition)
   ```rust
   // Ensure FusedKernels has Send + Sync bounds if needed
   // Location: crates/adapteros-lora-kernel-api/src/lib.rs
   pub trait FusedKernels: Send + Sync {  // ✅ Should have both
   ```

2. **Fix Generic Closure Pattern** (Line 274-310)
   ```rust
   // Current (broken):
   .route("/inference", post(|s: State<Arc<ApiState<K>>>, request: Json<InferenceRequest>| async move {
       inference_handler(s, request).await
   }))
   
   // Issue: Closures with generics may not satisfy Send bounds
   // Fix: Use explicit type erasure or ensure all captured types are Send + Sync
   ```

3. **Verify ApiState Send/Sync** (Line 61-62)
   ```rust
   // Current unsafe impls exist but may need adjustment:
   unsafe impl<K: FusedKernels + Send + Sync> Send for ApiState<K> {}
   unsafe impl<K: FusedKernels + Send + Sync> Sync for ApiState<K> {}
   ```

**Citation:** [source: crates/adapteros-api/src/lib.rs L61-L62] - Existing Send/Sync impls

---

## Implementation Steps

### Step 1: Fix Router Service Conversion (Priority: HIGH)

1. Replace direct Router cloning with proper service conversion
2. Reference: [source: crates/adapteros-server/src/main.rs L1512-L1575]
3. Use either:
   - `axum::serve()` pattern (simpler, recommended)
   - `Router::into_make_service()` + manual hyper handling

### Step 2: Fix Concrete Handlers (Priority: HIGH)

1. Verify handler signatures match Axum requirements
2. Reference: [source: crates/adapteros-service-supervisor/src/server.rs L77-L87]
3. Ensure State extractor types match Router state type exactly
4. Test compilation after changes

### Step 3: Fix Generic Implementation (Priority: MEDIUM)

1. Verify FusedKernels trait has Send + Sync bounds
2. Reference: [source: crates/adapteros-lora-kernel-api/src/lib.rs]
3. Adjust closure pattern if needed
4. Verify ApiState Send/Sync impls are correct

### Step 4: Validation

1. Run `cargo check --package adapteros-api --all-targets`
2. Verify no compilation errors
3. Run `cargo clippy --package adapteros-api -- -D warnings`
4. Verify no linting errors

---

## Code Standards Compliance

### Error Handling

- [ ] All errors use `AosError` from `adapteros-core`
- [ ] Error propagation uses `?` operator
- [ ] Reference: [source: crates/adapteros-api/src/lib.rs L543-L549]

### Logging

- [ ] Use `tracing` macros, not `println!`
- [ ] Structured logging with context fields
- [ ] Reference: [source: crates/adapteros-api/src/lib.rs L288-L293]

### Documentation

- [ ] Public functions have doc comments
- [ ] Examples in doc comments where applicable
- [ ] Reference: [source: crates/adapteros-api/src/lib.rs L77-L98]

---

## Testing Checklist

- [ ] `cargo check --package adapteros-api` passes
- [ ] `cargo check --workspace` passes
- [ ] `cargo clippy --package adapteros-api -- -D warnings` passes
- [ ] No unsafe code in application logic (only in FFI/transmute where justified)
- [ ] Handlers properly extract State
- [ ] Router can be used as service for hyper connections

---

## Notes

1. **Type Alias Issue**: `MetalApiState` is a type alias for `ApiState<MetalKernels>`. While they're the same type at runtime, Rust's type system treats them differently. The concrete handlers use the full type path to avoid alias issues.

2. **Service Pattern**: The codebase uses two patterns:
   - `axum::serve()` (simpler, used in adapteros-service-supervisor)
   - Manual hyper + Tower service (used in adapteros-server for UDS)

3. **Send/Sync**: The `*mut ()` error suggests a raw pointer is being captured. This may be from the Worker or FusedKernels implementation. Need to verify trait bounds.

---

## References

- [source: crates/adapteros-server/src/main.rs L1512-L1575] - UDS serving pattern
- [source: crates/adapteros-service-supervisor/src/server.rs L66-L71] - axum::serve pattern
- [source: crates/adapteros-service-supervisor/src/server.rs L77-L87] - Handler pattern
- [source: crates/adapteros-api/src/lib.rs L61-L62] - Send/Sync impls
- [source: crates/adapteros-api/src/lib.rs L543-L549] - Error handling
- [source: crates/adapteros-lora-kernel-api/src/lib.rs] - FusedKernels trait

---

**Next Actions:** Begin implementation starting with Phase 1 (Router Service Conversion) as it blocks the other fixes.

