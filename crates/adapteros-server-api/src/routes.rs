use crate::handlers;
use crate::handlers::domain_adapters;
use crate::middleware::{
    auth_middleware, dual_auth_middleware, metrics_auth_middleware, user_friendly_error_middleware,
};
use crate::rate_limit::per_tenant_rate_limit_middleware;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use axum::{Extension, Json};
// Note: Rate limiting disabled - consider using tower-governor for proper rate limiting
use crate::handlers::replay::replay_from_bundle;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::ready,
        handlers::get_status,
        handlers::auth_login,
        handlers::auth_refresh,
        handlers::auth_list_sessions,
        handlers::auth_revoke_session,
        handlers::auth_logout_all,
        handlers::auth_rotate_token,
        handlers::auth_token_metadata,
        handlers::auth_update_profile,
        handlers::auth_get_config,
        handlers::auth_update_config,
        handlers::propose_patch,
        handlers::infer,
        handlers::batch::batch_infer,
        handlers::list_adapters,
        handlers::get_adapter,
        handlers::register_adapter,
        handlers::delete_adapter,
        // handlers::load_adapter,  // Temporarily removed
        handlers::unload_adapter,
        // handlers::hot_swap_adapter,  // Temporarily removed from OpenAPI - route still registered below
        handlers::get_adapter_activations,
        handlers::list_repositories,
        handlers::get_quality_metrics,
        handlers::get_adapter_metrics,
        handlers::get_system_metrics,
        handlers::rag_list_retrievals,
        handlers::rag_stats,
        handlers::list_commits,
        handlers::get_commit,
        handlers::get_commit_diff,
        handlers::debug_routing,
        handlers::get_routing_history,
        handlers::routing_decisions,
        handlers::get_operation_status_handler,
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
        handlers::list_training_sessions,
        handlers::get_training_session,
        handlers::start_training_session,
        handlers::cancel_training,
        handlers::get_training_logs,
        handlers::get_training_metrics,
        handlers::list_training_templates,
        handlers::get_training_template,
        handlers::get_activity_events,
        handlers::telemetry::get_recent_activity,
        handlers::telemetry::recent_activity_stream,
        handlers::delete_plan,
        // Log file handlers
        handlers::list_log_files,
        handlers::get_log_file_content,
        handlers::stream_log_file,
        // Telemetry bundle handlers
        handlers::list_telemetry_bundles,
        handlers::generate_telemetry_bundle,
        handlers::export_telemetry_bundle,
        handlers::verify_bundle_signature,
        handlers::purge_old_bundles,
        // Git integration handlers excluded from OpenAPI documentation
        // Code intelligence handlers (disabled by default)
        // Federation handlers (TODO: integrate with AppState)
        // handlers::federation::get_federation_status,
        // handlers::federation::get_quarantine_status,
        // handlers::federation::release_quarantine,
        // Domain adapter handlers
        domain_adapters::list_domain_adapters,
        domain_adapters::get_domain_adapter,
        domain_adapters::create_domain_adapter,
        domain_adapters::load_domain_adapter,
        domain_adapters::unload_domain_adapter,
        domain_adapters::test_domain_adapter,
        domain_adapters::get_domain_adapter_manifest,
        // domain_adapters::execute_domain_adapter,  // Temporarily removed for compilation
        domain_adapters::delete_domain_adapter,
        // Model status handlers
        handlers::get_base_model_status,
        handlers::get_all_models_status,
        // Model management handlers - Citation: IMPLEMENTATION_PLAN.md Phase 1
        // handlers::models::get_model_status,  // Disabled due to OpenAPI feature flag
        // handlers::models::download_model,   // Disabled due to OpenAPI feature flag
        // Note: OpenAPI path macros disabled due to feature flag
        // OpenAI-compatible handlers
        handlers::openai::chat_completions,
        handlers::openai::list_models,
        // Tutorial handlers excluded from OpenAPI (no openapi feature enabled)
        // handlers::tutorials::list_tutorials,
        // handlers::tutorials::mark_tutorial_completed,
        // handlers::tutorials::unmark_tutorial_completed,
        // handlers::tutorials::mark_tutorial_dismissed,
        // handlers::tutorials::unmark_tutorial_dismissed,
        handlers::replay::replay_from_bundle,
    ),
    components(schemas(
        crate::types::ErrorResponse,
        // crate::types::LoginRequest,
        // crate::types::LoginResponse,
        // crate::types::HealthResponse,
        crate::types::TenantResponse,
        crate::types::CreateTenantRequest,
        crate::types::ProposePatchRequest,
        crate::types::ProposePatchResponse,
        crate::types::InferRequest,
        crate::types::InferResponse,
        crate::types::BatchInferRequest,
        crate::types::BatchInferResponse,
        crate::types::BatchInferItemRequest,
        crate::types::BatchInferItemResponse,
        // crate::types::InferenceTrace,
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
        crate::types::RagRetrievalRecordResponse,
        crate::types::RagRetrievalTenantCount,
        crate::types::AuditsQuery,
        crate::types::AuditExtended,
        crate::types::AuditsResponse,
        crate::types::PromotionRecord,
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
        adapteros_api_types::openai::ChatCompletionRequest,
        adapteros_api_types::openai::ChatCompletionResponse,
        adapteros_api_types::openai::ChatMessage,
        adapteros_api_types::openai::ChatChoice,
        adapteros_api_types::openai::ChatUsage,
        adapteros_api_types::openai::ModelsListResponse,
        adapteros_api_types::openai::ModelInfo,
        // Log file schemas
        crate::types::LogFileInfo,
        crate::types::ListLogFilesResponse,
        crate::types::LogFileContentResponse,
        crate::types::LogFileQueryParams,
        // Telemetry bundle schemas
        crate::types::TelemetryBundleResponse,
        crate::types::ExportTelemetryBundleResponse,
        crate::types::VerifyBundleSignatureResponse,
        crate::types::PurgeOldBundlesRequest,
        crate::types::PurgeOldBundlesResponse,
        // Tutorial schemas excluded (no openapi feature)
        // handlers::tutorials::TutorialResponse,
        // handlers::tutorials::TutorialStep,
        // handlers::tutorials::TutorialStatusResponse,
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
        (name = "logs", description = "Log file retrieval and streaming"),
        (name = "commits", description = "Commit inspection"),
        (name = "routing", description = "Routing debug and inspection"),
        (name = "contacts", description = "Contact discovery and management"),
        (name = "streams", description = "Real-time SSE event streams"),
        (name = "domain-adapters", description = "Domain adapter management"),
        (name = "git", description = "Git integration and session management"),
        (name = "federation", description = "Federation verification and quarantine management"),
        (name = "openai", description = "OpenAI-compatible endpoints for external tools"),
        (name = "inference", description = "Model inference endpoints"),
        (name = "telemetry", description = "Telemetry bundle management and events"),
    )
)]
pub struct ApiDoc;

