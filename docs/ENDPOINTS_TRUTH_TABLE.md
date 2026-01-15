# adapterOS API Endpoints Truth Table

> Generated from codebase analysis. Source: `crates/adapteros-server-api/src/routes.rs`

## Route Organization

Routes are organized into 6 main groups with specific middleware chains:

1. **Health Routes** - No middleware (fast probes)
2. **Public Routes** - No auth, policy enforcement middleware
3. **Metrics Routes** - Custom auth (bearer token)
4. **Optional Auth Routes** - Work with or without JWT
5. **Internal Routes** - Worker-to-CP (manifest binding, no user JWT)
6. **Protected Routes** - Require JWT authentication

---

## 1. Health & Liveness (No Middleware)

| Method | Path | Handler | Auth | Purpose |
|--------|------|---------|------|---------|
| GET | `/healthz` | `handlers::health` | None | Liveness probe |
| GET | `/readyz` | `handlers::ready` | None | Readiness probe |
| GET | `/version` | `infrastructure::get_version` | None | Version info |

---

## 2. Public Routes (Policy Enforcement Only)

| Method | Path | Handler | Auth | Purpose |
|--------|------|---------|------|---------|
| GET | `/healthz/all` | `check_all_health` | None | All health components |
| GET | `/healthz/{component}` | `check_component_health` | None | Specific component |
| GET | `/system/ready` | `system_ready` | None | System readiness |
| POST | `/v1/auth/login` | `auth_enhanced::login_handler` | None | User login |
| POST | `/v1/auth/bootstrap` | `auth_enhanced::bootstrap_admin_handler` | None | Bootstrap admin |
| GET | `/v1/auth/config` | `auth_enhanced::get_auth_config_handler` | None | Auth config |
| GET | `/v1/auth/health` | `auth_enhanced::auth_health_handler` | None | Auth health |
| POST | `/v1/auth/refresh` | `auth_enhanced::refresh_token_handler` | None | Refresh token |
| GET | `/v1/meta` | `handlers::meta` | None | System metadata |
| GET | `/v1/status` | `handlers::get_status` | None | System status |
| GET | `/v1/search` | `search::global_search` | None | Global search |
| POST | `/admin/lifecycle/request-shutdown` | `admin_lifecycle::request_shutdown` | None | Request shutdown |
| POST | `/admin/lifecycle/request-maintenance` | `admin_lifecycle::request_maintenance` | None | Maintenance mode |
| POST | `/admin/lifecycle/safe-restart` | `admin_lifecycle::safe_restart` | None | Safe restart |
| GET | `/v1/version` | versioning handler | None | API version |
| POST | `/v1/auth/dev-bypass` | `auth_enhanced::dev_bypass_handler` | None | Dev bypass (debug) |
| POST | `/v1/dev/bootstrap` | `auth_enhanced::dev_bootstrap_handler` | None | Dev bootstrap (debug) |

---

## 3. Metrics Routes (Custom Auth)

| Method | Path | Handler | Auth | Purpose |
|--------|------|---------|------|---------|
| GET | `/metrics` | `handlers::metrics_handler` | Bearer | Prometheus metrics |
| GET | `/v1/metrics` | `handlers::metrics_handler` | Bearer | Prometheus metrics (v1) |

---

## 4. Optional Auth Routes

| Method | Path | Handler | Auth | Purpose |
|--------|------|---------|------|---------|
| GET | `/v1/models/status` | `infrastructure::get_base_model_status` | Optional | Base model status |
| GET | `/v1/topology` | `topology::get_topology` | Optional | System topology |

---

## 5. Internal Routes (Worker Auth)

| Method | Path | Handler | Auth | Purpose |
|--------|------|---------|------|---------|
| POST | `/v1/workers/fatal` | `receive_worker_fatal` | Worker | Worker fatal error |
| POST | `/v1/workers/register` | `workers::register_worker` | Worker | Worker registration |
| GET | `/v1/tenants/{tenant_id}/manifests/{hash}` | `worker_manifests::fetch_manifest_by_hash` | Worker | Fetch manifest |
| POST | `/v1/workers/status` | `workers::notify_worker_status` | Worker | Worker status update |

