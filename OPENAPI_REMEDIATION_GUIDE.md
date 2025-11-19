# OpenAPI Remediation Guide

## Quick Summary

The AdapterOS REST API has **93 endpoints without OpenAPI documentation**. Most (52) are fully implemented with `#[utoipa::path]` annotations and routes registered—they just need to be **added to the OpenAPI macro**.

**Time to Fix (Phase 1)**: ~30 minutes
**Impact**: Expose 52 endpoints to Swagger UI and OpenAPI spec

---

## Phase 1: Add Missing Handler References to OpenAPI Macro (CRITICAL)

**File**: `crates/adapteros-server-api/src/routes.rs`

**Location**: Lines 16-110 (inside the `#[openapi(paths(...))]` macro)

**Current State**: Only ~41 handler references in paths() macro

**Action**: Insert these 52 handler references in alphabetical order within the `paths()` macro

### Code to Add

Insert after line 109 (before the closing `),` of the paths() macro):

```rust
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
        // Tenant Management (5 endpoints)
        handlers::list_tenants,
        handlers::create_tenant,
        handlers::update_tenant,
        handlers::pause_tenant,
        handlers::archive_tenant,
        // Node Management (6 endpoints)
        handlers::list_nodes,
        handlers::register_node,
        handlers::test_node_connection,
        handlers::mark_node_offline,
        handlers::evict_node,
        handlers::get_node_details,
        // Plan Management (8 endpoints)
        handlers::list_plans,
        handlers::build_plan,
        handlers::get_plan_details,
        handlers::rebuild_plan,
        handlers::compare_plans,
        handlers::export_plan_manifest,
        handlers::promotion_gates,
        handlers::get_promotion_history,
        // Control Plane Promotion (3 endpoints)
        handlers::cp_promote,
        handlers::cp_rollback,
        handlers::cp_dry_run_promote,
        // Policy Management (7 endpoints)
        handlers::list_policies,
        handlers::get_policy,
        handlers::apply_policy,
        handlers::validate_policy,
        handlers::sign_policy,
        handlers::compare_policy_versions,
        handlers::export_policy,
        // Job & Worker Management (3 endpoints)
        handlers::list_jobs,
        handlers::worker_spawn,
        handlers::list_workers,
        // Process & Debug (4 endpoints)
        handlers::list_process_logs,
        handlers::list_process_crashes,
        handlers::start_debug_session,
        handlers::run_troubleshooting_step,
        // Telemetry & Bundles (4 endpoints)
        handlers::list_telemetry_bundles,
        handlers::export_telemetry_bundle,
        handlers::verify_bundle_signature,
        handlers::purge_old_bundles,
        // Streaming & System (5 endpoints)
        handlers::meta,
        handlers::metrics_handler,
        handlers::system_metrics_stream,
        handlers::telemetry_events_stream,
        handlers::adapter_state_stream,
        // Adapter Lifecycle (4 endpoints)
        handlers::promote_adapter_lifecycle,
        handlers::demote_adapter_lifecycle,
        handlers::get_adapter_lineage,
        handlers::get_adapter_detail,
        // Validation (3 endpoints)
        handlers::validate_adapter_name,
        handlers::validate_stack_name,
        handlers::get_next_revision,
        // Audit (4 endpoints)
        handlers::get_compliance_audit,
        handlers::get_federation_audit,
        handlers::list_audits_extended,
        handlers::get_promotion,
        // Authentication (2 endpoints)
        handlers::auth_logout,
        handlers::auth_me,
        // Model Management (1 endpoint)
        handlers::import_model,
```

**Verification**:
```bash
# Build to verify no compilation errors
cargo build --release 2>&1 | grep -i "error"

# Check that Swagger UI now shows new endpoints
curl -s http://localhost:8080/swagger-ui/ | grep "monitoring\|telemetry\|workspace" | wc -l
# Should show multiple matches (was 0 before)
```

---

## Phase 2: Add OpenAPI Annotations to Notification Handlers

**File**: `crates/adapteros-server-api/src/handlers/notifications.rs`

### 1. Add annotation to `list_notifications` (line 37)

**Insert before line 37:**

