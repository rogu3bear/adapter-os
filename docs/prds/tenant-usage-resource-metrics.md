# PRD: Implement Real TenantUsage Resource Metrics

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-db/src/tenants.rs`, `crates/adapteros-api-types/src/tenants.rs`, `crates/adapteros-server-api/src/handlers/tenants.rs`

---

## 1. Summary

The TenantUsage API currently returns zero values for storage and system metrics, which makes capacity planning and dashboards unreliable. This PRD defines how to compute real tenant usage metrics using existing database state and system telemetry with a short TTL cache.

---

## 2. Problem Statement

Tenant usage endpoints expose key operational metrics, but storage, CPU, GPU, and memory values are hardcoded to zero. As a result, operators cannot see actual resource usage, and UI surfaces show misleading information.

---

## 3. Goals

- Report accurate storage consumption per tenant based on persisted artifacts and datasets.
- Report system CPU/GPU/memory usage with a bounded refresh interval.
- Preserve tenant isolation (no cross-tenant aggregation leakage).
- Keep API latency under 50 ms with caching.

---

## 4. Non-Goals

- Per-adapter or per-request time series analytics.
- Adding new external monitoring dependencies.
- Real-time GPU per-tenant attribution (only system-level metrics in this phase).
- Changes to billing or quota enforcement logic.

---

## 5. Current State

- `TenantUsage` in `crates/adapteros-db/src/tenants.rs` sets storage/CPU/GPU/memory to 0.0.
- `TenantUsageResponse` in `crates/adapteros-api-types/src/tenants.rs` does not include storage metrics.
- GPU memory stats exist in `adapteros-lora-kernel-mtl`, but are not wired into API responses.

---

## 6. Proposed Approach

### 6.1 Storage Calculation

- Sum artifact sizes from the artifacts table for the tenant.
- Sum training dataset sizes for the tenant.
- Convert bytes to GB using 1024 * 1024 * 1024.

### 6.2 System Metrics

- Use `sysinfo` for CPU and memory usage.
- Use `adapteros-lora-kernel-mtl` GPU stats where available; return 0 when no GPU or feature disabled.
- Keep all metrics collection non-blocking and fail-open (return 0 on errors).

### 6.3 Caching

- Cache computed metrics per tenant with a 5 second TTL.
- Use an in-memory cache (DashMap or similar) scoped to the API layer.

### 6.4 API Contract Updates

- Add `storage_used_gb` to `TenantUsageResponse` to align with DB model.
- Document which fields are system-level (CPU/GPU/memory) vs tenant-scoped (storage).

---

## 7. Acceptance Criteria

- `storage_used_gb` reflects the sum of tenant artifacts and datasets.
- `cpu_usage_pct`, `gpu_usage_pct`, and `memory_used_gb` report non-zero values on supported systems.
- Metrics refresh within 5 seconds of changes.
- API responses remain under 50 ms on steady state.
- Unit tests validate storage calculations against known values.

---

## 8. Test Plan

- Unit tests for SQL aggregation queries (artifacts and training datasets).
- Unit tests for cache TTL behavior.
- Integration test that isolates two tenants and ensures storage metrics do not leak.
- Conditional test for GPU stats when Metal feature is enabled.

---

## 9. Rollout Plan

1. Phase 1: Storage metrics from DB queries with caching.
2. Phase 2: System CPU and memory metrics via `sysinfo`.
3. Phase 3: GPU usage metrics when Metal backend is available.

---

## 10. Follow-up Tasks (Tracked)

- TASK-1: Implement storage aggregation queries for artifacts and datasets.
  - Acceptance: unit tests cover empty and non-empty tenant cases.
- TASK-2: Add system metrics collection with `sysinfo` and guard rails.
  - Acceptance: metrics return 0 on unsupported platforms without panics.
- TASK-3: Add GPU stats wiring for Metal backend.
  - Acceptance: GPU usage percentage is computed from pool stats when enabled.
- TASK-4: Add 5-second TTL cache in API layer.
  - Acceptance: repeated calls within TTL do not re-query DB.
- TASK-5: Update API types to include `storage_used_gb` and document field semantics.
  - Acceptance: schema and OpenAPI docs updated.