---

## 6. Protected Routes (JWT Required)

### Authentication & Sessions

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/v1/auth/logout` | `auth_enhanced::logout_handler` | Logout |
| GET | `/v1/auth/me` | `auth::auth_me` | Current user |
| GET | `/v1/auth/mfa/status` | `auth_enhanced::mfa_status_handler` | MFA status |
| POST | `/v1/auth/mfa/start` | `auth_enhanced::mfa_start_handler` | Start MFA |
| POST | `/v1/auth/mfa/verify` | `auth_enhanced::mfa_verify_handler` | Verify MFA |
| POST | `/v1/auth/mfa/disable` | `auth_enhanced::mfa_disable_handler` | Disable MFA |
| GET/POST | `/v1/api-keys` | `api_keys::list/create` | API key management |
| DELETE | `/v1/api-keys/{id}` | `api_keys::revoke_api_key` | Revoke key |
| GET | `/v1/auth/sessions` | `auth_enhanced::list_sessions_handler` | List sessions |
| DELETE | `/v1/auth/sessions/{jti}` | `auth_enhanced::revoke_session_handler` | Revoke session |
| GET | `/v1/auth/tenants` | `auth_enhanced::list_user_tenants_handler` | User tenants |
| POST | `/v1/auth/tenants/switch` | `auth_enhanced::switch_tenant_handler` | Switch tenant |

### Admin & Audit

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/admin/users` | `admin::list_users` | List users |
| GET | `/v1/audit/logs` | `admin::query_audit_logs` | Query audits |

### Tenants

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET/POST | `/v1/tenants` | `list/create_tenant` | Tenant CRUD |
| PUT | `/v1/tenants/{id}` | `update_tenant` | Update tenant |
| POST | `/v1/tenants/{id}/pause` | `pause_tenant` | Pause tenant |
| POST | `/v1/tenants/{id}/archive` | `archive_tenant` | Archive tenant |
| POST | `/v1/tenants/{id}/policies` | `assign_tenant_policies` | Assign policies |
| POST | `/v1/tenants/{id}/adapters` | `assign_tenant_adapters` | Assign adapters |
| GET | `/v1/tenants/{id}/usage` | `get_tenant_usage` | Usage stats |
| GET/PUT/DELETE | `/v1/tenants/{id}/default-stack` | default stack ops | Default stack |
| GET | `/v1/tenants/{id}/router/config` | `router_config::get_router_config` | Router config |
| GET | `/v1/tenants/{id}/policy-bindings` | `list_tenant_policy_bindings` | Policy bindings |
| POST | `/v1/tenants/{id}/policy-bindings/{pid}/toggle` | `toggle_tenant_policy` | Toggle policy |
| POST | `/v1/tenants/{id}/revoke-all-tokens` | `tenants::revoke_tenant_tokens` | Revoke tokens |

### Execution Policy

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET/POST | `/v1/tenants/{id}/execution-policy` | execution policy ops | Manage policy |
| DELETE | `/v1/tenants/{id}/execution-policy/{pid}` | `deactivate_execution_policy` | Deactivate |
| GET | `/v1/tenants/{id}/execution-policy/history` | `get_execution_policy_history` | History |

### Nodes

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/nodes` | `list_nodes` | List nodes |
| POST | `/v1/nodes/register` | `register_node` | Register node |
| POST | `/v1/nodes/{id}/ping` | `test_node_connection` | Test connection |
| POST | `/v1/nodes/{id}/offline` | `mark_node_offline` | Mark offline |
| DELETE | `/v1/nodes/{id}` | `evict_node` | Evict node |
| GET | `/v1/nodes/{id}/details` | `get_node_details` | Node details |

### Services

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/v1/services/{id}/start` | `services::start_service` | Start service |
| POST | `/v1/services/{id}/stop` | `services::stop_service` | Stop service |
| POST | `/v1/services/{id}/restart` | `services::restart_service` | Restart service |
| POST | `/v1/services/essential/start` | `start_essential_services` | Start essential |
| POST | `/v1/services/essential/stop` | `stop_essential_services` | Stop essential |
| GET | `/v1/services/{id}/logs` | `services::get_service_logs` | Service logs |

