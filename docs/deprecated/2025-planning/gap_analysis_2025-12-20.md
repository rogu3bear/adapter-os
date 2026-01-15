# adapterOS Gap Analysis: Tenant Isolation, Lifecycle, Runtime, and Cache

**Date:** 2025-12-20
**Scope:** PRD-RECT-001 through PRD-RECT-005

---

## Executive Summary

| Area | Status | Priority | Action Required |
|------|--------|----------|-----------------|
| **Tenant Isolation** | PARTIAL | HIGH | DB queries missing tenant predicates; tests needed |
| **Worker Lifecycle** | PARTIAL | MEDIUM | Tenant scoping + transition validation pending |
| **Cache Eviction** | IMPLEMENTED | LOW | Already production-ready; NOT over-engineered |
| **Runtime Paths** | IMPLEMENTED | LOW | Comprehensive /tmp rejection in place |
| **Q15 Denominator** | CORRECT | N/A | Uses 32767.0 throughout (verified) |

---

## 1. Tenant Isolation Gaps (PRD-RECT-001/004)

### Critical DB Queries Missing Tenant Predicates

| Function | File | Line | Risk |
|----------|------|------|------|
| `list_adapters()` | adapters.rs | 882 | CRITICAL (deprecated but callable) |
| `list_adapters_by_category()` | adapters.rs | 2106 | HIGH - cross-tenant enumeration |
| `list_adapters_by_scope()` | adapters.rs | 2131 | HIGH - cross-tenant enumeration |
| `list_adapters_by_state()` | adapters.rs | 2156 | HIGH - cross-tenant enumeration |
| `get_adapter_state_summary()` | adapters.rs | 2181 | HIGH - aggregate leakage |
| `list_models()` | models.rs | 359 | MEDIUM - returns all tenants |

### What's Working

- Handler layer uses tenant-scoped queries correctly
- `validate_tenant_isolation()` in security module is comprehensive
- Cross-tenant access returns 404 (not 403) - proper design

### Missing Tests

```
crates/adapteros-server-api/tests/tenant_isolation_adapters.rs
```

Need tests for:
- [ ] `list_adapters_by_category()` cross-tenant leakage
- [ ] `list_adapters_by_scope()` cross-tenant leakage
- [ ] `list_adapters_by_state()` cross-tenant leakage
- [ ] `get_adapter_state_summary()` information disclosure
- [ ] Adapter lineage traversing tenant boundaries

---

## 2. Worker Lifecycle Scoping (PRD-RECT-002)

### Current State

**Implemented:**
- `get_worker_for_tenant()` - returns None for cross-tenant (indistinguishable from not-found)
- `list_workers_by_tenant()` - correctly scoped
- Worker status history includes tenant_id
- Telemetry events tagged with tenant_id

**Gaps:**
1. `register_worker()` - No explicit cross-tenant denial test
2. `notify_worker_status()` - Could report status for wrong tenant's worker
3. Storage path tenant scoping not validated (UDS paths include tenant but no traversal tests)
4. Telemetry buffer filtering by tenant_id not verified

### Pending Validation

```
crates/adapteros-server-api/tests/worker_lifecycle_tenant_scoping.rs
```

Need tests for:
- [ ] Storage path cross-tenant traversal prevention
- [ ] Telemetry event routing to correct tenant sink
- [ ] Worker incident listing blocked for cross-tenant queries

---

## 3. Cache Eviction Logic Diagram (PRD-RECT-003)

### Decision: Cache is NECESSARY, NOT Over-Engineered

The cache prevents:
- Redundant 7B-14B model reloads on every adapter swap
- Memory waste from N threads loading same model
- Base model eviction during rapid adapter churn