```rust
/// List user notifications
///
/// Retrieves notifications for the current user, optionally filtered by workspace and type.
/// Supports pagination with limit/offset.
#[utoipa::path(
    get,
    path = "/v1/notifications",
    params(
        ("workspace_id" = Option<String>, Query, description = "Filter by workspace ID"),
        ("type" = Option<String>, Query, description = "Filter by notification type"),
        ("unread_only" = Option<bool>, Query, description = "Show only unread notifications"),
        ("limit" = Option<i64>, Query, description = "Maximum number of results"),
        ("offset" = Option<i64>, Query, description = "Pagination offset")
    ),
    responses(
        (status = 200, description = "List of notifications", body = Vec<NotificationResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "notifications"
)]
```

### 2. Add annotation to `get_notification_summary` (line 102)

**Insert before line 102:**

```rust
/// Get notification summary
///
/// Returns count of total and unread notifications for the current user.
#[utoipa::path(
    get,
    path = "/v1/notifications/summary",
    params(
        ("workspace_id" = Option<String>, Query, description = "Filter by workspace ID")
    ),
    responses(
        (status = 200, description = "Notification summary", body = NotificationSummary),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "notifications"
)]
```

### 3. Add annotation to `mark_notification_read` (line 147)

**Insert before line 147:**

```rust
/// Mark notification as read
///
/// Marks a single notification as read for the current user.
#[utoipa::path(
    post,
    path = "/v1/notifications/{notification_id}/read",
    params(
        ("notification_id" = String, Path, description = "Notification ID")
    ),
    responses(
        (status = 200, description = "Notification marked as read", body = serde_json::Value),
        (status = 404, description = "Notification not found", body = ErrorResponse),
        (status = 403, description = "Unauthorized to mark this notification", body = ErrorResponse)
    ),
    tag = "notifications"
)]
```

### 4. Add annotation to `mark_all_notifications_read` (line 200)

**Insert before line 200:**

```rust
/// Mark all notifications as read
///
/// Marks all notifications as read for the current user, optionally filtered by workspace.
#[utoipa::path(
    post,
    path = "/v1/notifications/read-all",
    params(
        ("workspace_id" = Option<String>, Query, description = "Workspace to mark as read")
    ),
    responses(
        (status = 200, description = "All notifications marked as read", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    ),
    tag = "notifications"
)]
```

---

## Phase 3: Add Routes for Notifications

**File**: `crates/adapteros-server-api/src/routes.rs`

**Location**: Protected routes section (after line 650, before workspace routes)

**Add these routes:**

```rust
        // Notification routes
        .route("/v1/notifications", get(handlers::list_notifications))
        .route("/v1/notifications/summary", get(handlers::get_notification_summary))
        .route("/v1/notifications/:notification_id/read", post(handlers::mark_notification_read))
        .route("/v1/notifications/read-all", post(handlers::mark_all_notifications_read))
```

---

## Phase 4: Add Notification Types to OpenAPI Schema

**File**: `crates/adapteros-server-api/src/routes.rs`

**Location**: Inside `components(schemas(...))` macro (after line 162)

**Add these types:**

```rust
        // Notification types
        crate::handlers::notifications::NotificationResponse,
        crate::handlers::notifications::NotificationSummary,
```

---

## Phase 5: Add OpenAPI Annotations to Workspace Handlers

**File**: `crates/adapteros-server-api/src/handlers/workspaces.rs`

### Template for workspace handlers

```rust
/// [Description]
///
/// [Detailed description of what this endpoint does]
#[utoipa::path(
    [METHOD],
    path = "[PATH]",
    params(
        [PARAMS if any]
    ),
    request_body = [REQUEST_TYPE if POST/PUT],
    responses(
        (status = 200, description = "[Description]", body = [RESPONSE_TYPE]),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "workspaces"
)]
```

**Endpoints requiring annotations** (see file for exact locations):
1. `list_workspaces` - GET /v1/workspaces
2. `list_user_workspaces` - GET /v1/me/workspaces
3. `create_workspace` - POST /v1/workspaces (needs CreateWorkspaceRequest body)
4. `get_workspace` - GET /v1/workspaces/{workspace_id}
5. `update_workspace` - PUT /v1/workspaces/{workspace_id} (needs UpdateWorkspaceRequest body)
6. `delete_workspace` - DELETE /v1/workspaces/{workspace_id}
7. `list_workspace_members` - GET /v1/workspaces/{workspace_id}/members
8. `add_workspace_member` - POST /v1/workspaces/{workspace_id}/members (needs AddWorkspaceMemberRequest body)
9. `update_workspace_member` - PUT /v1/workspaces/{workspace_id}/members/{member_id}
10. `remove_workspace_member` - DELETE /v1/workspaces/{workspace_id}/members/{member_id}
11. `list_workspace_resources` - GET /v1/workspaces/{workspace_id}/resources
12. `share_workspace_resource` - POST /v1/workspaces/{workspace_id}/resources
13. `unshare_workspace_resource` - DELETE /v1/workspaces/{workspace_id}/resources/{resource_id}

