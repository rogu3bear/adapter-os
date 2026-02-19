# Handler Hygiene Audit

> **Status:** Audit document tracking handler file sizes and split strategies.
>
> Last updated: January 2026

## Overview

This document tracks handler file sizes in `crates/adapteros-server-api/src/handlers/` to identify candidates for splitting. Large handlers (>1000 lines) increase cognitive load and merge conflicts.

## Size Thresholds

| Status | Line Count | Action |
|--------|------------|--------|
| 🟢 Green | < 500 | No action needed |
| 🟡 Yellow | 500–1000 | Monitor, consider split if growing |
| 🔴 Red | > 1000 | Split recommended |

## Handler Size Audit

### 🔴 Red (>1000 lines) — Split Recommended

| File | Lines | Split Strategy |
|------|-------|----------------|
| `training.rs` | 3,336 | Split into `training/jobs.rs`, `training/sessions.rs`, `training/metrics.rs`, `training/templates.rs` |
| `streaming_infer.rs` | 2,331 | Extract SSE machinery to `streaming_infer/sse.rs`, keep core in `streaming_infer/handler.rs` |
| `models.rs` | 2,033 | Split into `models/lifecycle.rs` (load/unload), `models/validation.rs`, `models/import.rs` |
| `datasets/safety.rs` | 1,939 | Extract trust override logic to `datasets/trust.rs`, keep safety checks in `safety.rs` |
| `workspaces.rs` | 1,921 | Split into `workspaces/crud.rs`, `workspaces/members.rs`, `workspaces/resources.rs` |
| `streaming.rs` | 1,741 | Extract per-stream handlers: `streams/notifications.rs`, `streams/activity.rs`, `streams/messages.rs` |
| `datasets/validation.rs` | 1,684 | Keep as-is (focused module, complexity is intrinsic) |
| `adapter_stacks.rs` | 1,515 | Split into `adapter_stacks/crud.rs`, `adapter_stacks/lifecycle.rs` |
| `replay.rs` | 1,488 | Split into `replay/sessions.rs`, `replay/execution.rs` |
| `datasets/chunked_handlers.rs` | 1,391 | Keep as-is (chunked upload is single cohesive feature) |
| `datasets/paths.rs` | 1,369 | Keep as-is (path utilities are internal) |
| `workers.rs` | 1,353 | Split into `workers/registration.rs`, `workers/status.rs`, `workers/history.rs` |
| `datasets/files.rs` | 1,339 | Keep as-is (file handling is cohesive) |
| `adapters_read.rs` | 1,319 | Merge into `adapters/read.rs` in new module structure |
| `testkit.rs` | 1,301 | Keep as-is (E2E helpers intentionally isolated) |
| `infrastructure.rs` | 1,279 | Split into `infrastructure/version.rs`, `infrastructure/status.rs` |
| `repos.rs` | 1,231 | Split into `repos/crud.rs`, `repos/versions.rs`, `repos/timeline.rs` |
| `domain_adapters.rs` | 1,180 | Split into `domain_adapters/crud.rs`, `domain_adapters/execution.rs` |
| `git_repository.rs` | 1,172 | Keep as-is (git integration is cohesive) |
| `replay_inference.rs` | 1,166 | Merge with `replay.rs` split |
| `batch.rs` | 1,165 | Split into `batch/inference.rs`, `batch/jobs.rs` |
| `documents.rs` | 1,143 | Split into `documents/crud.rs`, `documents/processing.rs` |
| `diagnostics.rs` | 1,084 | Split into `diagnostics/status.rs`, `diagnostics/runs.rs`, `diagnostics/diff.rs` |
| `tenants.rs` | 1,072 | Split into `tenants/crud.rs`, `tenants/policies.rs` |
| `adapters/import.rs` | 1,018 | Keep as-is (import is single cohesive operation) |
| `promotion.rs` | 1,004 | Keep as-is (promotion workflow is cohesive) |

### 🟡 Yellow (500–1000 lines) — Monitor

| File | Lines | Notes |
|------|-------|-------|
| `datasets/upload.rs` | 978 | Monitor for growth |
| `streams/mod.rs` | 976 | Monitor for growth |
| `policies.rs` | 936 | Monitor for growth |
| `auth_enhanced/dev_bypass.rs` | 889 | Dev-only, acceptable |
| `routing_decisions.rs` | 878 | Monitor for growth |
| `tenant_management.rs` | 873 | Monitor for growth |
| `chunked_upload.rs` | 863 | Duplicate of datasets/chunked_handlers? |
| `datasets/progress_sse.rs` | 839 | Monitor for growth |
| `rag_common.rs` | 817 | Monitor for growth |
| `diag_bundle.rs` | 814 | Monitor for growth |
| `health.rs` | 810 | Monitor for growth |
| `monitoring/mod.rs` | 803 | Monitor for growth |
| `chat_sessions/core.rs` | 777 | Monitor for growth |
| `code.rs` | 764 | Monitor for growth |
| `telemetry.rs` | 741 | Monitor for growth |
| `adapter_lifecycle.rs` | 707 | Monitor for growth |
| `system_status.rs` | 700 | Monitor for growth |
| `adapter_versions.rs` | 683 | Monitor for growth |
| `datasets/helpers.rs` | 639 | Internal helpers, acceptable |
| `discovery.rs` | 619 | Monitor for growth |
| `datasets/versions.rs` | 568 | Monitor for growth |
| `evidence.rs` | 565 | Monitor for growth |
| `process_monitoring.rs` | 558 | Monitor for growth |
| `adapters/lifecycle.rs` | 551 | Monitor for growth |
| `memory_detail.rs` | 549 | Monitor for growth |
| `tenant_policies.rs` | 535 | Monitor for growth |
| `adapters/pinning.rs` | 525 | Monitor for growth |
| `storage.rs` | 508 | Monitor for growth |

### 🟢 Green (<500 lines) — No Action

97 files with <500 lines. These are appropriately sized.

## Summary Statistics

| Metric | Count |
|--------|-------|
| Total handler files | 124 |
| 🔴 Red (>1000 lines) | 26 |
| 🟡 Yellow (500–1000 lines) | 28 |
| 🟢 Green (<500 lines) | 70 |
| Total handler lines | ~62,000 |

## Prioritized Split Roadmap

### Phase 1 (Immediate)
- [ ] `training.rs` → Split into 4 modules
- [ ] `streaming_infer.rs` → Extract SSE machinery
- [ ] `models.rs` → Split into 3 modules

### Phase 2 (Short-term)
- [ ] `workspaces.rs` → Split into 3 modules
- [ ] `adapter_stacks.rs` → Split into 2 modules
- [ ] `workers.rs` → Split into 3 modules

### Phase 3 (Medium-term)
- [ ] Consolidate `datasets/` submodule (already well-organized)
- [ ] Review `chat_sessions/` for further modularization
- [ ] Consider merging related small handlers

## Guidelines for New Handlers

1. **Target size:** Keep handlers under 500 lines
2. **Single responsibility:** One handler = one resource or operation type
3. **Use subdirectories:** Group related handlers (e.g., `adapters/`, `datasets/`)
4. **Shared types:** Extract types to `types.rs` in subdirectory
5. **Shared helpers:** Extract helpers to `helpers.rs` in subdirectory

## Related Documents

- [`api/ROUTE_MAP.md`](../api/ROUTE_MAP.md) — Route to handler mapping (auto-generated)
- [`../api/openapi.json`](../api/openapi.json) — OpenAPI specification