### Models

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/models` | `models::list_models_with_stats` | List models |
| POST | `/v1/models/import` | `models::import_model` | Import model |
| GET | `/v1/models/download-progress` | `models::get_download_progress` | Download progress |
| GET | `/v1/models/status/all` | `models::get_all_models_status` | All status |
| POST | `/v1/models/{id}/load` | `models::load_model` | Load model |
| POST | `/v1/models/{id}/unload` | `models::unload_model` | Unload model |
| GET | `/v1/models/{id}/status` | `models::get_model_status` | Model status |
| GET | `/v1/models/{id}/validate` | `models::validate_model` | Validate model |

### Plans & Control Plane

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/plans` | `list_plans` | List plans |
| POST | `/v1/plans/build` | `build_plan` | Build plan |
| GET | `/v1/plans/{id}/details` | `get_plan_details` | Plan details |
| POST | `/v1/plans/{id}/rebuild` | `rebuild_plan` | Rebuild plan |
| POST | `/v1/plans/compare` | `compare_plans` | Compare plans |
| GET | `/v1/plans/{id}/manifest` | `export_plan_manifest` | Export manifest |
| POST | `/v1/cp/promote` | `cp_promote` | CP promote |
| GET | `/v1/cp/promotion-gates/{cpid}` | `promotion_gates` | Promotion gates |
| POST | `/v1/cp/rollback` | `cp_rollback` | Rollback |
| POST | `/v1/cp/promote/dry-run` | `cp_dry_run_promote` | Dry-run |
| GET | `/v1/cp/promotions` | `get_promotion_history` | Promotion history |

### Workers

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/workers` | `list_workers` | List workers |
| POST | `/v1/workers/spawn` | `worker_spawn` | Spawn worker |
| GET | `/v1/workers/{id}/logs` | `list_process_logs` | Worker logs |
| GET | `/v1/workers/{id}/crashes` | `list_process_crashes` | Crashes |
| POST | `/v1/workers/{id}/debug` | `start_debug_session` | Debug session |
| POST | `/v1/workers/{id}/troubleshoot` | `run_troubleshooting_step` | Troubleshoot |
| POST | `/v1/workers/{id}/stop` | `stop_worker` | Stop worker |
| GET | `/v1/workers/{id}/incidents` | `list_worker_incidents` | Incidents |
| GET | `/v1/workers/health/summary` | `get_worker_health_summary` | Health summary |
| GET | `/v1/workers/{id}/history` | `workers::get_worker_history` | History |
| GET | `/v1/workers/{id}/detail` | `worker_detail::get_worker_detail` | Detail |

### Monitoring & Alerts

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET/POST | `/v1/monitoring/rules` | monitoring rules ops | Manage rules |
| PUT/DELETE | `/v1/monitoring/rules/{id}` | rule ops | Update/delete |
| GET | `/v1/monitoring/alerts` | `monitoring::list_alerts` | List alerts |
| POST | `/v1/monitoring/alerts/{id}/acknowledge` | `acknowledge_alert` | Acknowledge |
| POST | `/v1/monitoring/alerts/{id}/resolve` | `resolve_alert` | Resolve |
| GET | `/v1/monitoring/anomalies` | `list_process_anomalies` | Anomalies |
| POST | `/v1/monitoring/anomalies/{id}/status` | `update_status` | Update status |
| GET | `/v1/monitoring/dashboards` | `list_dashboards` | Dashboards |
| POST | `/v1/monitoring/dashboards` | `create_dashboard` | Create dashboard |
| GET | `/v1/monitoring/health-metrics` | `list_health_metrics` | Health metrics |
| GET/POST | `/v1/monitoring/reports` | report ops | Reports |

### Policies & Governance

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/policies` | `list_policies` | List policies |
| GET | `/v1/policies/{cpid}` | `get_policy` | Get policy |
| POST | `/v1/policies/validate` | `validate_policy` | Validate |
| POST | `/v1/policies/apply` | `apply_policy` | Apply policy |
| POST | `/v1/policies/{cpid}/sign` | `sign_policy` | Sign policy |
| GET | `/v1/policies/{cpid}/verify` | `verify_policy_signature` | Verify sig |
| POST | `/v1/policies/compare` | `compare_policy_versions` | Compare versions |
| GET | `/v1/policies/{cpid}/export` | `export_policy` | Export policy |
| POST | `/v1/policies/assign` | `tenant_policies::assign_policy` | Assign |
| GET | `/v1/policies/assignments` | `list_policy_assignments` | Assignments |
| GET | `/v1/policies/violations` | `list_violations` | Violations |