---

## Phase 6: Add Tutorial Annotations

**File**: `crates/adapteros-server-api/src/handlers/tutorials.rs`

**Endpoints requiring annotations**:
1. `list_tutorials` - GET /v1/tutorials
2. `mark_tutorial_completed` - POST /v1/tutorials/{tutorial_id}/completed
3. `unmark_tutorial_completed` - POST /v1/tutorials/{tutorial_id}/uncompleted
4. `mark_tutorial_dismissed` - POST /v1/tutorials/{tutorial_id}/dismissed
5. `unmark_tutorial_dismissed` - POST /v1/tutorials/{tutorial_id}/undismissed

---

## Verification Checklist

After making changes:

- [ ] Code compiles without errors: `cargo build --release`
- [ ] No clippy warnings: `cargo clippy --workspace`
- [ ] OpenAPI macro still valid: `cargo build --features openapi`
- [ ] New endpoints visible in Swagger UI: `curl http://localhost:8080/swagger-ui/`
- [ ] OpenAPI JSON validates: `./scripts/validate_openapi_docs.sh`
- [ ] No breaking changes to existing endpoints

---

## Expected Results

**Before Phase 1:**
- OpenAPI paths() macro: 41 entries
- Swagger UI endpoints: ~41 visible
- Coverage: 27%

**After Phase 1:**
- OpenAPI paths() macro: 93 entries
- Swagger UI endpoints: ~93 visible
- Coverage: 62%

**After All Phases:**
- OpenAPI paths() macro: 115+ entries
- Swagger UI endpoints: ~115+ visible
- Coverage: 100%

---

## Testing the Changes

### 1. Build and check for errors
```bash
cd /Users/star/Dev/aos
cargo build --release 2>&1 | tail -20
```

### 2. Start server and check Swagger UI
```bash
./target/release/adapteros-server &
sleep 2
curl -s http://localhost:8080/swagger-ui/ | grep -c "monitoring"
# Should be > 0 (was 0 before)
```

### 3. Export and validate OpenAPI spec
```bash
curl -s http://localhost:8080/api-docs/openapi.json | \
  jq '.paths | keys | length'
# Should show ~93+ (was ~41 before)
```

### 4. Check specific endpoint documentation
```bash
curl -s http://localhost:8080/api-docs/openapi.json | \
  jq '.paths["/v1/monitoring/rules"]'
# Should return path definition (was null before)
```

---

## Debugging Common Issues

### Issue: Handler not found in routes.rs
**Solution**: Verify handler is exported from handlers.rs module (check handlers/mod.rs or main handlers.rs file)

### Issue: Annotation not recognized
**Solution**: Ensure `#[utoipa::path(...)]` is immediately above the function definition with no blank lines

### Issue: Type not found in schema
**Solution**: Verify type is added to both:
1. `components(schemas(...))` macro
2. Types must be `pub` and implement `serde::Serialize`

### Issue: Route not accessible
**Solution**: Verify route is in `protected_routes` or `public_routes` depending on auth requirements

---

## References

- **Utoipa Docs**: https://docs.rs/utoipa/latest/utoipa/
- **OpenAPI 3.0 Spec**: https://spec.openapis.org/oas/v3.0.0
- **Axum Routing**: https://docs.rs/axum/latest/axum/routing/
- **Project Conventions**: See `CLAUDE.md` in repository root

---

## Summary

This remediation exposes ~115+ endpoints to OpenAPI, achieving full coverage. The three-phase approach prioritizes quick wins:
1. **Phase 1** (30 min): Macro entries - immediate impact
2. **Phase 2-6** (3-4 hours): Notifications, workspaces, tutorials - full coverage

Total effort: ~4-5 hours to achieve 100% OpenAPI coverage
