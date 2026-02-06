// Route submodules - extracted for maintainability
mod adapters;
mod auth_routes;
mod chat_routes;
mod tenant_routes;
mod training_routes;

use crate::caching;
use crate::handlers;
use crate::handlers::auth;
use crate::handlers::domain_adapters;
use crate::health::system_ready;
use crate::idempotency::idempotency_middleware;
use crate::middleware::audit::audit_middleware;
use crate::middleware::context::context_middleware;
use crate::middleware::error_code_enforcement::ErrorCodeEnforcementLayer;
use crate::middleware::policy_enforcement::policy_enforcement_middleware;
use crate::middleware::versioning;
use crate::middleware::{
    auth_middleware, client_ip_middleware, csrf_middleware, optional_auth_middleware,
    tenant_route_guard_middleware, worker_uid_middleware,
};
use crate::middleware_security::{
    cors_layer, drain_middleware, rate_limiting_middleware, request_size_limit_middleware,
    request_tracking_middleware, security_headers_middleware,
};
use crate::request_id;
use crate::state::AppState;
use adapteros_api_types::{ActivityEventResponse, CreateActivityEventRequest};
use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
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
        handlers::infrastructure::get_version,
        crate::health::check_all_health,
        crate::health::check_component_health,
        handlers::auth_enhanced::login_handler,
        handlers::auth_enhanced::bootstrap_admin_handler,
        handlers::auth_enhanced::get_auth_config_handler,
        handlers::ui_config::get_ui_config,
        handlers::auth_enhanced::auth_health_handler,
        handlers::auth_enhanced::refresh_token_handler,
        handlers::auth_enhanced::register_handler,
        handlers::auth::auth_me,
        handlers::meta,
        handlers::search::global_search,
        handlers::client_errors::report_client_error_anonymous,
        handlers::get_status,
        handlers::health::get_invariant_status,
        handlers::auth_enhanced::dev_bypass_handler,
        handlers::auth_enhanced::dev_bootstrap_handler,
        handlers::metrics_handler,
        handlers::infrastructure::get_base_model_status,
        handlers::topology::get_topology,
        handlers::receive_worker_fatal,
        handlers::workers::register_worker,
        handlers::worker_manifests::fetch_manifest_by_hash,
        handlers::workers::notify_worker_status,
        handlers::admin::list_users,
        handlers::admin_lifecycle::request_shutdown,
        handlers::admin_lifecycle::request_maintenance,
        handlers::admin_lifecycle::safe_restart,
        handlers::list_nodes,
        handlers::register_node,
        handlers::test_node_connection,
        handlers::mark_node_offline,
        handlers::evict_node,
        handlers::get_node_details,
        handlers::services::start_service,
        handlers::services::stop_service,
        handlers::services::restart_service,
        handlers::services::start_essential_services,
        handlers::services::stop_essential_services,
        handlers::services::get_service_logs,
        handlers::model_server::get_model_server_status,
        handlers::model_server::warmup_model_server,
        handlers::model_server::drain_model_server,
        handlers::openai_compat::list_models_openai,
        handlers::models::list_models_with_stats,
        handlers::models::import_model,
        handlers::models::get_download_progress,
        handlers::models::get_all_models_status,
        handlers::models::load_model,
        handlers::models::unload_model,
        handlers::models::get_model_status,
        handlers::models::validate_model,
        handlers::list_plans,
        handlers::build_plan,
        handlers::get_plan_details,
        handlers::rebuild_plan,
        handlers::compare_plans,
        handlers::export_plan_manifest,
        handlers::cp_promote,
        handlers::promotion_gates,
        handlers::cp_rollback,
        handlers::cp_dry_run_promote,
        handlers::get_promotion_history,
        handlers::workers::list_workers,
        handlers::workers::worker_spawn,
        handlers::workers::stop_worker,
        handlers::workers::drain_worker,
        handlers::list_process_logs,
        handlers::list_process_crashes,
        handlers::start_debug_session,
        handlers::run_troubleshooting_step,
        handlers::list_worker_incidents,
        handlers::get_worker_health_summary,
        handlers::workers::get_worker_history,
        handlers::worker_detail::get_worker_detail,
        handlers::monitoring::list_monitoring_rules,
        handlers::monitoring::create_monitoring_rule,
        handlers::monitoring::update_monitoring_rule,
        handlers::monitoring::delete_monitoring_rule,
        handlers::monitoring::list_alerts,
        handlers::monitoring::acknowledge_alert,
        handlers::monitoring::resolve_alert,
        handlers::monitoring::list_process_anomalies,
        handlers::update_process_anomaly_status,
        handlers::list_process_monitoring_dashboards,
        handlers::create_process_monitoring_dashboard,
        handlers::list_process_health_metrics,
        handlers::list_process_monitoring_reports,
        handlers::create_process_monitoring_report,
        handlers::list_jobs,
        handlers::list_policies,
        handlers::get_policy,
        handlers::code_policy::get_code_policy,
        handlers::code_policy::update_code_policy,
        handlers::validate_policy,
        handlers::apply_policy,
        handlers::sign_policy,
        handlers::verify_policy_signature,
        handlers::compare_policy_versions,
        handlers::export_policy,
        handlers::tenant_policies::assign_policy,
        handlers::tenant_policies::list_policy_assignments,
        handlers::tenant_policies::list_violations,
        handlers::list_telemetry_bundles,
        handlers::export_telemetry_bundle,
        handlers::verify_bundle_signature,
        handlers::purge_old_bundles,
        handlers::replay::list_replay_sessions,
        handlers::replay::create_replay_session,
        handlers::replay::get_replay_session,
        handlers::replay::verify_replay_session,
        handlers::replay::execute_replay_session,
        handlers::replay_inference::check_availability,
        handlers::replay_inference::execute_replay,
        handlers::replay_inference::get_replay_history,
        handlers::adapteros_receipts::get_receipt_by_digest,
        handlers::adapteros_receipts::adapteros_replay,
        handlers::run_evidence::download_run_evidence,
        handlers::aliases::run_evidence::download_run_evidence_alias,
        handlers::code::propose_patch,
        handlers::openai_compat::chat_completions,
        handlers::openai_compat::completions_openai,
        handlers::openai_compat::embeddings_openai,
        handlers::infer,
        handlers::streaming_infer::streaming_infer,
        handlers::streaming_infer::streaming_infer_with_progress,
        handlers::batch::batch_infer,
        handlers::review::get_inference_state,
        handlers::review::submit_review,
        handlers::review::list_paused,
        handlers::inference::get_inference_provenance,
        handlers::review::list_paused_reviews,
        handlers::review::get_pause_details,
        handlers::review::export_review_context,
        handlers::review::submit_review_response,
        handlers::verdicts::list_verdicts,
        handlers::verdicts::create_verdict,
        handlers::verdicts::get_verdict,
        handlers::verdicts::derive_verdict,
        handlers::batch::create_batch_job,
        handlers::batch::get_batch_status,
        handlers::batch::get_batch_items,
        handlers::owner_cli::run_owner_cli_command,
        domain_adapters::list_domain_adapters,
        domain_adapters::create_domain_adapter,
        domain_adapters::get_domain_adapter,
        domain_adapters::delete_domain_adapter,
        domain_adapters::load_domain_adapter,
        domain_adapters::unload_domain_adapter,
        domain_adapters::test_domain_adapter,
        domain_adapters::get_domain_adapter_manifest,
        domain_adapters::execute_domain_adapter,
        handlers::chat_sessions::list_contacts,
        handlers::chat_sessions::create_contact,
        handlers::chat_sessions::get_contact,
        handlers::chat_sessions::delete_contact,
        handlers::chat_sessions::get_contact_interactions,
        handlers::streams::training_stream,
        handlers::discovery::discovery_stream,
        handlers::discovery::contacts_stream,
        handlers::datasets::upload_dataset,
        handlers::datasets::list_datasets,
        handlers::datasets::initiate_chunked_upload,
        handlers::datasets::upload_chunk,
        handlers::datasets::retry_chunk,
        handlers::datasets::complete_chunked_upload,
        handlers::datasets::get_upload_session_status,
        handlers::datasets::cancel_chunked_upload,
        handlers::datasets::list_upload_sessions,
        handlers::datasets::cleanup_expired_sessions,
        handlers::datasets::get_dataset,
        handlers::datasets::list_dataset_versions,
        handlers::datasets::create_dataset_version,
        handlers::datasets::get_dataset_version,
        handlers::datasets::list_versions_by_codebase,
        handlers::datasets::apply_dataset_version_trust_override,
        handlers::datasets::update_dataset_version_safety,
        handlers::datasets::delete_dataset,
        handlers::datasets::get_dataset_files,
        handlers::datasets::get_dataset_statistics,
        handlers::datasets::validate_dataset,
        handlers::datasets::apply_dataset_trust_override,
        handlers::datasets::preview_dataset,
        handlers::datasets::start_preprocess,
        handlers::datasets::get_preprocess_status,
        handlers::datasets::dataset_upload_progress,
        handlers::datasets::create_dataset_from_documents,
        handlers::datasets::create_dataset_from_text,
        handlers::datasets::create_dataset_from_chat,
        handlers::training_datasets::create_training_dataset_from_upload,
        handlers::datasets::generate_dataset_from_file,
        handlers::training_datasets::get_training_dataset_manifest,
        handlers::training_datasets::stream_training_dataset_rows,
        handlers::documents::upload_document,
        handlers::documents::list_documents,
        handlers::documents::get_document,
        handlers::documents::delete_document,
        handlers::documents::list_document_chunks,
        handlers::documents::download_document,
        handlers::documents::process_document,
        handlers::documents::retry_document,
        handlers::documents::list_failed_documents,
        handlers::collections::create_collection,
        handlers::collections::list_collections,
        handlers::collections::get_collection,
        handlers::collections::delete_collection,
        handlers::collections::add_document_to_collection,
        handlers::collections::remove_document_from_collection,
        handlers::evidence::list_evidence,
        handlers::evidence::create_evidence,
        handlers::evidence::get_evidence,
        handlers::evidence::delete_evidence,
        handlers::evidence::get_dataset_evidence,
        handlers::evidence::get_adapter_evidence,
        handlers::discrepancies::create_discrepancy,
        handlers::discrepancies::get_discrepancy,
        handlers::discrepancies::list_discrepancies,
        handlers::discrepancies::resolve_discrepancy,
        handlers::discrepancies::export_discrepancies,
        handlers::code::register_repo,
        handlers::code::scan_repo,
        handlers::code::get_scan_status,
        handlers::code::list_repositories,
        handlers::code::get_repository,
        handlers::code::create_commit_delta,
        handlers::list_repositories_legacy,
        handlers::system::get_system_integrity,
        handlers::boot_attestation::get_boot_attestation,
        handlers::boot_attestation::verify_boot_attestation,
        handlers::system_status::get_system_status,
        handlers::system_overview::get_system_overview,
        handlers::pilot_status::get_pilot_status,
        handlers::system_state::get_system_state,
        handlers::adapters::get_quality_metrics,
        handlers::adapters::get_adapter_metrics,
        handlers::adapters::get_system_metrics,
        handlers::metrics::get_code_metrics,
        handlers::metrics::compare_metrics,
        handlers::metrics_time_series::get_metrics_time_series,
        handlers::metrics_time_series::get_metrics_snapshot,
        handlers::system_info::get_uma_memory,
        handlers::capacity::get_memory_report,
        handlers::system_info::get_resource_usage,
        handlers::registry::get_registry_status,
        handlers::memory_detail::get_combined_memory_usage,
        handlers::coreml_verification_status,
        handlers::memory_detail::get_uma_memory_breakdown,
        handlers::memory_detail::get_adapter_memory_usage,
        handlers::adapters::list_commits,
        handlers::adapters::get_commit,
        handlers::adapters::get_commit_diff,
        handlers::routing_rules::list_rules,
        handlers::routing_rules::create_rule,
        handlers::routing_rules::delete_rule,
        handlers::routing_decisions::debug_routing,
        handlers::routing_decisions::get_routing_history,
        handlers::routing_decisions::get_routing_decisions,
        handlers::routing_decisions::get_routing_decision_by_id,
        handlers::routing_decisions::get_routing_decision_chain,
        handlers::routing_decisions::get_session_router_view,
        handlers::routing_decisions::ingest_router_decision,
        handlers::diagnostics::get_determinism_status,
        handlers::diagnostics::get_quarantine_status,
        handlers::diagnostics::list_diag_runs,
        handlers::diagnostics::get_diag_run,
        handlers::diagnostics::list_diag_events,
        handlers::diagnostics::diff_diag_runs,
        handlers::diagnostics::export_diag_run,
        handlers::diag_bundle::create_bundle_export,
        handlers::diag_bundle::get_bundle_export,
        handlers::diag_bundle::download_bundle,
        handlers::diag_bundle::download_signature,
        handlers::telemetry::search_traces,
        handlers::telemetry::get_trace,
        handlers::telemetry::list_inference_traces,
        handlers::telemetry::get_inference_trace_detail,
        handlers::telemetry::query_logs,
        handlers::telemetry::stream_logs,
        handlers::telemetry::get_recent_activity,
        handlers::telemetry::recent_activity_stream,
        handlers::client_errors::list_client_errors,
        handlers::client_errors::report_client_error,
        handlers::client_errors::get_client_error_stats,
        handlers::client_errors::get_client_error,
        handlers::client_errors::stream_client_errors,
        handlers::error_alerts::list_error_alert_rules,
        handlers::error_alerts::create_error_alert_rule,
        handlers::error_alerts::get_error_alert_rule,
        handlers::error_alerts::update_error_alert_rule,
        handlers::error_alerts::delete_error_alert_rule,
        handlers::error_alerts::list_error_alert_history,
        handlers::error_alerts::acknowledge_error_alert,
        handlers::error_alerts::resolve_error_alert,
        handlers::embeddings::list_embedding_benchmarks,
        handlers::telemetry::get_metrics_snapshot,
        handlers::telemetry::get_metrics_series,
        handlers::repos::list_repos,
        handlers::repos::create_repo,
        handlers::repos::get_repo,
        handlers::repos::update_repo,
        handlers::repos::list_versions,
        handlers::repos::get_version,
        handlers::repos::promote_version,
        handlers::repos::tag_version,
        handlers::repos::start_training,
        handlers::repos::rollback_version,
        handlers::repos::get_timeline,
        handlers::repos::list_training_jobs,
        handlers::git::git_status,
        handlers::git::start_git_session,
        handlers::git::end_git_session,
        handlers::git::list_git_branches,
        handlers::git::file_changes_stream,
        handlers::git_repository::register_git_repository,
        handlers::git_repository::get_repository_analysis,
        handlers::git_repository::train_repository_adapter,
        handlers::federation::get_federation_status,
        handlers::federation::get_federation_quarantine_status,
        handlers::federation::release_quarantine,
        handlers::federation::get_federation_sync_status,
        handlers::quarantine::get_quarantine_status,
        handlers::quarantine::clear_policy_violations,
        handlers::quarantine::rollback_policy_config,
        handlers::admin::query_audit_logs,
        handlers::query_policy_decisions,
        handlers::verify_policy_audit_chain,
        handlers::get_promotion,
        handlers::streams::system_metrics_stream,
        handlers::streams::telemetry_events_stream,
        handlers::streams::adapter_state_stream,
        handlers::streams::workers_stream,
        handlers::streaming::boot_progress_stream,
        handlers::adapter_stacks::stack_policy_stream,
        handlers::streaming::notifications_stream,
        handlers::streaming::messages_stream,
        handlers::streaming::activity_stream,
        handlers::streaming::trace_receipts_stream,
        handlers::plugins::enable_plugin,
        handlers::plugins::disable_plugin,
        handlers::plugins::plugin_status,
        handlers::plugins::list_plugins,
        handlers::orchestration::analyze_orchestration_prompt,
        handlers::orchestration::get_orchestration_metrics,
        handlers::orchestration::get_orchestration_config,
        handlers::orchestration::list_orchestration_sessions,
        handlers::orchestration::update_orchestration_config,
        handlers::settings::get_settings,
        handlers::settings::update_settings,
        handlers::golden::list_golden_runs,
        handlers::golden::get_golden_run,
        handlers::golden::golden_compare,
        handlers::promotion::request_promotion,
        handlers::promotion::get_promotion_status,
        handlers::promotion::approve_or_reject_promotion,
        handlers::promotion::record_ci_attestation,
        handlers::promotion::get_gate_status,
        handlers::promotion::rollback_promotion,
        handlers::activity::list_activity_events,
        handlers::activity::create_activity_event,
        handlers::activity::list_user_workspace_activity,
        handlers::workspaces::list_workspaces,
        handlers::workspaces::create_workspace,
        handlers::workspaces::list_user_workspaces,
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
        handlers::workspaces::get_workspace_active_state,
        handlers::workspaces::set_workspace_active_state,
        handlers::aliases::workspaces::get_workspace_active_state_alias,
        handlers::notifications::list_notifications,
        handlers::notifications::get_notification_summary,
        handlers::notifications::mark_notification_read,
        handlers::notifications::mark_all_notifications_read,
        handlers::dashboard::get_dashboard_config,
        handlers::dashboard::update_dashboard_config,
        handlers::dashboard::reset_dashboard_config,
        handlers::tutorials::list_tutorials,
        handlers::tutorials::mark_tutorial_completed,
        handlers::tutorials::unmark_tutorial_completed,
        handlers::tutorials::mark_tutorial_dismissed,
        handlers::tutorials::unmark_tutorial_dismissed,
        handlers::storage::get_storage_mode,
        handlers::storage::get_storage_stats,
        handlers::storage::get_tenant_storage_usage,
        handlers::kv_isolation::get_kv_isolation_health,
        handlers::kv_isolation::trigger_kv_isolation_scan,
        handlers::runtime::get_current_session,
        handlers::runtime::list_sessions,
        handlers::get_federation_audit,
        handlers::get_compliance_audit,
        handlers::audit::get_audit_chain,
        handlers::audit::verify_audit_chain,
        handlers::list_audits_extended,
        handlers::adapters::list_adapters,
        handlers::adapters::get_adapter,
        handlers::adapters_lifecycle::register_adapter,
        handlers::adapters::import_adapter,
        handlers::adapters::list_adapter_repositories,
        handlers::create_adapter_repository,
        handlers::adapters::get_adapter_repository,
        handlers::adapters::get_adapter_repository_policy,
        handlers::upsert_adapter_repository_policy,
        handlers::archive_adapter_repository,
        handlers::adapters::list_adapter_versions,
        handlers::rollback_adapter_version_handler,
        handlers::resolve_adapter_version_handler,
        handlers::create_draft_version,
        handlers::adapters::get_adapter_version,
        handlers::promote_adapter_version_handler,
        handlers::tag_adapter_version_handler,
        handlers::adapters_lifecycle::delete_adapter,
        handlers::adapters_lifecycle::load_adapter,
        handlers::adapters_lifecycle::unload_adapter,
        handlers::adapters::activate_adapter,
        handlers::adapters::verify_gpu_integrity,
        handlers::adapters::get_adapter_activations,
        handlers::routing_decisions::get_adapter_usage,
        handlers::promote_adapter_lifecycle,
        handlers::demote_adapter_lifecycle,
        handlers::adapters::get_adapter_lineage,
        handlers::adapters::get_adapter_detail,
        handlers::download_adapter_manifest,
        handlers::adapters::get_adapter_training_snapshot,
        handlers::adapters::export_training_provenance,
        handlers::adapters::export_adapter,
        handlers::upsert_directory_adapter,
        handlers::adapters::get_adapter_health,
        handlers::get_pin_status,
        handlers::pin_adapter,
        handlers::unpin_adapter,
        handlers::adapters::get_archive_status,
        handlers::adapters::archive_adapter,
        handlers::adapters::unarchive_adapter,
        handlers::adapters::duplicate_adapter,
        handlers::adapters::promote_adapter_state,
        handlers::adapters::swap_adapters,
        handlers::adapters::get_adapter_stats,
        handlers::adapters::list_category_policies,
        handlers::adapters::get_category_policy,
        handlers::adapters::update_category_policy,
        handlers::validate_adapter_name,
        handlers::validate_stack_name,
        handlers::get_next_revision,
        handlers::adapter_stacks::list_stacks,
        handlers::adapter_stacks::create_stack,
        handlers::adapter_stacks::get_stack,
        handlers::adapter_stacks::update_stack,
        handlers::adapter_stacks::delete_stack,
        handlers::adapter_stacks::get_stack_history,
        handlers::adapter_stacks::get_stack_policies,
        handlers::adapter_stacks::activate_stack,
        handlers::adapter_stacks::deactivate_stack,
        handlers::auth_enhanced::logout_handler,
        handlers::auth_enhanced::mfa_status_handler,
        handlers::auth_enhanced::mfa_start_handler,
        handlers::auth_enhanced::mfa_verify_handler,
        handlers::auth_enhanced::mfa_disable_handler,
        handlers::api_keys::list_api_keys,
        handlers::api_keys::create_api_key,
        handlers::api_keys::revoke_api_key,
        handlers::auth_enhanced::list_sessions_handler,
        handlers::auth_enhanced::revoke_session_handler,
        handlers::auth_enhanced::list_user_tenants_handler,
        handlers::auth_enhanced::switch_tenant_handler,
        handlers::chat_sessions::create_chat_session,
        handlers::chat_sessions::list_chat_sessions,
        handlers::create_chat_from_training_job,
        handlers::chat_sessions::list_archived_sessions,
        handlers::chat_sessions::list_deleted_sessions,
        handlers::chat_sessions::search_chat_sessions,
        handlers::chat_sessions::get_sessions_shared_with_me,
        handlers::chat_sessions::get_chat_session,
        handlers::chat_sessions::update_chat_session,
        handlers::chat_sessions::delete_chat_session,
        handlers::chat_sessions::add_chat_message,
        handlers::chat_sessions::get_chat_messages,
        handlers::chat_sessions::get_session_summary,
        handlers::chat_sessions::update_session_collection,
        handlers::chat_sessions::get_message_evidence,
        handlers::chat_sessions::get_chat_provenance,
        handlers::chat_sessions::list_chat_tags,
        handlers::chat_sessions::create_chat_tag,
        handlers::chat_sessions::update_chat_tag,
        handlers::chat_sessions::delete_chat_tag,
        handlers::chat_sessions::list_chat_categories,
        handlers::chat_sessions::create_chat_category,
        handlers::chat_sessions::update_chat_category,
        handlers::chat_sessions::delete_chat_category,
        handlers::chat_sessions::get_session_tags,
        handlers::chat_sessions::assign_tags_to_session,
        handlers::chat_sessions::remove_tag_from_session,
        handlers::chat_sessions::set_session_category,
        handlers::chat_sessions::fork_chat_session,
        handlers::chat_sessions::archive_session,
        handlers::chat_sessions::restore_session,
        handlers::chat_sessions::hard_delete_session,
        handlers::chat_sessions::get_session_shares,
        handlers::chat_sessions::share_session,
        handlers::chat_sessions::revoke_session_share,
        handlers::list_tenants,
        handlers::create_tenant,
        handlers::update_tenant,
        handlers::pause_tenant,
        handlers::archive_tenant,
        handlers::assign_tenant_policies,
        handlers::assign_tenant_adapters,
        handlers::get_tenant_usage,
        handlers::tenants::get_tenant_metrics,
        handlers::get_default_stack,
        handlers::set_default_stack,
        handlers::clear_default_stack,
        handlers::router_config::get_router_config,
        handlers::list_tenant_policy_bindings,
        handlers::toggle_tenant_policy,
        handlers::tenants::revoke_tenant_tokens,
        handlers::execution_policy::get_execution_policy,
        handlers::execution_policy::create_execution_policy,
        handlers::execution_policy::deactivate_execution_policy,
        handlers::execution_policy::get_execution_policy_history,
        handlers::event_applier::apply_tenant_event,
        handlers::list_training_jobs,
        handlers::create_training_job,
        handlers::training::get_training_queue,
        handlers::training::get_training_backend_readiness,
        handlers::get_preprocess_status,
        handlers::training::get_preprocessed_cache_count,
        handlers::training::list_preprocessed_cache,
        handlers::get_training_job,
        handlers::start_training,
        handlers::promote_version,
        handlers::cancel_training,
        handlers::retry_training,
        handlers::training::update_training_priority,
        handlers::export_coreml_training_job,
        handlers::training::create_training_session,
        handlers::training::get_training_logs,
        handlers::training::get_training_metrics,
        handlers::training::get_training_report,
        handlers::training::stream_training_progress,
        handlers::training::batch_training_status,
        handlers::get_chat_bootstrap,
        handlers::training::list_training_templates,
        handlers::training::get_training_template,
    ),
    components(schemas(
        crate::types::ErrorResponse,
        crate::types::LoginRequest,
        crate::types::LoginResponse,
        crate::types::HealthResponse,
        handlers::health::ReadyzResponse,
        handlers::health::ReadyzChecks,
        handlers::health::ReadyzCheck,
        handlers::health::InvariantStatusResponse,
        handlers::health::InvariantViolationDto,
        crate::health::ComponentHealth,
        crate::health::ComponentStatus,
        crate::health::SystemHealthResponse,
        crate::types::TenantResponse,
        crate::types::CreateTenantRequest,
        crate::types::SetDefaultStackRequest,
        crate::types::DefaultStackResponse,
        crate::types::TokenRevocationResponse,
        // Event applier types
        crate::types::ApplyEventRequest,
        crate::types::ApplyEventResponse,
        crate::types::ApplyEventsBatchRequest,
        crate::types::ApplyEventsBatchResponse,
        crate::types::EventApplicationError,
        crate::types::TenantEventType,
        handlers::adapter_stacks::StackResponse,
        handlers::adapter_stacks::CreateStackRequest,
        handlers::adapter_stacks::WorkflowType,
        handlers::adapter_stacks::LifecycleHistoryResponse,
        crate::types::ProposePatchRequest,
        crate::types::ProposePatchResponse,
        crate::types::InferRequest,
        crate::types::InferResponse,
        handlers::inference::ProvenanceResponse,
        handlers::inference::AdapterProvenanceInfo,
        handlers::inference::DocumentProvenanceInfo,
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
        // Diagnostics run types
        adapteros_api_types::diagnostics::ListDiagRunsQuery,
        adapteros_api_types::diagnostics::ListDiagEventsQuery,
        adapteros_api_types::diagnostics::DiagRunResponse,
        adapteros_api_types::diagnostics::DiagEventResponse,
        adapteros_api_types::diagnostics::ListDiagRunsResponse,
        adapteros_api_types::diagnostics::ListDiagEventsResponse,
        adapteros_api_types::diagnostics::DiagDiffRequest,
        adapteros_api_types::diagnostics::DiagDiffResponse,
        adapteros_api_types::diagnostics::DiagDiffSummary,
        adapteros_api_types::diagnostics::AnchorComparison,
        adapteros_api_types::diagnostics::FirstDivergence,
        adapteros_api_types::diagnostics::EventDiff,
        adapteros_api_types::diagnostics::TimingDiff,
        adapteros_api_types::diagnostics::RouterStepDiff,
        adapteros_api_types::diagnostics::DiagExportRequest,
        adapteros_api_types::diagnostics::DiagExportResponse,
        adapteros_api_types::diagnostics::StageTiming,
        adapteros_api_types::diagnostics::ExportMetadata,
        // Bundle export types
        adapteros_api_types::diagnostics::DiagBundleExportRequest,
        adapteros_api_types::diagnostics::DiagBundleExportResponse,
        adapteros_api_types::diagnostics::BundleManifest,
        adapteros_api_types::diagnostics::BundleFileEntry,
        adapteros_api_types::diagnostics::BundleIdentity,
        adapteros_api_types::diagnostics::ConfigSnapshot,
        adapteros_api_types::diagnostics::RouterConfigSnapshot,
        adapteros_api_types::diagnostics::BackendConfigSnapshot,
        adapteros_api_types::diagnostics::DiagBundleVerifyRequest,
        adapteros_api_types::diagnostics::DiagBundleVerifyResponse,
        adapteros_api_types::diagnostics::VerificationResult,
        handlers::capacity::CapacityResponse,
        handlers::capacity::CapacityUsage,
        handlers::capacity::NodeHealth,
        handlers::capacity::MemoryReportResponse,
        handlers::capacity::AdapterMemoryUsage,
        crate::state::CapacityLimits,
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
        // Audit chain types (UI-compatible format)
        handlers::audit::AuditChainEntry,
        handlers::audit::AuditChainResponse,
        handlers::audit::ChainVerificationResponse,
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
        crate::types::PreprocessStatusRequest,
        crate::types::PreprocessStatusResponse,
        crate::types::TrainingJobResponse,
        crate::types::TrainingMetricsResponse,
        crate::types::TrainingTemplateResponse,
        handlers::training::BatchTrainingJobStatus,
        handlers::training::BatchStatusRequest,
        handlers::training::BatchStatusResponse,
        adapteros_api_types::training::PreprocessedCacheCountResponse,
        adapteros_api_types::training::PreprocessedCacheEntry,
        adapteros_api_types::training::PreprocessedCacheListResponse,
        adapteros_api_types::training::TrainingStatus,
        adapteros_api_types::training::TrustState,
        adapteros_api_types::training::DatasetSourceType,
        adapteros_api_types::training::DatasetValidationStatus,
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
        crate::handlers::promotion::ReleaseMetadata,
        crate::handlers::promotion::PromoteResponse,
        crate::handlers::promotion::PromotionStatusResponse,
        crate::handlers::promotion::GateStatus,
        crate::handlers::promotion::ApprovalRecord,
        crate::handlers::promotion::ApproveRequest,
        crate::handlers::promotion::ApproveResponse,
        crate::handlers::promotion::RollbackRequest,
        crate::handlers::promotion::RollbackResponse,
        crate::handlers::promotion::CiAttestationRequest,
        crate::handlers::promotion::CiAttestationResponse,
        // Federation types
        crate::handlers::federation::FederationStatusResponse,
        crate::handlers::federation::QuarantineStatusResponse,
        crate::handlers::federation::FederationSyncStatusResponse,
        crate::handlers::federation::PeerSyncSummary,
        adapteros_db::federation::QuarantineDetails,
        // Chunked upload types
        handlers::datasets::InitiateChunkedUploadRequest,
        handlers::datasets::InitiateChunkedUploadResponse,
        handlers::datasets::UploadChunkQuery,
        handlers::datasets::UploadChunkResponse,
        handlers::datasets::RetryChunkQuery,
        handlers::datasets::RetryChunkResponse,
        handlers::datasets::CompleteChunkedUploadRequest,
        handlers::datasets::CompleteChunkedUploadResponse,
        handlers::datasets::UpdateDatasetSafetyRequest,
        handlers::datasets::UpdateDatasetSafetyResponse,
        handlers::datasets::TrustOverrideRequest,
        handlers::datasets::TrustOverrideResponse,
        handlers::datasets::UploadSessionStatusResponse,
        handlers::datasets::UploadSessionSummary,
        handlers::datasets::ListUploadSessionsResponse,
        // Preprocessing types (PII scrub, dedupe)
        handlers::datasets::StartPreprocessRequest,
        handlers::datasets::StartPreprocessResponse,
        handlers::datasets::PreprocessStatus,
        handlers::datasets::PreprocessStatusResponse,
        // Evidence types (PRD-DATA-01 Phase 2)
        handlers::evidence::CreateEvidenceRequest,
        handlers::evidence::EvidenceResponse,
        handlers::evidence::ListEvidenceQuery,
        // Discrepancy case types (human-in-the-loop feedback)
        handlers::discrepancies::CreateDiscrepancyRequest,
        handlers::discrepancies::DiscrepancyResponse,
        handlers::discrepancies::ListDiscrepanciesQuery,
        handlers::discrepancies::ResolveDiscrepancyRequest,
        handlers::discrepancies::DiscrepancyExportRow,
        // Activity types
        CreateActivityEventRequest,
        ActivityEventResponse,
        // Service control types
        handlers::services::ServiceControlResponse,
        handlers::services::LogsQuery,
        // Model Server types
        handlers::model_server::ModelServerStatusResponse,
        handlers::model_server::WarmupRequest,
        handlers::model_server::WarmupResponse,
        handlers::model_server::DrainRequest,
        handlers::model_server::DrainResponse,
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
        handlers::auth_enhanced::RegisterRequest,
        handlers::auth_enhanced::RegisterResponse,
        handlers::auth_enhanced::SessionInfo,
        handlers::auth_enhanced::SessionsResponse,
        handlers::auth_enhanced::AuthConfigResponse,
        // Auth API types
        adapteros_api_types::auth::SessionInfo,
        adapteros_api_types::auth::RefreshResponse,
        adapteros_api_types::auth::AuthConfigResponse,
        adapteros_api_types::auth::Role,
        // Workspace types
        handlers::workspaces::WorkspaceResponse,
        handlers::workspaces::CreateWorkspaceRequest,
        handlers::workspaces::UpdateWorkspaceRequest,
        handlers::workspaces::AddWorkspaceMemberRequest,
        handlers::workspaces::UpdateWorkspaceMemberRequest,
        handlers::workspaces::ShareResourceRequest,
        handlers::workspaces::WorkspaceActiveStateRequest,
        handlers::workspaces::WorkspaceActiveStateResponse,
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
        handlers::adapteros_receipts::AdapterosReplayRequest,
        handlers::replay::ReceiptVerificationResult,
        adapteros_api_types::inference::RunReceipt,
        // Verdict types
        handlers::verdicts::CreateVerdictRequest,
        handlers::verdicts::VerdictResponse,
        handlers::verdicts::ListVerdictsQuery,
        handlers::verdicts::DeriveVerdictRequest,
        handlers::verdicts::DeriveVerdictResponse,
        handlers::verdicts::Verdict,
        handlers::verdicts::EvaluatorType,
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
        (name = "Policy", description = "Policy quarantine and configuration management"),
        (name = "inference", description = "Model inference endpoints"),
        (name = "discrepancies", description = "Discrepancy case management for training feedback"),
        (name = "verdicts", description = "Inference verdict quality assessment"),
        (name = "promotion", description = "Golden run promotion workflow"),
        (name = "activity", description = "Activity event tracking and feeds"),
        (name = "workspaces", description = "Workspace management and resource sharing"),
        (name = "notifications", description = "User notifications and alerts"),
        (name = "dashboard", description = "Dashboard configuration and widgets"),
        (name = "tutorials", description = "Tutorial management and progress tracking"),
        (name = "cli", description = "Owner CLI command execution"),
        (name = "storage", description = "Storage mode and statistics visibility"),
        (name = "runtime", description = "Runtime session and configuration tracking"),
        (name = "Embeddings", description = "Embedding benchmark operations"),
    )
)]
pub struct ApiDoc;