pub fn build(state: AppState) -> Router {
    // Public routes (no auth required)
    let public_routes: Router<AppState> = Router::new()
        .with_state(state.clone())
        .route("/healthz", get(handlers::health))
        .route("/readyz", get(handlers::ready))
        .route("/v1/auth/login", post(handlers::auth_login))
        .route("/v1/auth/dev-bypass", post(handlers::auth_dev_bypass))
        .route("/v1/meta", get(handlers::meta));

    // Metrics endpoint (bearer token auth)
    let metrics_route = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics_auth_middleware,
        ))
        .with_state(state.clone());

    // OpenAI-compatible endpoints (dual auth: API key or JWT)
    let openai_routes: Router<AppState> = Router::new()
        .with_state(state.clone())
        .route(
            "/v1/chat/completions",
            post(handlers::openai::chat_completions),
        )
        .route("/v1/models", get(handlers::openai::list_models))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            dual_auth_middleware,
        ));

    // Protected routes (require auth)
    use axum::routing::MethodRouter;

    // Help type inference for handlers that require state by wrapping in closures
    let cp_promote_route: MethodRouter<AppState> = post(
        |state: State<AppState>,
         claims: Extension<crate::auth::Claims>,
         req: Json<crate::types::PromoteCPRequest>| async move {
            handlers::cp_promote(state, claims, req).await
        },
    );
    let promotion_gates_route: MethodRouter<AppState> = get(
        |state: State<AppState>,
         claims: Extension<crate::auth::Claims>,
         Path(cpid): Path<String>| async move {
            handlers::promotion_gates(state, claims, Path(cpid)).await
        },
    );

    let protected_routes: Router<AppState> = Router::new()
        .with_state(state.clone())
        .route("/v1/auth/logout", post(handlers::auth_logout))
        .route("/v1/auth/me", get(handlers::auth_me))
        .route("/v1/auth/refresh", post(handlers::auth_refresh))
        .route("/v1/auth/logout-all", post(handlers::auth_logout_all))
        .route("/v1/auth/sessions", get(handlers::auth_list_sessions))
        .route(
            "/v1/auth/sessions/:session_id",
            delete(handlers::auth_revoke_session),
        )
        .route("/v1/auth/token/rotate", post(handlers::auth_rotate_token))
        .route("/v1/auth/token", get(handlers::auth_token_metadata))
        .route("/v1/auth/profile", put(handlers::auth_update_profile))
        .route(
            "/v1/auth/config",
            get(handlers::auth_get_config).put(handlers::auth_update_config),
        )
        .route("/v1/status", get(handlers::get_status))
        .route(
            "/v1/models/status/all",
            get(handlers::get_all_models_status),
        )
        .route(
            "/v1/tenants",
            get(handlers::list_tenants).post(handlers::create_tenant),
        )
        .route("/v1/tenants/:tenant_id", put(handlers::update_tenant))
        .route("/v1/tenants/:tenant_id/pause", post(handlers::pause_tenant))
        .route(
            "/v1/tenants/:tenant_id/archive",
            post(handlers::archive_tenant),
        )
        .route(
            "/v1/tenants/:tenant_id/policies",
            post(handlers::assign_tenant_policies),
        )
        .route(
            "/v1/tenants/:tenant_id/rename",
            post(handlers::rename_tenant),
        )
        .route(
            "/v1/tenants/:tenant_id/adapters",
            post(handlers::assign_tenant_adapters),
        )
        .route(
            "/v1/tenants/:tenant_id/usage",
            get(handlers::get_tenant_usage),
        )
        .route("/v1/nodes", get(handlers::list_nodes))
        .route("/v1/nodes/register", post(handlers::register_node))
        .route(
            "/v1/nodes/:node_id/ping",
            post(handlers::test_node_connection),
        )
        .route("/v1/nodes/:node_id/cordon", post(handlers::node_cordon))
        .route("/v1/nodes/:node_id/drain", post(handlers::node_drain))
        .route(
            "/v1/nodes/:node_id/offline",
            post(handlers::mark_node_offline),
        )
        .route(
            "/v1/nodes/:node_id",
            axum::routing::delete(handlers::evict_node),
        )
        .route(
            "/v1/nodes/:node_id/details",
            get(handlers::get_node_details),
        )
        .route("/v1/models/import", post(handlers::import_model))
        .route("/v1/models/status", get(handlers::get_base_model_status))
        .route("/v1/plans", get(handlers::list_plans))
        .route("/v1/plans/:plan_id", delete(handlers::delete_plan))
        .route("/v1/plans/build", post(handlers::build_plan))
        .route(
            "/v1/plans/:plan_id/details",
            get(handlers::get_plan_details),
        )
        .route("/v1/plans/:plan_id/rebuild", post(handlers::rebuild_plan))
        .route("/v1/plans/compare", post(handlers::compare_plans))
        .route("/v1/plans/:plan_id/pin", post(handlers::pin_plan_alias))
        .route(
            "/v1/plans/:plan_id/manifest",
            get(handlers::export_plan_manifest),
        )
        .route("/v1/cp/promote", cp_promote_route)
        .route("/v1/cp/promotion-gates/:cpid", promotion_gates_route)
        .route("/v1/cp/rollback", post(handlers::cp_rollback))
        .route("/v1/cp/promote/dry-run", post(handlers::cp_dry_run_promote))
        .route("/v1/cp/promotions", get(handlers::get_promotion_history))
        .route("/v1/workers", get(handlers::list_workers))
        .route("/v1/workers/spawn", post(handlers::worker_spawn))
        .route(
            "/v1/workers/register-local",
            post(handlers::worker_register_local),
        )
        .route(
            "/v1/workers/:worker_id/heartbeat",
            post(handlers::worker_heartbeat),
        )
        .route(
            "/v1/workers/:worker_id/logs",
            get(handlers::list_process_logs),
        )
        .route(
            "/v1/workers/:worker_id/crashes",
            get(handlers::list_process_crashes),
        )
        .route(
            "/v1/workers/:worker_id/debug",
            post(handlers::start_debug_session),
        )
        .route(
            "/v1/workers/:worker_id/troubleshoot",
            post(handlers::run_troubleshooting_step),
        )
        .route(
            "/v1/monitoring/rules",
            get(handlers::list_process_monitoring_rules),
        )
        .route(
            "/v1/monitoring/rules",
            post(handlers::create_process_monitoring_rule),
        )
        .route("/v1/monitoring/alerts", get(handlers::list_process_alerts))
        .route("/v1/monitoring/alerts/stream", get(handlers::alerts_stream))
        .route(
            "/v1/monitoring/alerts/:alert_id/acknowledge",
            post(handlers::acknowledge_process_alert),
        )
        .route(
            "/v1/monitoring/anomalies",
            get(handlers::list_process_anomalies),
        )
        .route(
            "/v1/monitoring/anomalies/:anomaly_id/status",
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
        .route(
            "/v1/journeys/:type/:id",
            get(handlers::journeys::get_journey),
        )
        .route("/v1/jobs", get(handlers::list_jobs))
        .route("/v1/policies", get(handlers::list_policies))
        .route("/v1/policies/:cpid", get(handlers::get_policy))
        .route("/v1/policies/validate", post(handlers::validate_policy))
        .route("/v1/policies/apply", post(handlers::apply_policy))
        .route("/v1/policies/:cpid/sign", post(handlers::sign_policy))
        .route(
            "/v1/policies/compare",
            post(handlers::compare_policy_versions),
        )
        .route("/v1/policies/:cpid/export", get(handlers::export_policy))
        // Golden baselines - removed to fix route conflict (see memory-leak-fixes.patch)
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
            "/v1/replay/sessions/:id",
            get(handlers::replay::get_replay_session),
        )
        .route(
            "/v1/replay/sessions/:id/verify",
            post(handlers::replay::verify_replay_session),
        )
        .route("/v1/replay/:bundle_id", post(replay_from_bundle))
        .route(
            "/v1/tenants/:tenant_id/cp-pointers",
            get(handlers::list_cp_pointers),
        )
        .route(
            "/v1/tenants/:tenant_id/cp-pointers/:alias/activate",
            post(handlers::activate_cp_pointer),
        )
        .route("/v1/patch/propose", post(handlers::propose_patch))
        .route("/v1/infer", post(handlers::infer))
        .route("/v1/infer/stream", post(handlers::infer_stream))
        .route("/v1/infer/batch", post(handlers::batch::batch_infer))
        // Adapter routes
        .route("/v1/adapters", get(handlers::list_adapters))
        .route("/v1/adapters/:adapter_id", get(handlers::get_adapter))
        .route("/v1/adapters/register", post(handlers::register_adapter))
        .route("/v1/adapters/import", post(handlers::import_adapter))
        .route(
            "/v1/adapters/:adapter_id",
            axum::routing::delete(handlers::delete_adapter),
        )
        // Temporarily removed load_adapter route
        // .route(
        //     "/v1/adapters/:adapter_id/load",
        //     post(handlers::load_adapter),
        // )
        .route(
            "/v1/adapters/:adapter_id/unload",
            post(handlers::unload_adapter),
        )
        // DISABLED: Duplicate route error - investigating
        // .route(
        //     "/v1/adapters/:adapter_id/hot-swap",
        //     post(handlers::hot_swap_adapter),
        // )
        .route(
            "/v1/adapters/:adapter_id/activations",
            get(handlers::get_adapter_activations),
        )
        .route(
            "/v1/adapters/:adapter_id/promote",
            post(handlers::promote_adapter_state),
        )
        .route("/v1/adapters/:adapter_id/pin", post(handlers::pin_adapter))
        .route(
            "/v1/adapters/:adapter_id/unpin",
            post(handlers::unpin_adapter),
        )
        .route(
            "/v1/adapters/:adapter_id/policy",
            put(handlers::update_adapter_policy),
        )
        .route(
            "/v1/adapters/:adapter_id/manifest",
            get(handlers::download_adapter_manifest),
        )
        .route(
            "/v1/adapters/directory/upsert",
            post(handlers::upsert_directory_adapter),
        )
        .route("/v1/adapters/bulk-load", post(handlers::bulk_adapter_load))
        .route(
            "/v1/adapters/:adapter_id/health",
            get(handlers::get_adapter_health),
        )
        // Adapter category policy routes
        // TODO: Fix Claims type trait bounds for axum handlers
        // .route(
        //     "/v1/adapters/category-policies",
        //     get(handlers::get_category_policies),
        // )
        // .route(
        //     "/v1/adapters/category-policies/:category",
        //     get(handlers::get_category_policy).put(handlers::update_category_policy),
        // )
        // Memory management routes
        .route("/v1/memory/usage", get(handlers::get_memory_usage))
        .route(
            "/v1/memory/adapters/:adapter_id/evict",
            post(handlers::evict_adapter),
        )
        // Base model management routes - Citation: IMPLEMENTATION_PLAN.md Phase 1
        .route(
            "/v1/models/:model_id/load",
            post(handlers::load_model_with_retry),
        )
        .route(
            "/v1/models/:model_id/unload",
            post(handlers::models::unload_model),
        )
        .route(
            "/v1/models/:model_id/cancel",
            post(handlers::models::cancel_model_operation),
        )
        // Temporarily disabled due to compilation issue
        // .route(
        //     "/v1/models/health",
        //     get(handlers::models::model_runtime_health),
        // )
        .route(
            "/v1/models/:model_id/status",
            get(handlers::models::get_model_status),
        )
        .route(
            "/v1/models/:model_id/validate",
            get(handlers::models::validate_model),
        )
        .route(
            "/v1/models/:model_id/download",
            get(handlers::models::download_model),
        )
        .route(
            "/v1/models/download/:token",
            get(handlers::models::download_model_artifact),
        )
        .route(
            "/v1/models/imports/:import_id",
            get(handlers::models::get_import_status),
        )
        .route(
            "/v1/models/cursor-config",
            get(handlers::models::get_cursor_config),
        )
        .route(
            "/v1/models/diagnostics",
            get(handlers::models::get_model_diagnostics),
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
            "/v1/domain-adapters/:adapter_id",
            get(domain_adapters::get_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/:adapter_id",
            delete(domain_adapters::delete_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/:adapter_id/load",
            post(domain_adapters::load_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/:adapter_id/unload",
            post(domain_adapters::unload_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/:adapter_id/test",
            post(domain_adapters::test_domain_adapter),
        )
        .route(
            "/v1/domain-adapters/:adapter_id/manifest",
            get(domain_adapters::get_domain_adapter_manifest),
        )
        .route(
            "/v1/domain-adapters/:adapter_id/execute",
            post(domain_adapters::execute_domain_adapter),
        )
        // Contacts routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6
        .route(
            "/v1/contacts",
            get(handlers::list_contacts).post(handlers::create_contact),
        )
        .route(
            "/v1/contacts/:id",
            get(handlers::get_contact).delete(handlers::delete_contact),
        )
        .route(
            "/v1/contacts/:id/interactions",
            get(handlers::get_contact_interactions),
        )
        // Workspace routes - Citation: Communication & Collaboration Implementation Plan
        .route(
            "/v1/workspaces",
            get(handlers::workspaces::list_workspaces).post(handlers::workspaces::create_workspace),
        )
        .route(
            "/v1/workspaces/my",
            get(handlers::workspaces::list_user_workspaces),
        )
        .route(
            "/v1/workspaces/:id",
            get(handlers::workspaces::get_workspace)
                .put(handlers::workspaces::update_workspace)
                .delete(handlers::workspaces::delete_workspace),
        )
        .route(
            "/v1/workspaces/:id/members",
            get(handlers::workspaces::list_workspace_members)
                .post(handlers::workspaces::add_workspace_member),
        )
        .route(
            "/v1/workspaces/:id/members/:member_id",
            put(handlers::workspaces::update_workspace_member)
                .delete(handlers::workspaces::remove_workspace_member),
        )
        .route(
            "/v1/workspaces/:id/resources",
            get(handlers::workspaces::list_workspace_resources)
                .post(handlers::workspaces::share_workspace_resource),
        )
        .route(
            "/v1/workspaces/:id/resources/:resource_id",
            delete(handlers::workspaces::unshare_workspace_resource),
        )
        // Messaging routes
        .route(
            "/v1/workspaces/:workspace_id/messages",
            get(handlers::messages::list_workspace_messages)
                .post(handlers::messages::create_message),
        )
        .route(
            "/v1/workspaces/:workspace_id/messages/:message_id",
            put(handlers::messages::edit_message),
        )
        .route(
            "/v1/workspaces/:workspace_id/messages/:thread_id/thread",
            get(handlers::messages::get_message_thread),
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
            "/v1/notifications/:id/read",
            post(handlers::notifications::mark_notification_read),
        )
        .route(
            "/v1/notifications/read-all",
            post(handlers::notifications::mark_all_notifications_read),
        )
        // Tutorial routes
        .route("/v1/tutorials", get(handlers::tutorials::list_tutorials))
        .route(
            "/v1/tutorials/:id/complete",
            post(handlers::tutorials::mark_tutorial_completed)
                .delete(handlers::tutorials::unmark_tutorial_completed),
        )
        .route(
            "/v1/tutorials/:id/dismiss",
            post(handlers::tutorials::mark_tutorial_dismissed)
                .delete(handlers::tutorials::unmark_tutorial_dismissed),
        )
        // Activity routes
        .route(
            "/v1/activity",
            get(handlers::activity::list_activity_events)
                .post(handlers::activity::create_activity_event),
        )
        .route(
            "/v1/activity/my",
            get(handlers::activity::list_user_workspace_activity),
        )
        // SSE Streaming routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4
        .route("/v1/streams/training", get(handlers::training_stream))
        .route("/v1/streams/discovery", get(handlers::discovery_stream))
        .route("/v1/streams/contacts", get(handlers::contacts_stream))
        // SSE routes for notifications and messages
        .route(
            "/v1/stream/notifications",
            get(handlers::notifications_stream),
        )
        .route(
            "/v1/stream/messages/:workspace_id",
            get(handlers::messages_stream),
        )
        .route(
            "/v1/stream/activity/:workspace_id",
            get(handlers::activity_stream),
        )
        // Code intelligence routes (compiled only with "cdp" feature)
        // Repository routes (deprecated - use /v1/code/repositories instead)
        .route("/v1/repositories", get(handlers::list_repositories))
        .route(
            "/v1/repositories/register",
            post(handlers::git_repository::register_git_repository),
        )
        .route(
            "/v1/repositories/:repo_id/scan",
            post(handlers::git_repository::trigger_repository_scan),
        )
        .route(
            "/v1/repositories/:repo_id/status",
            get(handlers::git_repository::get_repository_status),
        )
        .route(
            "/v1/repositories/:repo_id/analysis",
            get(handlers::git_repository::get_repository_analysis),
        )
        .route(
            "/v1/repositories/:repo_id",
            delete(handlers::git_repository::unregister_repository),
        )
        // Metrics routes
        .route("/v1/metrics/quality", get(handlers::get_quality_metrics))
        .route("/v1/metrics/adapters", get(handlers::get_adapter_metrics))
        .route("/v1/metrics/system", get(handlers::get_system_metrics))
        // RAG retrieval audit endpoints
        .route("/v1/rag/retrievals", get(handlers::rag_list_retrievals))
        .route("/v1/rag/stats", get(handlers::rag_stats))
        // Telemetry bundle routes
        .route(
            "/v1/telemetry/bundles",
            get(handlers::list_telemetry_bundles),
        )
        .route(
            "/v1/telemetry/bundles/generate",
            post(handlers::generate_telemetry_bundle),
        )
        .route(
            "/v1/telemetry/bundles/:bundle_id/export",
            get(handlers::export_telemetry_bundle),
        )
        .route(
            "/v1/telemetry/bundles/:bundle_id/verify",
            post(handlers::verify_bundle_signature),
        )
        .route(
            "/v1/telemetry/bundles/purge",
            post(handlers::purge_old_bundles),
        )
        // Commit routes
        .route("/v1/commits", get(handlers::list_commits))
        .route("/v1/commits/:sha", get(handlers::get_commit))
        .route("/v1/commits/:sha/diff", get(handlers::get_commit_diff))
        // Routing routes
        .route("/v1/routing/debug", post(handlers::debug_routing))
        .route("/v1/routing/history", get(handlers::get_routing_history))
        .route("/v1/routing/decisions", get(handlers::routing_decisions))
        // Training routes
        .route("/v1/training/jobs", get(handlers::list_training_jobs))
        .route("/v1/training/jobs/:job_id", get(handlers::get_training_job))
        .route("/v1/training/start", post(handlers::start_training))
        .route(
            "/v1/training/jobs/:job_id/artifacts",
            get(handlers::get_training_artifacts),
        )
        .route(
            "/v1/training/jobs/:job_id/cancel",
            post(handlers::cancel_training),
        )
        .route(
            "/v1/training/jobs/:job_id/logs",
            get(handlers::get_training_logs),
        )
        .route(
            "/v1/training/sessions",
            get(handlers::list_training_sessions).post(handlers::start_training_session),
        )
        .route(
            "/v1/training/sessions/:session_id",
            get(handlers::get_training_session),
        )
        .route(
            "/v1/training/sessions/:session_id/pause",
            post(handlers::pause_training_session),
        )
        .route(
            "/v1/training/sessions/:session_id/resume",
            post(handlers::resume_training_session),
        )
        .route(
            "/v1/training/jobs/:job_id/metrics",
            get(handlers::get_training_metrics),
        )
        .route(
            "/v1/training/templates",
            get(handlers::list_training_templates),
        )
        .route(
            "/v1/training/templates/:template_id",
            get(handlers::get_training_template),
        )
        // Git integration routes
        .route("/v1/git/status", get(handlers::git::git_status))
        .route(
            "/v1/git/sessions/start",
            post(handlers::git::start_git_session),
        )
        .route(
            "/v1/git/sessions/:session_id/end",
            post(handlers::git::end_git_session),
        )
        .route("/v1/git/branches", get(handlers::git::list_git_branches))
        .route(
            "/v1/streams/file-changes",
            get(handlers::git::file_changes_stream),
        )
        // Federation routes (TODO: integrate with AppState)
        // .route("/v1/federation/status", get(handlers::federation::get_federation_status))
        // .route("/v1/federation/quarantine", get(handlers::federation::get_quarantine_status))
        // .route("/v1/federation/release-quarantine", post(handlers::federation::release_quarantine))
        // Audit endpoints
        .route("/v1/audit/federation", get(handlers::get_federation_audit))
        .route("/v1/audit/compliance", get(handlers::get_compliance_audit))
        // Agent D contract endpoints
        .route("/v1/audits", get(handlers::list_audits_extended))
        .route("/v1/promotions/:id", get(handlers::get_promotion))
        // SSE stream endpoints
        .route("/v1/stream/metrics", get(handlers::system_metrics_stream))
        .route(
            "/v1/telemetry/stream",
            get(handlers::telemetry_events_stream),
        )
        .route(
            "/v1/stream/telemetry",
            get(handlers::telemetry_events_stream),
        )
        .route("/v1/stream/adapters", get(handlers::adapter_state_stream))
        .route(
            "/v1/stream/operations/progress",
            // Operation progress streaming
            // Citation: [source: crates/adapteros-server-api/src/handlers.rs L9677-9719]
            get(handlers::operation_progress_stream),
        )
        .route(
            "/v1/operations/:resource_id/status",
            // Operation status query
            // Citation: [source: crates/adapteros-server-api/src/handlers.rs L7131-7160]
            get(handlers::get_operation_status_handler),
        )
        .route("/v1/telemetry/events", get(handlers::get_activity_events))
        .route(
            "/v1/telemetry/events/recent",
            get(handlers::telemetry::get_recent_activity),
        )
        .route(
            "/v1/telemetry/events/recent/stream",
            get(handlers::telemetry::recent_activity_stream),
        )
        .route("/v1/telemetry/logs", post(handlers::submit_client_logs))
        .route("/v1/audits/export", get(handlers::export_audit_logs))
        // Log file endpoints
        .route("/v1/logs/files", get(handlers::list_log_files))
        .route(
            "/v1/logs/files/:filename",
            get(handlers::get_log_file_content),
        )
        .route(
            "/v1/logs/files/:filename/stream",
            get(handlers::stream_log_file),
        )
        // Telemetry endpoints for offline dashboard
        .route(
            "/api/metrics/snapshot",
            get(handlers::telemetry::get_metrics_snapshot),
        )
        .route(
            "/api/metrics/series",
            get(handlers::telemetry::get_metrics_series),
        )
        .route("/api/logs/query", get(handlers::telemetry::query_logs))
        .route("/api/logs/stream", get(handlers::telemetry::stream_logs))
        .route(
            "/api/traces/search",
            get(handlers::telemetry::search_traces),
        )
        .route("/api/traces/:trace_id", get(handlers::telemetry::get_trace))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(per_tenant_rate_limit_middleware(state.clone()));

    // Configure CORS for development
    let cors = CorsLayer::permissive(); // Allow all origins in dev mode

    // Combine routes
    Router::new()
        .merge(public_routes)
        .merge(metrics_route)
        .merge(openai_routes)
        .merge(protected_routes)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(user_friendly_error_middleware))
        .with_state(state)
}
