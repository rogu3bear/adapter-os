use crate::caching;
use crate::handlers;
use crate::handlers::auth;
use crate::handlers::domain_adapters;
use crate::health::system_ready;
use crate::middleware::audit::audit_middleware;
use crate::middleware::context::context_middleware;
use crate::middleware::policy_enforcement::policy_enforcement_middleware;
use crate::middleware::{
    auth_middleware, client_ip_middleware, csrf_middleware, optional_auth_middleware,
    tenant_route_guard_middleware,
};
use crate::middleware_security::{
    cors_layer, drain_middleware, rate_limiting_middleware, request_size_limit_middleware,
    request_tracking_middleware, security_headers_middleware,
};
use crate::request_id;
use crate::state::AppState;
use crate::versioning;
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::ready,
        handlers::get_status,
        handlers::admin_lifecycle::request_shutdown,
        handlers::admin_lifecycle::request_maintenance,
        handlers::admin_lifecycle::safe_restart,
        crate::health::check_all_health,
        crate::health::check_component_health,
        handlers::auth::auth_login,
        handlers::auth::auth_me,
        handlers::auth_enhanced::logout_handler,
        handlers::auth_enhanced::mfa_status_handler,
        handlers::auth_enhanced::mfa_start_handler,
        handlers::auth_enhanced::mfa_verify_handler,
        handlers::auth_enhanced::mfa_disable_handler,
        handlers::auth_enhanced::get_auth_config_handler,
        handlers::code::propose_patch,
        handlers::infer,
        handlers::streaming_infer::streaming_infer,
        handlers::streaming_infer::streaming_infer_with_progress,
        handlers::batch::batch_infer,
        handlers::batch::create_batch_job,
        handlers::batch::get_batch_status,
        handlers::batch::get_batch_items,
        // Chat session handlers
        handlers::chat_sessions::create_chat_session,
        handlers::chat_sessions::list_chat_sessions,
        handlers::chat_sessions::get_chat_session,
        handlers::chat_sessions::delete_chat_session,
        handlers::chat_sessions::add_chat_message,
        handlers::chat_sessions::get_chat_messages,
        handlers::chat_sessions::get_session_summary,
        handlers::chat_sessions::update_session_collection,
        handlers::chat_sessions::get_message_evidence,
        handlers::chat_sessions::get_chat_provenance,
        // Execution policy handlers
        handlers::execution_policy::get_execution_policy,
        handlers::execution_policy::create_execution_policy,
        handlers::execution_policy::deactivate_execution_policy,
        handlers::execution_policy::get_execution_policy_history,
        handlers::list_adapters,
        handlers::list_adapter_repositories,
        handlers::list_adapter_versions,
        handlers::get_adapter,
        handlers::register_adapter,
        handlers::delete_adapter,
        handlers::load_adapter,
        handlers::unload_adapter,
        handlers::verify_gpu_integrity,
        handlers::get_adapter_activations,
        handlers::routing_decisions::get_adapter_usage,
        handlers::promote_adapter_state,
        handlers::list_repositories,
        handlers::get_quality_metrics,
        handlers::get_adapter_metrics,
        handlers::get_system_metrics,
        handlers::list_commits,
        handlers::get_commit,
        handlers::get_commit_diff,
        handlers::routing_decisions::debug_routing,
        handlers::routing_decisions::get_routing_history,
        handlers::routing_decisions::get_routing_decision_chain,
        // Contacts and Streams handlers - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md
        handlers::chat_sessions::list_contacts,
        handlers::chat_sessions::create_contact,
        handlers::chat_sessions::get_contact,
        handlers::chat_sessions::delete_contact,
        handlers::chat_sessions::get_contact_interactions,
        handlers::streaming::activity_stream,
        handlers::streaming::discovery_stream,
        handlers::streaming::contacts_stream,
        // Training handlers
        handlers::list_training_jobs,
        handlers::get_training_job,
        handlers::start_training,
        handlers::promote_version,
        handlers::cancel_training,
        handlers::retry_training,
        handlers::create_training_session,
        handlers::get_training_logs,
        handlers::get_training_metrics,
        handlers::list_training_templates,
        handlers::get_training_template,
        handlers::get_chat_bootstrap,
        handlers::create_chat_from_training_job,
        // Repository handlers (new /v1/repos API)
        handlers::repos::list_repos,
        handlers::repos::get_repo,
        handlers::repos::create_repo,
        handlers::repos::update_repo,
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
        handlers::models::get_base_model_status,
        // Audit logs handler
        handlers::admin::query_audit_logs,
        handlers::plugins::enable_plugin,
        handlers::plugins::disable_plugin,
        handlers::plugins::plugin_status,
        handlers::plugins::list_plugins,
        handlers::settings::get_settings,
        handlers::settings::update_settings,
        handlers::system_info::get_uma_memory,
        handlers::pilot_status::get_pilot_status,
        handlers::registry::get_registry_status,
        handlers::tenants::hydrate_tenant_from_bundle,
        // Service handlers
        handlers::services::start_service,
        handlers::services::stop_service,
        handlers::services::restart_service,
        handlers::services::start_essential_services,
        handlers::services::stop_essential_services,
        handlers::services::get_service_logs,
        // Models handlers
        handlers::models::list_models_with_stats,
        handlers::models::import_model,
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
        handlers::routing_decisions::get_session_router_view,
        handlers::routing_decisions::ingest_router_decision,
        handlers::diagnostics::get_determinism_status,
        handlers::diagnostics::get_quarantine_status,
        handlers::capacity::get_capacity,
        // Storage handlers
        handlers::storage::get_storage_mode,
        handlers::storage::get_storage_stats,
        handlers::kv_isolation::get_kv_isolation_health,
        handlers::kv_isolation::trigger_kv_isolation_scan,
        // Runtime handlers
        handlers::runtime::get_current_session,
        handlers::runtime::list_sessions,
        // Trace handlers
        handlers::telemetry::search_traces,
        handlers::telemetry::get_trace,
        // Recent activity handlers
        handlers::telemetry::get_recent_activity,
        handlers::telemetry::recent_activity_stream,
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
        handlers::datasets::update_dataset_safety,
        handlers::datasets::override_dataset_trust,
        handlers::datasets::preview_dataset,
        handlers::datasets::delete_dataset,
        handlers::datasets::dataset_upload_progress,
        // Document handlers
        handlers::documents::upload_document,
        handlers::documents::list_documents,
        handlers::documents::get_document,
        handlers::documents::delete_document,
        handlers::documents::list_document_chunks,
        handlers::documents::download_document,
        handlers::documents::process_document,
        handlers::documents::retry_document,
        handlers::documents::list_failed_documents,
        // Collection handlers
        handlers::collections::create_collection,
        handlers::collections::list_collections,
        handlers::collections::get_collection,
        handlers::collections::delete_collection,
        handlers::collections::add_document_to_collection,
        handlers::collections::remove_document_from_collection,
        // Evidence handlers (PRD-DATA-01 Phase 2)
        handlers::evidence::list_evidence,
        handlers::evidence::create_evidence,
        handlers::evidence::get_evidence,
        handlers::evidence::delete_evidence,
        handlers::evidence::get_dataset_evidence,
        handlers::evidence::get_adapter_evidence,
        // Journey visualization handlers
        handlers::journeys::get_journey,
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
        // Tenant default stack handlers
        handlers::get_default_stack,
        handlers::set_default_stack,
        handlers::clear_default_stack,
        handlers::tenants::revoke_tenant_tokens,
        // Tenant policy binding handlers
        handlers::list_tenant_policy_bindings,
        handlers::toggle_tenant_policy,
        handlers::query_policy_decisions,
        handlers::verify_policy_audit_chain,
        // Policy assignment handlers (PRD-RBAC-01)
        handlers::tenant_policies::assign_policy,
        handlers::tenant_policies::list_policy_assignments,
        handlers::tenant_policies::list_violations,
        // Adapter stack handlers
        handlers::adapter_stacks::list_stacks,
        handlers::adapter_stacks::create_stack,
        handlers::adapter_stacks::get_stack,
        handlers::adapter_stacks::delete_stack,
        handlers::adapter_stacks::get_stack_history,
        handlers::adapter_stacks::activate_stack,
        handlers::adapter_stacks::deactivate_stack,
        // PRD-INFRA-01: System, Nodes, Workers, Memory, Metrics handlers
        handlers::system_overview::get_system_overview,
        handlers::system_state::get_system_state,
        handlers::node_detail::get_node_detail,
        handlers::worker_detail::get_worker_detail,
        handlers::memory_detail::get_uma_memory_breakdown,
        handlers::memory_detail::get_adapter_memory_usage,
        handlers::metrics_time_series::get_metrics_time_series,
        handlers::metrics_time_series::get_metrics_snapshot,
        // Owner CLI handler
        handlers::owner_cli::run_owner_cli_command,
    ),
    components(schemas(
        crate::types::ErrorResponse,
        crate::types::ApiErrorBody,
        crate::types::LoginRequest,
        crate::types::LoginResponse,
        crate::types::HealthResponse,
        handlers::health::ReadyzResponse,
        handlers::health::ReadyzChecks,
        handlers::health::ReadyzCheck,
        crate::health::ComponentHealth,
        crate::health::ComponentStatus,
        crate::health::SystemHealthResponse,
        crate::types::TenantResponse,
        crate::types::CreateTenantRequest,
        crate::types::SetDefaultStackRequest,
        crate::types::DefaultStackResponse,
        crate::types::TokenRevocationResponse,
        handlers::adapter_stacks::StackResponse,
        handlers::adapter_stacks::CreateStackRequest,
        handlers::adapter_stacks::WorkflowType,
        handlers::adapter_stacks::LifecycleHistoryResponse,
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
        // Chat session types
        handlers::chat_sessions::CreateChatSessionRequest,
        handlers::chat_sessions::CreateChatSessionResponse,
        handlers::chat_sessions::AddChatMessageRequest,
        handlers::chat_sessions::ListSessionsQuery,
        handlers::chat_sessions::UpdateCollectionRequest,
        adapteros_db::ChatSession,
        adapteros_db::ChatMessage,
        adapteros_db::InferenceEvidence,
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
        handlers::routing_decisions::AdapterUsageResponse,
        handlers::routing_decisions::SessionRouterViewResponse,
        handlers::routing_decisions::SessionStep,
        handlers::routing_decisions::AdapterFired,
        handlers::diagnostics::DeterminismStatusResponse,
        handlers::diagnostics::QuarantineStatusResponse,
        handlers::diagnostics::QuarantinedAdapter,
        handlers::capacity::CapacityResponse,
        handlers::capacity::CapacityResponse,
        handlers::capacity::CapacityUsage,
        handlers::capacity::NodeHealth,
        crate::state::CapacityLimits,
        handlers::capacity::CapacityUsage,
        handlers::capacity::NodeHealth,
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
        // Policy assignment types (PRD-RBAC-01)
        crate::types::AssignPolicyRequest,
        crate::types::PolicyAssignmentResponse,
        crate::types::PolicyViolationResponse,
        // Tenant policy binding types (PRD-06)
        handlers::tenant_policies::TenantPolicyBindingResponse,
        handlers::tenant_policies::TogglePolicyRequest,
        handlers::tenant_policies::PolicyAuditDecision,
        handlers::tenant_policies::PolicyDecisionsQuery,
        handlers::tenant_policies::ChainVerificationResult,
        handlers::tenant_policies::BrokenLink,
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
        // Repository types (new /v1/repos API)
        handlers::repos::RepoSummaryResponse,
        handlers::repos::RepoDetailResponse,
        handlers::repos::CreateRepoRequest,
        handlers::repos::CreateRepoResponse,
        handlers::repos::UpdateRepoRequest,
        handlers::repos::RepoTimelineEventResponse,
        handlers::repos::RepoTrainingJobLinkResponse,
        handlers::repos::BranchSummary,
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
        adapteros_db::federation::QuarantineDetails,
        // Chunked upload types
        handlers::datasets::InitiateChunkedUploadRequest,
        handlers::datasets::InitiateChunkedUploadResponse,
        handlers::datasets::UploadChunkQuery,
        handlers::datasets::UploadChunkResponse,
        handlers::datasets::CompleteChunkedUploadRequest,
        handlers::datasets::CompleteChunkedUploadResponse,
        handlers::datasets::UpdateDatasetSafetyRequest,
        handlers::datasets::UpdateDatasetSafetyResponse,
        handlers::datasets::TrustOverrideRequest,
        handlers::datasets::TrustOverrideResponse,
        handlers::datasets::UploadSessionStatusResponse,
        // Evidence types (PRD-DATA-01 Phase 2)
        handlers::evidence::CreateEvidenceRequest,
        handlers::evidence::EvidenceResponse,
        handlers::evidence::ListEvidenceQuery,
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
        handlers::models::ModelListResponse,
        handlers::models::ModelWithStatsResponse,
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
        // Owner CLI types
        handlers::owner_cli::CliRunRequest,
        handlers::owner_cli::CliRunResponse,
        // System state types
        adapteros_api_types::system_state::SystemStateResponse,
        adapteros_api_types::system_state::SystemStateQuery,
        adapteros_api_types::system_state::StateOrigin,
        adapteros_api_types::system_state::NodeState,
        adapteros_api_types::system_state::ServiceState,
        adapteros_api_types::system_state::ServiceHealthStatus,
        adapteros_api_types::system_state::TenantState,
        adapteros_api_types::system_state::StackSummary,
        adapteros_api_types::system_state::AdapterSummary,
        adapteros_api_types::system_state::AdapterLifecycleState,
        adapteros_api_types::system_state::MemoryState,
        adapteros_api_types::system_state::MemoryPressureLevel,
        adapteros_api_types::system_state::AneMemoryState,
        adapteros_api_types::system_state::AdapterMemorySummary,
        // Storage types
        handlers::storage::StorageModeResponse,
        handlers::storage::StorageStatsResponse,
        handlers::storage::TableCounts,
        handlers::storage::KvCounts,
        // Runtime types
        handlers::runtime::RuntimeSessionResponse,
        handlers::runtime::ListSessionsParams,
        handlers::runtime::DriftSummaryResponse,
        handlers::runtime::DriftFieldResponse,
        handlers::runtime::RuntimePathsResponse,
    )),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "tenants", description = "Tenant management"),
        (name = "settings", description = "System settings management"),
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
        (name = "cli", description = "Owner CLI command execution"),
        (name = "storage", description = "Storage mode and statistics visibility"),
        (name = "runtime", description = "Runtime session and configuration tracking"),
        (name = "journeys", description = "Journey visualization and workflow tracking"),
    )
)]
pub struct ApiDoc;