#[allow(deprecated)]
pub fn build(state: AppState) -> Router {
    // Liveness/readiness endpoints must be cheap and never depend on DB/policy middleware.
    // These routes intentionally bypass the global middleware stack applied to the main API.
    // Note: When exclude-spoke-routes is enabled, /healthz and /readyz come from
    // adapteros-server-api-health spoke crate instead.
    #[cfg(not(feature = "exclude-spoke-routes"))]
    let health_routes = Router::new()
        .route("/healthz", get(handlers::health))
        .route("/readyz", get(handlers::ready))
        .route("/version", get(handlers::infrastructure::get_version))
        .with_state(state.clone());

    #[cfg(feature = "exclude-spoke-routes")]
    let health_routes = Router::new()
        .route("/version", get(handlers::infrastructure::get_version))
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
        .route("/v1/ui/config", get(handlers::ui_config::get_ui_config))
        .route(
            "/v1/auth/health",
            get(handlers::auth_enhanced::auth_health_handler),
        )
        .route(
            "/v1/auth/refresh",
            post(handlers::auth_enhanced::refresh_token_handler),
        )
        .route(
            "/v1/auth/register",
            post(handlers::auth_enhanced::register_handler),
        )
        .route("/v1/meta", get(handlers::meta))
        .route("/v1/search", get(handlers::search::global_search))
        // Client error reporting (anonymous, for pre-auth errors)
        .route(
            "/v1/telemetry/client-errors/anonymous",
            post(handlers::client_errors::report_client_error_anonymous),
        )
        .route(
            "/v1/version",
            get(|| async { axum::Json(versioning::get_version_info()) }),
        );

    // Status and invariants routes - excluded when spoke crates provide them
    #[cfg(not(feature = "exclude-spoke-routes"))]
    {
        public_routes = public_routes
            .route("/v1/status", get(handlers::get_status))
            .route(
                "/v1/invariants",
                get(handlers::health::get_invariant_status),
            );
    }

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
    // Standard /metrics path for Prometheus scraping + versioned /v1/metrics
    let metrics_route = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .route("/v1/metrics", get(handlers::metrics_handler))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            policy_enforcement_middleware,
        ));

    // Routes with optional authentication (work with or without auth)
    // These routes provide enhanced functionality when authenticated but still work anonymously
    let optional_auth_routes = Router::new()
        .route(
            "/v1/models/status",
            get(handlers::infrastructure::get_base_model_status),
        )
        .route("/v1/topology", get(handlers::topology::get_topology))
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

    // Internal routes (worker-to-control-plane communication)
    //
    // SECURITY MODEL:
    // - These routes are NOT protected by JWT auth, CSRF, or tenant guard
    // - Policy enforcement middleware is applied
    // - Worker UID validation is applied when AOS_WORKER_UID is set
    // - Authentication relies on:
    //   1. Production mode enforces UDS-only binding (no TCP)
    //   2. Plan/manifest hash validation during registration
    //   3. Worker ID existence checks for status updates
    //   4. UCred UID validation (when AOS_WORKER_UID is configured)
    //
    // TRUST BOUNDARY: These routes assume the caller is a trusted worker process
    // on the same host. Defense-in-depth is provided by:
    // - UCred validation ensures connecting process UID matches expected worker UID
    // - A compromised process with different UID cannot access these routes
    // - Even with correct UID, cannot perform inference or access user data
    //   (requires protected routes with JWT)
    //
    // To enable UCred validation, set: AOS_WORKER_UID=<expected_uid>
    let internal_routes = Router::new()
        // Worker fatal error channel (PRD-09 Phase 4)
        .route("/v1/workers/fatal", post(handlers::receive_worker_fatal))
        // PRD-01: Worker Registration & Lifecycle
        .route(
            "/v1/workers/register",
            post(handlers::workers::register_worker),
        )
        .route(
            "/v1/tenants/{tenant_id}/manifests/{manifest_hash}",
            get(handlers::worker_manifests::fetch_manifest_by_hash),
        )
        .route(
            "/v1/workers/status",
            post(handlers::workers::notify_worker_status),
        )
        .route(
            "/v1/workers/heartbeat",
            post(handlers::workers::worker_heartbeat),
        )
        .with_state(state.clone())
        // Worker UID validation (defense-in-depth, opt-in via AOS_WORKER_UID)
        .layer(middleware::from_fn(worker_uid_middleware))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            policy_enforcement_middleware,
        ));

    // Protected routes (require auth)
    let mut protected_routes = Router::new()
        // Auth routes (extracted to routes/auth_routes.rs)
        .merge(auth_routes::protected_auth_routes())
        // Admin routes
        .route("/v1/admin/users", get(handlers::admin::list_users))
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
        // Tenant routes (extracted to routes/tenant_routes.rs)
        .merge(tenant_routes::tenant_routes())
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
        // Model Server routes (shared model inference)
        .route(
            "/v1/model-server/status",
            get(handlers::model_server::get_model_server_status),
        )
        .route(
            "/v1/model-server/warmup",
            post(handlers::model_server::warmup_model_server),
        )
        .route(
            "/v1/model-server/drain",
            post(handlers::model_server::drain_model_server),
        )
        .route(
            "/v1/models",
            get(handlers::openai_compat::list_models_openai),
        )
        // Internal endpoint preserves full AdapterOS model details
        .route(
            "/internal/models",
            get(handlers::models::list_models_with_stats),
        )
        .route("/v1/models/import", post(handlers::models::import_model))
        .route(
            "/v1/models/download-progress",
            get(handlers::models::get_download_progress),
        )
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
        .route("/v1/workers", get(handlers::workers::list_workers))
        .route("/v1/workers/spawn", post(handlers::workers::worker_spawn))
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
        // Worker stop and drain routes (PRD-RECT: use state-machine-validated handlers)
        .route(
            "/v1/workers/{worker_id}/stop",
            post(handlers::workers::stop_worker),
        )
        .route(
            "/v1/workers/{worker_id}/drain",
            post(handlers::workers::drain_worker),
        )
        // Worker health & incidents (PRD-09)
        .route(
            "/v1/workers/{worker_id}/incidents",
            get(handlers::list_worker_incidents),
        )
        .route(
            "/v1/workers/health/summary",
            get(handlers::get_worker_health_summary),
        )
        // PRD-01: Worker history & detail (auth required - user facing)
        // Note: register and status routes moved to internal_routes (no auth)
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
            get(handlers::monitoring::list_monitoring_rules),
        )
        .route(
            "/v1/monitoring/rules",
            post(handlers::monitoring::create_monitoring_rule),
        )
        .route(
            "/v1/monitoring/rules/{rule_id}",
            put(handlers::monitoring::update_monitoring_rule),
        )
        .route(
            "/v1/monitoring/rules/{rule_id}",
            delete(handlers::monitoring::delete_monitoring_rule),
        )
        .route(
            "/v1/monitoring/alerts",
            get(handlers::monitoring::list_alerts),
        )
        .route(
            "/v1/monitoring/alerts/{alert_id}/acknowledge",
            post(handlers::monitoring::acknowledge_alert),
        )
        .route(
            "/v1/monitoring/alerts/{alert_id}/resolve",
            post(handlers::monitoring::resolve_alert),
        )
        .route(
            "/v1/monitoring/anomalies",
            get(handlers::monitoring::list_process_anomalies),
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
        .route(
            "/v1/code-policy",
            get(handlers::code_policy::get_code_policy),
        )
        .route(
            "/v1/code-policy",
            put(handlers::code_policy::update_code_policy),
        )
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
        .route(
            "/v1/policies/assign",
            post(handlers::tenant_policies::assign_policy),
        )
        .route(
            "/v1/policies/assignments",
            get(handlers::tenant_policies::list_policy_assignments),
        )
        .route(
            "/v1/policies/violations",
            get(handlers::tenant_policies::list_violations),
        )
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
        .route(
            "/v1/adapteros/receipts/{digest}",
            get(handlers::adapteros_receipts::get_receipt_by_digest),
        )
        .route(
            "/v1/adapteros/replay",
            post(handlers::adapteros_receipts::adapteros_replay),
        )
        .route(
            "/v1/adapteros/sessions/mint",
            post(handlers::adapteros_sessions::mint_session_token),
        )
        .route(
            "/v1/runs/{run_id}/evidence",
            get(handlers::run_evidence::download_run_evidence),
        )
        .route(
            "/v1/evidence/runs/{run_id}/export",
            get(handlers::aliases::run_evidence::download_run_evidence_alias),
        )
        .route("/v1/patch/propose", post(handlers::code::propose_patch))
        // OpenAI-compatible shim (used by OpenCode and other OpenAI clients)
        .route(
            "/v1/chat/completions",
            post(handlers::openai_compat::chat_completions),
        )
        .route(
            "/v1/completions",
            post(handlers::openai_compat::completions_openai),
        )
        .route(
            "/v1/embeddings",
            post(handlers::openai_compat::embeddings_openai),
        )
        .route("/v1/tokenize", post(handlers::tokenize::tokenize))
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
        // Review protocol routes (human-in-the-loop)
        .route(
            "/v1/infer/{inference_id}/state",
            get(handlers::review::get_inference_state),
        )
        .route(
            "/v1/infer/{inference_id}/review",
            post(handlers::review::submit_review),
        )
        .route("/v1/infer/paused", get(handlers::review::list_paused))
        // Provenance chain query (AUDIT)
        .route(
            "/v1/inference/{trace_id}/provenance",
            get(handlers::inference::get_inference_provenance),
        )
        // CLI-compatible review routes
        .route(
            "/v1/reviews/paused",
            get(handlers::review::list_paused_reviews),
        )
        .route(
            "/v1/reviews/{pause_id}",
            get(handlers::review::get_pause_details),
        )
        .route(
            "/v1/reviews/{pause_id}/context",
            get(handlers::review::export_review_context),
        )
        .route(
            "/v1/reviews/submit",
            post(handlers::review::submit_review_response),
        )
        // Inference verdict routes (quality assessment)
        .route(
            "/v1/verdicts",
            get(handlers::verdicts::list_verdicts).post(handlers::verdicts::create_verdict),
        )
        .route(
            "/v1/verdicts/derive",
            post(handlers::verdicts::derive_verdict),
        )
        .route(
            "/v1/verdicts/{inference_id}",
            get(handlers::verdicts::get_verdict),
        )
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
        // Chat routes (extracted to routes/chat_routes.rs)
        .merge(chat_routes::chat_routes())
        // Owner CLI routes (admin only)
        .route(
            "/v1/cli/owner-run",
            post(handlers::owner_cli::run_owner_cli_command),
        )
        // Adapter routes (extracted to routes/adapters.rs)
        .merge(adapters::adapter_routes())
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
            get(handlers::chat_sessions::list_contacts)
                .post(handlers::chat_sessions::create_contact),
        )
        .route(
            "/v1/contacts/{id}",
            get(handlers::chat_sessions::get_contact)
                .delete(handlers::chat_sessions::delete_contact),
        )
        .route(
            "/v1/contacts/{id}/interactions",
            get(handlers::chat_sessions::get_contact_interactions),
        )
        // SSE Streaming routes - Citation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5, §4.4
        .route(
            "/v1/streams/training",
            get(handlers::streams::training_stream),
        )
        .route(
            "/v1/streams/discovery",
            get(handlers::discovery::discovery_stream),
        )
        .route(
            "/v1/streams/contacts",
            get(handlers::discovery::contacts_stream),
        )
        // Dataset routes
        .route(
            "/v1/datasets/upload",
            post(handlers::datasets::upload_dataset),
        )
        .route(
            "/v1/datasets",
            get(handlers::datasets::list_datasets).post(handlers::datasets::upload_dataset),
        )
        .route(
            "/v1/datasets/chunked-upload/initiate",
            post(handlers::datasets::initiate_chunked_upload),
        )
        // Chunked upload routes - upload individual chunks (POST) or retry chunks (PUT)
        .route(
            "/v1/datasets/chunked-upload/{session_id}/chunk",
            post(handlers::datasets::upload_chunk).put(handlers::datasets::retry_chunk),
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
        // Chunked upload routes - list all active upload sessions
        .route(
            "/v1/datasets/chunked-upload/sessions",
            get(handlers::datasets::list_upload_sessions),
        )
        // Chunked upload routes - cleanup expired sessions (admin only)
        .route(
            "/v1/datasets/chunked-upload/cleanup",
            post(handlers::datasets::cleanup_expired_sessions),
        )
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
            "/v1/datasets/{dataset_id}/versions/{revision}",
            get(handlers::datasets::get_dataset_version),
        )
        .route(
            "/v1/datasets/by-codebase/{codebase_id}/versions",
            get(handlers::datasets::list_versions_by_codebase),
        )
        .route(
            "/v1/datasets/{dataset_id}/versions/{version_id}/trust-override",
            post(handlers::datasets::apply_dataset_version_trust_override),
        )
        .route(
            "/v1/datasets/{dataset_id}/versions/{version_id}/safety",
            post(handlers::datasets::update_dataset_version_safety),
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
        // Dataset preprocessing routes (PII scrub, dedupe)
        .route(
            "/v1/datasets/{dataset_id}/preprocess",
            post(handlers::datasets::start_preprocess),
        )
        .route(
            "/v1/datasets/{dataset_id}/preprocess/status",
            get(handlers::datasets::get_preprocess_status),
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
        // Create dataset from pasted text or extracted content
        .route(
            "/v1/datasets/from-text",
            post(handlers::datasets::create_dataset_from_text),
        )
        // Create dataset from selected chat messages
        .route(
            "/v1/datasets/from-chat",
            post(handlers::datasets::create_dataset_from_chat),
        )
        .route(
            "/v1/training/datasets/from-upload",
            post(handlers::training_datasets::create_training_dataset_from_upload),
        )
        // Generate dataset from file using local inference
        .route(
            "/v1/training/datasets/generate",
            post(handlers::datasets::generate_dataset_from_file),
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
        // Discrepancy case routes (human-in-the-loop feedback collection)
        .route(
            "/v1/discrepancies",
            get(handlers::discrepancies::list_discrepancies)
                .post(handlers::discrepancies::create_discrepancy),
        )
        .route(
            "/v1/discrepancies/export",
            get(handlers::discrepancies::export_discrepancies),
        )
        .route(
            "/v1/discrepancies/{id}",
            get(handlers::discrepancies::get_discrepancy),
        )
        .route(
            "/v1/discrepancies/{id}/resolve",
            patch(handlers::discrepancies::resolve_discrepancy),
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
        // Repository routes (deprecated - use /v1/adapter-repositories instead)
        .route("/v1/repositories", get(handlers::list_repositories_legacy))
        // System overview routes
        .route(
            "/v1/system/integrity",
            get(handlers::system::get_system_integrity),
        )
        .route(
            "/v1/system/boot-attestation",
            get(handlers::boot_attestation::get_boot_attestation),
        )
        .route(
            "/v1/system/verify-boot-attestation",
            post(handlers::boot_attestation::verify_boot_attestation),
        )
        .route(
            "/v1/system/status",
            get(handlers::system_status::get_system_status),
        )
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
        .route(
            "/v1/metrics/quality",
            get(handlers::adapters::get_quality_metrics),
        )
        .route(
            "/v1/metrics/adapters",
            get(handlers::adapters::get_adapter_metrics),
        )
        .route(
            "/v1/metrics/system",
            get(handlers::adapters::get_system_metrics),
        )
        .route(
            "/v1/metrics/code",
            post(handlers::metrics::get_code_metrics),
        )
        .route(
            "/v1/metrics/compare",
            post(handlers::metrics::compare_metrics),
        )
        .route(
            "/v1/metrics/time-series",
            get(handlers::metrics_time_series::get_metrics_time_series),
        )
        .route(
            "/v1/metrics/current",
            get(handlers::metrics_time_series::get_metrics_snapshot),
        )
        // Memory routes
        .route(
            "/v1/system/memory",
            get(handlers::system_info::get_uma_memory),
        )
        .route(
            "/v1/system/memory/gpu",
            get(handlers::capacity::get_memory_report),
        )
        // Resource usage route
        .route(
            "/v1/system/resource-usage",
            get(handlers::system_info::get_resource_usage),
        )
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
        .route(
            "/v1/commits/{sha}/diff",
            get(handlers::adapters::get_commit_diff),
        )
        // Routing routes
        .route(
            "/v1/routing-rules/identity/{identity_dataset_id}",
            get(handlers::routing_rules::list_rules),
        )
        .route(
            "/v1/routing-rules",
            post(handlers::routing_rules::create_rule),
        )
        .route(
            "/v1/routing-rules/{rule_id}",
            delete(handlers::routing_rules::delete_rule),
        )
        .route(
            "/v1/routing/debug",
            post(handlers::routing_decisions::debug_routing),
        )
        .route(
            "/v1/routing/history",
            get(handlers::routing_decisions::get_routing_history),
        )
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
        // Diagnostic runs API (tenant-safe, paginated)
        .route("/v1/diag/runs", get(handlers::diagnostics::list_diag_runs))
        .route(
            "/v1/diag/runs/{trace_id}",
            get(handlers::diagnostics::get_diag_run),
        )
        .route(
            "/v1/diag/runs/{trace_id}/events",
            get(handlers::diagnostics::list_diag_events),
        )
        .route("/v1/diag/diff", post(handlers::diagnostics::diff_diag_runs))
        .route(
            "/v1/diag/export",
            post(handlers::diagnostics::export_diag_run),
        )
        // Bundle export routes
        .route(
            "/v1/diag/bundle",
            post(handlers::diag_bundle::create_bundle_export),
        )
        .route(
            "/v1/diag/bundle/{export_id}",
            get(handlers::diag_bundle::get_bundle_export),
        )
        .route(
            "/v1/diag/bundle/{export_id}/download",
            get(handlers::diag_bundle::download_bundle),
        )
        .route(
            "/v1/diag/bundle/{export_id}/signature",
            get(handlers::diag_bundle::download_signature),
        )
        // Trace routes
        .route("/v1/traces/search", get(handlers::telemetry::search_traces))
        .route(
            "/v1/traces/inference",
            get(handlers::telemetry::list_inference_traces),
        )
        .route(
            "/v1/ui/traces/inference/{trace_id}",
            get(handlers::telemetry::get_ui_inference_trace_detail),
        )
        .route(
            "/v1/traces/inference/{trace_id}",
            get(handlers::telemetry::get_inference_trace_detail),
        )
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
        // Client error reporting and querying (authenticated)
        .route(
            "/v1/telemetry/client-errors",
            get(handlers::client_errors::list_client_errors)
                .post(handlers::client_errors::report_client_error),
        )
        .route(
            "/v1/telemetry/client-errors/stats",
            get(handlers::client_errors::get_client_error_stats),
        )
        .route(
            "/v1/telemetry/client-errors/{id}",
            get(handlers::client_errors::get_client_error),
        )
        .route(
            "/v1/stream/client-errors",
            get(handlers::client_errors::stream_client_errors),
        )
        // Error alert routes
        .route(
            "/v1/error-alerts/rules",
            get(handlers::error_alerts::list_error_alert_rules)
                .post(handlers::error_alerts::create_error_alert_rule),
        )
        .route(
            "/v1/error-alerts/rules/{id}",
            get(handlers::error_alerts::get_error_alert_rule)
                .put(handlers::error_alerts::update_error_alert_rule)
                .delete(handlers::error_alerts::delete_error_alert_rule),
        )
        .route(
            "/v1/error-alerts/history",
            get(handlers::error_alerts::list_error_alert_history),
        )
        .route(
            "/v1/error-alerts/{id}/acknowledge",
            post(handlers::error_alerts::acknowledge_error_alert),
        )
        .route(
            "/v1/error-alerts/{id}/resolve",
            post(handlers::error_alerts::resolve_error_alert),
        )
        // Embedding benchmarks routes
        .route(
            "/v1/embeddings/benchmarks",
            get(handlers::embeddings::list_embedding_benchmarks),
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
        // Training routes (extracted to routes/training_routes.rs)
        .merge(training_routes::training_routes())
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
            get(handlers::federation::get_federation_quarantine_status),
        )
        .route(
            "/v1/federation/release-quarantine",
            post(handlers::federation::release_quarantine),
        )
        .route(
            "/v1/federation/sync-status",
            get(handlers::federation::get_federation_sync_status),
        )
        // Policy quarantine routes
        .route(
            "/v1/policy/quarantine/status",
            get(handlers::quarantine::get_quarantine_status),
        )
        .route(
            "/v1/policy/quarantine/clear",
            post(handlers::quarantine::clear_policy_violations),
        )
        .route(
            "/v1/policy/quarantine/rollback",
            post(handlers::quarantine::rollback_policy_config),
        );

    // Audit endpoints: /v1/audit/logs is provided by spoke crate when exclude-spoke-routes is enabled
    #[cfg(not(feature = "exclude-spoke-routes"))]
    {
        protected_routes =
            protected_routes.route("/v1/audit/logs", get(handlers::admin::query_audit_logs));
    }

    // Policy-related audit routes stay in hub (not provided by spoke)
    protected_routes = protected_routes
        .route(
            "/v1/audit/policy-decisions",
            get(handlers::query_policy_decisions),
        )
        .route(
            "/v1/audit/policy-decisions/verify-chain",
            get(handlers::verify_policy_audit_chain),
        )
        .route("/v1/promotions/{id}", get(handlers::get_promotion))
        // SSE stream endpoints
        .route(
            "/v1/stream/metrics",
            get(handlers::streams::system_metrics_stream),
        )
        .route(
            "/v1/stream/telemetry",
            get(handlers::streams::telemetry_events_stream),
        )
        .route(
            "/v1/stream/adapters",
            get(handlers::streams::adapter_state_stream),
        )
        .route("/v1/stream/workers", get(handlers::streams::workers_stream))
        .route(
            "/v1/stream/boot-progress",
            get(handlers::streaming::boot_progress_stream),
        )
        .route(
            "/v1/stream/stack-policies/{id}",
            get(handlers::adapter_stacks::stack_policy_stream),
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
            "/v1/stream/trace-receipts",
            get(handlers::streaming::trace_receipts_stream),
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
            "/v1/orchestration/sessions",
            get(handlers::orchestration::list_orchestration_sessions),
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
            "/v1/golden/{run_id}/ci-attestation",
            post(handlers::promotion::record_ci_attestation),
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
        .route(
            "/v1/workspaces/{workspace_id}/active",
            get(handlers::workspaces::get_workspace_active_state)
                .post(handlers::workspaces::set_workspace_active_state),
        )
        .route(
            "/v1/workspaces/{workspace_id}/active-state",
            get(handlers::aliases::workspaces::get_workspace_active_state_alias),
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

    // Spoke audit routes - conditionally included when NOT using spoke crates
    // These routes require the same auth middleware as protected_routes
    #[cfg(not(feature = "exclude-spoke-routes"))]
    let spoke_audit_routes = Router::new()
        .route("/v1/audit/federation", get(handlers::get_federation_audit))
        .route("/v1/audit/compliance", get(handlers::get_compliance_audit))
        .route("/v1/audit/chain", get(handlers::audit::get_audit_chain))
        .route(
            "/v1/audit/chain/verify",
            get(handlers::audit::verify_audit_chain),
        )
        .route("/v1/audits", get(handlers::list_audits_extended))
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                ))
                .layer(middleware::from_fn(tenant_route_guard_middleware))
                .layer(middleware::from_fn(csrf_middleware))
                .layer(middleware::from_fn(context_middleware))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    policy_enforcement_middleware,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    audit_middleware,
                )),
        );

    // Testkit routes (E2E testing endpoints)
    let mut app = Router::new()
        .merge(public_routes)
        .merge(metrics_route)
        .merge(optional_auth_routes)
        .merge(internal_routes) // Worker-to-CP internal routes (no user auth)
        .merge(protected_routes);

    #[cfg(not(feature = "exclude-spoke-routes"))]
    {
        app = app.merge(spoke_audit_routes);
    }

    // Add testkit routes if E2E_MODE is enabled
    if handlers::testkit::e2e_enabled() {
        app = app.merge(handlers::testkit::register_routes().with_state(state.clone()));
    }

    // Combine routes and apply security middleware layers
    // (layers are applied in reverse order - first layer applied processes last)
    // Capture idempotency store for middleware closure
    let idempotency_store = state.idempotency_store();

    let app = app
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        // Apply layers (innermost to outermost):
        .layer(TraceLayer::new_for_http()) // Request tracing (innermost)
        .layer(ErrorCodeEnforcementLayer) // Ensure all errors have machine-readable codes
        .layer(axum::middleware::from_fn(move |req, next| {
            let store = idempotency_store.clone();
            async move { idempotency_middleware(store, req, next).await }
        })) // Idempotency for mutation requests
        .layer(cors_layer()) // CORS configuration
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limiting_middleware,
        )) // Rate limiting
        .layer(axum::middleware::from_fn(request_size_limit_middleware)) // Limit request sizes
        .layer(axum::middleware::from_fn(security_headers_middleware)) // Add security headers
        .layer(axum::middleware::from_fn(caching::caching_middleware)) // HTTP caching
        .layer(axum::middleware::from_fn(versioning::versioning_middleware)) // API versioning
        .layer(axum::middleware::from_fn(
            crate::middleware::trace_context::trace_context_middleware,
        )) // W3C Trace Context propagation
        .layer(axum::middleware::from_fn(request_id::request_id_middleware)) // Request ID tracking
        .layer(axum::middleware::from_fn(
            crate::middleware::seed_isolation::seed_isolation_middleware,
        )) // Thread-local seed isolation for determinism
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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::observability_middleware,
        )) // Logging + error envelope
        .layer(CompressionLayer::new()) // Response compression (gzip, br, deflate)
        .layer(axum::middleware::from_fn(request_id::request_id_middleware)) // Request ID tracking (outermost)
        .with_state(state.clone());

    // Health routes are merged first and don't have middleware layers.
    // API routes (app) have middleware applied. Using merge() instead of
    // fallback_service() ensures proper HTTP method routing for POST requests.
    health_routes.merge(app)
}
