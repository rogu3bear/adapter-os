# AdapterOS API Endpoint Inventory

**Generated:** 2025-11-22
**Source:** `crates/adapteros-server-api/src/routes.rs`
**Total Endpoints:** ~189 registered routes

---

## Table of Contents

1. [Public Endpoints (No Auth)](#public-endpoints-no-auth)
2. [Protected Endpoints (Auth Required)](#protected-endpoints-auth-required)
3. [SSE Streaming Endpoints](#sse-streaming-endpoints)
4. [Middleware Stack](#middleware-stack)
5. [API Consistency Analysis](#api-consistency-analysis)
6. [Unwired Handlers](#unwired-handlers)
7. [Issues and Recommendations](#issues-and-recommendations)

---

## Public Endpoints (No Auth)

These endpoints do not require JWT authentication and are accessible to anyone.

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/healthz` | `handlers::health` | Basic health check |
| GET | `/healthz/all` | `health::check_all_health` | All components health |
| GET | `/healthz/{component}` | `health::check_component_health` | Specific component health |
| GET | `/readyz` | `handlers::ready` | Readiness probe |
| POST | `/v1/auth/login` | `auth_enhanced::login_handler` | User login |
| POST | `/v1/auth/bootstrap` | `auth_enhanced::bootstrap_admin_handler` | Initial admin setup (one-time) |
| POST | `/v1/auth/dev-bypass` | `auth_enhanced::dev_bypass_handler` | Dev bypass (debug builds only) |
| GET | `/v1/meta` | `handlers::meta` | API metadata |
| GET | `/swagger-ui` | SwaggerUi | OpenAPI UI |
| GET | `/api-docs/openapi.json` | SwaggerUi | OpenAPI spec JSON |

### Metrics Endpoint (Custom Auth)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/metrics` | `handlers::metrics_handler` | Prometheus metrics (bearer token auth) |

---

## Protected Endpoints (Auth Required)

All endpoints below require a valid JWT token via `Authorization: Bearer <token>` header.

### Authentication & Sessions

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/auth/logout` | `auth::auth_logout` | User logout |
| GET | `/v1/auth/me` | `auth::auth_me` | Current user info |
| POST | `/v1/auth/refresh` | `auth_enhanced::refresh_token_handler` | Refresh JWT token |
| GET | `/v1/auth/sessions` | `auth_enhanced::list_sessions_handler` | List active sessions |
| DELETE | `/v1/auth/sessions/{jti}` | `auth_enhanced::revoke_session_handler` | Revoke specific session |

### Tenants

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/tenants` | `handlers::list_tenants` | List tenants |
| POST | `/v1/tenants` | `handlers::create_tenant` | Create tenant |
| PUT | `/v1/tenants/{tenant_id}` | `handlers::update_tenant` | Update tenant |
| POST | `/v1/tenants/{tenant_id}/pause` | `handlers::pause_tenant` | Pause tenant |
| POST | `/v1/tenants/{tenant_id}/archive` | `handlers::archive_tenant` | Archive tenant |
| POST | `/v1/tenants/{tenant_id}/policies` | `handlers::assign_tenant_policies` | Assign policies |
| POST | `/v1/tenants/{tenant_id}/adapters` | `handlers::assign_tenant_adapters` | Assign adapters |
| GET | `/v1/tenants/{tenant_id}/usage` | `handlers::get_tenant_usage` | Usage statistics |

### Adapters

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/adapters` | `handlers::list_adapters` | List adapters |
| GET | `/v1/adapters/{adapter_id}` | `handlers::get_adapter` | Get adapter |
| POST | `/v1/adapters/register` | `handlers::register_adapter` | Register adapter |
| POST | `/v1/adapters/import` | `adapters::import_adapter` | Import adapter |
| DELETE | `/v1/adapters/{adapter_id}` | `handlers::delete_adapter` | Delete adapter |
| POST | `/v1/adapters/{adapter_id}/load` | `handlers::load_adapter` | Load adapter |
| POST | `/v1/adapters/{adapter_id}/unload` | `handlers::unload_adapter` | Unload adapter |
| GET | `/v1/adapters/verify-gpu` | `handlers::verify_gpu_integrity` | Verify GPU integrity |
| GET | `/v1/adapters/{adapter_id}/activations` | `handlers::get_adapter_activations` | Get activations |
| POST | `/v1/adapters/{adapter_id}/lifecycle/promote` | `handlers::promote_adapter_lifecycle` | Promote lifecycle state |
| POST | `/v1/adapters/{adapter_id}/lifecycle/demote` | `handlers::demote_adapter_lifecycle` | Demote lifecycle state |
| GET | `/v1/adapters/{adapter_id}/lineage` | `handlers::get_adapter_lineage` | Lineage tree |
| GET | `/v1/adapters/{adapter_id}/detail` | `handlers::get_adapter_detail` | Detail view |
| GET | `/v1/adapters/{adapter_id}/manifest` | `handlers::download_adapter_manifest` | Download manifest |
| POST | `/v1/adapters/directory/upsert` | `handlers::upsert_directory_adapter` | Upsert directory adapter |
| GET | `/v1/adapters/{adapter_id}/health` | `handlers::get_adapter_health` | Adapter health |
| GET/POST/DELETE | `/v1/adapters/{adapter_id}/pin` | `handlers::get_pin_status/pin_adapter/unpin_adapter` | Pin management |
| POST | `/v1/adapters/{adapter_id}/state/promote` | `handlers::promote_adapter_state` | Promote tier state |
| POST | `/v1/adapters/swap` | `adapters::swap_adapters` | Hot-swap adapters |
| GET | `/v1/adapters/{adapter_id}/stats` | `adapters::get_adapter_stats` | Adapter statistics |
| GET | `/v1/adapters/category-policies` | `adapters::list_category_policies` | List category policies |
| GET/PUT | `/v1/adapters/category-policies/{category}` | `adapters::get_category_policy/update_category_policy` | Category policy |
| POST | `/v1/adapters/validate-name` | `handlers::validate_adapter_name` | Validate adapter name |
| GET | `/v1/adapters/next-revision/{tenant}/{domain}/{purpose}` | `handlers::get_next_revision` | Get next revision |

### Adapter Stacks

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/adapter-stacks` | `adapter_stacks::list_stacks` | List stacks |
| POST | `/v1/adapter-stacks` | `adapter_stacks::create_stack` | Create stack |
| GET | `/v1/adapter-stacks/{id}` | `adapter_stacks::get_stack` | Get stack |
| DELETE | `/v1/adapter-stacks/{id}` | `adapter_stacks::delete_stack` | Delete stack |
| POST | `/v1/adapter-stacks/{id}/activate` | `adapter_stacks::activate_stack` | Activate stack |
| POST | `/v1/adapter-stacks/deactivate` | `adapter_stacks::deactivate_stack` | Deactivate stack |
| POST | `/v1/stacks/validate-name` | `handlers::validate_stack_name` | Validate stack name |

### Domain Adapters

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/domain-adapters` | `domain_adapters::list_domain_adapters` | List domain adapters |
| POST | `/v1/domain-adapters` | `domain_adapters::create_domain_adapter` | Create domain adapter |
| GET | `/v1/domain-adapters/{adapter_id}` | `domain_adapters::get_domain_adapter` | Get domain adapter |
| DELETE | `/v1/domain-adapters/{adapter_id}` | `domain_adapters::delete_domain_adapter` | Delete domain adapter |
| POST | `/v1/domain-adapters/{adapter_id}/load` | `domain_adapters::load_domain_adapter` | Load |
| POST | `/v1/domain-adapters/{adapter_id}/unload` | `domain_adapters::unload_domain_adapter` | Unload |
| POST | `/v1/domain-adapters/{adapter_id}/test` | `domain_adapters::test_domain_adapter` | Test |
| GET | `/v1/domain-adapters/{adapter_id}/manifest` | `domain_adapters::get_domain_adapter_manifest` | Get manifest |
| POST | `/v1/domain-adapters/{adapter_id}/execute` | `domain_adapters::execute_domain_adapter` | Execute |

### Inference

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/infer` | `handlers::infer` | Run inference |
| POST | `/v1/infer/stream` | `streaming_infer::streaming_infer` | Streaming inference |
| POST | `/v1/infer/batch` | `batch::batch_infer` | Batch inference |
| POST | `/v1/patch/propose` | `handlers::propose_patch` | Propose patch |

### Training

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/training/jobs` | `handlers::list_training_jobs` | List jobs |
| GET | `/v1/training/jobs/{job_id}` | `handlers::get_training_job` | Get job |
| POST | `/v1/training/start` | `handlers::start_training` | Start training |
| POST | `/v1/training/jobs/{job_id}/cancel` | `handlers::cancel_training` | Cancel job |
| POST | `/v1/training/sessions` | `handlers::create_training_session` | Create session |
| GET | `/v1/training/jobs/{job_id}/logs` | `handlers::get_training_logs` | Job logs |
| GET | `/v1/training/jobs/{job_id}/metrics` | `handlers::get_training_metrics` | Job metrics |
| GET | `/v1/training/jobs/{job_id}/artifacts` | `handlers::get_training_artifacts` | Job artifacts |
| GET | `/v1/training/templates` | `handlers::list_training_templates` | List templates |
| GET | `/v1/training/templates/{template_id}` | `handlers::get_training_template` | Get template |

### Datasets

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/datasets/upload` | `datasets::upload_dataset` | Upload dataset |
| POST | `/v1/datasets/chunked-upload/initiate` | `datasets::initiate_chunked_upload` | Start chunked upload |
| POST | `/v1/datasets/chunked-upload/{session_id}/chunk` | `datasets::upload_chunk` | Upload chunk |
| POST | `/v1/datasets/chunked-upload/{session_id}/complete` | `datasets::complete_chunked_upload` | Complete upload |
| GET | `/v1/datasets/chunked-upload/{session_id}/status` | `datasets::get_upload_session_status` | Session status |
| DELETE | `/v1/datasets/chunked-upload/{session_id}` | `datasets::cancel_chunked_upload` | Cancel upload |
| GET | `/v1/datasets` | `datasets::list_datasets` | List datasets |
| GET | `/v1/datasets/{dataset_id}` | `datasets::get_dataset` | Get dataset |
| DELETE | `/v1/datasets/{dataset_id}` | `datasets::delete_dataset` | Delete dataset |
| GET | `/v1/datasets/{dataset_id}/files` | `datasets::get_dataset_files` | Dataset files |
| GET | `/v1/datasets/{dataset_id}/statistics` | `datasets::get_dataset_statistics` | Statistics |
| POST | `/v1/datasets/{dataset_id}/validate` | `datasets::validate_dataset` | Validate |
| GET | `/v1/datasets/{dataset_id}/preview` | `datasets::preview_dataset` | Preview |
| GET | `/v1/datasets/upload/progress` | `datasets::dataset_upload_progress` | Upload progress |

### Nodes & Workers

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/nodes` | `handlers::list_nodes` | List nodes |
| POST | `/v1/nodes/register` | `handlers::register_node` | Register node |
| POST | `/v1/nodes/{node_id}/ping` | `handlers::test_node_connection` | Ping node |
| POST | `/v1/nodes/{node_id}/offline` | `handlers::mark_node_offline` | Mark offline |
| DELETE | `/v1/nodes/{node_id}` | `handlers::evict_node` | Evict node |
| GET | `/v1/nodes/{node_id}/details` | `handlers::get_node_details` | Node details |
| GET | `/v1/workers` | `handlers::list_workers` | List workers |
| POST | `/v1/workers/spawn` | `handlers::worker_spawn` | Spawn worker |
| GET | `/v1/workers/{worker_id}/logs` | `handlers::list_process_logs` | Worker logs |
| GET | `/v1/workers/{worker_id}/crashes` | `handlers::list_process_crashes` | Worker crashes |
| POST | `/v1/workers/{worker_id}/debug` | `handlers::start_debug_session` | Debug session |
| POST | `/v1/workers/{worker_id}/troubleshoot` | `handlers::run_troubleshooting_step` | Troubleshoot |
| POST | `/v1/workers/{worker_id}/stop` | `handlers::stop_worker` | Stop worker |

### Services (Supervisor)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/services/{service_id}/start` | `services::start_service` | Start service |
| POST | `/v1/services/{service_id}/stop` | `services::stop_service` | Stop service |
| POST | `/v1/services/{service_id}/restart` | `services::restart_service` | Restart service |
| POST | `/v1/services/essential/start` | `services::start_essential_services` | Start essential |
| POST | `/v1/services/essential/stop` | `services::stop_essential_services` | Stop essential |
| GET | `/v1/services/{service_id}/logs` | `services::get_service_logs` | Service logs |

### Models

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/models/import` | `handlers::import_model` | Import model |
| GET | `/v1/models/status` | `handlers::get_base_model_status` | Base model status |
| POST | `/v1/models/{model_id}/load` | `models::load_model` | Load model |
| POST | `/v1/models/{model_id}/unload` | `models::unload_model` | Unload model |
| GET | `/v1/models/{model_id}/status` | `models::get_model_status` | Model status |
| GET | `/v1/models/{model_id}/validate` | `models::validate_model` | Validate model |

### Policies

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/policies` | `handlers::list_policies` | List policies |
| GET | `/v1/policies/{cpid}` | `handlers::get_policy` | Get policy |
| POST | `/v1/policies/validate` | `handlers::validate_policy` | Validate policy |
| POST | `/v1/policies/apply` | `handlers::apply_policy` | Apply policy |
| POST | `/v1/policies/{cpid}/sign` | `handlers::sign_policy` | Sign policy |
| POST | `/v1/policies/compare` | `handlers::compare_policy_versions` | Compare versions |
| GET | `/v1/policies/{cpid}/export` | `handlers::export_policy` | Export policy |

### Routing

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/routing/debug` | `handlers::debug_routing` | Debug routing |
| GET | `/v1/routing/history` | `handlers::get_routing_history` | Routing history |
| GET | `/v1/routing/decisions` | `routing_decisions::get_routing_decisions` | List decisions |
| GET | `/v1/routing/decisions/{id}` | `routing_decisions::get_routing_decision_by_id` | Get decision |
| POST | `/v1/telemetry/routing` | `routing_decisions::ingest_router_decision` | Ingest decision |

### Metrics & Monitoring

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/metrics/quality` | `handlers::get_quality_metrics` | Quality metrics |
| GET | `/v1/metrics/adapters` | `handlers::get_adapter_metrics` | Adapter metrics |
| GET | `/v1/metrics/system` | `handlers::get_system_metrics` | System metrics |
| GET | `/v1/metrics/snapshot` | `telemetry::get_metrics_snapshot` | Metrics snapshot |
| GET | `/v1/metrics/series` | `telemetry::get_metrics_series` | Metrics series |
| GET | `/v1/system/memory` | `handlers::get_uma_memory` | UMA memory info |
| GET | `/v1/monitoring/rules` | `handlers::list_process_monitoring_rules` | List rules |
| POST | `/v1/monitoring/rules` | `handlers::create_process_monitoring_rule` | Create rule |
| GET | `/v1/monitoring/alerts` | `handlers::list_process_alerts` | List alerts |
| POST | `/v1/monitoring/alerts/{alert_id}/acknowledge` | `handlers::acknowledge_process_alert` | Ack alert |
| GET | `/v1/monitoring/anomalies` | `handlers::list_process_anomalies` | List anomalies |
| POST | `/v1/monitoring/anomalies/{anomaly_id}/status` | `handlers::update_process_anomaly_status` | Update status |
| GET | `/v1/monitoring/dashboards` | `handlers::list_process_monitoring_dashboards` | List dashboards |
| POST | `/v1/monitoring/dashboards` | `handlers::create_process_monitoring_dashboard` | Create dashboard |
| GET | `/v1/monitoring/health-metrics` | `handlers::list_process_health_metrics` | Health metrics |
| GET | `/v1/monitoring/reports` | `handlers::list_process_monitoring_reports` | List reports |
| POST | `/v1/monitoring/reports` | `handlers::create_process_monitoring_report` | Create report |

### Telemetry, Traces & Logs

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/telemetry/bundles` | `handlers::list_telemetry_bundles` | List bundles |
| GET | `/v1/telemetry/bundles/{bundle_id}/export` | `handlers::export_telemetry_bundle` | Export bundle |
| POST | `/v1/telemetry/bundles/{bundle_id}/verify` | `handlers::verify_bundle_signature` | Verify signature |
| POST | `/v1/telemetry/bundles/purge` | `handlers::purge_old_bundles` | Purge old bundles |
| GET | `/v1/traces/search` | `telemetry::search_traces` | Search traces |
| GET | `/v1/traces/{trace_id}` | `telemetry::get_trace` | Get trace |
| GET | `/v1/logs/query` | `telemetry::query_logs` | Query logs |

### Audit & Compliance

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/audit/logs` | `handlers::query_audit_logs` | Query audit logs |
| GET | `/v1/audit/federation` | `handlers::get_federation_audit` | Federation audit |
| GET | `/v1/audit/compliance` | `handlers::get_compliance_audit` | Compliance audit |
| GET | `/v1/audits` | `handlers::list_audits_extended` | Extended audits |

### Golden Runs & Promotions

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/golden/runs` | `golden::list_golden_runs` | List golden runs |
| GET | `/v1/golden/runs/{name}` | `golden::get_golden_run` | Get golden run |
| POST | `/v1/golden/compare` | `golden::golden_compare` | Compare runs |
| POST | `/v1/golden/{run_id}/promote` | `promotion::request_promotion` | Request promotion |
| GET | `/v1/golden/{run_id}/promotion` | `promotion::get_promotion_status` | Promotion status |
| POST | `/v1/golden/{run_id}/approve` | `promotion::approve_or_reject_promotion` | Approve/reject |
| GET | `/v1/golden/{run_id}/gates` | `promotion::get_gate_status` | Gate status |
| POST | `/v1/golden/{stage}/rollback` | `promotion::rollback_promotion` | Stage rollback |
| POST | `/v1/cp/promote` | `handlers::cp_promote` | CP promote |
| GET | `/v1/cp/promotion-gates/{cpid}` | `handlers::promotion_gates` | Promotion gates |
| POST | `/v1/cp/rollback` | `handlers::cp_rollback` | CP rollback |
| POST | `/v1/cp/promote/dry-run` | `handlers::cp_dry_run_promote` | Dry-run promote |
| GET | `/v1/cp/promotions` | `handlers::get_promotion_history` | Promotion history |
| GET | `/v1/promotions/{id}` | `handlers::get_promotion` | Get promotion |

### Plans

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/plans` | `handlers::list_plans` | List plans |
| POST | `/v1/plans/build` | `handlers::build_plan` | Build plan |
| GET | `/v1/plans/{plan_id}/details` | `handlers::get_plan_details` | Plan details |
| POST | `/v1/plans/{plan_id}/rebuild` | `handlers::rebuild_plan` | Rebuild plan |
| POST | `/v1/plans/compare` | `handlers::compare_plans` | Compare plans |
| GET | `/v1/plans/{plan_id}/manifest` | `handlers::export_plan_manifest` | Export manifest |

### Git Integration

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/git/status` | `git::git_status` | Git status |
| POST | `/v1/git/sessions/start` | `git::start_git_session` | Start session |
| POST | `/v1/git/sessions/{session_id}/end` | `git::end_git_session` | End session |
| GET | `/v1/git/branches` | `git::list_git_branches` | List branches |

### Code Intelligence

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/code/register-repo` | `code::register_repo` | Register repo |
| POST | `/v1/code/scan` | `code::scan_repo` | Scan repo |
| GET | `/v1/code/scan/{job_id}` | `code::get_scan_status` | Scan status |
| GET | `/v1/code/repositories` | `code::list_repositories` | List repositories |
| GET | `/v1/code/repositories/{repo_id}` | `code::get_repository` | Get repository |
| POST | `/v1/code/commit-delta` | `code::create_commit_delta` | Create commit delta |

### Federation

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/federation/status` | `federation::get_federation_status` | Federation status |
| GET | `/v1/federation/quarantine` | `federation::get_quarantine_status` | Quarantine status |
| POST | `/v1/federation/release-quarantine` | `federation::release_quarantine` | Release quarantine |

### Commits

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/commits` | `handlers::list_commits` | List commits |
| GET | `/v1/commits/{sha}` | `handlers::get_commit` | Get commit |
| GET | `/v1/commits/{sha}/diff` | `handlers::get_commit_diff` | Commit diff |

### Contacts

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/contacts` | `handlers::list_contacts` | List contacts |
| POST | `/v1/contacts` | `handlers::create_contact` | Create contact |
| GET | `/v1/contacts/{id}` | `handlers::get_contact` | Get contact |
| DELETE | `/v1/contacts/{id}` | `handlers::delete_contact` | Delete contact |
| GET | `/v1/contacts/{id}/interactions` | `handlers::get_contact_interactions` | Interactions |

### Replay Sessions

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/replay/sessions` | `replay::list_replay_sessions` | List sessions |
| POST | `/v1/replay/sessions` | `replay::create_replay_session` | Create session |
| GET | `/v1/replay/sessions/{id}` | `replay::get_replay_session` | Get session |
| POST | `/v1/replay/sessions/{id}/verify` | `replay::verify_replay_session` | Verify session |

### Plugins

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| POST | `/v1/plugins/{name}/enable` | `plugins::enable_plugin` | Enable plugin |
| POST | `/v1/plugins/{name}/disable` | `plugins::disable_plugin` | Disable plugin |
| GET | `/v1/plugins/{name}` | `plugins::plugin_status` | Plugin status |
| GET | `/v1/plugins` | `plugins::list_plugins` | List plugins |

### Activity

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/activity/events` | `activity::list_activity_events` | List events |
| POST | `/v1/activity/events` | `activity::create_activity_event` | Create event |
| GET | `/v1/activity/feed` | `activity::list_user_workspace_activity` | Activity feed |

### Workspaces

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/workspaces` | `workspaces::list_workspaces` | List workspaces |
| POST | `/v1/workspaces` | `workspaces::create_workspace` | Create workspace |
| GET | `/v1/workspaces/me` | `workspaces::list_user_workspaces` | User workspaces |
| GET | `/v1/workspaces/{workspace_id}` | `workspaces::get_workspace` | Get workspace |
| PUT | `/v1/workspaces/{workspace_id}` | `workspaces::update_workspace` | Update workspace |
| DELETE | `/v1/workspaces/{workspace_id}` | `workspaces::delete_workspace` | Delete workspace |
| GET | `/v1/workspaces/{workspace_id}/members` | `workspaces::list_workspace_members` | List members |
| POST | `/v1/workspaces/{workspace_id}/members` | `workspaces::add_workspace_member` | Add member |
| PUT | `/v1/workspaces/{workspace_id}/members/{member_id}` | `workspaces::update_workspace_member` | Update member |
| DELETE | `/v1/workspaces/{workspace_id}/members/{member_id}` | `workspaces::remove_workspace_member` | Remove member |
| GET | `/v1/workspaces/{workspace_id}/resources` | `workspaces::list_workspace_resources` | List resources |
| POST | `/v1/workspaces/{workspace_id}/resources` | `workspaces::share_workspace_resource` | Share resource |
| DELETE | `/v1/workspaces/{workspace_id}/resources/{resource_id}` | `workspaces::unshare_workspace_resource` | Unshare resource |

### Notifications

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/notifications` | `notifications::list_notifications` | List notifications |
| GET | `/v1/notifications/summary` | `notifications::get_notification_summary` | Summary |
| POST | `/v1/notifications/{notification_id}/read` | `notifications::mark_notification_read` | Mark read |
| POST | `/v1/notifications/read-all` | `notifications::mark_all_notifications_read` | Mark all read |

### Dashboard

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/dashboard/config` | `dashboard::get_dashboard_config` | Get config |
| PUT | `/v1/dashboard/config` | `dashboard::update_dashboard_config` | Update config |
| POST | `/v1/dashboard/config/reset` | `dashboard::reset_dashboard_config` | Reset config |

### Tutorials

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/tutorials` | `tutorials::list_tutorials` | List tutorials |
| POST | `/v1/tutorials/{tutorial_id}/complete` | `tutorials::mark_tutorial_completed` | Mark completed |
| DELETE | `/v1/tutorials/{tutorial_id}/complete` | `tutorials::unmark_tutorial_completed` | Unmark completed |
| POST | `/v1/tutorials/{tutorial_id}/dismiss` | `tutorials::mark_tutorial_dismissed` | Mark dismissed |
| DELETE | `/v1/tutorials/{tutorial_id}/dismiss` | `tutorials::unmark_tutorial_dismissed` | Unmark dismissed |

### Deprecated Endpoints

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/repositories` | `handlers::list_repositories` | **Deprecated** - Use `/v1/code/repositories` |

---

## SSE Streaming Endpoints

All streaming endpoints require authentication and use Server-Sent Events (SSE).

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/streams/training` | `handlers::training_stream` | Training events |
| GET | `/v1/streams/discovery` | `handlers::discovery_stream` | Discovery events |
| GET | `/v1/streams/contacts` | `handlers::contacts_stream` | Contacts events |
| GET | `/v1/streams/file-changes` | `git::file_changes_stream` | File changes |
| GET | `/v1/stream/metrics` | `handlers::system_metrics_stream` | System metrics |
| GET | `/v1/stream/telemetry` | `handlers::telemetry_events_stream` | Telemetry events |
| GET | `/v1/stream/adapters` | `handlers::adapter_state_stream` | Adapter state |
| GET | `/v1/logs/stream` | `telemetry::stream_logs` | Log stream |

---

## Middleware Stack

The middleware is applied in the following order (outermost to innermost):

1. **`client_ip_middleware`** - Extracts client IP from `X-Forwarded-For` / `X-Real-IP` headers
2. **`security_headers_middleware`** - Adds CSP, X-Frame-Options, X-Content-Type-Options, etc.
3. **`request_size_limit_middleware`** - Limits request body size (10MB POST/PUT, 1KB GET/DELETE)
4. **`rate_limiting_middleware`** - Per-tenant rate limiting with headers
5. **`cors_layer`** - CORS configuration (dev: allow all origins, prod: restricted)
6. **`TraceLayer`** - HTTP request tracing
7. **`auth_middleware`** - JWT validation (only for protected routes)

### Security Headers Applied

- `Content-Security-Policy`
- `X-Frame-Options: DENY`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: strict-origin-when-cross-origin`
- `Permissions-Policy` (disables camera, microphone, geolocation, etc.)
- `Cache-Control: no-cache, no-store, must-revalidate` (for 401/403 responses)

### Rate Limiting Headers

- `X-RateLimit-Remaining`
- `X-RateLimit-Reset`
- `X-RateLimit-Limit`
- `Retry-After` (when rate limit exceeded)

---

## API Consistency Analysis

### Positive Findings

1. **Consistent versioning** - All endpoints use `/v1/` prefix
2. **Consistent error response format** - All handlers use `ErrorResponse` struct
3. **Proper HTTP methods** - GET for reads, POST for creates/actions, PUT for updates, DELETE for removals
4. **Authentication properly separated** - Public vs protected routes clearly defined
5. **OpenAPI/Swagger integration** - Comprehensive schema documentation
6. **SSE streaming** - Proper keep-alive and disconnect handling

### Pagination Patterns

The API uses two pagination patterns:

1. **Offset-based** (most endpoints):
   - Query params: `limit`, `offset`
   - Example: `/v1/audit/logs?limit=50&offset=0`

2. **Page-based** (code intelligence):
   - Query params: `page`, `limit`
   - Example: `/v1/code/repositories?page=1&limit=20`

**Recommendation:** Standardize on offset-based pagination across all endpoints.

---

## Unwired Handlers

The following handler modules exist but are NOT wired to routes:

| Module | File | Status |
|--------|------|--------|
| `messages` | `handlers/messages.rs` | **NOT WIRED** - Workspace messaging |
| `journeys` | `handlers/journeys.rs` | **NOT WIRED** - Journey tracking |
| `git_repository` | `handlers/git_repository.rs` | **NOT WIRED** - Extended git repo analysis |
| `streaming` | `handlers/streaming.rs` | **PARTIAL** - Some types used internally |
| `chunked_upload` | `handlers/chunked_upload.rs` | **INTERNAL** - Used by dataset handlers |

### Missing Tutorial Routes

The tutorial handlers are registered in OpenAPI but routes appear incomplete. The following routes may need verification:

- `POST /v1/tutorials/{tutorial_id}/complete`
- `DELETE /v1/tutorials/{tutorial_id}/complete`
- `POST /v1/tutorials/{tutorial_id}/dismiss`
- `DELETE /v1/tutorials/{tutorial_id}/dismiss`

---

## Issues and Recommendations

### Critical Issues

1. **No issues found** - Route configuration is sound

### Warnings

1. **Deprecated endpoint** - `/v1/repositories` should be removed; redirect to `/v1/code/repositories`

2. **Unwired handlers** - Consider wiring or removing:
   - `messages.rs` - Could be useful for collaboration features
   - `journeys.rs` - Could track adapter/training journeys
   - `git_repository.rs` - Extended git analysis

### Recommendations

1. **Standardize pagination** - Use offset-based pagination consistently

2. **Add missing routes for tutorials** - Verify tutorial completion/dismissal routes are working

3. **Add LIST endpoints for services** - Currently missing `GET /v1/services` to list all services

4. **Consider adding PATCH methods** - For partial updates on resources (adapters, tenants, etc.)

5. **Add rate limit documentation** - Document rate limits per endpoint category

6. **Add request/response examples** - Enhance OpenAPI spec with concrete examples

---

## Testing

Run the endpoint test script:

```bash
# Without authentication (public endpoints only)
./scripts/test_api_endpoints.sh

# With authentication
./scripts/test_api_endpoints.sh http://localhost:8080 "your-jwt-token"

# Or use environment variables
export AOS_API_BASE_URL=http://localhost:8080
export AOS_AUTH_TOKEN="your-jwt-token"
./scripts/test_api_endpoints.sh
```

---

**Document maintained by:** AdapterOS Development Team
**Last Updated:** 2025-11-22