### Inference & Chat

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/v1/patch/propose` | `code::propose_patch` | Propose patch |
| POST | `/v1/chat/completions` | `openai_compat::chat_completions` | OpenAI compat |
| POST | `/v1/infer` | `infer` | Inference |
| POST | `/v1/infer/stream` | `streaming_infer::streaming_infer` | Stream infer |
| POST | `/v1/infer/stream/progress` | `streaming_infer_with_progress` | With progress |
| POST | `/v1/infer/batch` | `batch::batch_infer` | Batch infer |

### Review Protocol (Human-in-the-Loop)

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/infer/{id}/state` | `review::get_inference_state` | Inference state |
| POST | `/v1/infer/{id}/review` | `review::submit_review` | Submit review |
| GET | `/v1/infer/paused` | `review::list_paused` | Paused inferences |
| GET | `/v1/reviews/paused` | `review::list_paused_reviews` | Paused reviews |
| GET | `/v1/reviews/{pause_id}` | `review::get_pause_details` | Pause details |
| GET | `/v1/reviews/{pause_id}/context` | `review::export_review_context` | Review context |
| POST | `/v1/reviews/submit` | `review::submit_review_response` | Submit response |

### Adapters

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/adapters` | `adapters::list_adapters` | List adapters |
| GET | `/v1/adapters/{id}` | `adapters::get_adapter` | Get adapter |
| POST | `/v1/adapters/register` | `adapters_lifecycle::register_adapter` | Register |
| POST | `/v1/adapters/import` | `adapters::import_adapter` | Import |
| GET/POST | `/v1/adapter-repositories` | repo ops | Repo management |
| GET | `/v1/adapter-repositories/{id}` | `get_adapter_repository` | Get repo |
| GET/PUT | `/v1/adapter-repositories/{id}/policy` | policy ops | Repo policy |
| POST | `/v1/adapter-repositories/{id}/archive` | `archive_repository` | Archive |
| GET | `/v1/adapter-repositories/{id}/versions` | `list_adapter_versions` | Versions |
| POST | `/v1/adapter-repositories/{id}/versions/rollback` | `rollback_version` | Rollback |
| POST | `/v1/adapter-repositories/{id}/resolve-version` | `resolve_version` | Resolve |
| POST | `/v1/adapter-versions/draft` | `create_draft_version` | Create draft |
| GET | `/v1/adapter-versions/{id}` | `get_adapter_version` | Get version |
| POST | `/v1/adapter-versions/{id}/promote` | `promote_version` | Promote |
| POST | `/v1/adapter-versions/{id}/tag` | `tag_version` | Tag version |
| DELETE | `/v1/adapters/{id}` | `delete_adapter` | Delete |
| POST | `/v1/adapters/{id}/load` | `load_adapter` | Load |
| POST | `/v1/adapters/{id}/unload` | `unload_adapter` | Unload |
| POST | `/v1/adapters/{id}/activate` | `activate_adapter` | Activate |
| GET | `/v1/adapters/verify-gpu` | `verify_gpu_integrity` | Verify GPU |
| GET | `/v1/adapters/{id}/activations` | `get_adapter_activations` | Activations |
| GET | `/v1/adapters/{id}/usage` | `get_adapter_usage` | Usage |
| POST | `/v1/adapters/{id}/lifecycle/promote` | `promote_lifecycle` | Promote lifecycle |
| POST | `/v1/adapters/{id}/lifecycle/demote` | `demote_lifecycle` | Demote lifecycle |
| GET | `/v1/adapters/{id}/lineage` | `get_adapter_lineage` | Lineage |
| GET | `/v1/adapters/{id}/detail` | `get_adapter_detail` | Detail |
| GET | `/v1/adapters/{id}/manifest` | `download_manifest` | Manifest |
| GET | `/v1/adapters/{id}/training-snapshot` | `get_training_snapshot` | Training snapshot |
| GET | `/v1/adapters/{id}/training-export` | `export_training_provenance` | Training export |
| GET | `/v1/adapters/{id}/export` | `export_adapter` | Export |
| POST | `/v1/adapters/directory/upsert` | `upsert_directory_adapter` | Upsert directory |
| GET | `/v1/adapters/{id}/health` | `get_adapter_health` | Health |
| GET/POST/DELETE | `/v1/adapters/{id}/pin` | pin ops | Pin management |
| GET/POST/DELETE | `/v1/adapters/{id}/archive` | archive ops | Archive |
| POST | `/v1/adapters/{id}/duplicate` | `duplicate_adapter` | Duplicate |
| POST | `/v1/adapters/{id}/state/promote` | `promote_adapter_state` | State promote |
| POST | `/v1/adapters/swap` | `swap_adapters` | Hot-swap |
| GET | `/v1/adapters/{id}/stats` | `get_adapter_stats` | Stats |
| GET | `/v1/adapters/category-policies` | `list_category_policies` | Category policies |
| GET/PUT | `/v1/adapters/category-policies/{cat}` | category ops | Category policy |
| POST | `/v1/adapters/validate-name` | `validate_adapter_name` | Validate name |
| POST | `/v1/stacks/validate-name` | `validate_stack_name` | Validate stack name |
| GET | `/v1/adapters/next-revision/{t}/{d}/{p}` | `get_next_revision` | Next revision |

### Adapter Stacks

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/adapter-stacks` | `adapter_stacks::list_stacks` | List stacks |
| POST | `/v1/adapter-stacks` | `adapter_stacks::create_stack` | Create stack |
| GET/DELETE | `/v1/adapter-stacks/{id}` | stack ops | Stack CRUD |
| GET | `/v1/adapter-stacks/{id}/history` | `get_stack_history` | History |
| GET | `/v1/adapter-stacks/{id}/policies` | `get_stack_policies` | Policies |
| POST | `/v1/adapter-stacks/{id}/activate` | `activate_stack` | Activate |
| POST | `/v1/adapter-stacks/deactivate` | `deactivate_stack` | Deactivate |

