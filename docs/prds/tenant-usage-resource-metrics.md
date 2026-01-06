# PRD: Tenant Usage Resource Metrics

**Status:** Draft  
**Last Updated:** 2026-01-05  
**Owner:** Engineering  
**Related Docs:** docs/API_REFERENCE.md, docs/OPERATIONS.md, docs/ARCHITECTURE.md

---

## 1. Summary

The `TenantUsage` API currently returns zero values for storage, CPU, GPU, and memory. This PRD defines the work to compute real per-tenant metrics with caching and to keep the API fast and safe for production use.

---

## 2. Problem Statement

Tenant dashboards and capacity planning rely on resource usage metrics, but the API reports placeholder values. This makes operational decisions unreliable and hides actual resource consumption.

---

## 3. Goals

1. Report real `storage_used_gb` per tenant.
2. Report GPU and memory usage where available and return 0 gracefully where not.
3. Provide CPU usage with acceptable cost and no blocking behavior.
4. Cache metrics with a short TTL to keep latency below 50ms.

---

## 4. Non-Goals

- Building a new monitoring or telemetry pipeline.
- Providing per-process or per-request GPU metrics.
- Changing the `TenantUsage` API shape beyond populating existing fields.

---

## 5. Proposed Approach

- Compute storage from existing database artifacts and dataset size tables.
- Read GPU memory stats from the Metal kernel pool when available.
- Use system memory and CPU metrics from a lightweight system snapshot.
- Cache computed results with a 5-second TTL to protect API latency.

---

## 6. Requirements and Implementation Plan

### R1: Storage Metrics

**Requirement:** `storage_used_gb` sums artifact and dataset sizes per tenant.

**Implementation Tasks:**
- Add a query to sum `artifacts.size_bytes` by tenant.
- Add a query to sum `training_datasets.total_size_bytes` by tenant.
- Convert to GiB with stable constants.

**Acceptance Criteria:**
- Storage value equals the sum of known test fixtures.
- Empty tenants return 0.0 without errors.

---

### R2: GPU Usage Metrics

**Requirement:** `gpu_usage_pct` reflects current GPU memory utilization when available.

**Implementation Tasks:**
- Read `GpuMemoryStats` from the Metal pool when the backend is enabled.
- Return 0.0 when GPU stats are unavailable.

**Acceptance Criteria:**
- GPU usage is non-zero when allocations exist.
- Systems without GPU support return 0.0.

---

### R3: Memory and CPU Metrics

**Requirement:** `memory_used_gb`, `memory_total_gb`, and `cpu_usage_pct` are populated.

**Implementation Tasks:**
- Capture system metrics with a lightweight snapshot.
- Convert bytes to GiB and compute CPU percent.

**Acceptance Criteria:**
- Values are within expected ranges for a test environment.
- No blocking or long-running sampling in the request path.

---

### R4: Metrics Caching

**Requirement:** Metrics are cached with a 5-second TTL.

**Implementation Tasks:**
- Add a cache keyed by tenant id.
- Recompute metrics only when the cache is stale.

**Acceptance Criteria:**
- Repeated calls within TTL return cached values.
- Cache refresh does not exceed 50ms per request.

---

### R5: Tests and Documentation

**Requirement:** Coverage and docs reflect the new metrics.

**Implementation Tasks:**
- Add unit tests for storage calculations and cache behavior.
- Add tests for GPU/memory fallback paths.
- Update API docs with metric definitions.

**Acceptance Criteria:**
- Tests pass for both GPU-present and GPU-absent environments.
- API docs describe units and caching behavior.

---

## 7. Test Plan

- Unit tests for storage sums and cache TTL behavior.
- Integration test to validate non-zero metrics with seeded data.
- Regression test to ensure metrics return 0.0 on systems without GPU.

---

## 8. Rollout Plan

1. Phase 1: Ship storage metrics and caching.
2. Phase 2: Add GPU and memory metrics with safe fallbacks.
3. Phase 3: Tune caching and document operational guidance.

---

## 9. Open Questions

1. Should CPU usage be host-wide or scoped to AdapterOS processes?
2. Do we need per-tenant GPU metrics or only system-level utilization?
