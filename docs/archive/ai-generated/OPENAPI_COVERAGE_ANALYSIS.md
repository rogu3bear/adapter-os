# OpenAPI Coverage Analysis - AdapterOS REST API

## Executive Summary

The AdapterOS REST API has **significant gaps** between registered routes and OpenAPI documentation:
- **93 endpoints** are registered in routes but missing from OpenAPI `paths()` macro
- **52 endpoints** have `#[utoipa::path]` annotations but are NOT referenced in the OpenAPI spec
- **20+ endpoints** are fully implemented handlers but have NO OpenAPI documentation or route registration

**Impact**: Most endpoints are "invisible" to the OpenAPI spec and Swagger UI, making it impossible for API consumers to discover or validate against the spec.

---

## Critical Issue: Missing OpenAPI Macro Entries

### Root Cause
Many handlers have proper `#[utoipa::path]` annotations but are **not listed in the `paths()` macro** within the OpenAPI struct in `routes.rs` (lines 16-110).

### Example
```rust
// handlers.rs - Has annotation ✓
#[utoipa::path(
    get,
    path = "/v1/monitoring/rules",
    responses((status = 200, body = Vec<ProcessMonitoringRuleResponse>))
)]
pub async fn list_process_monitoring_rules(...) { ... }

// routes.rs - NOT in paths() macro ✗
#[openapi(
    paths(
        handlers::health,
        handlers::ready,
        // ... missing: handlers::list_process_monitoring_rules
    ),
)]
pub struct ApiDoc;
```

---

## Category 1: Process Monitoring Endpoints (11 Missing)

### Status: Has Annotations + Routes, BUT Not in OpenAPI Macro

All of these endpoints have complete implementations with `#[utoipa::path]` annotations and are registered in routes, but are **invisible to the OpenAPI spec**.

| # | Handler Function | HTTP | Path | Line | OpenAPI | Routes | Macro |
|---|---|---|---|---|---|---|---|
| 1 | `list_process_monitoring_rules` | GET | `/v1/monitoring/rules` | 3965 | ✓ | ✓ | ✗ |
| 2 | `create_process_monitoring_rule` | POST | `/v1/monitoring/rules` | 3990 | ✓ | ✓ | ✗ |
| 3 | `list_process_alerts` | GET | `/v1/monitoring/alerts` | 4033 | ✓ | ✓ | ✗ |
| 4 | `acknowledge_process_alert` | POST | `/v1/monitoring/alerts/{alert_id}/acknowledge` | 4061 | ✓ | ✓ | ✗ |
| 5 | `list_process_anomalies` | GET | `/v1/monitoring/anomalies` | 4108 | ✓ | ✓ | ✗ |
| 6 | `update_process_anomaly_status` | POST | `/v1/monitoring/anomalies/{anomaly_id}/status` | 4136 | ✓ | ✓ | ✗ |
| 7 | `list_process_monitoring_dashboards` | GET | `/v1/monitoring/dashboards` | 4183 | ✓ | ✓ | ✗ |
| 8 | `create_process_monitoring_dashboard` | POST | `/v1/monitoring/dashboards` | 4206 | ✓ | ✓ | ✗ |
| 9 | `list_process_health_metrics` | GET | `/v1/monitoring/health-metrics` | 4241 | ✓ | ✓ | ✗ |
| 10 | `list_process_monitoring_reports` | GET | `/v1/monitoring/reports` | 4311 | ✓ | ✓ | ✗ |
| 11 | `create_process_monitoring_report` | POST | `/v1/monitoring/reports` | 4334 | ✓ | ✓ | ✗ |

**Required Addition to `routes.rs`:**
```rust
#[openapi(
    paths(
        // ... existing entries ...
        // Process Monitoring (11 endpoints)
        handlers::list_process_monitoring_rules,
        handlers::create_process_monitoring_rule,
        handlers::list_process_alerts,
        handlers::acknowledge_process_alert,
        handlers::list_process_anomalies,
        handlers::update_process_anomaly_status,
        handlers::list_process_monitoring_dashboards,
        handlers::create_process_monitoring_dashboard,
        handlers::list_process_health_metrics,
        handlers::list_process_monitoring_reports,
        handlers::create_process_monitoring_report,
    ),
)]
```

