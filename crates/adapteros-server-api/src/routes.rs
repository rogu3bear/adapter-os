use crate::handlers;
use crate::handlers::domain_adapters;
use crate::middleware::{auth_middleware, dual_auth_middleware};
use crate::state::AppState;
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health,
        handlers::ready,
        handlers::auth_login,
        handlers::propose_patch,
        handlers::infer,
        handlers::batch::batch_infer,
        handlers::list_adapters,
        handlers::get_adapter,
        handlers::register_adapter,
        handlers::delete_adapter,
        handlers::load_adapter,
        handlers::unload_adapter,
        handlers::verify_gpu_integrity,
        handlers::get_adapter_activations,
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
    ),
    components(schemas(
        crate::types::ErrorResponse,
        crate::types::LoginRequest,
        crate::types::LoginResponse,
        crate::types::HealthResponse,
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
    )
)]
pub struct ApiDoc;

pub fn build(state: AppState) -> Router {
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/healthz", get(handlers::health))
        .route("/readyz", get(handlers::ready))
        .route("/v1/auth/login", post(handlers::auth_login))
        .route("/v1/meta", get(handlers::meta));

    // Metrics endpoint (custom auth, not JWT)
    let metrics_route = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .with_state(state.clone());

    // Protected routes (require auth)
    let protected_routes = Router::new()
        .route("/v1/auth/logout", post(handlers::auth_logout))
        .route("/v1/auth/me", get(handlers::auth_me))
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
        .route("/v1/plans/build", post(handlers::build_plan))
        .route(
            "/v1/plans/:plan_id/details",
            get(handlers::get_plan_details),
        )
        .route("/v1/plans/:plan_id/rebuild", post(handlers::rebuild_plan))
        .route("/v1/plans/compare", post(handlers::compare_plans))
        .route(
            "/v1/plans/:plan_id/manifest",
            get(handlers::export_plan_manifest),
        )
        .route("/v1/cp/promote", post(handlers::cp_promote))
        .route(
            "/v1/cp/promotion-gates/:cpid",
            get(handlers::promotion_gates),
        )
        .route("/v1/cp/rollback", post(handlers::cp_rollback))
        .route("/v1/cp/promote/dry-run", post(handlers::cp_dry_run_promote))
        .route("/v1/cp/promotions", get(handlers::get_promotion_history))
        .route("/v1/workers", get(handlers::list_workers))
        .route("/v1/workers/spawn", post(handlers::worker_spawn))
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
        .route(
            "/v1/telemetry/bundles",
            get(handlers::list_telemetry_bundles),
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
        .route("/v1/patch/propose", post(handlers::propose_patch))
        .route("/v1/infer", post(handlers::infer))
        .route("/v1/infer/batch", post(handlers::batch::batch_infer))
        // Adapter routes
        .route("/v1/adapters", get(handlers::list_adapters))
        .route("/v1/adapters/:adapter_id", get(handlers::get_adapter))
        .route("/v1/adapters/register", post(handlers::register_adapter))
        .route(
            "/v1/adapters/:adapter_id",
            axum::routing::delete(handlers::delete_adapter),
        )
        .route(
            "/v1/adapters/:adapter_id/load",
            post(handlers::load_adapter),
        )
        .route(
            "/v1/adapters/:adapter_id/unload",
            post(handlers::unload_adapter),
        )
        .route(
            "/v1/adapters/verify-gpu",
            get(handlers::verify_gpu_integrity),
        )
        .route(
            "/v1/adapters/:adapter_id/activations",
            get(handlers::get_adapter_activations),
        )
        // PRD-07: Lifecycle promotion/demotion (distinct from tier-based promotion)
        .route(
            "/v1/adapters/:adapter_id/lifecycle/promote",
            post(handlers::promote_adapter_lifecycle),
        )
        .route(
            "/v1/adapters/:adapter_id/lifecycle/demote",
            post(handlers::demote_adapter_lifecycle),
        )
        // PRD-08: Lineage and detail views
        .route(
            "/v1/adapters/:adapter_id/lineage",
            get(handlers::get_adapter_lineage),
        )
        .route(
            "/v1/adapters/:adapter_id/detail",
            get(handlers::get_adapter_detail),
        )
        .route(
            "/v1/adapters/:adapter_id/manifest",
            get(handlers::download_adapter_manifest),
        )
        .route(
            "/v1/adapters/directory/upsert",
            post(handlers::upsert_directory_adapter),
        )
        .route(
            "/v1/adapters/:adapter_id/health",
            get(handlers::get_adapter_health),
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
            "/v1/adapters/next-revision/:tenant/:domain/:purpose",
            get(handlers::get_next_revision),
        )
        // Adapter stacks routes
        .route(
            "/v1/adapter-stacks",
            get(handlers::adapter_stacks::list_stacks).post(handlers::adapter_stacks::create_stack),
        )
        .route(
            "/v1/adapter-stacks/:id",
            get(handlers::adapter_stacks::get_stack).delete(handlers::adapter_stacks::delete_stack),
        )
        .route(
            "/v1/adapter-stacks/:id/activate",
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
        // SSE Streaming routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4
        .route("/v1/streams/training", get(handlers::training_stream))
        .route("/v1/streams/discovery", get(handlers::discovery_stream))
        .route("/v1/streams/contacts", get(handlers::contacts_stream))
        // Code intelligence routes
        .route(
            "/v1/code/register-repo",
            post(handlers::code::register_repo),
        )
        .route("/v1/code/scan", post(handlers::code::scan_repo))
        .route(
            "/v1/code/scan/:job_id",
            get(handlers::code::get_scan_status),
        )
        .route(
            "/v1/code/repositories",
            get(handlers::code::list_repositories),
        )
        .route(
            "/v1/code/repositories/:repo_id",
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
            "/v1/training/jobs/:job_id/cancel",
            post(handlers::cancel_training),
        )
        .route(
            "/v1/training/jobs/:job_id/logs",
            get(handlers::get_training_logs),
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
        .route("/v1/audit/logs", get(handlers::query_audit_logs))
        // Agent D contract endpoints
        .route("/v1/audits", get(handlers::list_audits_extended))
        .route("/v1/promotions/:id", get(handlers::get_promotion))
        // SSE stream endpoints
        .route("/v1/stream/metrics", get(handlers::system_metrics_stream))
        .route(
            "/v1/stream/telemetry",
            get(handlers::telemetry_events_stream),
        )
        .route("/v1/stream/adapters", get(handlers::adapter_state_stream))
        .route(
            "/v1/plugins/:name/enable",
            post(handlers::plugins::enable_plugin),
        )
        .route(
            "/v1/plugins/:name/disable",
            post(handlers::plugins::disable_plugin),
        )
        .route("/v1/plugins/:name", get(handlers::plugins::plugin_status))
        .route("/v1/plugins", get(handlers::plugins::list_plugins))
        .route("/v1/system/memory", get(handlers::get_uma_memory))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Configure CORS for development
    let cors = CorsLayer::permissive(); // Allow all origins in dev mode

    // Combine routes
    Router::new()
        .merge(public_routes)
        .merge(metrics_route)
        .merge(protected_routes)
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