```
┌─────────────────────────────────────────────────────────────────────┐
│                     MODEL HANDLE CACHE FLOW                         │
│                    (32767.0 Q15 denominator)                        │
└─────────────────────────────────────────────────────────────────────┘

                         ┌──────────────┐
                         │ get_or_load()│
                         └──────┬───────┘
                                │
                    ┌───────────▼───────────┐
                    │   Cache Hit Check     │
                    │   (by ModelKey)       │
                    └───────────┬───────────┘
                                │
                 ┌──────────────┴──────────────┐
                 │                             │
            ┌────▼────┐                   ┌────▼────┐
            │   HIT   │                   │  MISS   │
            └────┬────┘                   └────┬────┘
                 │                             │
         ┌───────▼───────┐            ┌────────▼────────┐
         │ Update Stats  │            │ Calculate Size  │
         │ access_count++│            │ needed_bytes    │
         └───────┬───────┘            └────────┬────────┘
                 │                             │
                 │                    ┌────────▼────────┐
                 │                    │ current + needed│
                 │                    │ > max_memory?   │
                 │                    └────────┬────────┘
                 │                             │
                 │              ┌──────────────┴──────────────┐
                 │              │                             │
                 │         ┌────▼────┐                   ┌────▼────┐
                 │         │   NO    │                   │   YES   │
                 │         └────┬────┘                   └────┬────┘
                 │              │                             │
                 │              │                    ┌────────▼────────┐
                 │              │                    │  EVICTION LOOP  │
                 │              │                    │  (see below)    │
                 │              │                    └────────┬────────┘
                 │              │                             │
                 │         ┌────▼────────────────────────────▼────┐
                 │         │           Load Model                 │
                 │         │  (Metal/MLX/CoreML based on key)     │
                 │         └────────────────┬─────────────────────┘
                 │                          │
                 │                 ┌────────▼────────┐
                 │                 │  Insert Entry   │
                 │                 │  loaded_at=now  │
                 │                 └────────┬────────┘
                 │                          │
                 └────────────┬─────────────┘
                              │
                     ┌────────▼────────┐
                     │ Return Handle   │
                     └─────────────────┘


┌─────────────────────────────────────────────────────────────────────┐
│                      EVICTION DECISION TREE                         │
└─────────────────────────────────────────────────────────────────────┘

                    ┌───────────────────┐
                    │ Sort Candidates   │
                    │ loaded_at ASC     │◄─── Oldest first (LRU)
                    │ access_count ASC  │◄─── Least used tie-break
                    │ ModelKey Ord      │◄─── Deterministic final tie-break
                    └─────────┬─────────┘
                              │
                    ┌─────────▼─────────┐
                    │ For each candidate│
                    └─────────┬─────────┘
                              │
                 ┌────────────▼────────────┐
                 │   BLOCKING FACTOR #1    │
                 │   is_pinned(key)?       │
                 └────────────┬────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
         ┌────▼────┐                     ┌────▼────┐
         │   YES   │                     │   NO    │
         └────┬────┘                     └────┬────┘
              │                               │
    ┌─────────▼─────────┐          ┌──────────▼──────────┐
    │ SKIP (base model  │          │ BLOCKING FACTOR #2  │
    │ must stay loaded) │          │ has ActiveGuard?    │
    │ eviction_skip++   │          └──────────┬──────────┘
    └───────────────────┘                     │
                               ┌──────────────┴──────────────┐
                               │                             │
                          ┌────▼────┐                   ┌────▼────┐
                          │   YES   │                   │   NO    │
                          └────┬────┘                   └────┬────┘
                               │                             │
                     ┌─────────▼─────────┐         ┌─────────▼─────────┐
                     │ SKIP (in-flight   │         │ BLOCKING FACTOR #3│
                     │ inference active) │         │ Re-validate status│
                     │ eviction_skip++   │         │ (race condition)  │
                     └───────────────────┘         └─────────┬─────────┘
                                                             │
                                            ┌────────────────┴────────────────┐
                                            │                                 │
                                       ┌────▼────┐                       ┌────▼────┐
                                       │ CHANGED │                       │  SAME   │
                                       └────┬────┘                       └────┬────┘
                                            │                                 │
                                  ┌─────────▼─────────┐              ┌────────▼────────┐
                                  │ SKIP (concurrent  │              │    EVICT IT     │
                                  │ operation changed │              │ freed += size   │
                                  │ entry state)      │              └────────┬────────┘
                                  └───────────────────┘                       │
                                                                    ┌─────────▼─────────┐
                                                                    │ freed >= target?  │
                                                                    └─────────┬─────────┘
                                                               ┌──────────────┴──────────────┐
                                                               │                             │
                                                          ┌────▼────┐                   ┌────▼────┐
                                                          │   YES   │                   │   NO    │
                                                          └────┬────┘                   └────┬────┘
                                                               │                             │
                                                      ┌────────▼────────┐         ┌──────────▼──────────┐
                                                      │     SUCCESS     │         │ Continue to next    │
                                                      │  Return Ok(())  │         │ candidate...        │
                                                      └─────────────────┘         └─────────────────────┘


┌─────────────────────────────────────────────────────────────────────┐
│                    BLOCKING FACTOR #4                               │
│         (When all candidates exhausted but still over budget)       │
└─────────────────────────────────────────────────────────────────────┘

                    ┌───────────────────┐
                    │ All candidates    │
                    │ exhausted?        │
                    └─────────┬─────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
         ┌────▼────┐                     ┌────▼────┐
         │   YES   │                     │   NO    │
         └────┬────┘                     └────┬────┘
              │                               │
    ┌─────────▼─────────┐              (continue loop)
    │ freed >= target?  │
    └─────────┬─────────┘
              │
   ┌──────────┴──────────┐
   │                     │
┌──▼──┐              ┌───▼───┐
│ YES │              │  NO   │
└──┬──┘              └───┬───┘
   │                     │
┌──▼──────────┐    ┌─────▼─────────────────────────────┐
│  SUCCESS    │    │  OVER-LIMIT ALLOWED               │
│  Ok(())     │    │  (All pinned/active - can't evict │
│             │    │   Return Ok but cache exceeds max)│
└─────────────┘    │  Emit: model_eviction_budget_error│
                   └───────────────────────────────────┘


┌─────────────────────────────────────────────────────────────────────┐
│                         MODEL KEY (Cache Key)                       │
└─────────────────────────────────────────────────────────────────────┘

    ModelKey = (
        backend_type,      // Metal | MLX | CoreML
        manifest_hash,     // BLAKE3 of model manifest
        kernel_version,    // Shader/kernel build version
        quantization_mode, // Q15 (32767.0 denom) | F16 | F32
        fusion_mode        // Fused | Unfused
    )

    Different backends/builds/modes cache separately for correctness.


┌─────────────────────────────────────────────────────────────────────┐
│                      METRICS / OBSERVABILITY                        │
└─────────────────────────────────────────────────────────────────────┘

    Prometheus Counters:
    ├── model_cache_hits_total
    ├── model_cache_misses_total
    ├── model_cache_eviction_blocked_pinned_total
    └── model_cache_pinned_entries (Gauge)

    CacheStats struct:
    ├── hits: u64
    ├── misses: u64
    ├── evictions: u64
    ├── total_memory_bytes: u64
    ├── eviction_skip_pinned_count: u64
    └── eviction_skip_active_count: u64

    Telemetry Events:
    ├── model_load_failed_event
    └── model_eviction_budget_error_event (with details)
```