---

## Category 2: Notification Endpoints (4 Missing)

### Status: Handlers Defined, NO Routes, NO Annotations

Located in: `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/notifications.rs`

| # | Handler | HTTP | Proposed Path | Line | Annotation | Routes | Status |
|---|---|---|---|---|---|---|---|
| 1 | `list_notifications` | GET | `/v1/notifications` | 37 | ✗ | ✗ | TODO |
| 2 | `get_notification_summary` | GET | `/v1/notifications/summary` | 102 | ✗ | ✗ | TODO |
| 3 | `mark_notification_read` | POST | `/v1/notifications/{notification_id}/read` | 147 | ✗ | ✗ | TODO |
| 4 | `mark_all_notifications_read` | POST | `/v1/notifications/read-all` | 200 | ✗ | ✗ | TODO |

**What's Needed:**
1. Add `#[utoipa::path]` annotations to each function
2. Register routes in `protected_routes` section
3. Add response types to OpenAPI `components(schemas(...))`
4. Add types to `routes.rs` OpenAPI macro `paths()`

**Types to Add:**
- `NotificationResponse` (with fields: id, user_id, workspace_id, type_, target_type, target_id, title, content, read_at, created_at)
- `NotificationSummary` (with fields: total_count, unread_count)

---

## Category 3: Workspace Endpoints (13 Missing)

### Status: Handlers Defined, NO Routes, NO Annotations

Located in: `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/workspaces.rs`

| # | Handler | HTTP | Proposed Path | Line | Status |
|---|---|---|---|---|---|
| 1 | `list_workspaces` | GET | `/v1/workspaces` | 60 | TODO |
| 2 | `list_user_workspaces` | GET | `/v1/me/workspaces` | 97 | TODO |
| 3 | `create_workspace` | POST | `/v1/workspaces` | 136 | TODO |
| 4 | `get_workspace` | GET | `/v1/workspaces/{workspace_id}` | 213 | TODO |
| 5 | `update_workspace` | PUT | `/v1/workspaces/{workspace_id}` | 270 | TODO |
| 6 | `delete_workspace` | DELETE | `/v1/workspaces/{workspace_id}` | 361 | TODO |
| 7 | `list_workspace_members` | GET | `/v1/workspaces/{workspace_id}/members` | 415 | TODO |
| 8 | `add_workspace_member` | POST | `/v1/workspaces/{workspace_id}/members` | 478 | TODO |
| 9 | `update_workspace_member` | PUT | `/v1/workspaces/{workspace_id}/members/{member_id}` | 555 | TODO |
| 10 | `remove_workspace_member` | DELETE | `/v1/workspaces/{workspace_id}/members/{member_id}` | 653 | TODO |
| 11 | `list_workspace_resources` | GET | `/v1/workspaces/{workspace_id}/resources` | 734 | TODO |
| 12 | `share_workspace_resource` | POST | `/v1/workspaces/{workspace_id}/resources` | 796 | TODO |
| 13 | `unshare_workspace_resource` | DELETE | `/v1/workspaces/{workspace_id}/resources/{resource_id}` | 876 | TODO |

**Types to Add:**
- `WorkspaceResponse`
- `CreateWorkspaceRequest`
- `UpdateWorkspaceRequest`
- `AddWorkspaceMemberRequest`
- `UpdateWorkspaceMemberRequest`
- `ShareResourceRequest`

---

## Category 4: Tutorial Endpoints (5 Missing)

### Status: Handlers Defined, NO Routes, NO Annotations

Located in: `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/tutorials.rs`

