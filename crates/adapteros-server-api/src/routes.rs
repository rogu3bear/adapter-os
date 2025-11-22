use crate::handlers;
use crate::handlers::auth;
use crate::handlers::domain_adapters;
use crate::middleware::{auth_middleware, client_ip_middleware, dual_auth_middleware};
use crate::middleware_security::{
    cors_layer, rate_limiting_middleware, request_size_limit_middleware,
    security_headers_middleware,
};
use crate::state::AppState;
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::ready,
        crate::health::check_all_health,
        crate::health::check_component_health,
        handlers::auth::auth_login,
        handlers::auth::auth_logout,
        handlers::auth::auth_me,
        handlers::propose_patch,
        handlers::infer,
        handlers::streaming_infer::streaming_infer,
        handlers::batch::batch_infer,
        handlers::list_adapters,
        handlers::get_adapter,
        handlers::register_adapter,
        handlers::delete_adapter,
        handlers::load_adapter,
        handlers::unload_adapter,
        handlers::verify_gpu_integrity,
        handlers::get_adapter_activations,
        handlers::promote_adapter_state,
        handlers::list_repositories,
        handlers::get_quality_metrics,
        handlers::get_adapter_metrics,
        handlers::get_system_metrics,
        handlers::list_commits,
        handlers::get_commit,
        handlers::get_commit_diff,
        handlers::debug_routing,
        handlers::get_routing_history,
        // Contacts and Streams handlers - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md
        handlers::list_contacts,
        handlers::create_contact,
        handlers::get_contact,
        handlers::delete_contact,
        handlers::get_contact_interactions,
        handlers::training_stream,
        handlers::discovery_stream,
        handlers::contacts_stream,
        // Training handlers
        handlers::list_training_jobs,
        handlers::get_training_job,
        handlers::start_training,
        handlers::cancel_training,
        handlers::create_training_session,
        handlers::get_training_logs,
        handlers::get_training_metrics,
        handlers::list_training_templates,
        handlers::get_training_template,
        // Git integration handlers
        handlers::git::git_status,
        handlers::git::start_git_session,
        handlers::git::end_git_session,
        handlers::git::list_git_branches,
        handlers::git::file_changes_stream,
        // Code intelligence handlers
        handlers::code::register_repo,
        handlers::code::scan_repo,
        handlers::code::get_scan_status,
        handlers::code::list_repositories,
        handlers::code::get_repository,
        handlers::code::create_commit_delta,
        // Federation handlers
        handlers::federation::get_federation_status,
        handlers::federation::get_quarantine_status,
        handlers::federation::release_quarantine,
        // Domain adapter handlers
        domain_adapters::list_domain_adapters,
        domain_adapters::get_domain_adapter,
        domain_adapters::create_domain_adapter,
        domain_adapters::load_domain_adapter,
        domain_adapters::unload_domain_adapter,
        domain_adapters::test_domain_adapter,
        domain_adapters::get_domain_adapter_manifest,
        domain_adapters::execute_domain_adapter,
        domain_adapters::delete_domain_adapter,
        // Model status handlers
        handlers::get_base_model_status,
        // Audit logs handler
        handlers::query_audit_logs,
        handlers::plugins::enable_plugin,
        handlers::plugins::disable_plugin,
        handlers::plugins::plugin_status,
        handlers::plugins::list_plugins,
        handlers::get_uma_memory,
        handlers::hydrate_tenant_from_bundle,
        // Service handlers
        handlers::services::start_service,
        handlers::services::stop_service,
        handlers::services::restart_service,
        handlers::services::start_essential_services,
        handlers::services::stop_essential_services,
        handlers::services::get_service_logs,
        // Models handlers
        handlers::models::load_model,
        handlers::models::unload_model,
        handlers::models::get_model_status,
        handlers::models::validate_model,
        // Auth enhanced handlers
        handlers::auth_enhanced::refresh_token_handler,
        handlers::auth_enhanced::bootstrap_admin_handler,
        handlers::auth_enhanced::list_sessions_handler,
        handlers::auth_enhanced::revoke_session_handler,
        // Routing decision handlers (PRD-04)
        handlers::routing_decisions::get_routing_decisions,
        handlers::routing_decisions::get_routing_decision_by_id,
        handlers::routing_decisions::ingest_router_decision,
        // Trace handlers
        handlers::telemetry::search_traces,
        handlers::telemetry::get_trace,
        // Dataset handlers
        handlers::datasets::upload_dataset,
        handlers::datasets::initiate_chunked_upload,
        handlers::datasets::upload_chunk,
        handlers::datasets::complete_chunked_upload,
        handlers::datasets::get_upload_session_status,
        handlers::datasets::cancel_chunked_upload,
        handlers::datasets::list_datasets,
        handlers::datasets::get_dataset,
        handlers::datasets::get_dataset_files,
        handlers::datasets::get_dataset_statistics,
        handlers::datasets::validate_dataset,
        handlers::datasets::preview_dataset,
        handlers::datasets::delete_dataset,
        handlers::datasets::dataset_upload_progress,
        // Golden run handlers
        handlers::golden::list_golden_runs,
        handlers::golden::get_golden_run,
        handlers::golden::golden_compare,
        // Promotion workflow handlers
        handlers::promotion::request_promotion,
        handlers::promotion::get_promotion_status,
        handlers::promotion::approve_or_reject_promotion,
        handlers::promotion::rollback_promotion,
        handlers::promotion::get_gate_status,
        // Activity handlers
        handlers::activity::create_activity_event,
        handlers::activity::list_activity_events,
        handlers::activity::list_user_workspace_activity,
        // Workspace handlers
        handlers::workspaces::list_workspaces,
        handlers::workspaces::list_user_workspaces,
        handlers::workspaces::create_workspace,
        handlers::workspaces::get_workspace,
        handlers::workspaces::update_workspace,
        handlers::workspaces::delete_workspace,
        handlers::workspaces::list_workspace_members,
        handlers::workspaces::add_workspace_member,
        handlers::workspaces::update_workspace_member,
        handlers::workspaces::remove_workspace_member,
        handlers::workspaces::list_workspace_resources,
        handlers::workspaces::share_workspace_resource,
        handlers::workspaces::unshare_workspace_resource,
        // Notification handlers
        handlers::notifications::list_notifications,
        handlers::notifications::get_notification_summary,
        handlers::notifications::mark_notification_read,
        handlers::notifications::mark_all_notifications_read,
        // Dashboard handlers
        handlers::dashboard::get_dashboard_config,
        handlers::dashboard::update_dashboard_config,
        handlers::dashboard::reset_dashboard_config,
        // Tutorial handlers
        handlers::tutorials::list_tutorials,
        handlers::tutorials::mark_tutorial_completed,
        handlers::tutorials::unmark_tutorial_completed,
        handlers::tutorials::mark_tutorial_dismissed,
        handlers::tutorials::unmark_tutorial_dismissed,
    ),
    components(schemas(
        crate::types::ErrorResponse,
        crate::types::LoginRequest,
        crate::types::LoginResponse,
        crate::types::HealthResponse,
        crate::health::ComponentHealth,
        crate::health::ComponentStatus,
        crate::health::SystemHealthResponse,
        crate::types::TenantResponse,
        crate::types::CreateTenantRequest,
        crate::types::ProposePatchRequest,
        crate::types::ProposePatchResponse,
        crate::types::InferRequest,
        crate::types::InferResponse,
        handlers::streaming_infer::StreamingInferRequest,
        handlers::streaming_infer::StreamingChunk,
        handlers::streaming_infer::StreamingChoice,
        handlers::streaming_infer::Delta,
        crate::types::BatchInferRequest,
        crate::types::BatchInferResponse,
        crate::types::BatchInferItemRequest,
        crate::types::BatchInferItemResponse,
        crate::types::InferenceTrace,
        crate::types::RouterDecision,
        crate::types::AdapterResponse,
        crate::types::AdapterStats,
        crate::types::RegisterAdapterRequest,
        crate::types::AdapterActivationResponse,
        crate::types::RepositoryResponse,
        crate::types::RegisterRepositoryRequest,
        crate::types::TriggerScanRequest,
        crate::types::ScanStatusResponse,
        crate::types::QualityMetricsResponse,
        crate::types::AdapterMetricsResponse,
        crate::types::AdapterPerformance,
        crate::types::SystemMetricsResponse,
        crate::types::LoadAverageResponse,
        crate::types::CommitResponse,
        crate::types::CommitDiffResponse,
        crate::types::DiffStats,
        crate::types::RoutingDebugRequest,
        crate::types::RoutingDebugResponse,
        crate::types::FeatureVector,
        crate::types::AdapterScore,
        crate::types::MetaResponse,
        crate::types::RoutingDecisionsQuery,
        crate::types::RoutingDecision,
        crate::types::RoutingDecisionsResponse,
        crate::types::AuditsQuery,
        crate::types::AuditExtended,
        crate::types::AuditsResponse,
        crate::types::PromotionRecord,
        // Audit logs types
        crate::types::AuditLogsQuery,
        crate::types::AuditLogResponse,
        crate::types::AuditLogsResponse,
        // Adapter hot-swap and statistics types
        crate::types::AdapterSwapRequest,
        crate::types::AdapterSwapResponse,
        crate::types::AdapterStatsResponse,
        // Category policy types
        crate::types::CategoryPolicyRequest,
        crate::types::CategoryPolicyResponse,
        crate::types::CategoryPoliciesResponse,
        // Worker stop types
        crate::types::WorkerStopResponse,
        // Contacts and Streams types - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md
        crate::types::ContactResponse,
        crate::types::CreateContactRequest,
        crate::types::ContactsResponse,
        crate::types::ContactInteractionResponse,
        crate::types::ContactInteractionsResponse,
        crate::types::StreamQuery,
        crate::types::DiscoveryStreamQuery,
        // Training types
        crate::types::TrainingConfigRequest,
        crate::types::StartTrainingRequest,
        crate::types::TrainingJobResponse,
        crate::types::TrainingMetricsResponse,
        crate::types::TrainingTemplateResponse,
        // Domain adapter types
        crate::types::DomainAdapterResponse,
        crate::types::EpsilonStatsResponse,
        crate::types::CreateDomainAdapterRequest,
        crate::types::TestDomainAdapterRequest,
        crate::types::TestDomainAdapterResponse,
        crate::types::DomainAdapterManifestResponse,
        crate::types::LoadDomainAdapterRequest,
        crate::types::DomainAdapterExecutionResponse,
        // Model status types
        crate::types::BaseModelStatusResponse,
        // Git types
        handlers::git::GitBranchInfo,
        handlers::git::GitStatusResponse,
        handlers::git::StartGitSessionRequest,
        handlers::git::StartGitSessionResponse,
        handlers::git::EndGitSessionRequest,
        handlers::git::EndGitSessionResponse,
        handlers::git::SessionAction,
        handlers::git::FileChangeEvent,
        // Promotion types
        crate::handlers::promotion::PromoteRequest,
        crate::handlers::promotion::PromoteResponse,
        crate::handlers::promotion::PromotionStatusResponse,
        crate::handlers::promotion::GateStatus,
        crate::handlers::promotion::ApprovalRecord,
        crate::handlers::promotion::ApproveRequest,
        crate::handlers::promotion::ApproveResponse,
        crate::handlers::promotion::RollbackRequest,
        crate::handlers::promotion::RollbackResponse,
        // Federation types
        crate::handlers::federation::FederationStatusResponse,
        crate::handlers::federation::QuarantineStatusResponse,
        crate::handlers::federation::QuarantineDetails,
        // Chunked upload types
        handlers::datasets::InitiateChunkedUploadRequest,
        handlers::datasets::InitiateChunkedUploadResponse,
        handlers::datasets::UploadChunkQuery,
        handlers::datasets::UploadChunkResponse,
        handlers::datasets::CompleteChunkedUploadRequest,
        handlers::datasets::CompleteChunkedUploadResponse,
        handlers::datasets::UploadSessionStatusResponse,
        // Activity types
        handlers::activity::CreateActivityEventRequest,
        handlers::activity::ActivityEventResponse,
        // Service control types
        handlers::services::ServiceControlResponse,
        handlers::services::LogsQuery,
        // Models types
        handlers::models::ImportModelRequest,
        handlers::models::ImportModelResponse,
        handlers::models::ModelStatusResponse,
        handlers::models::ModelValidationResponse,
        handlers::models::ModelRuntimeHealthResponse,
        // Auth enhanced types
        handlers::auth_enhanced::BootstrapRequest,
        handlers::auth_enhanced::BootstrapResponse,
        handlers::auth_enhanced::RefreshResponse,
        handlers::auth_enhanced::LogoutResponse,
        handlers::auth_enhanced::SessionInfo,
        handlers::auth_enhanced::SessionsResponse,
        // Workspace types
        handlers::workspaces::WorkspaceResponse,
        handlers::workspaces::CreateWorkspaceRequest,
        handlers::workspaces::UpdateWorkspaceRequest,
        handlers::workspaces::AddWorkspaceMemberRequest,
        handlers::workspaces::UpdateWorkspaceMemberRequest,
        handlers::workspaces::ShareResourceRequest,
        // Notification types
        handlers::notifications::NotificationResponse,
        handlers::notifications::NotificationSummary,
        // Dashboard types
        adapteros_api_types::dashboard::DashboardWidgetConfig,
        adapteros_api_types::dashboard::GetDashboardConfigResponse,
        adapteros_api_types::dashboard::UpdateDashboardConfigRequest,
        adapteros_api_types::dashboard::UpdateDashboardConfigResponse,
        adapteros_api_types::dashboard::ResetDashboardConfigResponse,
        // Tutorial types
        handlers::tutorials::TutorialStep,
        handlers::tutorials::TutorialResponse,
        handlers::tutorials::TutorialStatusResponse,
        // Auth types
        crate::types::UserInfoResponse,
    )),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "tenants", description = "Tenant management"),
        (name = "nodes", description = "Node management"),
        (name = "models", description = "Model registry"),
        (name = "jobs", description = "Job management"),
        (name = "code", description = "Code intelligence operations"),
        (name = "adapters", description = "Adapter management"),
        (name = "repositories", description = "Repository management"),
        (name = "metrics", description = "System and quality metrics"),
        (name = "commits", description = "Commit inspection"),
        (name = "routing", description = "Routing debug and inspection"),
        (name = "contacts", description = "Contact discovery and management"),
        (name = "streams", description = "Real-time SSE event streams"),
        (name = "domain-adapters", description = "Domain adapter management"),
        (name = "git", description = "Git integration and session management"),
        (name = "federation", description = "Federation verification and quarantine management"),
        (name = "inference", description = "Model inference endpoints"),
        (name = "promotion", description = "Golden run promotion workflow"),
        (name = "activity", description = "Activity event tracking and feeds"),
        (name = "workspaces", description = "Workspace management and resource sharing"),
        (name = "notifications", description = "User notifications and alerts"),
        (name = "dashboard", description = "Dashboard configuration and widgets"),
        (name = "tutorials", description = "Tutorial management and progress tracking"),
    )
)]
pub struct ApiDoc;