### Datasets

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/v1/datasets/upload` | `datasets::upload_dataset` | Upload |
| GET/POST | `/v1/datasets` | dataset ops | List/upload |
| POST | `/v1/datasets/chunked-upload/initiate` | `initiate_chunked_upload` | Chunked init |
| POST/PUT | `/v1/datasets/chunked-upload/{s}/chunk` | chunk ops | Chunk upload |
| POST | `/v1/datasets/chunked-upload/{s}/complete` | `complete_upload` | Complete |
| GET | `/v1/datasets/chunked-upload/{s}/status` | `get_upload_status` | Status |
| DELETE | `/v1/datasets/chunked-upload/{s}` | `cancel_upload` | Cancel |
| GET | `/v1/datasets/chunked-upload/sessions` | `list_sessions` | Sessions |
| POST | `/v1/datasets/chunked-upload/cleanup` | `cleanup_expired` | Cleanup |
| GET | `/v1/datasets/{id}` | `get_dataset` | Get dataset |
| GET/POST | `/v1/datasets/{id}/versions` | version ops | Versions |
| GET | `/v1/datasets/{id}/versions/{rev}` | `get_version` | Get version |
| GET | `/v1/datasets/by-codebase/{id}/versions` | `list_by_codebase` | By codebase |
| POST | `/v1/datasets/{id}/versions/{v}/trust-override` | `trust_override` | Trust override |
| POST | `/v1/datasets/{id}/versions/{v}/safety` | `update_safety` | Safety |
| DELETE | `/v1/datasets/{id}` | `delete_dataset` | Delete |
| GET | `/v1/datasets/{id}/files` | `get_files` | Files |
| GET | `/v1/datasets/{id}/statistics` | `get_statistics` | Stats |
| POST | `/v1/datasets/{id}/validate` | `validate_dataset` | Validate |
| POST | `/v1/datasets/{id}/trust_override` | `apply_trust_override` | Trust override |
| GET | `/v1/datasets/{id}/preview` | `preview_dataset` | Preview |
| GET | `/v1/datasets/upload/progress` | `upload_progress` | Progress |
| POST | `/v1/datasets/from-documents` | `from_documents` | From docs |
| POST | `/v1/training/datasets/from-upload` | `from_upload` | Training dataset |
| GET | `/v1/training/dataset_versions/{id}/manifest` | `get_manifest` | Manifest |
| GET | `/v1/training/dataset_versions/{id}/rows` | `stream_rows` | Stream rows |

### Documents

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| POST | `/v1/documents/upload` | `documents::upload_document` | Upload |
| GET | `/v1/documents` | `documents::list_documents` | List |
| GET | `/v1/documents/{id}` | `documents::get_document` | Get document |
| DELETE | `/v1/documents/{id}` | `documents::delete_document` | Delete |
| GET | `/v1/documents/{id}/chunks` | `list_chunks` | Chunks |
| GET | `/v1/documents/{id}/download` | `download_document` | Download |
| POST | `/v1/documents/{id}/process` | `process_document` | Process |
| POST | `/v1/documents/{id}/retry` | `retry_document` | Retry |
| GET | `/v1/documents/failed` | `list_failed` | Failed docs |

### Training

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET/POST | `/v1/training/jobs` | job ops | List/create |
| GET | `/v1/training/queue` | `get_training_queue` | Queue |
| GET | `/v1/training/jobs/{id}` | `get_training_job` | Get job |
| POST | `/v1/training/start` | `start_training` | Start |
| POST | `/v1/training/repos/{r}/versions/{v}/promote` | `promote_version` | Promote |
| POST | `/v1/training/jobs/{id}/cancel` | `cancel_training` | Cancel |
| POST | `/v1/training/jobs/{id}/retry` | `retry_training` | Retry |
| PATCH | `/v1/training/jobs/{id}/priority` | `update_priority` | Priority |
| POST | `/v1/training/jobs/{id}/export/coreml` | `export_coreml` | Export CoreML |
| POST | `/v1/training/sessions` | `create_session` | Create session |
| GET | `/v1/training/jobs/{id}/logs` | `get_logs` | Logs |
| GET | `/v1/training/jobs/{id}/metrics` | `get_metrics` | Metrics |
| GET | `/v1/training/jobs/{id}/progress` | `stream_progress` | Progress stream |
| POST | `/v1/training/jobs/batch-status` | `batch_status` | Batch status |
| GET | `/v1/training/jobs/{id}/chat_bootstrap` | `get_chat_bootstrap` | Chat bootstrap |
| GET | `/v1/training/templates` | `list_templates` | Templates |
| GET | `/v1/training/templates/{id}` | `get_template` | Get template |

### Diagnostics

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/diagnostics/determinism-status` | `get_determinism_status` | Determinism |
| GET | `/v1/diagnostics/quarantine-status` | `get_quarantine_status` | Quarantine |
| GET | `/v1/diag/runs` | `list_diag_runs` | Diag runs |
| GET | `/v1/diag/runs/{trace_id}` | `get_diag_run` | Get run |
| GET | `/v1/diag/runs/{trace_id}/events` | `list_diag_events` | Events |
| POST | `/v1/diag/diff` | `diff_diag_runs` | Diff runs |
| POST | `/v1/diag/export` | `export_diag_run` | Export run |
| POST | `/v1/diag/bundle` | `create_bundle_export` | Create bundle |
| GET | `/v1/diag/bundle/{id}` | `get_bundle_export` | Get bundle |
| GET | `/v1/diag/bundle/{id}/download` | `download_bundle` | Download |