| # | Handler | HTTP | Proposed Path | Line | Status |
|---|---|---|---|---|---|
| 1 | `list_tutorials` | GET | `/v1/tutorials` | 64 | TODO |
| 2 | `mark_tutorial_completed` | POST | `/v1/tutorials/{tutorial_id}/completed` | 259 | TODO |
| 3 | `unmark_tutorial_completed` | POST | `/v1/tutorials/{tutorial_id}/uncompleted` | 296 | TODO |
| 4 | `mark_tutorial_dismissed` | POST | `/v1/tutorials/{tutorial_id}/dismissed` | 327 | TODO |
| 5 | `unmark_tutorial_dismissed` | POST | `/v1/tutorials/{tutorial_id}/undismissed` | 358 | TODO |

**Types to Add:**
- `TutorialResponse`
- `TutorialStatusResponse`
- `TutorialStep`

---

## Category 5: Other Missing OpenAPI Macro Entries (52 Endpoints)

These handlers have both `#[utoipa::path]` annotations AND routes registered, but are **NOT listed in the OpenAPI `paths()` macro**:

### Tenant Management
- `list_tenants`
- `create_tenant`
- `update_tenant`
- `pause_tenant`
- `archive_tenant`

### Node Management
- `list_nodes`
- `register_node`
- `test_node_connection`
- `mark_node_offline`
- `evict_node`
- `get_node_details`

### Plan & CP Management
- `list_plans`
- `build_plan`
- `get_plan_details`
- `rebuild_plan`
- `compare_plans`
- `export_plan_manifest`
- `cp_promote`
- `cp_rollback`
- `cp_dry_run_promote`
- `promotion_gates`
- `get_promotion_history`

### Policy Management
- `list_policies`
- `get_policy`
- `apply_policy`
- `validate_policy`
- `sign_policy`
- `compare_policy_versions`
- `export_policy`

### Job & Worker Management
- `list_jobs`
- `worker_spawn`
- `list_workers`

### Process & Debug
- `list_process_logs`
- `list_process_crashes`
- `start_debug_session`
- `run_troubleshooting_step`

### Telemetry & Bundles
- `list_telemetry_bundles`
- `export_telemetry_bundle`
- `verify_bundle_signature`
- `purge_old_bundles`

### Streaming & System
- `meta`
- `metrics_handler`
- `system_metrics_stream`
- `telemetry_events_stream`
- `adapter_state_stream`

### Adapter Lifecycle & Details
- `promote_adapter_lifecycle`
- `demote_adapter_lifecycle`
- `get_adapter_lineage`
- `get_adapter_detail`

### Validation
- `validate_adapter_name`
- `validate_stack_name`
- `get_next_revision`

### Audit
- `get_compliance_audit`
- `get_federation_audit`
- `list_audits_extended`
- `get_promotion`

### Authentication
- `auth_logout`
- `auth_me`

### Model Management
- `import_model`

---

## Impact Assessment

### Current State
- **Total Endpoints Implemented**: ~150+
- **Endpoints with OpenAPI Annotation**: ~93
- **Endpoints in OpenAPI Macro**: ~41
- **Coverage Rate**: 27%

### Issues
1. **52 endpoints** have full implementations but are invisible to OpenAPI spec
2. **20+ endpoints** are completely unregistered
3. Swagger UI only shows ~41 endpoints, missing 75%+ of actual API surface
4. API consumers cannot auto-generate clients from spec
5. Breaking changes go undetected without spec validation

### Risk Level: **CRITICAL**

---

## Remediation Plan

### Phase 1: Quick Win (30 minutes)
**Add 52 missing entries to OpenAPI macro in `routes.rs`**

Location: `crates/adapteros-server-api/src/routes.rs`, lines 16-110

