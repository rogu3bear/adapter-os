# AdapterOS Routes Reference

**Copyright:** 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-11-22
**Maintained by:** James KC Auchterlonie

This document provides a comprehensive reference for all routes in the AdapterOS system, mapping frontend pages to backend API endpoints.

---

## Table of Contents

1. [Overview](#overview)
2. [Frontend Routes](#frontend-routes)
3. [Backend API Endpoints](#backend-api-endpoints)
4. [Route-to-API Mapping](#route-to-api-mapping)
5. [Unwired Handlers](#unwired-handlers)
6. [Authentication Flow](#authentication-flow)
7. [Developer Guide](#developer-guide)
8. [Migration Guide](#migration-guide)

---

## Overview

### Architecture Summary

- **Frontend**: React + TypeScript with react-router-dom
- **Backend**: Rust + Axum with OpenAPI/Swagger documentation
- **Authentication**: JWT (Ed25519) with httpOnly cookies
- **API Version**: v1 (prefix: `/v1/`)

### Route Types

| Type | Description | Auth Required |
|------|-------------|---------------|
| Public | Health checks, login, meta | No |
| Protected | Most API endpoints | Yes (JWT) |
| Admin | Tenant/user management | Yes (Admin role) |
| Metrics | Prometheus-style metrics | Custom auth |

---

## Frontend Routes

### Navigation Groups

The frontend organizes routes into navigation groups for the sidebar menu:

| Group | Routes | Purpose |
|-------|--------|---------|
| Home | Dashboard, Management, Workflow, Personas | Entry points |
| ML Pipeline | Trainer, Training, Testing, Golden, Promotion, Adapters | ML operations |
| Monitoring | Metrics, System Health, Routing | Observability |
| System | Overview, Nodes, Workers, Memory, Metrics | Infrastructure |
| Operations | Inference, Telemetry, Replay | Runtime ops |
| Security | Policies, Audit, Compliance | Security |
| Administration | Admin, Tenants, Stacks, Plugins, Settings, Reports | Admin |

### Complete Frontend Route List

| Path | Component | Auth | Roles | Permissions | Nav Group |
|------|-----------|------|-------|-------------|-----------|
| `/` | Redirect to `/dashboard` | - | - | - | - |
| `/login` | LoginForm | No | - | - | - |
| `/dashboard` | DashboardPage | Yes | - | - | Home |
| `/management` | ManagementPage | Yes | - | - | Home |
| `/workflow` | WorkflowPage | Yes | - | - | Home |
| `/personas` | PersonasPage | No | - | - | Home |
| `/trainer` | TrainerPage | Yes | - | - | ML Pipeline |
| `/training` | TrainingPage | Yes | - | - | ML Pipeline |
| `/training/jobs` | TrainingJobsPage | Yes | - | - | - |
| `/training/jobs/:jobId` | TrainingJobDetailPage | Yes | - | - | - |
| `/training/datasets` | TrainingDatasetsPage | Yes | - | - | - |
| `/training/templates` | TrainingTemplatesPage | Yes | - | - | - |
| `/testing` | TestingPage | Yes | - | - | ML Pipeline |
| `/golden` | GoldenPage | Yes | - | - | ML Pipeline |
| `/promotion` | PromotionPage | Yes | - | - | ML Pipeline |
| `/adapters` | AdaptersPage | Yes | - | - | ML Pipeline |
| `/adapters/new` | AdapterRegisterPage | Yes | - | adapter.register | - |
| `/adapters/:adapterId` | AdapterDetail | Yes | - | - | - |
| `/adapters/:adapterId/activations` | AdapterActivationsPage | Yes | - | - | - |
| `/adapters/:adapterId/lineage` | AdapterLineagePage | Yes | - | - | - |
| `/adapters/:adapterId/manifest` | AdapterManifestPage | Yes | - | - | - |
| `/metrics` | MetricsPage | Yes | - | - | Monitoring |
| `/monitoring` | ObservabilityPage | Yes | - | - | Monitoring |
| `/routing` | RoutingPage | Yes | - | - | Monitoring |
| `/system` | SystemOverviewPage | Yes | - | - | System |
| `/system/nodes` | SystemNodesPage | Yes | - | - | System |
| `/system/workers` | SystemWorkersPage | Yes | - | - | System |
| `/system/memory` | SystemMemoryPage | Yes | - | - | System |
| `/system/metrics` | SystemMetricsPage | Yes | - | - | System |
| `/inference` | InferencePage | Yes | - | - | Operations |
| `/telemetry` | TelemetryPage | Yes | - | - | Operations |
| `/replay` | ReplayPage | Yes | - | - | Operations |
| `/security/policies` | PoliciesPage | Yes | - | - | Security |
| `/security/audit` | AuditPage | Yes | - | audit.view | Security |
| `/security/compliance` | CompliancePage | Yes | - | audit.view | Security |
| `/policies` | PoliciesPage | Yes | - | - | - |
| `/audit` | AuditPage | Yes | - | audit.view | - |
| `/admin` | AdminPage | Yes | admin | - | Administration |
| `/admin/tenants` | TenantsPage | Yes | admin | - | Administration |
| `/admin/tenants/:tenantId` | TenantDetailPage | Yes | admin | - | - |
| `/admin/stacks` | AdminStacksPage | Yes | admin | - | Administration |
| `/admin/plugins` | AdminPluginsPage | Yes | admin | - | Administration |
| `/admin/settings` | AdminSettingsPage | Yes | admin | - | Administration |
| `/reports` | ReportsPage | Yes | - | - | Administration |
| `/tenants` | TenantsPage | Yes | admin | - | - |
| `/base-models` | BaseModelsPage | Yes | - | - | - |

### Legacy Redirects

| From | To | Notes |
|------|-----|-------|
| `/alerts` | `/metrics` | Deprecated |
| `/journeys` | `/audit` | Deprecated |

---

## Backend API Endpoints

### Public Endpoints (No Auth)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/healthz` | `handlers::health` | Health check |
| GET | `/healthz/all` | `health::check_all_health` | All component health |
| GET | `/healthz/{component}` | `health::check_component_health` | Specific component |
| GET | `/readyz` | `handlers::ready` | Readiness check |
| POST | `/v1/auth/login` | `auth_enhanced::login_handler` | User login |
| POST | `/v1/auth/bootstrap` | `auth_enhanced::bootstrap_admin_handler` | Bootstrap admin |
| POST | `/v1/auth/dev-bypass` | `auth_enhanced::dev_bypass_handler` | Dev bypass (dev only) |
| GET | `/v1/meta` | `handlers::meta` | API metadata |

### Authentication Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/auth/logout` | `auth::auth_logout` | Yes | Logout |
| GET | `/v1/auth/me` | `auth::auth_me` | Yes | Current user info |
| POST | `/v1/auth/refresh` | `auth_enhanced::refresh_token_handler` | Yes | Refresh token |
| GET | `/v1/auth/sessions` | `auth_enhanced::list_sessions_handler` | Yes | List sessions |
| DELETE | `/v1/auth/sessions/{jti}` | `auth_enhanced::revoke_session_handler` | Yes | Revoke session |

### Tenant Endpoints

| Method | Path | Handler | Auth | Roles | Description |
|--------|------|---------|------|-------|-------------|
| GET | `/v1/tenants` | `handlers::list_tenants` | Yes | - | List tenants |
| POST | `/v1/tenants` | `handlers::create_tenant` | Yes | Admin | Create tenant |
| PUT | `/v1/tenants/{tenant_id}` | `handlers::update_tenant` | Yes | Admin | Update tenant |
| POST | `/v1/tenants/{tenant_id}/pause` | `handlers::pause_tenant` | Yes | Admin | Pause tenant |
| POST | `/v1/tenants/{tenant_id}/archive` | `handlers::archive_tenant` | Yes | Admin | Archive tenant |
| POST | `/v1/tenants/{tenant_id}/policies` | `handlers::assign_tenant_policies` | Yes | Admin | Assign policies |
| POST | `/v1/tenants/{tenant_id}/adapters` | `handlers::assign_tenant_adapters` | Yes | Admin | Assign adapters |
| GET | `/v1/tenants/{tenant_id}/usage` | `handlers::get_tenant_usage` | Yes | - | Usage stats |

### Adapter Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/adapters` | `handlers::list_adapters` | Yes | List adapters |
| GET | `/v1/adapters/{adapter_id}` | `handlers::get_adapter` | Yes | Get adapter |
| POST | `/v1/adapters/register` | `handlers::register_adapter` | Yes | Register adapter |
| POST | `/v1/adapters/import` | `adapters::import_adapter` | Yes | Import adapter file |
| DELETE | `/v1/adapters/{adapter_id}` | `handlers::delete_adapter` | Yes | Delete adapter |
| POST | `/v1/adapters/{adapter_id}/load` | `handlers::load_adapter` | Yes | Load adapter |
| POST | `/v1/adapters/{adapter_id}/unload` | `handlers::unload_adapter` | Yes | Unload adapter |
| GET | `/v1/adapters/verify-gpu` | `handlers::verify_gpu_integrity` | Yes | Verify GPU |
| GET | `/v1/adapters/{adapter_id}/activations` | `handlers::get_adapter_activations` | Yes | Get activations |
| POST | `/v1/adapters/{adapter_id}/lifecycle/promote` | `handlers::promote_adapter_lifecycle` | Yes | Promote lifecycle |
| POST | `/v1/adapters/{adapter_id}/lifecycle/demote` | `handlers::demote_adapter_lifecycle` | Yes | Demote lifecycle |
| GET | `/v1/adapters/{adapter_id}/lineage` | `handlers::get_adapter_lineage` | Yes | Get lineage |
| GET | `/v1/adapters/{adapter_id}/detail` | `handlers::get_adapter_detail` | Yes | Get detail |
| GET | `/v1/adapters/{adapter_id}/manifest` | `handlers::download_adapter_manifest` | Yes | Download manifest |
| POST | `/v1/adapters/directory/upsert` | `handlers::upsert_directory_adapter` | Yes | Upsert directory |
| GET | `/v1/adapters/{adapter_id}/health` | `handlers::get_adapter_health` | Yes | Get health |
| GET | `/v1/adapters/{adapter_id}/pin` | `handlers::get_pin_status` | Yes | Get pin status |
| POST | `/v1/adapters/{adapter_id}/pin` | `handlers::pin_adapter` | Yes | Pin adapter |
| DELETE | `/v1/adapters/{adapter_id}/pin` | `handlers::unpin_adapter` | Yes | Unpin adapter |
| POST | `/v1/adapters/{adapter_id}/state/promote` | `handlers::promote_adapter_state` | Yes | Promote state |
| POST | `/v1/adapters/swap` | `adapters::swap_adapters` | Yes | Hot-swap adapters |
| GET | `/v1/adapters/{adapter_id}/stats` | `adapters::get_adapter_stats` | Yes | Get stats |
| GET | `/v1/adapters/category-policies` | `adapters::list_category_policies` | Yes | List policies |
| GET | `/v1/adapters/category-policies/{category}` | `adapters::get_category_policy` | Yes | Get policy |
| PUT | `/v1/adapters/category-policies/{category}` | `adapters::update_category_policy` | Yes | Update policy |
| POST | `/v1/adapters/validate-name` | `handlers::validate_adapter_name` | Yes | Validate name |
| GET | `/v1/adapters/next-revision/{tenant}/{domain}/{purpose}` | `handlers::get_next_revision` | Yes | Next revision |

### Adapter Stack Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/adapter-stacks` | `adapter_stacks::list_stacks` | Yes | List stacks |
| POST | `/v1/adapter-stacks` | `adapter_stacks::create_stack` | Yes | Create stack |
| GET | `/v1/adapter-stacks/{id}` | `adapter_stacks::get_stack` | Yes | Get stack |
| DELETE | `/v1/adapter-stacks/{id}` | `adapter_stacks::delete_stack` | Yes | Delete stack |
| POST | `/v1/adapter-stacks/{id}/activate` | `adapter_stacks::activate_stack` | Yes | Activate stack |
| POST | `/v1/adapter-stacks/deactivate` | `adapter_stacks::deactivate_stack` | Yes | Deactivate |
| POST | `/v1/stacks/validate-name` | `handlers::validate_stack_name` | Yes | Validate name |

### Domain Adapter Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/domain-adapters` | `domain_adapters::list_domain_adapters` | Yes | List |
| POST | `/v1/domain-adapters` | `domain_adapters::create_domain_adapter` | Yes | Create |
| GET | `/v1/domain-adapters/{adapter_id}` | `domain_adapters::get_domain_adapter` | Yes | Get |
| DELETE | `/v1/domain-adapters/{adapter_id}` | `domain_adapters::delete_domain_adapter` | Yes | Delete |
| POST | `/v1/domain-adapters/{adapter_id}/load` | `domain_adapters::load_domain_adapter` | Yes | Load |
| POST | `/v1/domain-adapters/{adapter_id}/unload` | `domain_adapters::unload_domain_adapter` | Yes | Unload |
| POST | `/v1/domain-adapters/{adapter_id}/test` | `domain_adapters::test_domain_adapter` | Yes | Test |
| GET | `/v1/domain-adapters/{adapter_id}/manifest` | `domain_adapters::get_domain_adapter_manifest` | Yes | Get manifest |
| POST | `/v1/domain-adapters/{adapter_id}/execute` | `domain_adapters::execute_domain_adapter` | Yes | Execute |

### Training Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/training/jobs` | `handlers::list_training_jobs` | Yes | List jobs |
| GET | `/v1/training/jobs/{job_id}` | `handlers::get_training_job` | Yes | Get job |
| POST | `/v1/training/start` | `handlers::start_training` | Yes | Start training |
| POST | `/v1/training/jobs/{job_id}/cancel` | `handlers::cancel_training` | Yes | Cancel job |
| POST | `/v1/training/sessions` | `handlers::create_training_session` | Yes | Create session |
| GET | `/v1/training/jobs/{job_id}/logs` | `handlers::get_training_logs` | Yes | Get logs |
| GET | `/v1/training/jobs/{job_id}/metrics` | `handlers::get_training_metrics` | Yes | Get metrics |
| GET | `/v1/training/jobs/{job_id}/artifacts` | `handlers::get_training_artifacts` | Yes | Get artifacts |
| GET | `/v1/training/templates` | `handlers::list_training_templates` | Yes | List templates |
| GET | `/v1/training/templates/{template_id}` | `handlers::get_training_template` | Yes | Get template |

### Dataset Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/datasets/upload` | `datasets::upload_dataset` | Yes | Upload dataset |
| POST | `/v1/datasets/chunked-upload/initiate` | `datasets::initiate_chunked_upload` | Yes | Initiate chunked |
| POST | `/v1/datasets/chunked-upload/{session_id}/chunk` | `datasets::upload_chunk` | Yes | Upload chunk |
| POST | `/v1/datasets/chunked-upload/{session_id}/complete` | `datasets::complete_chunked_upload` | Yes | Complete upload |
| GET | `/v1/datasets/chunked-upload/{session_id}/status` | `datasets::get_upload_session_status` | Yes | Get status |
| DELETE | `/v1/datasets/chunked-upload/{session_id}` | `datasets::cancel_chunked_upload` | Yes | Cancel upload |
| GET | `/v1/datasets` | `datasets::list_datasets` | Yes | List datasets |
| GET | `/v1/datasets/{dataset_id}` | `datasets::get_dataset` | Yes | Get dataset |
| DELETE | `/v1/datasets/{dataset_id}` | `datasets::delete_dataset` | Yes | Delete dataset |
| GET | `/v1/datasets/{dataset_id}/files` | `datasets::get_dataset_files` | Yes | Get files |
| GET | `/v1/datasets/{dataset_id}/statistics` | `datasets::get_dataset_statistics` | Yes | Get statistics |
| POST | `/v1/datasets/{dataset_id}/validate` | `datasets::validate_dataset` | Yes | Validate |
| GET | `/v1/datasets/{dataset_id}/preview` | `datasets::preview_dataset` | Yes | Preview |
| GET | `/v1/datasets/upload/progress` | `datasets::dataset_upload_progress` | Yes | Upload progress |

### Inference Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/infer` | `handlers::infer` | Yes | Run inference |
| POST | `/v1/infer/stream` | `streaming_infer::streaming_infer` | Yes | Stream inference (SSE) |
| POST | `/v1/infer/batch` | `batch::batch_infer` | Yes | Batch inference |
| POST | `/v1/patch/propose` | `handlers::propose_patch` | Yes | Propose patch |

### Node & Worker Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/nodes` | `handlers::list_nodes` | Yes | List nodes |
| POST | `/v1/nodes/register` | `handlers::register_node` | Yes | Register node |
| POST | `/v1/nodes/{node_id}/ping` | `handlers::test_node_connection` | Yes | Ping node |
| POST | `/v1/nodes/{node_id}/offline` | `handlers::mark_node_offline` | Yes | Mark offline |
| DELETE | `/v1/nodes/{node_id}` | `handlers::evict_node` | Yes | Evict node |
| GET | `/v1/nodes/{node_id}/details` | `handlers::get_node_details` | Yes | Node details |
| GET | `/v1/workers` | `handlers::list_workers` | Yes | List workers |
| POST | `/v1/workers/spawn` | `handlers::worker_spawn` | Yes | Spawn worker |
| GET | `/v1/workers/{worker_id}/logs` | `handlers::list_process_logs` | Yes | Worker logs |
| GET | `/v1/workers/{worker_id}/crashes` | `handlers::list_process_crashes` | Yes | Worker crashes |
| POST | `/v1/workers/{worker_id}/debug` | `handlers::start_debug_session` | Yes | Debug session |
| POST | `/v1/workers/{worker_id}/troubleshoot` | `handlers::run_troubleshooting_step` | Yes | Troubleshoot |
| POST | `/v1/workers/{worker_id}/stop` | `handlers::stop_worker` | Yes | Stop worker |

### Service Control Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/services/{service_id}/start` | `services::start_service` | Yes | Start service |
| POST | `/v1/services/{service_id}/stop` | `services::stop_service` | Yes | Stop service |
| POST | `/v1/services/{service_id}/restart` | `services::restart_service` | Yes | Restart service |
| POST | `/v1/services/essential/start` | `services::start_essential_services` | Yes | Start essential |
| POST | `/v1/services/essential/stop` | `services::stop_essential_services` | Yes | Stop essential |
| GET | `/v1/services/{service_id}/logs` | `services::get_service_logs` | Yes | Service logs |

### Model Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/models/import` | `handlers::import_model` | Yes | Import model |
| GET | `/v1/models/status` | `handlers::get_base_model_status` | Yes | Model status |
| POST | `/v1/models/{model_id}/load` | `models::load_model` | Yes | Load model |
| POST | `/v1/models/{model_id}/unload` | `models::unload_model` | Yes | Unload model |
| GET | `/v1/models/{model_id}/status` | `models::get_model_status` | Yes | Get status |
| GET | `/v1/models/{model_id}/validate` | `models::validate_model` | Yes | Validate model |

### Policy Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/policies` | `handlers::list_policies` | Yes | List policies |
| GET | `/v1/policies/{cpid}` | `handlers::get_policy` | Yes | Get policy |
| POST | `/v1/policies/validate` | `handlers::validate_policy` | Yes | Validate policy |
| POST | `/v1/policies/apply` | `handlers::apply_policy` | Yes | Apply policy |
| POST | `/v1/policies/{cpid}/sign` | `handlers::sign_policy` | Yes | Sign policy |
| POST | `/v1/policies/compare` | `handlers::compare_policy_versions` | Yes | Compare versions |
| GET | `/v1/policies/{cpid}/export` | `handlers::export_policy` | Yes | Export policy |

### Metrics Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/metrics` | `handlers::metrics_handler` | Custom | Prometheus metrics |
| GET | `/v1/metrics/quality` | `handlers::get_quality_metrics` | Yes | Quality metrics |
| GET | `/v1/metrics/adapters` | `handlers::get_adapter_metrics` | Yes | Adapter metrics |
| GET | `/v1/metrics/system` | `handlers::get_system_metrics` | Yes | System metrics |
| GET | `/v1/metrics/snapshot` | `telemetry::get_metrics_snapshot` | Yes | Metrics snapshot |
| GET | `/v1/metrics/series` | `telemetry::get_metrics_series` | Yes | Metrics series |

### Routing Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/routing/debug` | `handlers::debug_routing` | Yes | Debug routing |
| GET | `/v1/routing/history` | `handlers::get_routing_history` | Yes | Routing history |
| GET | `/v1/routing/decisions` | `routing_decisions::get_routing_decisions` | Yes | List decisions |
| GET | `/v1/routing/decisions/{id}` | `routing_decisions::get_routing_decision_by_id` | Yes | Get decision |
| POST | `/v1/telemetry/routing` | `routing_decisions::ingest_router_decision` | Yes | Ingest decision |

### Telemetry & Logs Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/telemetry/bundles` | `handlers::list_telemetry_bundles` | Yes | List bundles |
| GET | `/v1/telemetry/bundles/{bundle_id}/export` | `handlers::export_telemetry_bundle` | Yes | Export bundle |
| POST | `/v1/telemetry/bundles/{bundle_id}/verify` | `handlers::verify_bundle_signature` | Yes | Verify bundle |
| POST | `/v1/telemetry/bundles/purge` | `handlers::purge_old_bundles` | Yes | Purge old |
| GET | `/v1/traces/search` | `telemetry::search_traces` | Yes | Search traces |
| GET | `/v1/traces/{trace_id}` | `telemetry::get_trace` | Yes | Get trace |
| GET | `/v1/logs/query` | `telemetry::query_logs` | Yes | Query logs |
| GET | `/v1/logs/stream` | `telemetry::stream_logs` | Yes | Stream logs (SSE) |

### Audit Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/audit/logs` | `handlers::query_audit_logs` | Yes | Query audit logs |
| GET | `/v1/audit/federation` | `handlers::get_federation_audit` | Yes | Federation audit |
| GET | `/v1/audit/compliance` | `handlers::get_compliance_audit` | Yes | Compliance audit |
| GET | `/v1/audits` | `handlers::list_audits_extended` | Yes | Extended audits |

### Golden Run & Promotion Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/golden/runs` | `golden::list_golden_runs` | Yes | List golden runs |
| GET | `/v1/golden/runs/{name}` | `golden::get_golden_run` | Yes | Get golden run |
| POST | `/v1/golden/compare` | `golden::golden_compare` | Yes | Compare runs |
| POST | `/v1/golden/{run_id}/promote` | `promotion::request_promotion` | Yes | Request promotion |
| GET | `/v1/golden/{run_id}/promotion` | `promotion::get_promotion_status` | Yes | Promotion status |
| POST | `/v1/golden/{run_id}/approve` | `promotion::approve_or_reject_promotion` | Yes | Approve/reject |
| GET | `/v1/golden/{run_id}/gates` | `promotion::get_gate_status` | Yes | Gate status |
| POST | `/v1/golden/{stage}/rollback` | `promotion::rollback_promotion` | Yes | Rollback |
| POST | `/v1/cp/promote` | `handlers::cp_promote` | Yes | CP promote |
| GET | `/v1/cp/promotion-gates/{cpid}` | `handlers::promotion_gates` | Yes | Promotion gates |
| POST | `/v1/cp/rollback` | `handlers::cp_rollback` | Yes | CP rollback |
| POST | `/v1/cp/promote/dry-run` | `handlers::cp_dry_run_promote` | Yes | Dry-run promote |
| GET | `/v1/cp/promotions` | `handlers::get_promotion_history` | Yes | Promotion history |
| GET | `/v1/promotions/{id}` | `handlers::get_promotion` | Yes | Get promotion |

### SSE Stream Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/streams/training` | `handlers::training_stream` | Yes | Training events |
| GET | `/v1/streams/discovery` | `handlers::discovery_stream` | Yes | Discovery events |
| GET | `/v1/streams/contacts` | `handlers::contacts_stream` | Yes | Contacts events |
| GET | `/v1/streams/file-changes` | `git::file_changes_stream` | Yes | File changes |
| GET | `/v1/stream/metrics` | `handlers::system_metrics_stream` | Yes | System metrics |
| GET | `/v1/stream/telemetry` | `handlers::telemetry_events_stream` | Yes | Telemetry events |
| GET | `/v1/stream/adapters` | `handlers::adapter_state_stream` | Yes | Adapter state |

### Git Integration Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/git/status` | `git::git_status` | Yes | Git status |
| POST | `/v1/git/sessions/start` | `git::start_git_session` | Yes | Start session |
| POST | `/v1/git/sessions/{session_id}/end` | `git::end_git_session` | Yes | End session |
| GET | `/v1/git/branches` | `git::list_git_branches` | Yes | List branches |

### Code Intelligence Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| POST | `/v1/code/register-repo` | `code::register_repo` | Yes | Register repo |
| POST | `/v1/code/scan` | `code::scan_repo` | Yes | Scan repo |
| GET | `/v1/code/scan/{job_id}` | `code::get_scan_status` | Yes | Scan status |
| GET | `/v1/code/repositories` | `code::list_repositories` | Yes | List repos |
| GET | `/v1/code/repositories/{repo_id}` | `code::get_repository` | Yes | Get repo |
| POST | `/v1/code/commit-delta` | `code::create_commit_delta` | Yes | Commit delta |
| GET | `/v1/repositories` | `handlers::list_repositories` | Yes | List repos (deprecated) |

### Federation Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/federation/status` | `federation::get_federation_status` | Yes | Federation status |
| GET | `/v1/federation/quarantine` | `federation::get_quarantine_status` | Yes | Quarantine status |
| POST | `/v1/federation/release-quarantine` | `federation::release_quarantine` | Yes | Release quarantine |

### Contacts Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/contacts` | `handlers::list_contacts` | Yes | List contacts |
| POST | `/v1/contacts` | `handlers::create_contact` | Yes | Create contact |
| GET | `/v1/contacts/{id}` | `handlers::get_contact` | Yes | Get contact |
| DELETE | `/v1/contacts/{id}` | `handlers::delete_contact` | Yes | Delete contact |
| GET | `/v1/contacts/{id}/interactions` | `handlers::get_contact_interactions` | Yes | Get interactions |

### Plugin Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/plugins` | `plugins::list_plugins` | Yes | List plugins |
| GET | `/v1/plugins/{name}` | `plugins::plugin_status` | Yes | Plugin status |
| POST | `/v1/plugins/{name}/enable` | `plugins::enable_plugin` | Yes | Enable plugin |
| POST | `/v1/plugins/{name}/disable` | `plugins::disable_plugin` | Yes | Disable plugin |

### Monitoring Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/monitoring/rules` | `handlers::list_process_monitoring_rules` | Yes | List rules |
| POST | `/v1/monitoring/rules` | `handlers::create_process_monitoring_rule` | Yes | Create rule |
| GET | `/v1/monitoring/alerts` | `handlers::list_process_alerts` | Yes | List alerts |
| POST | `/v1/monitoring/alerts/{alert_id}/acknowledge` | `handlers::acknowledge_process_alert` | Yes | Ack alert |
| GET | `/v1/monitoring/anomalies` | `handlers::list_process_anomalies` | Yes | List anomalies |
| POST | `/v1/monitoring/anomalies/{anomaly_id}/status` | `handlers::update_process_anomaly_status` | Yes | Update status |
| GET | `/v1/monitoring/dashboards` | `handlers::list_process_monitoring_dashboards` | Yes | List dashboards |
| POST | `/v1/monitoring/dashboards` | `handlers::create_process_monitoring_dashboard` | Yes | Create dashboard |
| GET | `/v1/monitoring/health-metrics` | `handlers::list_process_health_metrics` | Yes | Health metrics |
| GET | `/v1/monitoring/reports` | `handlers::list_process_monitoring_reports` | Yes | List reports |
| POST | `/v1/monitoring/reports` | `handlers::create_process_monitoring_report` | Yes | Create report |

### Replay Session Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/replay/sessions` | `replay::list_replay_sessions` | Yes | List sessions |
| POST | `/v1/replay/sessions` | `replay::create_replay_session` | Yes | Create session |
| GET | `/v1/replay/sessions/{id}` | `replay::get_replay_session` | Yes | Get session |
| POST | `/v1/replay/sessions/{id}/verify` | `replay::verify_replay_session` | Yes | Verify session |

### Plan Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/plans` | `handlers::list_plans` | Yes | List plans |
| POST | `/v1/plans/build` | `handlers::build_plan` | Yes | Build plan |
| GET | `/v1/plans/{plan_id}/details` | `handlers::get_plan_details` | Yes | Plan details |
| POST | `/v1/plans/{plan_id}/rebuild` | `handlers::rebuild_plan` | Yes | Rebuild plan |
| POST | `/v1/plans/compare` | `handlers::compare_plans` | Yes | Compare plans |
| GET | `/v1/plans/{plan_id}/manifest` | `handlers::export_plan_manifest` | Yes | Export manifest |

### Activity & Workspace Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/activity/events` | `activity::list_activity_events` | Yes | List events |
| POST | `/v1/activity/events` | `activity::create_activity_event` | Yes | Create event |
| GET | `/v1/activity/feed` | `activity::list_user_workspace_activity` | Yes | Activity feed |
| GET | `/v1/workspaces` | `workspaces::list_workspaces` | Yes | List workspaces |
| POST | `/v1/workspaces` | `workspaces::create_workspace` | Yes | Create workspace |
| GET | `/v1/workspaces/me` | `workspaces::list_user_workspaces` | Yes | My workspaces |
| GET | `/v1/workspaces/{workspace_id}` | `workspaces::get_workspace` | Yes | Get workspace |
| PUT | `/v1/workspaces/{workspace_id}` | `workspaces::update_workspace` | Yes | Update workspace |
| DELETE | `/v1/workspaces/{workspace_id}` | `workspaces::delete_workspace` | Yes | Delete workspace |
| GET | `/v1/workspaces/{workspace_id}/members` | `workspaces::list_workspace_members` | Yes | List members |
| POST | `/v1/workspaces/{workspace_id}/members` | `workspaces::add_workspace_member` | Yes | Add member |
| PUT | `/v1/workspaces/{workspace_id}/members/{member_id}` | `workspaces::update_workspace_member` | Yes | Update member |
| DELETE | `/v1/workspaces/{workspace_id}/members/{member_id}` | `workspaces::remove_workspace_member` | Yes | Remove member |
| GET | `/v1/workspaces/{workspace_id}/resources` | `workspaces::list_workspace_resources` | Yes | List resources |
| POST | `/v1/workspaces/{workspace_id}/resources` | `workspaces::share_workspace_resource` | Yes | Share resource |
| DELETE | `/v1/workspaces/{workspace_id}/resources/{resource_id}` | `workspaces::unshare_workspace_resource` | Yes | Unshare |

### Notification Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/notifications` | `notifications::list_notifications` | Yes | List notifications |
| GET | `/v1/notifications/summary` | `notifications::get_notification_summary` | Yes | Summary |
| POST | `/v1/notifications/{notification_id}/read` | `notifications::mark_notification_read` | Yes | Mark read |
| POST | `/v1/notifications/read-all` | `notifications::mark_all_notifications_read` | Yes | Mark all read |

### Dashboard & Tutorial Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/dashboard/config` | `dashboard::get_dashboard_config` | Yes | Get config |
| PUT | `/v1/dashboard/config` | `dashboard::update_dashboard_config` | Yes | Update config |
| POST | `/v1/dashboard/config/reset` | `dashboard::reset_dashboard_config` | Yes | Reset config |

### System Endpoints

| Method | Path | Handler | Auth | Description |
|--------|------|---------|------|-------------|
| GET | `/v1/system/memory` | `handlers::get_uma_memory` | Yes | UMA memory info |
| GET | `/v1/commits` | `handlers::list_commits` | Yes | List commits |
| GET | `/v1/commits/{sha}` | `handlers::get_commit` | Yes | Get commit |
| GET | `/v1/commits/{sha}/diff` | `handlers::get_commit_diff` | Yes | Get diff |
| GET | `/v1/jobs` | `handlers::list_jobs` | Yes | List jobs |

### OpenAPI/Swagger

| Method | Path | Description |
|--------|------|-------------|
| GET | `/swagger-ui` | Swagger UI interface |
| GET | `/api-docs/openapi.json` | OpenAPI spec (JSON) |

---

## Route-to-API Mapping

### Dashboard Page (`/dashboard`)

| UI Component | API Endpoint |
|--------------|--------------|
| Health Widget | `GET /healthz/all` |
| Adapter Status | `GET /v1/adapters`, `GET /v1/metrics/adapters` |
| System Metrics | `GET /v1/metrics/system` |
| Activity Feed | `GET /v1/activity/feed` |
| Notifications | `GET /v1/notifications/summary` |

### Adapters Page (`/adapters`)

| UI Component | API Endpoint |
|--------------|--------------|
| Adapter List | `GET /v1/adapters` |
| Adapter Detail | `GET /v1/adapters/{id}` |
| Register Form | `POST /v1/adapters/register` |
| Import Adapter | `POST /v1/adapters/import` |
| Load/Unload | `POST /v1/adapters/{id}/load`, `POST /v1/adapters/{id}/unload` |
| Pin/Unpin | `POST /v1/adapters/{id}/pin`, `DELETE /v1/adapters/{id}/pin` |
| Activations | `GET /v1/adapters/{id}/activations` |
| Lineage | `GET /v1/adapters/{id}/lineage` |
| Manifest | `GET /v1/adapters/{id}/manifest` |

### Training Page (`/training`)

| UI Component | API Endpoint |
|--------------|--------------|
| Job List | `GET /v1/training/jobs` |
| Job Detail | `GET /v1/training/jobs/{id}` |
| Start Training | `POST /v1/training/start` |
| Cancel Job | `POST /v1/training/jobs/{id}/cancel` |
| Job Logs | `GET /v1/training/jobs/{id}/logs` |
| Job Metrics | `GET /v1/training/jobs/{id}/metrics` |
| Templates | `GET /v1/training/templates` |
| Datasets | `GET /v1/datasets` |
| Stream Events | `GET /v1/streams/training` (SSE) |

### Inference Page (`/inference`)

| UI Component | API Endpoint |
|--------------|--------------|
| Infer Form | `POST /v1/infer` |
| Stream Output | `POST /v1/infer/stream` (SSE) |
| Batch Infer | `POST /v1/infer/batch` |
| Model Status | `GET /v1/models/status` |

### Metrics Page (`/metrics`)

| UI Component | API Endpoint |
|--------------|--------------|
| System Metrics | `GET /v1/metrics/system` |
| Quality Metrics | `GET /v1/metrics/quality` |
| Adapter Metrics | `GET /v1/metrics/adapters` |
| Metrics Stream | `GET /v1/stream/metrics` (SSE) |

### System Pages (`/system/*`)

| UI Component | API Endpoint |
|--------------|--------------|
| Nodes List | `GET /v1/nodes` |
| Node Details | `GET /v1/nodes/{id}/details` |
| Workers List | `GET /v1/workers` |
| Worker Logs | `GET /v1/workers/{id}/logs` |
| Memory Info | `GET /v1/system/memory` |

### Admin Pages (`/admin/*`)

| UI Component | API Endpoint |
|--------------|--------------|
| Tenants List | `GET /v1/tenants` |
| Create Tenant | `POST /v1/tenants` |
| Adapter Stacks | `GET /v1/adapter-stacks` |
| Plugins | `GET /v1/plugins` |

### Audit Page (`/security/audit`)

| UI Component | API Endpoint |
|--------------|--------------|
| Audit Logs | `GET /v1/audit/logs` |
| Federation Audit | `GET /v1/audit/federation` |
| Compliance Audit | `GET /v1/audit/compliance` |

---

## Unwired Handlers

The following handler modules exist in `crates/adapteros-server-api/src/handlers/` but are **not wired** to routes in `routes.rs`:

| Module | Status | Description |
|--------|--------|-------------|
| `messages.rs` | Unwired | Workspace messaging with thread support |
| `journeys.rs` | Unwired | Journey tracking for adapters/training |
| `streaming.rs` | Partially wired | SSE streaming utilities (base module) |
| `chunked_upload.rs` | Used internally | Upload session management (used by datasets) |
| `git_repository.rs` | Unwired | Git repository management (separate from `git.rs`) |

### Recommendations

1. **messages.rs**: Wire to `/v1/workspaces/{id}/messages` if workspace messaging is needed
2. **journeys.rs**: Wire to `/v1/journeys/{type}/{id}` or integrate with audit system
3. **git_repository.rs**: Consider merging with `code.rs` or adding dedicated routes

---

## Authentication Flow

### Login Flow

```
1. User submits credentials to POST /v1/auth/login
2. Server validates and returns JWT in httpOnly cookie
3. All subsequent requests include cookie automatically
4. Frontend stores user info, NOT the token
```

### Token Refresh Flow

```
1. Frontend detects 401 response
2. Calls POST /v1/auth/refresh
3. Server issues new token in cookie
4. Retry original request
```

### Logout Flow

```
1. User clicks logout
2. Frontend calls POST /v1/auth/logout
3. Server invalidates session and clears cookie
4. Frontend redirects to /login
```

### Permission Check

```rust
// Backend: Require specific permission
require_permission(&claims, Permission::AdapterRegister)?;

// Backend: Require specific role
require_role(&claims, Role::Admin)?;

// Frontend: Route guard checks requiredRoles and requiredPermissions
<RouteGuard route={routeConfig} />
```

---

## Developer Guide

### Adding a New Backend Route

1. **Create handler function** in appropriate handler module:

```rust
// crates/adapteros-server-api/src/handlers/my_feature.rs

#[utoipa::path(
    tag = "my-feature",
    get,
    path = "/v1/my-feature",
    responses(
        (status = 200, description = "Success", body = MyResponse)
    )
)]
pub async fn my_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Implementation
}
```

2. **Add to routes.rs**:

```rust
// In protected_routes:
.route("/v1/my-feature", get(handlers::my_feature::my_handler))
```

3. **Add to OpenAPI spec** in `ApiDoc` struct:

```rust
#[openapi(
    paths(
        // ... existing paths
        handlers::my_feature::my_handler,
    ),
    // ...
)]
```

### Adding a New Frontend Route

1. **Create page component**:

```tsx
// ui/src/pages/MyFeaturePage.tsx
export default function MyFeaturePage() {
  return <div>My Feature</div>;
}
```

2. **Add to routes config**:

```tsx
// ui/src/config/routes.ts
const MyFeaturePage = lazy(() => import('@/pages/MyFeaturePage'));

export const routes: RouteConfig[] = [
  // ... existing routes
  {
    path: '/my-feature',
    component: MyFeaturePage,
    requiresAuth: true,
    navGroup: 'Operations',
    navTitle: 'My Feature',
    navIcon: Star,
    navOrder: 10,
    breadcrumb: 'My Feature',
  },
];
```

3. **Add API client method**:

```tsx
// ui/src/api/client.ts
async myFeature(): Promise<MyResponse> {
  return this.request<MyResponse>('/v1/my-feature');
}
```

### Route Naming Conventions

- Use kebab-case for URL paths: `/adapter-stacks`
- Use `{param}` for path parameters in backend: `/v1/adapters/{adapter_id}`
- Use `:param` for path parameters in frontend: `/adapters/:adapterId`
- Prefix all API routes with `/v1/`
- Group related endpoints under common prefix

### Authentication Requirements Checklist

- [ ] Route added to `protected_routes` (not `public_routes`)
- [ ] Permission check added if needed (`require_permission`)
- [ ] Role check added if needed (`require_role`)
- [ ] Frontend route has `requiresAuth: true`
- [ ] Frontend route has `requiredRoles` if admin-only
- [ ] Frontend route has `requiredPermissions` if specific permission needed

---

## Migration Guide

### Deprecated Routes

| Deprecated | Replacement | Notes |
|------------|-------------|-------|
| `GET /v1/repositories` | `GET /v1/code/repositories` | Use code intelligence API |
| `/alerts` (frontend) | `/metrics` | UI redirect in place |
| `/journeys` (frontend) | `/audit` | UI redirect in place |

### Breaking Changes

None currently documented.

### Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2025-11-22 | Initial documentation |

---

## Appendix: API Statistics

| Category | Count |
|----------|-------|
| Total Backend Endpoints | ~189 |
| Public Endpoints | 8 |
| Protected Endpoints | ~181 |
| SSE Stream Endpoints | 7 |
| Frontend Routes | 44 |
| Handler Modules | 31 |
| Unwired Handlers | 3 |

---

**Source Files:**
- Backend routes: `/Users/star/Dev/aos/crates/adapteros-server-api/src/routes.rs`
- Frontend routes: `/Users/star/Dev/aos/ui/src/config/routes.ts`
- API client: `/Users/star/Dev/aos/ui/src/api/client.ts`