### SSE Streaming

| Method | Path | Handler | Purpose |
|--------|------|---------|---------|
| GET | `/v1/stream/metrics` | `streams::system_metrics_stream` | Metrics stream |
| GET | `/v1/stream/telemetry` | `streams::telemetry_events_stream` | Telemetry stream |
| GET | `/v1/stream/adapters` | `streams::adapter_state_stream` | Adapter stream |
| GET | `/v1/stream/boot-progress` | `streaming::boot_progress_stream` | Boot progress |
| GET | `/v1/stream/stack-policies/{id}` | `adapter_stacks::stack_policy_stream` | Stack policy |
| GET/HEAD | `/v1/stream/notifications` | `streaming::notifications_stream` | Notifications |
| GET | `/v1/stream/messages/{ws_id}` | `streaming::messages_stream` | Messages |
| GET | `/v1/stream/activity/{ws_id}` | `streaming::activity_stream` | Activity |
| GET | `/v1/stream/trace-receipts` | `streaming::trace_receipts_stream` | Trace receipts |
| GET | `/v1/streams/training` | `streams::training_stream` | Training events |
| GET | `/v1/streams/discovery` | `discovery::discovery_stream` | Discovery events |
| GET | `/v1/streams/contacts` | `discovery::contacts_stream` | Contacts events |
| GET | `/v1/streams/file-changes` | `git::file_changes_stream` | File change notifications |
| GET | `/v1/logs/stream` | `telemetry::stream_logs` | Log stream |