Add these handler references inside the `paths()` macro:
```rust
// Process Monitoring (11 entries)
handlers::list_process_monitoring_rules,
handlers::create_process_monitoring_rule,
handlers::list_process_alerts,
handlers::acknowledge_process_alert,
handlers::list_process_anomalies,
handlers::update_process_anomaly_status,
handlers::list_process_monitoring_dashboards,
handlers::create_process_monitoring_dashboard,
handlers::list_process_health_metrics,
handlers::list_process_monitoring_reports,
handlers::create_process_monitoring_report,

// Tenant Management (5 entries)
handlers::list_tenants,
handlers::create_tenant,
handlers::update_tenant,
handlers::pause_tenant,
handlers::archive_tenant,

// [Continue with remaining 36 entries...]
```

**Expected Result**: Expose ~52 additional endpoints to OpenAPI spec

---

### Phase 2: Annotation Addition (1-2 hours)
**Add `#[utoipa::path]` annotations to notification/workspace/tutorial handlers**

Files to modify:
1. `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/notifications.rs` (4 handlers)
2. `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/workspaces.rs` (13 handlers)
3. `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/tutorials.rs` (5 handlers)

---

### Phase 3: Route Registration (1 hour)
**Add route registrations to `protected_routes` in `routes.rs`**

Add sections:
```rust
// Notification routes
.route("/v1/notifications", get(handlers::list_notifications))
.route("/v1/notifications/summary", get(handlers::get_notification_summary))
// [... etc]

// Workspace routes
.route("/v1/workspaces", get(handlers::list_workspaces).post(handlers::create_workspace))
// [... etc]

// Tutorial routes
.route("/v1/tutorials", get(handlers::list_tutorials))
// [... etc]
```

---

### Phase 4: Type Additions (30 minutes)
**Add missing schema types to OpenAPI components section in `routes.rs`**

Add to `components(schemas(...))`:
```rust
// Notification types
crate::handlers::notifications::NotificationResponse,
crate::handlers::notifications::NotificationSummary,

// Workspace types
crate::handlers::workspaces::WorkspaceResponse,
crate::handlers::workspaces::CreateWorkspaceRequest,
// [... etc]

// Tutorial types
crate::handlers::tutorials::TutorialResponse,
// [... etc]
```

---

## Verification Steps

Step 1: Build and generate OpenAPI spec
```bash
cargo build --release
```

Step 2: Verify Swagger UI includes new endpoints
```bash
curl http://localhost:8080/swagger-ui/
# Check that monitoring, notification, workspace, tutorial endpoints are visible
```

Step 3: Generate OpenAPI JSON and validate
```bash
cargo xtask openapi-docs
./scripts/validate_openapi_docs.sh
```

Step 4: Count endpoint coverage
```bash
# Should increase from ~41 to ~150+ endpoints
```

---

## Files Involved

| File | Changes | Lines |
|---|---|---|
| `/Users/star/Dev/aos/crates/adapteros-server-api/src/routes.rs` | Add 52 handler refs to paths() macro, register ~22 new routes, add types to components | 16-110, 221-683, 111-197 |
| `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/notifications.rs` | Add 4 `#[utoipa::path]` annotations | 37, 102, 147, 200 |
| `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/workspaces.rs` | Add 13 `#[utoipa::path]` annotations | 60, 97, 136, 213, 270, 361, 415, 478, 555, 653, 734, 796, 876 |
| `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/tutorials.rs` | Add 5 `#[utoipa::path]` annotations | 64, 259, 296, 327, 358 |

---

## References

- **OpenAPI Spec**: `crates/adapteros-server-api/src/routes.rs`
- **Handler Implementations**: `crates/adapteros-server-api/src/handlers/`
- **Type Definitions**: `crates/adapteros-api-types/src/`
- **Utoipa Documentation**: https://docs.rs/utoipa/latest/utoipa/
- **OpenAPI 3.0 Spec**: https://spec.openapis.org/oas/v3.0.0

---

## Status

- **Analysis Date**: 2025-11-19
- **Repository**: /Users/star/Dev/aos
- **Branch**: main
- **Priority**: CRITICAL (blocks API documentation and client generation)