pub fn build(state: AppState) -> Router {
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/healthz", get(handlers::health))
        .route("/healthz/all", get(crate::health::check_all_health))
        .route(
            "/healthz/{component}",
            get(crate::health::check_component_health),
        )
        .route("/readyz", get(handlers::ready))
        .route(
            "/v1/auth/login",
            post(handlers::auth_enhanced::login_handler),
        )
        .route(
            "/v1/auth/bootstrap",
            post(handlers::auth_enhanced::bootstrap_admin_handler),
        )
        .route(
            "/v1/auth/dev-bypass",
            post(handlers::auth_enhanced::dev_bypass_handler),
        )
        .route("/v1/meta", get(handlers::meta));

    // Metrics endpoint (custom auth, not JWT)
    let metrics_route = Router::new()
        .route("/v1/metrics", get(handlers::metrics_handler))
        .with_state(state.clone());

    // Protected routes (require auth)
    let protected_routes = Router::new()
        .route("/v1/auth/logout", post(auth::auth_logout))
        .route("/v1/auth/me", get(auth::auth_me))
        .route(
            "/v1/auth/refresh",
            post(handlers::auth_enhanced::refresh_token_handler),
        )
        .route(
            "/v1/auth/sessions",
            get(handlers::auth_enhanced::list_sessions_handler),
        )
        .route(
            "/v1/auth/sessions/{jti}",
            delete(handlers::auth_enhanced::revoke_session_handler),
        )
        .route(
            "/v1/tenants",
            get(handlers::list_tenants).post(handlers::create_tenant),
        )
        .route("/v1/tenants/{tenant_id}", put(handlers::update_tenant))
        .route(
            "/v1/tenants/{tenant_id}/pause",
            post(handlers::pause_tenant),
        )
        .route(
            "/v1/tenants/{tenant_id}/archive",
            post(handlers::archive_tenant),
        )
        .route(
            "/v1/tenants/{tenant_id}/policies",
            post(handlers::assign_tenant_policies),
        )
        .route(
            "/v1/tenants/{tenant_id}/adapters",
            post(handlers::assign_tenant_adapters),
        )
        .route(
            "/v1/tenants/{tenant_id}/usage",
            get(handlers::get_tenant_usage),
        )
        .route("/v1/nodes", get(handlers::list_nodes))
        .route("/v1/nodes/register", post(handlers::register_node))
        .route(
            "/v1/nodes/{node_id}/ping",
            post(handlers::test_node_connection),
        )
        .route(
            "/v1/nodes/{node_id}/offline",
            post(handlers::mark_node_offline),
        )
        .route(
            "/v1/nodes/{node_id}",
            axum::routing::delete(handlers::evict_node),
        )
        .route(
            "/v1/nodes/{node_id}/details",
            get(handlers::get_node_details),
        )
        // Service control routes
        .route(
            "/v1/services/{service_id}/start",
            post(handlers::services::start_service),
        )
        .route(
            "/v1/services/{service_id}/stop",
            post(handlers::services::stop_service),
        )
        .route(
            "/v1/services/{service_id}/restart",
            post(handlers::services::restart_service),
        )
        .route(
            "/v1/services/essential/start",
            post(handlers::services::start_essential_services),
        )
        .route(
            "/v1/services/essential/stop",
            post(handlers::services::stop_essential_services),
        )
        .route(
            "/v1/services/{service_id}/logs",
            get(handlers::services::get_service_logs),
        )
        .route("/v1/models/import", post(handlers::import_model))
        .route("/v1/models/status", get(handlers::get_base_model_status))
        .route(
            "/v1/models/{model_id}/load",
            post(handlers::models::load_model),
        )
        .route(
            "/v1/models/{model_id}/unload",
            post(handlers::models::unload_model),
        )
        .route(
            "/v1/models/{model_id}/status",
            get(handlers::models::get_model_status),
        )
        .route(
            "/v1/models/{model_id}/validate",
            get(handlers::models::validate_model),
        )
        .route("/v1/plans", get(handlers::list_plans))
        .route("/v1/plans/build", post(handlers::build_plan))
        .route(
            "/v1/plans/{plan_id}/details",
            get(handlers::get_plan_details),
        )
        .route("/v1/plans/{plan_id}/rebuild", post(handlers::rebuild_plan))
        .route("/v1/plans/compare", post(handlers::compare_plans))
        .route(
            "/v1/plans/{plan_id}/manifest",
            get(handlers::export_plan_manifest),
        )
        .route("/v1/cp/promote", post(handlers::cp_promote))
        .route(
            "/v1/cp/promotion-gates/{cpid}",
            get(handlers::promotion_gates),
        )
        .route("/v1/cp/rollback", post(handlers::cp_rollback))
        .route("/v1/cp/promote/dry-run", post(handlers::cp_dry_run_promote))
        .route("/v1/cp/promotions", get(handlers::get_promotion_history))
        .route("/v1/workers", get(handlers::list_workers))
        .route("/v1/workers/spawn", post(handlers::worker_spawn))
        .route(
            "/v1/workers/{worker_id}/logs",
            get(handlers::list_process_logs),
        )
        .route(
            "/v1/workers/{worker_id}/crashes",
            get(handlers::list_process_crashes),
        )
        .route(
            "/v1/workers/{worker_id}/debug",
            post(handlers::start_debug_session),
        )
        .route(
            "/v1/workers/{worker_id}/troubleshoot",
            post(handlers::run_troubleshooting_step),
        )
        // Worker stop route
        .route("/v1/workers/{worker_id}/stop", post(handlers::stop_worker))
        .route(
            "/v1/monitoring/rules",
            get(handlers::list_process_monitoring_rules),
        )
        .route(
            "/v1/monitoring/rules",
            post(handlers::create_process_monitoring_rule),
        )
        .route("/v1/monitoring/alerts", get(handlers::list_process_alerts))
        .route(
            "/v1/monitoring/alerts/{alert_id}/acknowledge",
            post(handlers::acknowledge_process_alert),
        )
        .route(
            "/v1/monitoring/anomalies",
            get(handlers::list_process_anomalies),
        )
        .route(
            "/v1/monitoring/anomalies/{anomaly_id}/status",
            post(handlers::update_process_anomaly_status),
        )
        .route(
            "/v1/monitoring/dashboards",
            get(handlers::list_process_monitoring_dashboards),
        )
        .route(
            "/v1/monitoring/dashboards",
            post(handlers::create_process_monitoring_dashboard),
        )
        .route(
            "/v1/monitoring/health-metrics",
            get(handlers::list_process_health_metrics),
        )
        .route(
            "/v1/monitoring/reports",
            get(handlers::list_process_monitoring_reports),
        )
        .route(
            "/v1/monitoring/reports",
            post(handlers::create_process_monitoring_report),
        )
        .route("/v1/jobs", get(handlers::list_jobs))
        .route("/v1/policies", get(handlers::list_policies))
        .route("/v1/policies/{cpid}", get(handlers::get_policy))
        .route("/v1/policies/validate", post(handlers::validate_policy))
        .route("/v1/policies/apply", post(handlers::apply_policy))
        .route("/v1/policies/{cpid}/sign", post(handlers::sign_policy))
        .route(
            "/v1/policies/compare",
            post(handlers::compare_policy_versions),
        )
        .route("/v1/policies/{cpid}/export", get(handlers::export_policy))
        .route(
            "/v1/telemetry/bundles",
            get(handlers::list_telemetry_bundles),
        )
        .route(
            "/v1/telemetry/bundles/{bundle_id}/export",
            get(handlers::export_telemetry_bundle),
        )
        .route(
            "/v1/telemetry/bundles/{bundle_id}/verify",
            post(handlers::verify_bundle_signature),
        )
        .route(
            "/v1/telemetry/bundles/purge",
            post(handlers::purge_old_bundles),
        )
        // Replay session routes
        .route(
            "/v1/replay/sessions",
            get(handlers::replay::list_replay_sessions),
        )
        .route(
            "/v1/replay/sessions",
            post(handlers::replay::create_replay_session),
        )
        .route(
            "/v1/replay/sessions/{id}",
            get(handlers::replay::get_replay_session),
        )
        .route(
            "/v1/replay/sessions/{id}/verify",
            post(handlers::replay::verify_replay_session),
        )
        .route("/v1/patch/propose", post(handlers::propose_patch))
        .route("/v1/infer", post(handlers::infer))
        .route("/v1/infer/stream", post(handlers::streaming_infer::streaming_infer))
        .route("/v1/infer/batch", post(handlers::batch::batch_infer))
        // Adapter routes
        .route("/v1/adapters", get(handlers::list_adapters))
        .route("/v1/adapters/{adapter_id}", get(handlers::get_adapter))
        .route("/v1/adapters/register", post(handlers::register_adapter))
        .route(
            "/v1/adapters/import",
            post(handlers::adapters::import_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}",
            axum::routing::delete(handlers::delete_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/load",
            post(handlers::load_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/unload",
            post(handlers::unload_adapter),
        )
        .route(
            "/v1/adapters/verify-gpu",
            get(handlers::verify_gpu_integrity),
        )
        .route(
            "/v1/adapters/{adapter_id}/activations",
            get(handlers::get_adapter_activations),
        )
        // PRD-07: Lifecycle promotion/demotion (distinct from tier-based promotion)
        .route(
            "/v1/adapters/{adapter_id}/lifecycle/promote",
            post(handlers::promote_adapter_lifecycle),
        )
        .route(
            "/v1/adapters/{adapter_id}/lifecycle/demote",
            post(handlers::demote_adapter_lifecycle),
        )
        // PRD-08: Lineage and detail views
        .route(
            "/v1/adapters/{adapter_id}/lineage",
            get(handlers::get_adapter_lineage),
        )
        .route(
            "/v1/adapters/{adapter_id}/detail",
            get(handlers::get_adapter_detail),
        )
        .route(
            "/v1/adapters/{adapter_id}/manifest",
            get(handlers::download_adapter_manifest),
        )
        .route(
            "/v1/adapters/directory/upsert",
            post(handlers::upsert_directory_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/health",
            get(handlers::get_adapter_health),
        )
        // Adapter pinning routes
        .route(
            "/v1/adapters/{adapter_id}/pin",
            get(handlers::get_pin_status)
                .post(handlers::pin_adapter)
                .delete(handlers::unpin_adapter),
        )
        // Tier-based state promotion (distinct from lifecycle promotion)
        .route(
            "/v1/adapters/{adapter_id}/state/promote",
            post(handlers::promote_adapter_state),
        )
        // Adapter hot-swap route
        .route("/v1/adapters/swap", post(handlers::adapters::swap_adapters))
        // Adapter statistics route
        .route(
            "/v1/adapters/{adapter_id}/stats",
            get(handlers::adapters::get_adapter_stats),
        )
        // Category policies routes
        .route(
            "/v1/adapters/category-policies",
            get(handlers::adapters::list_category_policies),
        )
        .route(
            "/v1/adapters/category-policies/{category}",
            get(handlers::adapters::get_category_policy)
                .put(handlers::adapters::update_category_policy),
        )
        // Semantic name validation routes
        .route(
            "/v1/adapters/validate-name",
            post(handlers::validate_adapter_name),
        )
        .route(
            "/v1/stacks/validate-name",
            post(handlers::validate_stack_name),
        )
        .route(
            "/v1/adapters/next-revision/{tenant}/{domain}/{purpose}",
            get(handlers::get_next_revision),
        )
        // Adapter stacks routes
        .route(
            "/v1/adapter-stacks",
            get(handlers::adapter_stacks::list_stacks).post(handlers::adapter_stacks::create_stack),
        )
        .route(
            "/v1/adapter-stacks/{id}",
            get(handlers::adapter_stacks::get_stack).delete(handlers::adapter_stacks::delete_stack),
        )
        .route(
            "/v1/adapter-stacks/{id}/activate",
            post(handlers::adapter_stacks::activate_stack),
        )
        .route(
            "/v1/adapter-stacks/deactivate",
            post(handlers::adapter_stacks::deactivate_stack),
        )
        // Domain adapter routes
        .route(
            "/v1/domain-adapters",
            get(domain_adapters::list_domain_adapters),
        )
        .route(
            "/v1/domain-adapters",
            post(domain_adapters::create_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}",
            get(domain_adapters::get_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}",
            delete(domain_adapters::delete_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}/load",
            post(domain_adapters::load_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}/unload",
            post(domain_adapters::unload_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}/test",
            post(domain_adapters::test_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}/manifest",
            get(domain_adapters::get_domain_adapter_manifest),
        )
        .route(
            "/v1/domain-adapters/{adapter_id}/execute",
            post(domain_adapters::execute_domain_adapter),
        )
        // Contacts routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
        .route(
            "/v1/contacts",
            get(handlers::list_contacts).post(handlers::create_contact),
        )
        .route(
            "/v1/contacts/{id}",
            get(handlers::get_contact).delete(handlers::delete_contact),
        )
        .route(
            "/v1/contacts/{id}/interactions",
            get(handlers::get_contact_interactions),
        )
        // SSE Streaming routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4
        .route("/v1/streams/training", get(handlers::training_stream))
        .route("/v1/streams/discovery", get(handlers::discovery_stream))
        .route("/v1/streams/contacts", get(handlers::contacts_stream))
        // Dataset routes
        .route(
            "/v1/datasets/upload",
            post(handlers::datasets::upload_dataset),
        )
        .route(
            "/v1/datasets/chunked-upload/initiate",
            post(handlers::datasets::initiate_chunked_upload),
        )
        // Chunked upload routes - upload individual chunks
        .route(
            "/v1/datasets/chunked-upload/{session_id}/chunk",
            post(handlers::datasets::upload_chunk),
        )
        // Chunked upload routes - complete upload and create dataset
        .route(
            "/v1/datasets/chunked-upload/{session_id}/complete",
            post(handlers::datasets::complete_chunked_upload),
        )
        // Chunked upload routes - get session status
        .route(
            "/v1/datasets/chunked-upload/{session_id}/status",
            get(handlers::datasets::get_upload_session_status),
        )
        // Chunked upload routes - cancel upload session
        .route(
            "/v1/datasets/chunked-upload/{session_id}",
            delete(handlers::datasets::cancel_chunked_upload),
        )
        .route("/v1/datasets", get(handlers::datasets::list_datasets))
        .route(
            "/v1/datasets/{dataset_id}",
            get(handlers::datasets::get_dataset),
        )
        .route(
            "/v1/datasets/{dataset_id}",
            delete(handlers::datasets::delete_dataset),
        )
        .route(
            "/v1/datasets/{dataset_id}/files",
            get(handlers::datasets::get_dataset_files),
        )
        .route(
            "/v1/datasets/{dataset_id}/statistics",
            get(handlers::datasets::get_dataset_statistics),
        )
        .route(
            "/v1/datasets/{dataset_id}/validate",
            post(handlers::datasets::validate_dataset),
        )
        .route(
            "/v1/datasets/{dataset_id}/preview",
            get(handlers::datasets::preview_dataset),
        )
        .route(
            "/v1/datasets/upload/progress",
            get(handlers::datasets::dataset_upload_progress),
        )
        // Code intelligence routes
        .route(
            "/v1/code/register-repo",
            post(handlers::code::register_repo),
        )
        .route("/v1/code/scan", post(handlers::code::scan_repo))
        .route(
            "/v1/code/scan/{job_id}",
            get(handlers::code::get_scan_status),
        )
        .route(
            "/v1/code/repositories",
            get(handlers::code::list_repositories),
        )
        .route(
            "/v1/code/repositories/{repo_id}",
            get(handlers::code::get_repository),
        )
        .route(
            "/v1/code/commit-delta",
            post(handlers::code::create_commit_delta),
        )
        // Repository routes (deprecated - use /v1/code/repositories instead)
        .route("/v1/repositories", get(handlers::list_repositories))
        // Metrics routes
        .route("/v1/metrics/quality", get(handlers::get_quality_metrics))
        .route("/v1/metrics/adapters", get(handlers::get_adapter_metrics))
        .route("/v1/metrics/system", get(handlers::get_system_metrics))
        .route("/v1/system/memory", get(handlers::get_uma_memory))
        // Commit routes
        .route("/v1/commits", get(handlers::list_commits))
        .route("/v1/commits/{sha}", get(handlers::get_commit))
        .route("/v1/commits/{sha}/diff", get(handlers::get_commit_diff))
        // Routing routes
        .route("/v1/routing/debug", post(handlers::debug_routing))
        .route("/v1/routing/history", get(handlers::get_routing_history))
        .route(
            "/v1/routing/decisions",
            get(handlers::routing_decisions::get_routing_decisions),
        )
        .route(
            "/v1/routing/decisions/{id}",
            get(handlers::routing_decisions::get_routing_decision_by_id),
        )
        .route(
            "/v1/telemetry/routing",
            post(handlers::routing_decisions::ingest_router_decision),
        )
        // Trace routes
        .route("/v1/traces/search", get(handlers::telemetry::search_traces))
        .route("/v1/traces/{trace_id}", get(handlers::telemetry::get_trace))
        // Logs routes
        .route("/v1/logs/query", get(handlers::telemetry::query_logs))
        .route("/v1/logs/stream", get(handlers::telemetry::stream_logs))
        // Metrics snapshot/series routes
        .route(
            "/v1/metrics/snapshot",
            get(handlers::telemetry::get_metrics_snapshot),
        )
        .route(
            "/v1/metrics/series",
            get(handlers::telemetry::get_metrics_series),
        )
        // Training routes
        .route("/v1/training/jobs", get(handlers::list_training_jobs))
        .route(
            "/v1/training/jobs/{job_id}",
            get(handlers::get_training_job),
        )
        .route("/v1/training/start", post(handlers::start_training))
        .route(
            "/v1/training/jobs/{job_id}/cancel",
            post(handlers::cancel_training),
        )
        .route(
            "/v1/training/sessions",
            post(handlers::create_training_session),
        )
        .route(
            "/v1/training/jobs/{job_id}/logs",
            get(handlers::get_training_logs),
        )
        .route(
            "/v1/training/jobs/{job_id}/metrics",
            get(handlers::get_training_metrics),
        )
        .route(
            "/v1/training/jobs/{job_id}/artifacts",
            get(handlers::get_training_artifacts),
        )
        .route(
            "/v1/training/templates",
            get(handlers::list_training_templates),
        )
        .route(
            "/v1/training/templates/{template_id}",
            get(handlers::get_training_template),
        )
        // Git integration routes
        .route("/v1/git/status", get(handlers::git::git_status))
        .route(
            "/v1/git/sessions/start",
            post(handlers::git::start_git_session),
        )
        .route(
            "/v1/git/sessions/{session_id}/end",
            post(handlers::git::end_git_session),
        )
        .route("/v1/git/branches", get(handlers::git::list_git_branches))
        .route(
            "/v1/streams/file-changes",
            get(handlers::git::file_changes_stream),
        )
        // Federation routes
        .route(
            "/v1/federation/status",
            get(handlers::federation::get_federation_status),
        )
        .route(
            "/v1/federation/quarantine",
            get(handlers::federation::get_quarantine_status),
        )
        .route(
            "/v1/federation/release-quarantine",
            post(handlers::federation::release_quarantine),
        )
        // Audit endpoints
        .route("/v1/audit/federation", get(handlers::get_federation_audit))
        .route("/v1/audit/compliance", get(handlers::get_compliance_audit))
        .route("/v1/audit/logs", get(handlers::query_audit_logs))
        // Agent D contract endpoints
        .route("/v1/audits", get(handlers::list_audits_extended))
        .route("/v1/promotions/{id}", get(handlers::get_promotion))
        // SSE stream endpoints
        .route("/v1/stream/metrics", get(handlers::system_metrics_stream))
        .route(
            "/v1/stream/telemetry",
            get(handlers::telemetry_events_stream),
        )
        .route("/v1/stream/adapters", get(handlers::adapter_state_stream))
        .route(
            "/v1/plugins/{name}/enable",
            post(handlers::plugins::enable_plugin),
        )
        .route(
            "/v1/plugins/{name}/disable",
            post(handlers::plugins::disable_plugin),
        )
        .route("/v1/plugins/{name}", get(handlers::plugins::plugin_status))
        .route("/v1/plugins", get(handlers::plugins::list_plugins))
        // Golden run promotion routes
        .route("/v1/golden/runs", get(handlers::golden::list_golden_runs))
        .route(
            "/v1/golden/runs/{name}",
            get(handlers::golden::get_golden_run),
        )
        .route("/v1/golden/compare", post(handlers::golden::golden_compare))
        .route(
            "/v1/golden/{run_id}/promote",
            post(handlers::promotion::request_promotion),
        )
        .route(
            "/v1/golden/{run_id}/promotion",
            get(handlers::promotion::get_promotion_status),
        )
        .route(
            "/v1/golden/{run_id}/approve",
            post(handlers::promotion::approve_or_reject_promotion),
        )
        .route(
            "/v1/golden/{run_id}/gates",
            get(handlers::promotion::get_gate_status),
        )
        .route(
            "/v1/golden/{stage}/rollback",
            post(handlers::promotion::rollback_promotion),
        )
        // Activity routes
        .route(
            "/v1/activity/events",
            get(handlers::activity::list_activity_events)
                .post(handlers::activity::create_activity_event),
        )
        .route(
            "/v1/activity/feed",
            get(handlers::activity::list_user_workspace_activity),
        )
        // Workspace routes
        .route(
            "/v1/workspaces",
            get(handlers::workspaces::list_workspaces).post(handlers::workspaces::create_workspace),
        )
        .route(
            "/v1/workspaces/me",
            get(handlers::workspaces::list_user_workspaces),
        )
        .route(
            "/v1/workspaces/{workspace_id}",
            get(handlers::workspaces::get_workspace)
                .put(handlers::workspaces::update_workspace)
                .delete(handlers::workspaces::delete_workspace),
        )
        .route(
            "/v1/workspaces/{workspace_id}/members",
            get(handlers::workspaces::list_workspace_members)
                .post(handlers::workspaces::add_workspace_member),
        )
        .route(
            "/v1/workspaces/{workspace_id}/members/{member_id}",
            put(handlers::workspaces::update_workspace_member)
                .delete(handlers::workspaces::remove_workspace_member),
        )
        .route(
            "/v1/workspaces/{workspace_id}/resources",
            get(handlers::workspaces::list_workspace_resources)
                .post(handlers::workspaces::share_workspace_resource),
        )
        .route(
            "/v1/workspaces/{workspace_id}/resources/{resource_id}",
            delete(handlers::workspaces::unshare_workspace_resource),
        )
        // Notification routes
        .route(
            "/v1/notifications",
            get(handlers::notifications::list_notifications),
        )
        .route(
            "/v1/notifications/summary",
            get(handlers::notifications::get_notification_summary),
        )
        .route(
            "/v1/notifications/{notification_id}/read",
            post(handlers::notifications::mark_notification_read),
        )
        .route(
            "/v1/notifications/read-all",
            post(handlers::notifications::mark_all_notifications_read),
        )
        // Dashboard configuration routes
        .route(
            "/v1/dashboard/config",
            get(handlers::dashboard::get_dashboard_config)
                .put(handlers::dashboard::update_dashboard_config),
        )
        .route(
            "/v1/dashboard/config/reset",
            post(handlers::dashboard::reset_dashboard_config),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Combine routes and apply security middleware layers
    // (layers are applied in reverse order - first layer applied processes last)
    Router::new()
        .merge(public_routes)
        .merge(metrics_route)
        .merge(protected_routes)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Apply layers (innermost to outermost):
        .layer(TraceLayer::new_for_http()) // Request tracing (innermost)
        .layer(cors_layer()) // CORS configuration
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limiting_middleware,
        )) // Rate limiting
        .layer(axum::middleware::from_fn(request_size_limit_middleware)) // Limit request sizes
        .layer(axum::middleware::from_fn(security_headers_middleware)) // Add security headers
        .layer(axum::middleware::from_fn(client_ip_middleware)) // Extract client IP (outermost)
        .with_state(state)
}