### Cache Eviction Verdict

**Keep the implementation as-is.** It is:
- Essential for model deduplication (prevents N threads loading 7B-14B model)
- Essential for adapter churn (base model stays pinned)
- Well-tested (22 tests in `model_handle_cache_eviction.rs`)
- Properly observable (Prometheus metrics, event listeners)
- Deterministic (sorted eviction with tie-breakers)

---

## 4. Runtime Path Security

### Status: FULLY IMPLEMENTED

Protected paths (reject `/tmp` and `/private/tmp`):
- Telemetry directory
- Manifest cache
- Adapters root
- Database URL
- Index root
- Model cache
- Status path
- Datasets root
- Documents root
- Artifacts root
- Bundles root
- Worker sockets (UDS)

Features:
- Literal string check (fast path)
- Symlink canonicalization (detects symlink attacks)
- Parent directory validation (catches parent symlink attacks)
- 25+ dedicated tests

---

## 5. Q15 Denominator Verification

### Status: CORRECT (32767.0)

All references use correct denominator:

| Location | Status |
|----------|--------|
| `adapteros-lora-router/src/lib.rs:46` | `ROUTER_GATE_Q15_DENOM: f32 = 32767.0` |
| `adapteros-lora-router/src/scoring.rs` | Uses constant |
| `adapteros-lora-router/src/orthogonal.rs:83` | `/ ROUTER_GATE_Q15_DENOM` |
| `adapteros-lora-kernel-mtl/src/ring_buffer.rs` | `* 32767.0` / `/ 32767.0` |
| `adapteros-db/src/routing_decisions.rs` | `/ 32767.0` |

**DO NOT CHANGE TO 32768** - breaks determinism proofs and replay verification.

---

## Action Items

### High Priority
1. Add tenant predicates to `list_adapters_by_*` functions in `adapters.rs`
2. Add cross-tenant denial tests for category/scope/state queries
3. Fix `list_models()` to require tenant_id

### Medium Priority
4. Add storage path cross-tenant traversal tests (PRD-RECT-002)
5. Verify telemetry buffer tenant filtering
6. Add worker incident listing handler with tenant scoping

### Low Priority (Monitoring Only)
7. Monitor cache eviction metrics in production
8. Watch for pinned entry leaks via `model_cache_pinned_entries` gauge

---

## File References

| Component | Key Files |
|-----------|-----------|
| Tenant Isolation | `crates/adapteros-db/src/adapters.rs`, `crates/adapteros-server-api/src/security/mod.rs` |
| Worker Lifecycle | `crates/adapteros-db/src/workers.rs`, `crates/adapteros-server-api/src/handlers/workers.rs` |
| Cache Eviction | `crates/adapteros-lora-worker/src/model_handle_cache.rs` |
| Runtime Paths | `crates/adapteros-config/src/path_resolver.rs` |
| Q15 Constants | `crates/adapteros-lora-router/src/lib.rs` |