pub fn build(state: AppState) -> Router {
    // Liveness/readiness endpoints must be cheap and never depend on DB/policy middleware.
    // These routes intentionally bypass the global middleware stack applied to the main API.
    let health_routes = Router::new()
        .route("/healthz", get(handlers::health))
        .route("/readyz", get(handlers::ready))
        .with_state(state.clone());

    // Public routes (no auth required)
    let mut public_routes = Router::new()
        .route("/healthz/all", get(crate::health::check_all_health))
        .route(
            "/healthz/{component}",
            get(crate::health::check_component_health),
        )
        .route("/system/ready", get(system_ready))
        .route(
            "/v1/auth/login",
            post(handlers::auth_enhanced::login_handler),
        )
        .route(
            "/v1/auth/bootstrap",
            post(handlers::auth_enhanced::bootstrap_admin_handler),
        )
        .route(
            "/v1/auth/config",
            get(handlers::auth_enhanced::get_auth_config_handler),
        )
        .route(
            "/v1/auth/health",
            get(handlers::auth_enhanced::auth_health_handler),
        )
        .route(
            "/v1/auth/refresh",
            post(handlers::auth_enhanced::refresh_token_handler),
        )
        .route("/v1/meta", get(handlers::meta))
        .route("/v1/status", get(handlers::get_status))
        .route(
            "/admin/lifecycle/request-shutdown",
            post(handlers::admin_lifecycle::request_shutdown),
        )
        .route(
            "/admin/lifecycle/request-maintenance",
            post(handlers::admin_lifecycle::request_maintenance),
        )
        .route(
            "/admin/lifecycle/safe-restart",
            post(handlers::admin_lifecycle::safe_restart),
        )
        .route(
            "/v1/version",
            get(|| async { axum::Json(versioning::get_version_info()) }),
        );

    if cfg!(debug_assertions) {
        public_routes =
            public_routes.route("/docs", get(|| async { axum::Json(ApiDoc::openapi()) }));
    }

    #[cfg(all(feature = "dev-bypass", debug_assertions))]
    {
        public_routes = public_routes
            .route(
                "/v1/auth/dev-bypass",
                post(handlers::auth_enhanced::dev_bypass_handler),
            )
            .route(
                "/v1/dev/bootstrap",
                post(handlers::auth_enhanced::dev_bootstrap_handler),
            );
    }

    let public_routes =
        public_routes
            .with_state(state.clone())
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                policy_enforcement_middleware,
            ));

    // Metrics endpoint (custom auth, not JWT)
    let metrics_route = Router::new()
        .route("/v1/metrics", get(handlers::metrics_handler))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            policy_enforcement_middleware,
        ));

    // Routes with optional authentication (work with or without auth)
    // These routes provide enhanced functionality when authenticated but still work anonymously
    let optional_auth_routes = Router::new()
        .route("/v1/models/status", get(handlers::models::get_base_model_status))
        .with_state(state.clone())
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    policy_enforcement_middleware,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    audit_middleware,
                )) // Automatic audit logging (needs RequestContext)
                .layer(middleware::from_fn(context_middleware)) // Consolidate request context after auth
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    optional_auth_middleware,
                )),
        );

    // Protected routes (require auth)
    let protected_routes = Router::new()
        .route(
            "/v1/auth/logout",
            post(handlers::auth_enhanced::logout_handler),
        )
        .route("/v1/auth/me", get(auth::auth_me))
        .route(
            "/v1/auth/mfa/status",
            get(handlers::auth_enhanced::mfa_status_handler),
        )
        .route(
            "/v1/auth/mfa/start",
            post(handlers::auth_enhanced::mfa_start_handler),
        )
        .route(
            "/v1/auth/mfa/verify",
            post(handlers::auth_enhanced::mfa_verify_handler),
        )
        .route(
            "/v1/auth/mfa/disable",
            post(handlers::auth_enhanced::mfa_disable_handler),
        )
        .route(
            "/v1/api-keys",
            get(handlers::api_keys::list_api_keys).post(handlers::api_keys::create_api_key),
        )
        .route(
            "/v1/api-keys/{id}",
            delete(handlers::api_keys::revoke_api_key),
        )
        // Admin routes
        .route("/v1/admin/users", get(handlers::admin::list_users))
        .route(
            "/v1/auth/sessions",
            get(handlers::auth_enhanced::list_sessions_handler),
        )
        .route(
            "/v1/auth/sessions/{jti}",
            delete(handlers::auth_enhanced::revoke_session_handler),
        )
        .route(
            "/v1/auth/tenants",
            get(handlers::auth_enhanced::list_user_tenants_handler),
        )
        .route(
            "/v1/auth/tenants/switch",
            post(handlers::auth_enhanced::switch_tenant_handler),
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
        .route(
            "/v1/tenants/{tenant_id}/default-stack",
            get(handlers::get_default_stack)
                .put(handlers::set_default_stack)
                .delete(handlers::clear_default_stack),
        )
        .route(
            "/v1/tenants/{tenant_id}/router/config",
            get(handlers::router_config::get_router_config),
        )
        .route(
            "/v1/tenants/{tenant_id}/policy-bindings",
            get(handlers::list_tenant_policy_bindings),
        )
        .route(
            "/v1/tenants/{tenant_id}/policy-bindings/{policy_pack_id}/toggle",
            post(handlers::toggle_tenant_policy),
        )
        .route(
            "/v1/tenants/{tenant_id}/revoke-all-tokens",
            post(handlers::tenants::revoke_tenant_tokens),
        )
        // Tenant execution policy routes
        .route(
            "/v1/tenants/{tenant_id}/execution-policy",
            get(handlers::execution_policy::get_execution_policy)
                .post(handlers::execution_policy::create_execution_policy),
        )
        .route(
            "/v1/tenants/{tenant_id}/execution-policy/{policy_id}",
            axum::routing::delete(handlers::execution_policy::deactivate_execution_policy),
        )
        .route(
            "/v1/tenants/{tenant_id}/execution-policy/history",
            get(handlers::execution_policy::get_execution_policy_history),
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
        .route("/v1/models", get(handlers::models::list_models_with_stats))
        .route("/v1/models/import", post(handlers::models::import_model))
        .route(
            "/v1/models/status/all",
            get(handlers::models::get_all_models_status),
        )
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
            "/v1/tenants/{tenant_id}/manifests/{manifest_hash}",
            get(handlers::worker_manifests::fetch_manifest_by_hash),
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
        // Worker fatal error channel (PRD-09 Phase 4)
        .route("/v1/workers/fatal", post(handlers::receive_worker_fatal))
        // Worker health & incidents (PRD-09)
        .route(
            "/v1/workers/{worker_id}/incidents",
            get(handlers::list_worker_incidents),
        )
        .route(
            "/v1/workers/health-summary",
            get(handlers::get_worker_health_summary),
        )
        // PRD-01: Worker Registration & Lifecycle
        .route(
            "/v1/workers/register",
            post(handlers::workers::register_worker),
        )
        .route(
            "/v1/workers/status",
            post(handlers::workers::notify_worker_status),
        )
        .route(
            "/v1/workers/{worker_id}/history",
            get(handlers::workers::get_worker_history),
        )
        .route(
            "/v1/workers/{worker_id}/detail",
            get(handlers::worker_detail::get_worker_detail),
        )
        .route(
            "/v1/monitoring/rules",
            get(handlers::list_process_monitoring_rules),
        )
        .route(
            "/v1/monitoring/rules",
            post(handlers::create_process_monitoring_rule),
        )
        .route(
            "/v1/monitoring/alerts",
            get(handlers::monitoring::list_alerts),
        )
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
        // Journey visualization routes
        .route(
            "/v1/journeys/{journey_type}/{id}",
            get(handlers::journeys::get_journey),
        )
        .route("/v1/policies", get(handlers::list_policies))
        .route("/v1/policies/{cpid}", get(handlers::get_policy))
        .route("/v1/policies/validate", post(handlers::validate_policy))
        .route("/v1/policies/apply", post(handlers::apply_policy))
        .route("/v1/policies/{cpid}/sign", post(handlers::sign_policy))
        .route(
            "/v1/policies/{cpid}/verify",
            get(handlers::verify_policy_signature),
        )
        .route(
            "/v1/policies/compare",
            post(handlers::compare_policy_versions),
        )
        .route("/v1/policies/{cpid}/export", get(handlers::export_policy))
        .route("/v1/policies/assign", post(handlers::tenant_policies::assign_policy))
        .route(
            "/v1/policies/assignments",
            get(handlers::tenant_policies::list_policy_assignments),
        )
        .route("/v1/policies/violations", get(handlers::tenant_policies::list_violations))
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
        .route(
            "/v1/replay/sessions/{id}/execute",
            post(handlers::replay::execute_replay_session),
        )
        // Deterministic replay inference routes (PRD-02)
        .route(
            "/v1/replay/check/{inference_id}",
            get(handlers::replay_inference::check_availability),
        )
        .route(
            "/v1/replay",
            post(handlers::replay_inference::execute_replay),
        )
        .route(
            "/v1/replay/history/{inference_id}",
            get(handlers::replay_inference::get_replay_history),
        )
        .route("/v1/patch/propose", post(handlers::code::propose_patch))
        // OpenAI-compatible shim (used by OpenCode and other OpenAI clients)
        .route(
            "/v1/chat/completions",
            post(handlers::openai_compat::chat_completions),
        )
        .route("/v1/infer", post(handlers::infer))
        .route(
            "/v1/infer/stream",
            post(handlers::streaming_infer::streaming_infer),
        )
        .route(
            "/v1/infer/stream/progress",
            post(handlers::streaming_infer::streaming_infer_with_progress),
        )
        .route("/v1/infer/batch", post(handlers::batch::batch_infer))
        // Async batch job routes
        .route("/v1/batches", post(handlers::batch::create_batch_job))
        .route(
            "/v1/batches/{batch_id}",
            get(handlers::batch::get_batch_status),
        )
        .route(
            "/v1/batches/{batch_id}/items",
            get(handlers::batch::get_batch_items),
        )
        // Chat session routes
        .route(
            "/v1/chat/sessions",
            post(handlers::chat_sessions::create_chat_session)
                .get(handlers::chat_sessions::list_chat_sessions),
        )
        .route(
            "/v1/chats/from_training_job",
            post(handlers::create_chat_from_training_job),
        )
        // Special paths MUST come before the {session_id} wildcard
        .route(
            "/v1/chat/sessions/archived",
            get(handlers::chat_sessions::list_archived_sessions),
        )
        .route(
            "/v1/chat/sessions/trash",
            get(handlers::chat_sessions::list_deleted_sessions),
        )
        .route(
            "/v1/chat/sessions/search",
            get(handlers::chat_sessions::search_chat_sessions),
        )
        .route(
            "/v1/chat/sessions/shared-with-me",
            get(handlers::chat_sessions::get_sessions_shared_with_me),
        )
        // Wildcard route after special paths
        .route(
            "/v1/chat/sessions/{session_id}",
            get(handlers::chat_sessions::get_chat_session)
                .put(handlers::chat_sessions::update_chat_session)
                .delete(handlers::chat_sessions::delete_chat_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/messages",
            post(handlers::chat_sessions::add_chat_message)
                .get(handlers::chat_sessions::get_chat_messages),
        )
        .route(
            "/v1/chat/sessions/{session_id}/summary",
            get(handlers::chat_sessions::get_session_summary),
        )
        .route(
            "/v1/chat/sessions/{session_id}/collection",
            put(handlers::chat_sessions::update_session_collection),
        )
        .route(
            "/v1/chat/messages/{message_id}/evidence",
            get(handlers::chat_sessions::get_message_evidence),
        )
        .route(
            "/v1/chat/sessions/{session_id}/provenance",
            get(handlers::chat_sessions::get_chat_provenance),
        )
        // Chat tags routes
        .route(
            "/v1/chat/tags",
            get(handlers::chat_sessions::list_chat_tags)
                .post(handlers::chat_sessions::create_chat_tag),
        )
        .route(
            "/v1/chat/tags/{tag_id}",
            put(handlers::chat_sessions::update_chat_tag)
                .delete(handlers::chat_sessions::delete_chat_tag),
        )
        // Chat categories routes
        .route(
            "/v1/chat/categories",
            get(handlers::chat_sessions::list_chat_categories)
                .post(handlers::chat_sessions::create_chat_category),
        )
        .route(
            "/v1/chat/categories/{category_id}",
            put(handlers::chat_sessions::update_chat_category)
                .delete(handlers::chat_sessions::delete_chat_category),
        )
        // Chat session tags
        .route(
            "/v1/chat/sessions/{session_id}/tags",
            get(handlers::chat_sessions::get_session_tags)
                .post(handlers::chat_sessions::assign_tags_to_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/tags/{tag_id}",
            axum::routing::delete(handlers::chat_sessions::remove_tag_from_session),
        )
        // Chat session category
        .route(
            "/v1/chat/sessions/{session_id}/category",
            put(handlers::chat_sessions::set_session_category),
        )
        // Chat session archive/restore
        .route(
            "/v1/chat/sessions/{session_id}/archive",
            post(handlers::chat_sessions::archive_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/restore",
            post(handlers::chat_sessions::restore_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/permanent",
            axum::routing::delete(handlers::chat_sessions::hard_delete_session),
        )
        // Chat session shares
        .route(
            "/v1/chat/sessions/{session_id}/shares",
            get(handlers::chat_sessions::get_session_shares)
                .post(handlers::chat_sessions::share_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/shares/{share_id}",
            axum::routing::delete(handlers::chat_sessions::revoke_session_share),
        )
        // Owner CLI and Chat routes (admin only)
        .route(
            "/v1/cli/owner-run",
            post(handlers::owner_cli::run_owner_cli_command),
        )
        .route(
            "/v1/chat/owner-system",
            post(handlers::owner_chat::handle_owner_chat),
        )
        // Adapter routes
        .route("/v1/adapters", get(handlers::adapters::list_adapters))
        .route("/v1/adapters/{adapter_id}", get(handlers::adapters::get_adapter))
        .route("/v1/adapters/register", post(handlers::adapters_lifecycle::register_adapter))
        .route(
            "/v1/adapters/import",
            post(handlers::adapters::import_adapter),
        )
        .route(
            "/v1/adapter-repositories",
            get(handlers::adapters::list_adapter_repositories).post(handlers::create_adapter_repository),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}",
            get(handlers::adapters::get_adapter_repository),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/policy",
            get(handlers::adapters::get_adapter_repository_policy)
                .put(handlers::upsert_adapter_repository_policy),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/archive",
            post(handlers::archive_adapter_repository),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/versions",
            get(handlers::adapters::list_adapter_versions),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/versions/rollback",
            post(handlers::rollback_adapter_version_handler),
        )
        .route(
            "/v1/adapter-repositories/{repo_id}/resolve-version",
            post(handlers::resolve_adapter_version_handler),
        )
        .route(
            "/v1/adapter-versions/draft",
            post(handlers::create_draft_version),
        )
        .route(
            "/v1/adapter-versions/{version_id}",
            get(handlers::adapters::get_adapter_version),
        )
        .route(
            "/v1/adapter-versions/{version_id}/promote",
            post(handlers::promote_adapter_version_handler),
        )
        .route(
            "/v1/adapter-versions/{version_id}/tag",
            post(handlers::tag_adapter_version_handler),
        )
        .route(
            "/v1/adapters/{adapter_id}",
            axum::routing::delete(handlers::adapters_lifecycle::delete_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/load",
            post(handlers::adapters_lifecycle::load_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/unload",
            post(handlers::adapters_lifecycle::unload_adapter),
        )
        .route(
            "/v1/adapters/verify-gpu",
            get(handlers::adapters::verify_gpu_integrity),
        )
        .route(
            "/v1/adapters/{adapter_id}/activations",
            get(handlers::adapters::get_adapter_activations),
        )
        .route(
            "/v1/adapters/{adapter_id}/usage",
            get(handlers::routing_decisions::get_adapter_usage),
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
            get(handlers::adapters::get_adapter_lineage),
        )
        .route(
            "/v1/adapters/{adapter_id}/detail",
            get(handlers::adapters::get_adapter_detail),
        )
        .route(
            "/v1/adapters/{adapter_id}/manifest",
            get(handlers::download_adapter_manifest),
        )
        .route(
            "/v1/adapters/{adapter_id}/training-snapshot",
            get(handlers::adapters::get_adapter_training_snapshot),
        )
        .route(
            "/v1/adapters/{adapter_id}/training-export",
            get(handlers::adapters::export_training_provenance),
        )
        // PRD-ART-01: Adapter export as .aos file
        .route(
            "/v1/adapters/{adapter_id}/export",
            get(handlers::adapters::export_adapter),
        )
        .route(
            "/v1/adapters/directory/upsert",
            post(handlers::upsert_directory_adapter),
        )
        .route(
            "/v1/adapters/{adapter_id}/health",
            get(handlers::adapters::get_adapter_health),
        )
        // Adapter pinning routes
        .route(
            "/v1/adapters/{adapter_id}/pin",
            get(handlers::get_pin_status)
                .post(handlers::pin_adapter)
                .delete(handlers::unpin_adapter),
        )
        // Adapter archive routes
        .route(
            "/v1/adapters/{adapter_id}/archive",
            get(handlers::adapters::get_archive_status)
                .post(handlers::adapters::archive_adapter)
                .delete(handlers::adapters::unarchive_adapter),
        )
        // Tier-based state promotion (distinct from lifecycle promotion)
        .route(
            "/v1/adapters/{adapter_id}/state/promote",
            post(handlers::adapters::promote_adapter_state),
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
            get(handlers::adapter_stacks::list_stacks),
        )
        .route(
            "/v1/adapter-stacks",
            post(handlers::adapter_stacks::create_stack),
        )
        .route(
            "/v1/adapter-stacks/{id}",
            get(handlers::adapter_stacks::get_stack).delete(handlers::adapter_stacks::delete_stack),
        )
        .route(
            "/v1/adapter-stacks/{id}/history",
            get(handlers::adapter_stacks::get_stack_history),
        )
        .route(
            "/v1/adapter-stacks/{id}/policies",
            get(handlers::adapter_stacks::get_stack_policies),
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
            get(handlers::chat_sessions::list_contacts).post(handlers::chat_sessions::create_contact),
        )
        .route(
            "/v1/contacts/{id}",
            get(handlers::chat_sessions::get_contact).delete(handlers::chat_sessions::delete_contact),
        )
        .route(
            "/v1/contacts/{id}/interactions",
            get(handlers::chat_sessions::get_contact_interactions),
        )
        // SSE Streaming routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4
        .route("/v1/streams/training", get(handlers::streaming::activity_stream))
        .route("/v1/streams/discovery", get(handlers::streaming::discovery_stream))
        .route("/v1/streams/contacts", get(handlers::streaming::contacts_stream))
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
            "/v1/datasets/{dataset_id}/versions",
            get(handlers::datasets::list_dataset_versions)
                .post(handlers::datasets::create_dataset_version),
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
            "/v1/datasets/{dataset_id}/trust_override",
            post(handlers::datasets::apply_dataset_trust_override),
        )
        .route(
            "/v1/datasets/{dataset_id}/preview",
            get(handlers::datasets::preview_dataset),
        )
        .route(
            "/v1/datasets/upload/progress",
            get(handlers::datasets::dataset_upload_progress),
        )
        // Create dataset from documents (doc→dataset→adapter flow)
        .route(
            "/v1/datasets/from-documents",
            post(handlers::datasets::create_dataset_from_documents),
        )
        .route(
            "/v1/training/datasets/from-upload",
            post(handlers::training_datasets::create_training_dataset_from_upload),
        )
        .route(
            "/v1/training/dataset_versions/{dataset_version_id}/manifest",
            get(handlers::training_datasets::get_training_dataset_manifest),
        )
        .route(
            "/v1/training/dataset_versions/{dataset_version_id}/rows",
            get(handlers::training_datasets::stream_training_dataset_rows),
        )
        // Document routes
        .route(
            "/v1/documents/upload",
            post(handlers::documents::upload_document),
        )
        .route("/v1/documents", get(handlers::documents::list_documents))
        .route("/v1/documents/{id}", get(handlers::documents::get_document))
        .route(
            "/v1/documents/{id}",
            delete(handlers::documents::delete_document),
        )
        .route(
            "/v1/documents/{id}/chunks",
            get(handlers::documents::list_document_chunks),
        )
        .route(
            "/v1/documents/{id}/download",
            get(handlers::documents::download_document),
        )
        .route(
            "/v1/documents/{id}/process",
            post(handlers::documents::process_document),
        )
        .route(
            "/v1/documents/{id}/retry",
            post(handlers::documents::retry_document),
        )
        .route(
            "/v1/documents/failed",
            get(handlers::documents::list_failed_documents),
        )
        // Collection routes
        .route(
            "/v1/collections",
            post(handlers::collections::create_collection),
        )
        .route(
            "/v1/collections",
            get(handlers::collections::list_collections),
        )
        .route(
            "/v1/collections/{id}",
            get(handlers::collections::get_collection),
        )
        .route(
            "/v1/collections/{id}",
            delete(handlers::collections::delete_collection),
        )
        .route(
            "/v1/collections/{id}/documents",
            post(handlers::collections::add_document_to_collection),
        )
        .route(
            "/v1/collections/{id}/documents/{doc_id}",
            delete(handlers::collections::remove_document_from_collection),
        )
        // Evidence routes (PRD-DATA-01 Phase 2)
        .route(
            "/v1/evidence",
            get(handlers::evidence::list_evidence).post(handlers::evidence::create_evidence),
        )
        .route(
            "/v1/evidence/{id}",
            get(handlers::evidence::get_evidence).delete(handlers::evidence::delete_evidence),
        )
        .route(
            "/v1/datasets/{dataset_id}/evidence",
            get(handlers::evidence::get_dataset_evidence),
        )
        .route(
            "/v1/adapters/{adapter_id}/evidence",
            get(handlers::evidence::get_adapter_evidence),
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
        .route("/v1/repositories", get(handlers::adapters::list_repositories))
        // System overview routes
        .route(
            "/v1/system/overview",
            get(handlers::system_overview::get_system_overview),
        )
        .route(
            "/v1/system/pilot-status",
            get(handlers::pilot_status::get_pilot_status),
        )
        // System state (ground truth) route
        .route(
            "/v1/system/state",
            get(handlers::system_state::get_system_state),
        )
        // Metrics routes
        .route("/v1/metrics/quality", get(handlers::adapters::get_quality_metrics))
        .route("/v1/metrics/adapters", get(handlers::adapters::get_adapter_metrics))
        .route("/v1/metrics/system", get(handlers::adapters::get_system_metrics))
        .route(
            "/v1/metrics/time-series",
            get(handlers::metrics_time_series::get_metrics_time_series),
        )
        .route(
            "/v1/metrics/current",
            get(handlers::metrics_time_series::get_metrics_snapshot),
        )
        // Memory routes
        .route("/v1/system/memory", get(handlers::system_info::get_uma_memory))
        // Registry status route
        .route(
            "/v1/registry/status",
            get(handlers::registry::get_registry_status),
        )
        .route(
            "/v1/memory/usage",
            get(handlers::memory_detail::get_combined_memory_usage),
        )
        .route(
            "/v1/debug/coreml_verification_status",
            get(handlers::coreml_verification_status),
        )
        .route(
            "/v1/memory/uma-breakdown",
            get(handlers::memory_detail::get_uma_memory_breakdown),
        )
        .route(
            "/v1/memory/adapters",
            get(handlers::memory_detail::get_adapter_memory_usage),
        )
        // Commit routes
        .route("/v1/commits", get(handlers::adapters::list_commits))
        .route("/v1/commits/{sha}", get(handlers::adapters::get_commit))
        .route("/v1/commits/{sha}/diff", get(handlers::adapters::get_commit_diff))
        // Routing routes
        .route("/v1/routing/debug", post(handlers::routing_decisions::debug_routing))
        .route("/v1/routing/history", get(handlers::routing_decisions::get_routing_history))
        .route(
            "/v1/routing/decisions",
            get(handlers::routing_decisions::get_routing_decisions),
        )
        .route(
            "/v1/routing/decisions/{id}",
            get(handlers::routing_decisions::get_routing_decision_by_id),
        )
        .route(
            "/v1/routing/chain",
            get(handlers::routing_decisions::get_routing_decision_chain),
        )
        .route(
            "/v1/routing/sessions/{request_id}",
            get(handlers::routing_decisions::get_session_router_view),
        )
        .route(
            "/v1/telemetry/routing",
            post(handlers::routing_decisions::ingest_router_decision),
        )
        // Diagnostics routes (PRD G2)
        .route(
            "/v1/diagnostics/determinism-status",
            get(handlers::diagnostics::get_determinism_status),
        )
        .route(
            "/v1/diagnostics/quarantine-status",
            get(handlers::diagnostics::get_quarantine_status),
        )
        // Trace routes
        .route("/v1/traces/search", get(handlers::telemetry::search_traces))
        .route("/v1/traces/{trace_id}", get(handlers::telemetry::get_trace))
        // Logs routes
        .route("/v1/logs/query", get(handlers::telemetry::query_logs))
        .route("/v1/logs/stream", get(handlers::telemetry::stream_logs))
        // Recent activity events routes
        .route(
            "/v1/telemetry/events/recent",
            get(handlers::telemetry::get_recent_activity),
        )
        .route(
            "/v1/telemetry/events/recent/stream",
            get(handlers::telemetry::recent_activity_stream),
        )
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
            "/v1/training/repos/{repo_id}/versions/{version_id}/promote",
            post(handlers::promote_version),
        )
        .route(
            "/v1/training/jobs/{job_id}/cancel",
            post(handlers::cancel_training),
        )
        .route(
            "/v1/training/jobs/{job_id}/retry",
            post(handlers::retry_training),
        )
        .route(
            "/v1/training/jobs/{job_id}/export/coreml",
            post(handlers::export_coreml_training_job),
        )
        .route(
            "/v1/training/sessions",
            post(handlers::training::create_training_session),
        )
        .route(
            "/v1/training/jobs/{job_id}/logs",
            get(handlers::training::get_training_logs),
        )
        .route(
            "/v1/training/jobs/{job_id}/metrics",
            get(handlers::training::get_training_metrics),
        )
        .route(
            "/v1/training/jobs/{job_id}/artifacts",
            get(handlers::get_training_artifacts),
        )
        .route(
            "/v1/training/jobs/{job_id}/chat_bootstrap",
            get(handlers::get_chat_bootstrap),
        )
        .route(
            "/v1/training/templates",
            get(handlers::training::list_training_templates),
        )
        .route(
            "/v1/training/templates/{template_id}",
            get(handlers::training::get_training_template),
        )
        // Repository routes (new /v1/repos API)
        .route(
            "/v1/repos",
            get(handlers::repos::list_repos).post(handlers::repos::create_repo),
        )
        .route(
            "/v1/repos/{repo_id}",
            get(handlers::repos::get_repo).patch(handlers::repos::update_repo),
        )
        .route(
            "/v1/repos/{repo_id}/versions",
            get(handlers::repos::list_versions),
        )
        .route(
            "/v1/repos/{repo_id}/versions/{version_id}",
            get(handlers::repos::get_version),
        )
        .route(
            "/v1/repos/{repo_id}/versions/{version_id}/promote",
            post(handlers::repos::promote_version),
        )
        .route(
            "/v1/repos/{repo_id}/versions/{version_id}/tag",
            post(handlers::repos::tag_version),
        )
        .route(
            "/v1/repos/{repo_id}/versions/{version_id}/train",
            post(handlers::repos::start_training),
        )
        .route(
            "/v1/repos/{repo_id}/rollback/{branch}",
            post(handlers::repos::rollback_version),
        )
        .route(
            "/v1/repos/{repo_id}/timeline",
            get(handlers::repos::get_timeline),
        )
        .route(
            "/v1/repos/{repo_id}/training-jobs",
            get(handlers::repos::list_training_jobs),
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
        // Git repository management routes
        .route(
            "/v1/git/repositories",
            post(handlers::git_repository::register_git_repository),
        )
        .route(
            "/v1/git/repositories/{repo_id}/analysis",
            get(handlers::git_repository::get_repository_analysis),
        )
        .route(
            "/v1/git/repositories/{repo_id}/train",
            post(handlers::git_repository::train_repository_adapter),
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
        .route("/v1/audit/logs", get(handlers::admin::query_audit_logs))
        .route(
            "/v1/audit/policy-decisions",
            get(handlers::query_policy_decisions),
        )
        .route(
            "/v1/audit/policy-decisions/verify-chain",
            get(handlers::verify_policy_audit_chain),
        )
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
            "/v1/stream/boot-progress",
            get(handlers::boot_progress::boot_progress_stream),
        )
        .route(
            "/v1/stream/stack-policies/{id}",
            get(handlers::stack_policy_stream),
        )
        .route(
            "/v1/stream/notifications",
            get(handlers::streaming::notifications_stream)
                .head(handlers::streaming::sse_preflight_check),
        )
        .route(
            "/v1/stream/messages/{workspace_id}",
            get(handlers::streaming::messages_stream),
        )
        .route(
            "/v1/stream/activity/{workspace_id}",
            get(handlers::streaming::activity_stream),
        )
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
        // Orchestration (single-node stub)
        .route(
            "/v1/orchestration/analyze",
            post(handlers::orchestration::analyze_orchestration_prompt),
        )
        .route(
            "/v1/orchestration/metrics",
            get(handlers::orchestration::get_orchestration_metrics),
        )
        .route(
            "/v1/orchestration/config",
            get(handlers::orchestration::get_orchestration_config)
                .put(handlers::orchestration::update_orchestration_config),
        )
        // Settings routes
        .route(
            "/v1/settings",
            get(handlers::settings::get_settings).put(handlers::settings::update_settings),
        )
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
        // Tutorial routes
        .route("/v1/tutorials", get(handlers::tutorials::list_tutorials))
        .route(
            "/v1/tutorials/{tutorial_id}/complete",
            post(handlers::tutorials::mark_tutorial_completed)
                .delete(handlers::tutorials::unmark_tutorial_completed),
        )
        .route(
            "/v1/tutorials/{tutorial_id}/dismiss",
            post(handlers::tutorials::mark_tutorial_dismissed)
                .delete(handlers::tutorials::unmark_tutorial_dismissed),
        )
        // Storage visibility routes (admin only)
        .route("/v1/storage/mode", get(handlers::storage::get_storage_mode))
        .route(
            "/v1/storage/stats",
            get(handlers::storage::get_storage_stats),
        )
        .route(
            "/v1/storage/tenant-usage",
            get(handlers::storage::get_tenant_storage_usage),
        )
        .route(
            "/v1/storage/kv-isolation/health",
            get(handlers::kv_isolation::get_kv_isolation_health),
        )
        .route(
            "/v1/storage/kv-isolation/scan",
            post(handlers::kv_isolation::trigger_kv_isolation_scan),
        )
        // Runtime session routes
        .route(
            "/v1/runtime/session",
            get(handlers::runtime::get_current_session),
        )
        .route(
            "/v1/runtime/sessions",
            get(handlers::runtime::list_sessions),
        )
        .layer(
            ServiceBuilder::new()
                // Middleware execution order (outermost -> innermost):
                // auth -> tenant guard -> CSRF -> context -> policy -> audit.
                // This ensures identity is established before tenant/CSRF checks,
                // context is populated for downstream hooks, and audit runs with
                // the final request context.
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                ))
                .layer(middleware::from_fn(tenant_route_guard_middleware)) // Enforce tenant isolation for /tenants/{id}
                .layer(middleware::from_fn(csrf_middleware)) // CSRF double-submit for cookie auth
                .layer(middleware::from_fn(context_middleware)) // Consolidate request context after auth
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    policy_enforcement_middleware,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    audit_middleware,
                )), // Automatic audit logging (needs RequestContext)
        );

    // Testkit routes (E2E testing endpoints)
    let mut app = Router::new()
        .merge(public_routes)
        .merge(metrics_route)
        .merge(optional_auth_routes)
        .merge(protected_routes);

    // Add testkit routes if E2E_MODE is enabled
    if handlers::testkit::e2e_enabled() {
        app = app.merge(handlers::testkit::register_routes().with_state(state.clone()));
    }

    // Combine routes and apply security middleware layers
    // (layers are applied in reverse order - first layer applied processes last)
    let app = app
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Apply layers (innermost to outermost):
        .layer(TraceLayer::new_for_http()) // Request tracing (innermost)
        .layer(CompressionLayer::new()) // Response compression (gzip, br, deflate)
        .layer(cors_layer()) // CORS configuration
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limiting_middleware,
        )) // Rate limiting
        .layer(axum::middleware::from_fn(request_size_limit_middleware)) // Limit request sizes
        .layer(axum::middleware::from_fn(security_headers_middleware)) // Add security headers
        .layer(axum::middleware::from_fn(caching::caching_middleware)) // HTTP caching
        .layer(axum::middleware::from_fn(versioning::versioning_middleware)) // API versioning
        .layer(axum::middleware::from_fn(request_id::request_id_middleware)) // Request ID tracking
        .layer(axum::middleware::from_fn(client_ip_middleware)) // Extract client IP
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            request_tracking_middleware,
        )) // Track in-flight requests for graceful shutdown
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::lifecycle_gate,
        )) // Reject during maintenance/drain
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            drain_middleware,
        )) // Reject new requests during drain
        .layer(axum::middleware::from_fn(
            crate::middleware::observability_middleware,
        )) // Logging + error envelope
        .layer(axum::middleware::from_fn(request_id::request_id_middleware)) // Request ID tracking (outermost)
        .with_state(state.clone());

    Router::new()
        .merge(health_routes)
        .fallback_service(app)
        .with_state(state)
}