---

## Middleware Stack

### Global Middleware (All Routes, Outermost to Innermost)

1. `request_id_middleware` - Request ID tracking
2. `observability_middleware` - Logging + error envelope
3. `drain_middleware` - Reject during drain
4. `lifecycle_gate` - Reject during maintenance/drain
5. `request_tracking_middleware` - Track in-flight requests
6. `client_ip_middleware` - Extract client IP
7. `seed_isolation_middleware` - Thread-local seed isolation
8. `trace_context_middleware` - W3C Trace Context
9. `versioning_middleware` - API versioning
10. `caching_middleware` - HTTP caching
11. `security_headers_middleware` - Security headers
12. `request_size_limit_middleware` - Limit request sizes
13. `rate_limiting_middleware` - Rate limiting
14. `cors_layer` - CORS
15. `CompressionLayer` - gzip/br/deflate
16. `idempotency_middleware` - Idempotency
17. `ErrorCodeEnforcementLayer` - Machine-readable error codes
18. `TraceLayer` - Request tracing

### Protected Routes Middleware

1. `auth_middleware` - JWT validation
2. `tenant_route_guard_middleware` - Tenant isolation
3. `csrf_middleware` - CSRF protection
4. `context_middleware` - Request context
5. `policy_enforcement_middleware` - Policy decisions
6. `audit_middleware` - Audit logging

---

## Summary Statistics

| Category | Count |
|----------|-------|
| Total Endpoints | ~350+ |
| Public Routes | ~17 |
| Protected Routes | ~300+ |
| Internal Routes | 4 |
| Optional Auth | 2 |
| SSE Streaming | 9 |
