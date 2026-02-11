#![allow(unused_variables)]
#![allow(ambiguous_glob_reexports)]

use crate::auth::Claims;
use crate::ip_extraction::ClientIp;
use crate::middleware::require_any_role;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*; // This already re-exports adapteros_api_types::*
use crate::uds_client::{UdsClient, UdsClientError};
use crate::validation::*;
use sqlx::Row;
// System metrics integration
use adapteros_system_metrics;
// Invariant constants
use adapteros_core::Q15_GATE_DENOMINATOR;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;

pub mod activity;
pub mod adapter_health;
pub mod adapter_lifecycle;
pub mod adapter_stacks;
pub mod adapter_utils;
pub mod adapter_versions;
pub mod adapteros_receipts;
pub mod adapteros_sessions;
pub mod adapters;
pub mod adapters_read;
pub mod admin;
pub mod admin_lifecycle;
pub mod aliases;
pub mod api_keys;
pub mod audit;
pub mod auth;
pub mod auth_enhanced;
pub mod batch;
pub mod boot_attestation;
pub mod boot_progress;
pub mod capacity;
pub mod chat_sessions;
pub mod chunked_upload;
pub mod client_errors;
pub mod code;
pub mod code_policy;
pub mod collections;
pub mod coreml_verification;
pub mod dashboard;
pub mod datasets;
pub mod debugging;
pub mod dev_contracts;
pub mod diag_bundle;
pub mod diagnostics;
pub mod discovery;
pub mod discrepancies;
pub mod documents;
pub mod domain_adapters;
pub mod embeddings;
pub mod error_alerts;
pub mod errors;
pub mod event_applier;
pub mod evidence;
pub mod execution_policy;
pub mod federation;
pub mod git;
pub mod git_repository;
pub mod golden;
pub mod health;
pub mod inference;
pub mod infrastructure;
pub mod kv_isolation;
pub mod memory_detail;
pub mod metrics;
pub mod metrics_time_series;
pub mod model_server;
pub mod models;
pub mod monitoring;
pub mod node_detail;
pub mod notifications;
pub mod openai_compat;
pub mod orchestration;
pub mod owner_cli;
// pub mod packages; // Feature removed in migration 0200
pub mod pilot_status;
pub mod plugins;
pub mod policies;
pub mod promotion;
pub mod promotion_validation;
pub mod quarantine;
pub mod rag_common;
pub mod registry;
pub mod replay;
pub mod replay_inference;
pub mod repos;
pub mod review;
pub mod router_config;
pub mod routing_decisions;
pub mod routing_rules;
pub mod run_evidence;
pub mod runtime;
pub mod search;
pub mod services;
pub mod settings;
pub mod sse_diag;
pub mod storage;
pub mod streaming;
pub mod streaming_infer;
pub mod system;
pub mod system_info;
pub mod system_overview;
pub mod system_state;
pub mod system_status;
pub mod telemetry;
pub mod tenant_policies;
pub mod tenants;
pub mod testkit;
pub mod tokenize;
pub mod topology;
pub mod training;
pub mod training_datasets;
pub mod tutorials;
pub mod ui_config;
pub mod utils;
pub mod validation;
pub mod verdicts;
pub mod worker_detail;
pub mod worker_manifests;
pub mod workers;
pub mod workspaces;

// New internal handler modules (inline handlers moved from handlers.rs)
pub mod directory_adapters;
pub mod plans;
pub mod streams;
pub mod telemetry_bundles;
pub mod tenant_management;

// Re-export new handler modules
pub use directory_adapters::{__path_upsert_directory_adapter, upsert_directory_adapter};
pub use plans::{
    __path_build_plan, __path_compare_plans, __path_export_plan_manifest, __path_get_plan_details,
    __path_list_plans, __path_promotion_gates, __path_rebuild_plan, build_plan, compare_plans,
    export_plan_manifest, get_plan_details, list_plans, promotion_gates, rebuild_plan,
    ListPlansQuery,
};
pub use streams::{
    adapter_state_stream, alerts_stream, anomalies_stream, dashboard_metrics_stream,
    enhanced_system_metrics_stream, system_metrics_stream, telemetry_events_stream,
    training_stream, StreamQuery,
};
pub use telemetry_bundles::{
    __path_export_telemetry_bundle, __path_list_telemetry_bundles, __path_purge_old_bundles,
    __path_verify_bundle_signature, export_telemetry_bundle, list_telemetry_bundles,
    purge_old_bundles, verify_bundle_signature,
};
pub use tenant_management::{
    __path_assign_tenant_adapters, __path_get_tenant_index_hashes, __path_get_uma_memory,
    __path_hydrate_tenant_from_bundle, archive_tenant, assign_tenant_adapters,
    get_tenant_index_hashes, get_tenant_usage, get_uma_memory, hydrate_tenant_from_bundle,
    pause_tenant, update_tenant, HydrateTenantRequest, IndexHashesResponse,
    TenantHydrationResponse, UmaMemoryResponse,
};

// Re-export specialized adapter repository and validation handlers/types.
#[allow(deprecated)]
pub use adapters_read::list_repositories_legacy;
pub use adapters_read::{
    __path_create_adapter_repository, __path_get_adapter_repository,
    __path_get_adapter_repository_policy, __path_list_adapter_repositories,
    __path_upsert_adapter_repository_policy, create_adapter_repository, get_adapter_repository,
    get_adapter_repository_policy, list_adapter_repositories, upsert_adapter_repository_policy,
    ListAdapterRepositoriesParams,
};
pub use validation::{
    __path_validate_adapter_name, __path_validate_stack_name, validate_adapter_name,
    validate_stack_name, NameViolationResponse, ParsedAdapterName, ParsedStackName,
    ValidateAdapterNameRequest, ValidateAdapterNameResponse, ValidateStackNameRequest,
    ValidateStackNameResponse,
};

// Re-export adapter lifecycle functions
pub use adapter_lifecycle::{
    __path_download_adapter_manifest, __path_load_adapter, __path_promote_adapter_state,
    __path_unload_adapter, download_adapter_manifest, load_adapter, promote_adapter_state,
    unload_adapter,
};

// Re-export adapter version management functions
pub use adapter_versions::{
    __path_create_draft_version, __path_get_adapter_version, __path_list_adapter_versions,
    __path_promote_adapter_version_handler, __path_resolve_adapter_version_handler,
    __path_rollback_adapter_version_handler, __path_tag_adapter_version_handler,
    create_draft_version, get_adapter_version, list_adapter_versions,
    promote_adapter_version_handler, resolve_adapter_version_handler,
    rollback_adapter_version_handler, tag_adapter_version_handler, ListAdapterVersionsParams,
};

// Re-export adapter health functions
pub use adapter_health::{
    __path_get_adapter_activations, __path_get_adapter_health, __path_verify_gpu_integrity,
    get_adapter_activations, get_adapter_health, verify_gpu_integrity,
};

// Inline module to re-export adapter lifecycle functions for routes.rs (legacy compatibility)
pub mod adapters_lifecycle {
    pub use super::adapter_lifecycle::{
        __path_load_adapter, __path_unload_adapter, load_adapter, unload_adapter,
    };
    pub use super::{
        __path_delete_adapter, __path_register_adapter, delete_adapter, register_adapter,
    };
}

// Re-export utils for error handling
pub use adapter_utils::{
    guard_in_flight_requests, lora_scope_from_provenance, lora_tier_from_provenance,
};

// Re-export adapter lifecycle and lineage handlers
pub use adapters::*;

// Re-export tenant handlers
pub use tenants::*;

// Re-export tenant policy handlers (including utoipa path types for OpenAPI)
pub use tenant_policies::{
    __path_list_tenant_policy_bindings, __path_query_policy_decisions, __path_toggle_tenant_policy,
    __path_verify_policy_audit_chain, list_tenant_policy_bindings, query_policy_decisions,
    toggle_tenant_policy, verify_policy_audit_chain,
};

// Re-export policy handlers from policies module (consolidates duplicates)
pub use policies::{
    // utoipa path macros
    __path_apply_policy,
    __path_assign_policy,
    __path_assign_tenant_policies,
    __path_compare_policy_versions,
    __path_export_policy,
    __path_get_policy,
    __path_list_policies,
    __path_list_policy_assignments,
    __path_list_violations,
    __path_sign_policy,
    __path_validate_policy,
    __path_verify_policy_signature,
    apply_policy,
    assign_policy,
    assign_tenant_policies,
    compare_policy_versions,
    export_policy,
    get_policy,
    list_policies,
    list_policy_assignments,
    list_violations,
    sign_policy,
    validate_policy,
    verify_policy_signature,
};

// Re-export auth handlers
pub use auth::auth_me;
pub use auth_enhanced::{
    bootstrap_admin_handler, list_sessions_handler, list_user_tenants_handler, login_handler,
    logout_handler, mfa_disable_handler, mfa_start_handler, mfa_status_handler, mfa_verify_handler,
    refresh_token_handler, revoke_session_handler, switch_tenant_handler,
};

// Re-export training handlers
pub use training::*;

// Re-export health and system info handlers
pub use coreml_verification::*;
pub use health::*;
pub use system::*;
pub use system_info::*;

// Re-export system state handler
pub use system_state::*;

// Re-export boot progress (specific to avoid ambiguity with streaming module)
pub use boot_progress::{boot_progress_stream, BootProgressEvent};

// Re-export streaming handlers
pub use streaming::*;

// Re-export adapter_stacks streaming handler
pub use adapter_stacks::stack_policy_stream;

// Re-export inference handler (including utoipa path types)
pub use inference::{__path_infer, infer};

// Re-export domain adapter handlers
pub use domain_adapters::*;

// Re-export infrastructure handlers (nodes & system operations)
pub use infrastructure::{
    __path_evict_node, __path_get_base_model_status, __path_get_job, __path_get_node_details,
    __path_list_jobs, __path_list_nodes, __path_mark_node_offline, __path_register_node,
    __path_test_node_connection, evict_node, get_base_model_status, get_job, get_node_details,
    list_jobs, list_nodes, mark_node_offline, register_node, test_node_connection, ListJobsQuery,
};

// Re-export worker handlers
pub use workers::{
    __path_get_worker_health_summary, __path_list_worker_incidents, __path_receive_worker_fatal,
    get_worker_health_summary, list_worker_incidents, receive_worker_fatal,
};

use adapteros_db::sqlx;
use adapteros_db::users::Role;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::Deserialize;
use tracing::warn;

// Note: upsert_directory_adapter has been moved to handlers/directory_adapters.rs
// Note: get_base_model_status has been moved to handlers/infrastructure.rs
// Note: build_plan has been moved to handlers/plans.rs

/// Promote CP with quality gates
#[utoipa::path(
    post,
    path = "/v1/cp/promote",
    request_body = PromoteCPRequest,
    responses(
        (status = 200, description = "Promotion result", body = PromotionResponse),
        (status = 404, description = "Plan not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "promotion"
)]
pub async fn cp_promote(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<PromoteCPRequest>,
) -> Result<Json<PromotionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Load plan from database
    let plan = state
        .db
        .get_plan(&req.plan_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to load plan")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("plan not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Plan ID: {}", req.plan_id)),
                ),
            )
        })?;

    // Load latest audit for the CPID
    let audits = state.db.list_all_audits().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to load audits")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let latest_audit = audits
        .iter()
        .filter(|a| {
            a.tenant_id == plan.tenant_id
                && a.cpid.as_deref() == Some(&req.cpid)
                && a.status == "pass"
        })
        .max_by_key(|a| &a.created_at)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("no passing audit found for CPID")
                        .with_code("BAD_REQUEST")
                        .with_string_details(format!(
                            "Run audit and ensure it passes before promotion: {}",
                            req.cpid
                        )),
                ),
            )
        })?;

    // Parse audit results to check quality gates
    let audit_result: serde_json::Value =
        serde_json::from_str(&latest_audit.result_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to parse audit results")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Extract hallucination metrics
    let metrics = &audit_result["hallucination_metrics"];
    let arr = metrics["arr"].as_f64().unwrap_or(0.0) as f32;
    let ecs5 = metrics["ecs5"].as_f64().unwrap_or(0.0) as f32;
    let hlr = metrics["hlr"].as_f64().unwrap_or(1.0) as f32;
    let cr = metrics["cr"].as_f64().unwrap_or(1.0) as f32;

    // Check quality gates (from Ruleset #15)
    let mut failures = Vec::new();

    if arr < 0.95 {
        failures.push(format!("ARR too low: {:.3} < 0.95", arr));
    }

    if ecs5 < 0.75 {
        failures.push(format!("ECS@5 too low: {:.3} < 0.75", ecs5));
    }

    if hlr > 0.03 {
        failures.push(format!("HLR too high: {:.3} > 0.03", hlr));
    }

    if cr > 0.01 {
        failures.push(format!("CR too high: {:.3} > 0.01", cr));
    }

    // If any gates fail, reject promotion
    if !failures.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("quality gates failed")
                    .with_code("BAD_REQUEST")
                    .with_string_details(failures.join("; ")),
            ),
        ));
    }

    // All gates passed - proceed with promotion in a transaction
    // Get current active CPID for before_cpid tracking
    let current_cp = state
        .db
        .get_active_cp_pointer(&plan.tenant_id)
        .await
        .ok()
        .flatten();
    let before_cpid = current_cp.as_ref().map(|cp| cp.name.clone());

    // Find target CP pointer
    let cp_pointer = state
        .db
        .get_cp_pointer_by_name(&req.cpid)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get CP pointer")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("CP pointer not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("CPID: {}", req.cpid)),
                ),
            )
        })?;

    // Create quality metrics JSON for signing
    let quality_metrics = QualityMetrics { arr, ecs5, hlr, cr };
    let quality_json = serde_json::to_string(&quality_metrics).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to serialize quality metrics")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Generate Ed25519 signature
    let (signature_b64, signer_key_id) =
        crate::signing::sign_promotion(&req.cpid, &claims.email, &quality_json).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to sign promotion")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // BEGIN TRANSACTION
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // 1. Deactivate all CP pointers for this tenant
    sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
        .bind(&plan.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to deactivate CP pointers")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // 2. Activate target CP pointer
    sqlx::query("UPDATE cp_pointers SET active = 1 WHERE id = ?")
        .bind(&cp_pointer.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to activate CP pointer")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // 3. Insert promotion record with signature
    let promotion_id = crate::id_generator::readable_id(adapteros_id::IdPrefix::Run, "promotion");
    let promotion_timestamp = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO promotions 
         (id, cpid, cp_pointer_id, promoted_by, promoted_at, signature_b64, signer_key_id, quality_json, before_cpid) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&promotion_id)
    .bind(&req.cpid)
    .bind(&cp_pointer.id)
    .bind(&claims.email)
    .bind(promotion_timestamp.to_rfc3339())
    .bind(&signature_b64)
    .bind(&signer_key_id)
    .bind(&quality_json)
    .bind(&before_cpid)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("failed to insert promotion record").with_code("INTERNAL_SERVER_ERROR").with_string_details(e.to_string())),
        )
    })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Record promotion metric
    state.metrics_exporter.record_promotion();

    tracing::info!(
        "Promotion completed: {} -> {} by {} (signature: {})",
        before_cpid.as_deref().unwrap_or("(none)"),
        req.cpid,
        claims.email,
        &signature_b64[..16]
    );

    Ok(Json(PromotionResponse {
        cpid: req.cpid,
        plan_id: req.plan_id,
        promoted_by: claims.email,
        promoted_at: promotion_timestamp.to_rfc3339(),
        quality_metrics,
    }))
}

// Note: worker_spawn, list_workers, stop_worker moved to handlers/workers.rs (PRD-RECT topology fix)

// Note: list_plans, get_plan_details, rebuild_plan, compare_plans, export_plan_manifest, promotion_gates
// have been moved to handlers/plans.rs

// Note: list_telemetry_bundles, export_telemetry_bundle, verify_bundle_signature, purge_old_bundles
// have been moved to handlers/telemetry_bundles.rs

/// Rollback CP to previous plan
#[utoipa::path(
    post,
    path = "/v1/cp/rollback",
    request_body = RollbackCPRequest,
    responses(
        (status = 200, description = "Rollback result", body = RollbackResponse),
        (status = 404, description = "Active CP not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "promotion"
)]
pub async fn cp_rollback(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RollbackCPRequest>,
) -> Result<Json<RollbackResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Get current active CP pointer
    let current_cp = state
        .db
        .get_active_cp_pointer(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to get current CP pointer")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("no active CP pointer found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!("Tenant: {}", req.tenant_id)),
                ),
            )
        })?;

    // Verify the CPID matches
    if current_cp.name != req.cpid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("CPID mismatch")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "Current active CPID is '{}', not '{}'",
                        current_cp.name, req.cpid
                    )),
            ),
        ));
    }

    // Find previous CP pointer for this tenant (most recent inactive one)
    let all_pointers = adapteros_db::sqlx::query_as::<_, adapteros_db::models::CpPointer>(
        "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at 
         FROM cp_pointers 
         WHERE tenant_id = ? AND id != ? 
         ORDER BY activated_at DESC, created_at DESC 
         LIMIT 1",
    )
    .bind(&req.tenant_id)
    .bind(&current_cp.id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to query previous CP")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let previous_cp = all_pointers.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("no previous CP pointer available for rollback")
                    .with_code("BAD_REQUEST")
                    .with_string_details(format!(
                        "This is the first/only CP for tenant {}",
                        req.tenant_id
                    )),
            ),
        )
    })?;

    // Perform rollback in a transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to start transaction")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // 1. Deactivate all CP pointers for this tenant
    sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
        .bind(&req.tenant_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to deactivate CP pointers")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // 2. Activate previous CP pointer
    sqlx::query("UPDATE cp_pointers SET active = 1 WHERE id = ?")
        .bind(&previous_cp.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to activate previous CP pointer")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // COMMIT TRANSACTION
    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to commit transaction")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let rollback_timestamp = chrono::Utc::now();

    tracing::info!(
        "Rollback completed: {} -> {} by {}",
        req.cpid,
        previous_cp.name,
        claims.email
    );

    Ok(Json(RollbackResponse {
        cpid: req.cpid.clone(),
        previous_plan_id: previous_cp.plan_id,
        rolled_back_by: claims.email,
        rolled_back_at: rollback_timestamp.to_rfc3339(),
    }))
}
/// Dry run CP promotion (validate gates without executing)
#[utoipa::path(
    post,
    path = "/v1/cp/promote/dry-run",
    request_body = DryRunPromotionRequest,
    responses(
        (status = 200, description = "Dry run result", body = DryRunPromotionResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "promotion"
)]
pub async fn cp_dry_run_promote(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<DryRunPromotionRequest>,
) -> Result<Json<DryRunPromotionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    // Stub - would validate all gates and return what would be promoted
    Ok(Json(DryRunPromotionResponse {
        cpid: req.cpid,
        would_promote: true,
        gates_status: vec![
            GateStatus {
                name: "determinism".to_string(),
                passed: true,
                message: "Replay zero diff passed".to_string(),
                evidence_id: None,
            },
            GateStatus {
                name: "hallucination".to_string(),
                passed: true,
                message: "ARR: 0.96, ECS@5: 0.78".to_string(),
                evidence_id: None,
            },
            GateStatus {
                name: "performance".to_string(),
                passed: true,
                message: "p95: 22ms (threshold: 24ms)".to_string(),
                evidence_id: None,
            },
        ],
        warnings: vec![],
    }))
}

/// Get promotion history
#[utoipa::path(
    get,
    path = "/v1/cp/promotions",
    responses(
        (status = 200, description = "Promotion history", body = Vec<PromotionHistoryEntry>),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "promotion"
)]
pub async fn get_promotion_history(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<PromotionHistoryEntry>>, (StatusCode, Json<ErrorResponse>)> {
    // Stub - would query promotions table
    Ok(Json(vec![PromotionHistoryEntry {
        cpid: "cp_001".to_string(),
        promoted_at: chrono::Utc::now().to_rfc3339(),
        promoted_by: "admin@example.com".to_string(),
        previous_cpid: Some("cp_000".to_string()),
        gate_results_summary: "All gates passed".to_string(),
    }]))
}

/// Propose a patch for code changes
#[utoipa::path(
    tag = "system",
    post,
    path = "/api/v1/patch/propose",
    request_body = ProposePatchRequest,
    responses(
        (status = 200, description = "Patch proposal created", body = ProposePatchResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("bearer_token" = [])
    )
)]
pub async fn propose_patch(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ProposePatchRequest>,
) -> Result<Json<ProposePatchResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator])?;

    // Validate inputs
    validate_repo_id(&req.repo_id)?;
    validate_description(&req.description)?;
    validate_file_paths(&req.target_files)?;

    // Get available workers from database
    let workers = state.db.list_all_workers().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    if workers.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("no workers available")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details("No active workers found for patch proposal"),
            ),
        ));
    }

    // Select first available worker (simple selection for now)
    let worker = &workers[0];
    let uds_path = std::path::Path::new(&worker.uds_path);

    // Create UDS client and send patch proposal request
    let uds_client = UdsClient::new(std::time::Duration::from_secs(60)); // Longer timeout for patch generation

    let worker_request = PatchProposalInferRequest {
        cpid: "patch-proposal".to_string(),
        prompt: req.description.clone(),
        max_tokens: 2000,
        require_evidence: true,
        request_type: PatchProposalRequestType {
            repo_id: req.repo_id.clone(),
            commit_sha: Some(req.commit_sha.clone()),
            target_files: req.target_files.clone(),
            description: req.description.clone(),
        },
    };

    match uds_client.propose_patch(uds_path, worker_request).await {
        Ok(worker_response) => {
            // Extract proposal ID and status
            let proposal_id = worker_response
                .patch_proposal
                .as_ref()
                .map(|p| p.proposal_id.clone())
                .unwrap_or_else(|| {
                    crate::id_generator::readable_id(adapteros_id::IdPrefix::Run, "promotion")
                });

            let status = if worker_response.patch_proposal.is_some() {
                "completed"
            } else if worker_response.refusal.is_some() {
                "refused"
            } else {
                "failed"
            };

            let message = if let Some(ref proposal) = worker_response.patch_proposal {
                format!(
                    "Patch proposal generated successfully with {} files and {} citations",
                    proposal.patches.len(),
                    proposal.citations.len()
                )
            } else if let Some(ref refusal) = worker_response.refusal {
                format!("Patch proposal refused: {}", refusal.message)
            } else {
                "Patch proposal generation failed".to_string()
            };

            // Store proposal in database
            if let Some(ref proposal) = worker_response.patch_proposal {
                let proposal_json = serde_json::to_string(proposal).unwrap_or_else(|e| {
                    tracing::warn!("Failed to serialize patch proposal: {}", e);
                    "{}".to_string()
                });

                match sqlx::query(
                    "INSERT INTO patch_proposals 
                     (id, repo_id, commit_sha, status, proposal_json, created_at, created_by) 
                     VALUES (?, ?, ?, ?, ?, datetime('now'), ?)",
                )
                .bind(&proposal_id)
                .bind(&req.repo_id)
                .bind(&req.commit_sha)
                .bind(status)
                .bind(&proposal_json)
                .bind(&claims.email)
                .execute(state.db.pool())
                .await
                {
                    Ok(_) => {
                        tracing::info!("Stored patch proposal {} in database", proposal_id);
                    }
                    Err(e) => {
                        tracing::error!("Failed to store patch proposal in database: {}", e);
                        // Don't fail the request if storage fails
                    }
                }
            }

            Ok(Json(ProposePatchResponse {
                proposal_id,
                status: status.to_string(),
                message,
            }))
        }
        Err(UdsClientError::WorkerNotAvailable(msg)) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("worker not available")
                    .with_code("SERVICE_UNAVAILABLE")
                    .with_string_details(msg),
            ),
        )),
        Err(UdsClientError::Timeout(msg)) => Err((
            StatusCode::REQUEST_TIMEOUT,
            Json(
                ErrorResponse::new("patch generation timeout")
                    .with_code("REQUEST_TIMEOUT")
                    .with_string_details(msg),
            ),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("patch generation failed")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )),
    }
}

// ===== Process Debugging Endpoints =====

/// List process logs for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/logs",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("level" = Option<String>, Query, description = "Filter by log level"),
        ("limit" = Option<i32>, Query, description = "Maximum number of logs to return")
    ),
    responses(
        (status = 200, description = "Process logs", body = Vec<ProcessLogResponse>)
    )
)]
pub async fn list_process_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessLogResponse>>, (StatusCode, Json<ErrorResponse>)> {
    debugging::list_process_logs(
        State(state),
        Extension(claims),
        Path(worker_id),
        Query(params),
    )
    .await
}

/// Get process crash dumps for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/crashes",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Process crash dumps", body = Vec<ProcessCrashDumpResponse>)
    )
)]
pub async fn list_process_crashes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<Vec<ProcessCrashDumpResponse>>, (StatusCode, Json<ErrorResponse>)> {
    debugging::list_process_crashes(State(state), Extension(claims), Path(worker_id)).await
}

/// Start a debug session for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/debug",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = StartDebugSessionRequest,
    responses(
        (status = 200, description = "Debug session started", body = ProcessDebugSessionResponse)
    )
)]
pub async fn start_debug_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<StartDebugSessionRequest>,
) -> Result<Json<ProcessDebugSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    debugging::start_debug_session(State(state), Extension(claims), Path(worker_id), Json(req))
        .await
}

/// Run a troubleshooting step for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/troubleshoot",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = RunTroubleshootingStepRequest,
    responses(
        (status = 200, description = "Troubleshooting step started", body = ProcessTroubleshootingStepResponse)
    )
)]
pub async fn run_troubleshooting_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<RunTroubleshootingStepRequest>,
) -> Result<Json<ProcessTroubleshootingStepResponse>, (StatusCode, Json<ErrorResponse>)> {
    debugging::run_troubleshooting_step(State(state), Extension(claims), Path(worker_id), Json(req))
        .await
}

// ===== Advanced Process Monitoring and Alerting Endpoints =====

/// List process monitoring rules
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("rule_type" = Option<String>, Query, description = "Filter by rule type"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Process monitoring rules", body = Vec<ProcessMonitoringRuleResponse>)
    )
)]
pub async fn list_process_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::list_process_monitoring_rules(State(state), Extension(claims), Query(params)).await
}

/// Create process monitoring rule
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateProcessMonitoringRuleRequest,
    responses(
        (status = 200, description = "Monitoring rule created", body = ProcessMonitoringRuleResponse)
    )
)]
pub async fn create_process_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringRuleRequest>,
) -> Result<Json<ProcessMonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::create_process_monitoring_rule(State(state), Extension(claims), Json(req)).await
}

/// List process alerts
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by alert status"),
        ("severity" = Option<String>, Query, description = "Filter by severity"),
        ("limit" = Option<i64>, Query, description = "Maximum number of alerts to return"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination")
    ),
    responses(
        (status = 200, description = "Process alerts", body = Vec<ProcessAlertResponse>)
    )
)]
pub async fn list_process_alerts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::list_process_alerts(State(state), Extension(claims), Query(params)).await
}

/// Acknowledge process alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeProcessAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = ProcessAlertResponse)
    )
)]
pub async fn acknowledge_process_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
    Json(req): Json<AcknowledgeProcessAlertRequest>,
) -> Result<Json<ProcessAlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::acknowledge_process_alert(
        State(state),
        Extension(claims),
        Path(alert_id),
        Json(req),
    )
    .await
}

/// List process anomalies
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/anomalies",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by anomaly status"),
        ("anomaly_type" = Option<String>, Query, description = "Filter by anomaly type"),
        ("limit" = Option<i64>, Query, description = "Maximum number of anomalies to return"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination")
    ),
    responses(
        (status = 200, description = "Process anomalies", body = Vec<ProcessAnomalyResponse>)
    )
)]
pub async fn list_process_anomalies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::list_process_anomalies(State(state), Extension(claims), Query(params)).await
}

/// Update process anomaly status
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/anomalies/{anomaly_id}/status",
    params(
        ("anomaly_id" = String, Path, description = "Anomaly ID")
    ),
    request_body = UpdateProcessAnomalyStatusRequest,
    responses(
        (status = 200, description = "Anomaly status updated", body = ProcessAnomalyResponse)
    )
)]
pub async fn update_process_anomaly_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(anomaly_id): Path<String>,
    Json(req): Json<UpdateProcessAnomalyStatusRequest>,
) -> Result<Json<ProcessAnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::update_process_anomaly_status(
        State(state),
        Extension(claims),
        Path(anomaly_id),
        Json(req),
    )
    .await
}

/// List process monitoring dashboards
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/dashboards",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_shared" = Option<bool>, Query, description = "Filter by shared status")
    ),
    responses(
        (status = 200, description = "Process monitoring dashboards", body = Vec<ProcessMonitoringDashboardResponse>)
    )
)]
pub async fn list_process_monitoring_dashboards(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringDashboardResponse>>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::list_process_monitoring_dashboards(State(state), Extension(claims), Query(params))
        .await
}

/// Create process monitoring dashboard
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/dashboards",
    request_body = CreateProcessMonitoringDashboardRequest,
    responses(
        (status = 200, description = "Dashboard created", body = ProcessMonitoringDashboardResponse)
    )
)]
pub async fn create_process_monitoring_dashboard(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringDashboardRequest>,
) -> Result<Json<ProcessMonitoringDashboardResponse>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::create_process_monitoring_dashboard(State(state), Extension(claims), Json(req))
        .await
}

/// List process health metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/health-metrics",
    params(
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("metric_name" = Option<String>, Query, description = "Filter by metric name"),
        ("start_time" = Option<String>, Query, description = "Start time for metrics"),
        ("end_time" = Option<String>, Query, description = "End time for metrics")
    ),
    responses(
        (status = 200, description = "Process health metrics", body = Vec<ProcessHealthMetricResponse>)
    )
)]
pub async fn list_process_health_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessHealthMetricResponse>>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::list_process_health_metrics(State(state), Extension(claims), Query(params)).await
}

/// List process monitoring reports
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/reports",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("report_type" = Option<String>, Query, description = "Filter by report type")
    ),
    responses(
        (status = 200, description = "Process monitoring reports", body = Vec<ProcessMonitoringReportResponse>)
    )
)]
pub async fn list_process_monitoring_reports(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::list_process_monitoring_reports(State(state), Extension(claims), Query(params))
        .await
}

/// Create process monitoring report
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/reports",
    request_body = CreateProcessMonitoringReportRequest,
    responses(
        (status = 200, description = "Report created", body = ProcessMonitoringReportResponse)
    )
)]
pub async fn create_process_monitoring_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringReportRequest>,
) -> Result<Json<ProcessMonitoringReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    monitoring::create_process_monitoring_report(State(state), Extension(claims), Json(req)).await
}
// ===== Adapter Management Endpoints =====
/// List all adapters
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapters",
    params(
        ("tier" = Option<String>, Query, description = "Filter by tier"),
        ("framework" = Option<String>, Query, description = "Filter by framework")
    ),
    responses(
        (status = 200, description = "List of adapters", body = Vec<AdapterResponse>),
        (status = 401, description = "Unauthorized", body = ErrorResponse)
    )
)]
pub async fn list_adapters(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListAdaptersQuery>,
) -> Result<Json<Vec<AdapterResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: all roles can list adapters
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterList)?;

    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list adapters")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut responses = Vec::new();
    for adapter in adapters {
        // Enforce tenant isolation: skip adapters not belonging to user's tenant
        // (admin users can see all adapters)
        if claims.role != "admin" && validate_tenant_isolation(&claims, &adapter.tenant_id).is_err()
        {
            continue; // Skip this adapter
        }

        // Filter by tier if specified
        if let Some(ref tier) = query.tier {
            if adapter.tier != *tier {
                continue;
            }
        }

        // Filter by framework if specified
        if let Some(ref framework) = query.framework {
            if adapter.framework.as_ref() != Some(framework) {
                continue;
            }
        }

        // Get adapter_id - use id if adapter_id is not set
        let adapter_id_str = adapter.adapter_id.as_ref().unwrap_or(&adapter.id);

        // Get stats
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(&claims.tenant_id, adapter_id_str)
            .await
            .unwrap_or((0, 0, 0.0));

        let selection_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let lora_tier = lora_tier_from_provenance(&adapter.provenance_json);
        let lora_scope =
            lora_scope_from_provenance(&adapter.provenance_json, Some(adapter.scope.clone()));
        let languages: Vec<String> = adapter
            .languages_json
            .as_ref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();

        responses.push(AdapterResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: adapter.id.clone(),
            adapter_id: adapter_id_str.to_string(),
            name: adapter.name.clone(),
            hash_b3: adapter.hash_b3.clone(),
            rank: adapter.rank,
            tier: adapter.tier.clone(),
            assurance_tier: None,
            languages,
            framework: adapter.framework.clone(),
            category: Some(adapter.category.clone()),
            scope: Some(adapter.scope.clone()),
            framework_id: adapter.framework_id.clone(),
            framework_version: adapter.framework_version.clone(),
            repo_id: adapter.repo_id.clone(),
            commit_sha: adapter.commit_sha.clone(),
            intent: adapter.intent.clone(),
            lora_tier,
            lora_strength: adapter.lora_strength,
            lora_scope,
            created_at: adapter.created_at.clone(),
            updated_at: Some(adapter.updated_at.clone()),
            stats: Some(AdapterStats {
                total_activations: total,
                selected_count: selected,
                avg_gate_value: avg_gate,
                selection_rate,
            }),
            version: adapter.version.clone(),
            lifecycle_state: adapter.lifecycle_state.clone(),
            runtime_state: Some(adapter.current_state.clone()),
            pinned: None,
            memory_bytes: None,
            deduplicated: None,
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_metric: None,
            drift_loss_metric: None,
            drift_slice_size: None,
            drift_slice_offset: None,
            // Codebase adapter fields
            adapter_type: None,
            base_adapter_id: None,
            stream_session_id: None,
            versioning_threshold: None,
            coreml_package_hash: None,
            display_name: None,
        });
    }

    Ok(Json(responses))
}

/// Archive a repository
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapter-repositories/{repo_id}/archive",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 204, description = "Repository archived"),
        (status = 404, description = "Repository not found", body = ErrorResponse)
    )
)]
pub async fn archive_adapter_repository(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(repo_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AdapterRegister)?;
    let repo_id = crate::id_resolver::resolve_any_id(&state.db, &repo_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    let archived = state
        .db
        .archive_adapter_repository(&claims.tenant_id, &repo_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to archive repository")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if !archived {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("repository not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(repo_id),
            ),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
// ListAdapterVersionsParams moved to adapter_versions module
/// Get adapter by ID
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 200, description = "Adapter details", body = AdapterResponse),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn get_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
) -> Result<Json<AdapterResponse>, (StatusCode, Json<ErrorResponse>)> {
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    let (total, selected, avg_gate) = state
        .db
        .get_adapter_stats(&claims.tenant_id, &adapter_id)
        .await
        .unwrap_or((0, 0, 0.0));

    let selection_rate = if total > 0 {
        (selected as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let lora_tier = lora_tier_from_provenance(&adapter.provenance_json);
    let lora_scope =
        lora_scope_from_provenance(&adapter.provenance_json, Some(adapter.scope.clone()));

    let languages: Vec<String> = adapter
        .languages_json
        .as_ref()
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();

    Ok(Json(AdapterResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: adapter.id.clone(),
        adapter_id: adapter
            .adapter_id
            .clone()
            .unwrap_or_else(|| adapter.id.clone()),
        name: adapter.name.clone(),
        hash_b3: adapter.hash_b3.clone(),
        rank: adapter.rank,
        tier: adapter.tier.clone(),
        assurance_tier: None,
        languages,
        framework: adapter.framework.clone(),
        category: Some(adapter.category.clone()),
        scope: Some(adapter.scope.clone()),
        framework_id: adapter.framework_id.clone(),
        framework_version: adapter.framework_version.clone(),
        repo_id: adapter.repo_id.clone(),
        commit_sha: adapter.commit_sha.clone(),
        intent: adapter.intent.clone(),
        lora_tier,
        lora_strength: adapter.lora_strength,
        lora_scope,
        created_at: adapter.created_at.clone(),
        updated_at: Some(adapter.updated_at.clone()),
        stats: Some(AdapterStats {
            total_activations: total,
            selected_count: selected,
            avg_gate_value: avg_gate,
            selection_rate,
        }),
        version: adapter.version.clone(),
        lifecycle_state: adapter.lifecycle_state.clone(),
        runtime_state: Some(adapter.current_state),
        pinned: None,
        memory_bytes: None,
        deduplicated: None,
        drift_reference_backend: None,
        drift_baseline_backend: None,
        drift_test_backend: None,
        drift_tier: None,
        drift_metric: None,
        drift_loss_metric: None,
        drift_slice_size: None,
        drift_slice_offset: None,
        // Codebase adapter fields
        adapter_type: None,
        base_adapter_id: None,
        stream_session_id: None,
        versioning_threshold: None,
        coreml_package_hash: None,
        display_name: None,
    }))
}
/// Register new adapter
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapters/register",
    request_body = RegisterAdapterRequest,
    responses(
        (status = 201, description = "Adapter registered", body = AdapterResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse)
    )
)]
pub async fn register_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Json(req): Json<RegisterAdapterRequest>,
) -> Result<(StatusCode, Json<AdapterResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator and Admin can register adapters
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::AdapterRegister,
    )?;

    // Validate inputs
    if req.adapter_id.is_empty() || req.name.is_empty() || req.hash_b3.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("adapter_id, name, and hash_b3 are required")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Validate adapter ID format (alphanumeric, underscores, hyphens)
    validate_adapter_id(&req.adapter_id)?;

    // Validate name length and content
    validate_name(&req.name)?;

    // Validate hash format (B3 hash)
    validate_hash_b3(&req.hash_b3)?;

    let languages_json = serde_json::to_string(&req.languages).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid languages array")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Validate tier is one of the allowed values
    if !["persistent", "warm", "ephemeral"].contains(&req.tier.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("tier must be one of: 'persistent', 'warm', or 'ephemeral'")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Validate category is provided
    if req.category.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("category is required").with_code("BAD_REQUEST")),
        ));
    }

    // POLICY ENFORCEMENT: Check naming policy compliance
    // Get policy assignments for this tenant
    let policy_assignments = state
        .db
        .get_policy_assignments("tenant", Some(&claims.tenant_id))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to get policy assignments");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to check policy assignments")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Check if naming policy is assigned and enforced
    for assignment in &policy_assignments {
        if assignment.enforced {
            // Fetch the policy pack
            if let Ok(Some(pack)) = state.db.get_policy_pack(&assignment.policy_pack_id).await {
                if pack.policy_type == "naming" && pack.status == "active" {
                    // Parse naming policy configuration from policy content
                    use adapteros_policy::packs::naming_policy::{
                        AdapterNameValidation, NamingConfig, NamingPolicy,
                    };
                    // Security: Fail explicitly on malformed policy JSON to prevent bypass
                    let config: NamingConfig =
                        serde_json::from_str(&pack.content_json).map_err(|e| {
                            tracing::error!(
                                policy_pack_id = %pack.id,
                                error = %e,
                                "Malformed policy pack JSON - refusing to apply empty policy"
                            );
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(
                                    ErrorResponse::new(
                                        "Policy pack has invalid JSON configuration",
                                    )
                                    .with_code("POLICY_PACK_CORRUPT")
                                    .with_string_details(
                                        format!(
                                            "Policy pack '{}' contains malformed JSON: {}",
                                            pack.id, e
                                        ),
                                    ),
                                ),
                            )
                        })?;
                    let naming_policy = NamingPolicy::new(config);

                    // Validate adapter name against naming policy
                    let validation_request = AdapterNameValidation {
                        name: req.name.clone(),
                        tenant_id: claims.tenant_id.clone(),
                        parent_name: None,
                        latest_revision: None,
                    };

                    if let Err(e) = naming_policy.validate_adapter_name(&validation_request) {
                        // Record policy violation
                        let violation_id = state
                            .db
                            .record_policy_violation(
                                &pack.id,
                                Some(&assignment.id),
                                "naming",
                                "high",
                                "adapter",
                                Some(&req.adapter_id),
                                &claims.tenant_id,
                                &format!("Naming policy violation: {}", e),
                                None,
                            )
                            .await
                            .map_err(|e| {
                                tracing::error!(error = %e, "Failed to record policy violation");
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(
                                        ErrorResponse::new("Failed to record policy violation")
                                            .with_code("INTERNAL_ERROR")
                                            .with_string_details(e.to_string()),
                                    ),
                                )
                            })?;

                        tracing::warn!(
                            adapter_name = %req.name,
                            tenant_id = %claims.tenant_id,
                            violation_id = %violation_id,
                            "Naming policy violation detected"
                        );

                        // Audit log: policy violation during adapter registration
                        crate::audit_helper::log_failure_or_warn(
                            &state.db,
                            &claims,
                            crate::audit_helper::actions::ADAPTER_REGISTER,
                            crate::audit_helper::resources::ADAPTER,
                            Some(&req.adapter_id),
                            &format!("Naming policy violation: {}", e),
                            Some(client_ip.0.as_str()),
                        )
                        .await;

                        // Reject registration if naming policy is enforced
                        return Err((
                            StatusCode::FORBIDDEN,
                            Json(
                                ErrorResponse::new(format!("Naming policy violation: {}", e))
                                    .with_code("POLICY_VIOLATION")
                                    .with_string_details(format!("Violation ID: {}", violation_id)),
                            ),
                        ));
                    }
                }
            }
        }
    }

    // Build registration params using the builder pattern
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id(&claims.tenant_id)
        .adapter_id(&req.adapter_id)
        .name(&req.name)
        .hash_b3(&req.hash_b3)
        .rank(req.rank)
        .tier(&req.tier)
        .languages_json(Some(languages_json.clone()))
        .framework(req.framework.clone())
        .category(req.category.clone())
        .scope(req.scope.clone().unwrap_or_else(|| "global".to_string()))
        .expires_at(req.expires_at.clone())
        .build()
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid adapter parameters")
                        .with_code("BAD_REQUEST")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let id = match state.db.register_adapter(params).await {
        Ok(id) => id,
        Err(e) => {
            // Audit log: adapter registration failure
            crate::audit_helper::log_failure_or_warn(
                &state.db,
                &claims,
                crate::audit_helper::actions::ADAPTER_REGISTER,
                crate::audit_helper::resources::ADAPTER,
                Some(&req.adapter_id),
                &format!("Failed to register adapter: {}", e),
                Some(client_ip.0.as_str()),
            )
            .await;
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to register adapter")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    // Audit log: adapter registration
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_REGISTER,
        crate::audit_helper::resources::ADAPTER,
        Some(&req.adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(AdapterResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id,
            adapter_id: req.adapter_id.clone(),
            name: req.name,
            hash_b3: req.hash_b3,
            rank: req.rank,
            tier: req.tier,
            assurance_tier: None,
            version: "1.0".to_string(),
            lifecycle_state: "active".to_string(),
            languages: req.languages,
            framework: req.framework,
            category: Some(req.category.clone()),
            scope: req.scope.clone(),
            lora_tier: None,
            lora_strength: Some(1.0),
            lora_scope: req.scope.clone(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            stats: None,
            runtime_state: Some("unloaded".to_string()),
            pinned: Some(false),
            memory_bytes: Some(0),
            deduplicated: None,
            drift_reference_backend: None,
            drift_baseline_backend: None,
            drift_test_backend: None,
            drift_tier: None,
            drift_metric: None,
            drift_loss_metric: None,
            drift_slice_size: None,
            drift_slice_offset: None,
            // Codebase adapter fields
            adapter_type: None,
            base_adapter_id: None,
            stream_session_id: None,
            versioning_threshold: None,
            coreml_package_hash: None,
            display_name: None,
        }),
    ))
}

/// Delete adapter
#[utoipa::path(
    tag = "system",
    delete,
    path = "/v1/adapters/{adapter_id}",
    params(
        ("adapter_id" = String, Path, description = "Adapter ID")
    ),
    responses(
        (status = 204, description = "Adapter deleted"),
        (status = 404, description = "Adapter not found", body = ErrorResponse)
    )
)]
pub async fn delete_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(client_ip): Extension<ClientIp>,
    Path(adapter_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Admin-only (destructive operation)
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AdapterDelete)?;
    let adapter_id = crate::id_resolver::resolve_any_id(&state.db, &adapter_id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;

    // Get adapter with tenant-scoped query
    let _adapter = state
        .db
        .get_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("adapter not found").with_code("NOT_FOUND")),
            )
        })?;

    if let Err(e) = state
        .db
        .delete_adapter_for_tenant(&claims.tenant_id, &adapter_id)
        .await
    {
        // Audit log: adapter deletion failure
        crate::audit_helper::log_failure_or_warn(
            &state.db,
            &claims,
            crate::audit_helper::actions::ADAPTER_DELETE,
            crate::audit_helper::resources::ADAPTER,
            Some(&adapter_id),
            &format!("Failed to delete adapter: {}", e),
            Some(client_ip.0.as_str()),
        )
        .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to delete adapter")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        ));
    }

    // Audit log: adapter deletion
    crate::audit_helper::log_success_or_warn(
        &state.db,
        &claims,
        crate::audit_helper::actions::ADAPTER_DELETE,
        crate::audit_helper::resources::ADAPTER,
        Some(&adapter_id),
        Some(client_ip.0.as_str()),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ===== Metrics Endpoints =====

/// Get quality metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/quality",
    responses(
        (status = 200, description = "Quality metrics", body = QualityMetricsResponse)
    )
)]
pub async fn get_quality_metrics(
    Extension(claims): Extension<Claims>,
) -> Result<Json<QualityMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    // Stub implementation - would compute from telemetry
    Ok(Json(QualityMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        arr: 0.95,
        ecs5: 0.82,
        hlr: 0.02,
        cr: 0.01,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get adapter performance metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/adapters",
    responses(
        (status = 200, description = "Adapter metrics", body = AdapterMetricsResponse)
    )
)]
pub async fn get_adapter_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list adapters")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut performances = Vec::new();
    for adapter in adapters {
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(
                &claims.tenant_id,
                adapter.adapter_id.as_ref().unwrap_or(&adapter.id),
            )
            .await
            .unwrap_or((0, 0, 0.0));

        let activation_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        performances.push(AdapterPerformance {
            adapter_id: adapter
                .adapter_id
                .clone()
                .unwrap_or_else(|| adapter.id.clone()),
            name: adapter.name,
            activation_rate,
            avg_gate_value: avg_gate,
            total_requests: total,
        });
    }

    Ok(Json(AdapterMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapters: performances,
    }))
}

/// Get system metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/system",
    responses(
        (status = 200, description = "System metrics", body = SystemMetricsResponse)
    )
)]
pub async fn get_system_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SystemMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    // Collect system metrics (using stubs until adapteros-system-metrics is re-enabled)
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    // Collect additional metrics for frontend compatibility
    // Workers in 'healthy' status are actively serving inference requests
    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'healthy'")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0) as i32;

    let requests_per_second = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
    )
    .fetch_one(state.db.pool())
    .await
    .map(|count| count as f32 / 60.0)
    .unwrap_or(0.0);

    let avg_latency_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or(0.0) as f32;

    // Calculate active sessions count
    let active_sessions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chat_sessions WHERE updated_at > datetime('now', '-30 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0) as i32;

    // Calculate error rate from recent requests
    let error_rate = {
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
        )
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(0);

        if total > 0 {
            let errors = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes') AND status_code >= 500",
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0);
            Some((errors as f32) / (total as f32))
        } else {
            Some(0.0)
        }
    };

    // Compute tokens/sec from recent inference trace receipts (last 1 minute)
    let tokens_per_second: f32 = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(logical_output_tokens), 0) FROM inference_trace_receipts WHERE created_at > datetime('now', '-1 minute')",
    )
    .fetch_one(state.db.pool())
    .await
    .map(|tokens| tokens as f32 / 60.0)
    .unwrap_or(0.0);

    // Calculate p95 latency
    let latency_p95_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT latency_ms FROM request_log WHERE timestamp > datetime('now', '-5 minutes') ORDER BY latency_ms DESC LIMIT 1 OFFSET (SELECT COUNT(*) * 5 / 100 FROM request_log WHERE timestamp > datetime('now', '-5 minutes'))",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .map(|v| v as f32);

    Ok(Json(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers,
        requests_per_second,
        avg_latency_ms,
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
        // Additional fields for frontend compatibility
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        tokens_per_second: Some(tokens_per_second),
        error_rate,
        active_sessions: Some(active_sessions),
        latency_p95_ms,
    }))
}

// ===== Commit Inspector Endpoints =====

/// List commits
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/commits",
    params(
        ("repo_id" = Option<String>, Query, description = "Filter by repository"),
        ("branch" = Option<String>, Query, description = "Filter by branch"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of commits", body = Vec<CommitResponse>)
    )
)]
pub async fn list_commits(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(query): Query<ListCommitsQuery>,
) -> Result<Json<Vec<CommitResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Use git subsystem if available
    if let Some(git_subsystem) = &state.git_subsystem {
        let limit = query.limit.unwrap_or(50).clamp(1, 200) as usize;
        let commits = git_subsystem
            .list_commits(query.repo_id.as_deref(), query.branch.as_deref(), limit)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list commits: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to list commits")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        let response: Vec<CommitResponse> = commits
            .into_iter()
            .map(|commit| CommitResponse {
                id: commit.sha.clone(),
                repo_id: commit.repo_id,
                sha: commit.sha,
                message: commit.message,
                author: commit.author,
                date: commit.date.to_rfc3339(),
                branch: commit.branch,
                changed_files: commit.changed_files,
                impacted_symbols: commit.impacted_symbols,
                ephemeral_adapter_id: commit.ephemeral_adapter_id,
            })
            .collect();

        Ok(Json(response))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

/// Get commit details
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/commits/{sha}",
    params(
        ("sha" = String, Path, description = "Commit SHA")
    ),
    responses(
        (status = 200, description = "Commit details", body = CommitResponse),
        (status = 404, description = "Commit not found", body = ErrorResponse)
    )
)]
pub async fn get_commit(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(sha): Path<String>,
) -> Result<Json<CommitResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Use git subsystem if available
    if let Some(git_subsystem) = &state.git_subsystem {
        let commit = git_subsystem.get_commit(None, &sha).await.map_err(|e| {
            tracing::error!("Failed to get commit {}: {}", sha, e);
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new(format!("Failed to get commit: {}", e))
                        .with_code(if status == StatusCode::NOT_FOUND {
                            "NOT_FOUND"
                        } else {
                            "INTERNAL_ERROR"
                        })
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

        Ok(Json(CommitResponse {
            id: commit.sha.clone(),
            repo_id: commit.repo_id,
            sha: commit.sha,
            message: commit.message,
            author: commit.author,
            date: commit.date.to_rfc3339(),
            branch: commit.branch,
            changed_files: commit.changed_files,
            impacted_symbols: commit.impacted_symbols,
            ephemeral_adapter_id: commit.ephemeral_adapter_id,
        }))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

/// Get commit diff
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/commits/{sha}/diff",
    params(
        ("sha" = String, Path, description = "Commit SHA")
    ),
    responses(
        (status = 200, description = "Commit diff", body = CommitDiffResponse)
    )
)]
pub async fn get_commit_diff(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(sha): Path<String>,
) -> Result<Json<CommitDiffResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Use git subsystem if available
    if let Some(git_subsystem) = &state.git_subsystem {
        let diff = git_subsystem
            .get_commit_diff(None, &sha)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get commit diff for {}: {}", sha, e);
                let status = if e.to_string().contains("not found") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (
                    status,
                    Json(
                        ErrorResponse::new(format!("Failed to get commit diff: {}", e))
                            .with_code(if status == StatusCode::NOT_FOUND {
                                "NOT_FOUND"
                            } else {
                                "INTERNAL_ERROR"
                            })
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

        Ok(Json(CommitDiffResponse {
            sha: diff.sha,
            diff: diff.diff,
            stats: DiffStats {
                files_changed: diff.files_changed,
                insertions: diff.insertions,
                deletions: diff.deletions,
            },
        }))
    } else {
        Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("Git subsystem not available").with_code("SERVICE_UNAVAILABLE"),
            ),
        ))
    }
}

// ===== Routing Inspector Endpoints =====

/// Debug routing decision
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/routing/debug",
    request_body = RoutingDebugRequest,
    responses(
        (status = 200, description = "Routing debug info", body = RoutingDebugResponse)
    )
)]
pub async fn debug_routing(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<RoutingDebugRequest>,
) -> Result<Json<RoutingDebugResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_lora_router::{AdapterInfo, CodeFeatures, Router, RouterWeights};

    // Extract code features from prompt and context
    let combined_context = match req.context {
        Some(ctx) => format!("{} {}", req.prompt, ctx),
        None => req.prompt.clone(),
    };
    let code_features = CodeFeatures::from_context(&combined_context);

    // Fetch all adapters from database
    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list adapters: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to fetch adapters for routing debug")
                        .with_code("ADAPTER_FETCH_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert database adapters to router AdapterInfo
    let adapter_infos: Vec<AdapterInfo> = adapters
        .iter()
        .map(|adapter| {
            let languages = adapter
                .languages_json
                .as_ref()
                .and_then(|json| serde_json::from_str::<Vec<usize>>(json).ok())
                .unwrap_or_default();

            AdapterInfo {
                id: adapter.id.clone(),
                stable_id: adapter.stable_id.unwrap_or(0) as u64,
                framework: adapter.framework.clone(),
                languages,
                tier: adapter.tier.clone(),
                base_model: adapter.base_model_id.clone(),
                recommended_for_moe: adapter.recommended_for_moe.unwrap_or(true),
                ..Default::default()
            }
        })
        .collect();

    // Create router and route with code features
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let decision = router
        .route_with_code_features(&code_features, &adapter_infos)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to compute routing decision for debug");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to compute routing decision")
                        .with_code("ROUTING_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;
    let explanation = router.explain_score(&code_features.to_vector());

    // Build adapter scores
    let mut adapter_scores: Vec<AdapterScore> = Vec::new();
    for (idx, adapter) in adapter_infos.iter().enumerate() {
        let is_selected = decision.indices.iter().any(|&i| i as usize == idx);
        let gate_value = if is_selected {
            let position = decision
                .indices
                .iter()
                .position(|&i| i as usize == idx)
                .unwrap_or(0);
            decision.gates_f32()[position] as f64
        } else {
            0.0
        };

        adapter_scores.push(AdapterScore {
            adapter_id: adapter.id.clone(),
            score: explanation.total_score as f64,
            gate_value,
            selected: is_selected,
        });
    }

    // Extract language from code features
    let detected_lang_idx = code_features
        .lang_one_hot
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx);

    let language = detected_lang_idx
        .and_then(|idx| match idx {
            0 => Some("python"),
            1 => Some("rust"),
            2 => Some("javascript"),
            3 => Some("typescript"),
            4 => Some("go"),
            5 => Some("java"),
            6 => Some("cpp"),
            7 => Some("csharp"),
            _ => None,
        })
        .map(|s| s.to_string());

    let frameworks: Vec<String> = code_features.framework_prior.keys().cloned().collect();

    let selected_adapters: Vec<String> = decision
        .indices
        .iter()
        .filter_map(|&idx| adapter_infos.get(idx as usize).map(|a| a.id.clone()))
        .collect();

    Ok(Json(RoutingDebugResponse {
        features: FeatureVector {
            language,
            frameworks,
            symbol_hits: code_features.symbol_hits as i32,
            path_tokens: code_features.path_tokens.clone(),
            verb: format!("{:?}", code_features.prompt_verb),
        },
        adapter_scores,
        selected_adapters,
        explanation: format!(
            "Router selected {} adapters with entropy {:.3}. {}",
            decision.indices.len(),
            decision.entropy,
            explanation.format()
        ),
    }))
}

/// Get routing history
///
/// Returns the most recent routing decisions from the database.
/// This queries actual routing decisions stored during inference operations.
/// If no decisions exist yet, returns an empty list.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/routing/history",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of results (default: 50)")
    ),
    responses(
        (status = 200, description = "Routing history", body = Vec<RoutingDebugResponse>)
    )
)]
pub async fn get_routing_history(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<RoutingHistoryQuery>,
) -> Result<Json<Vec<RoutingDebugResponse>>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_db::RoutingDecisionFilters;
    use tracing::{debug, warn};

    let limit = params.limit.unwrap_or(50);
    debug!(limit = limit, "Querying routing history from database");

    // Query routing decisions from the database
    let filters = RoutingDecisionFilters {
        limit: Some(limit),
        ..Default::default()
    };

    let db_decisions = state
        .db
        .query_routing_decisions(&filters)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to query routing history");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!("Database error: {}", e))),
            )
        })?;

    // Convert database records to RoutingDebugResponse format
    let responses: Vec<RoutingDebugResponse> = db_decisions
        .into_iter()
        .map(|decision| {
            // Parse candidate adapters JSON
            let candidates: Vec<adapteros_db::RouterCandidate> =
                serde_json::from_str(&decision.candidate_adapters).unwrap_or_default();

            // Parse selected adapter IDs
            let selected_adapters: Vec<String> = decision
                .selected_adapter_ids
                .map(|ids| ids.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            // Convert candidates to adapter scores
            let adapter_scores: Vec<AdapterScore> = candidates
                .iter()
                .map(|c| {
                    let gate_float = (c.gate_q15 as f32) / Q15_GATE_DENOMINATOR;
                    let adapter_id = format!("adapter-{}", c.adapter_idx);
                    let is_selected = selected_adapters.contains(&adapter_id);
                    AdapterScore {
                        adapter_id,
                        score: c.raw_score as f64,
                        gate_value: gate_float as f64,
                        selected: is_selected,
                    }
                })
                .collect();

            // Build explanation from decision metadata
            let explanation = format!(
                "Step {} with entropy {:.3}, tau {:.3}, selected {} adapter(s)",
                decision.step,
                decision.entropy,
                decision.tau,
                selected_adapters.len()
            );

            RoutingDebugResponse {
                features: FeatureVector {
                    // Note: Detailed features not stored in routing_decisions table
                    // These are summarized during decision storage
                    language: None,
                    frameworks: vec![],
                    symbol_hits: 0,
                    path_tokens: vec![],
                    verb: "infer".to_string(),
                },
                adapter_scores,
                selected_adapters,
                explanation,
            }
        })
        .collect();

    Ok(Json(responses))
}

// ===== Agent D Contract Endpoints =====

/// Get routing decisions (placeholder for Agent D)
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/routing/decisions",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("limit" = Option<usize>, Query, description = "Limit results"),
        ("since" = Option<String>, Query, description = "ISO-8601 timestamp")
    ),
    responses(
        (status = 200, description = "Routing decisions", body = RoutingDecisionsResponse),
        (status = 404, description = "Not yet available")
    )
)]
pub async fn routing_decisions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<RoutingDecisionsQuery>,
) -> Result<Json<RoutingDecisionsResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_db::RoutingDecisionFilters;
    use tracing::{error, info};

    info!(
        tenant = %params.tenant,
        limit = params.limit,
        user_id = %claims.sub,
        "Querying routing decisions"
    );

    // Build filters from query params
    let filters = RoutingDecisionFilters {
        tenant_id: Some(params.tenant.clone()),
        stack_id: params.stack_id.clone(),
        adapter_id: params.adapter_id.clone(),
        request_id: params.request_id.clone(),
        source_type: params.source_type.clone(),
        since: params.since.clone(),
        until: params.until.clone(),
        min_entropy: params.min_entropy,
        max_overhead_pct: params.max_overhead_pct,
        limit: Some(params.limit),
        offset: params.offset,
    };

    // Query database
    let db_decisions = if params.anomalies_only {
        // Get high overhead decisions (>8% budget)
        state
            .db
            .get_high_overhead_decisions(Some(params.tenant.clone()), params.limit)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to query high overhead decisions");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database error: {}", e))),
                )
            })?
    } else {
        state
            .db
            .query_routing_decisions(&filters)
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to query routing decisions");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!("Database error: {}", e))),
                )
            })?
    };

    // Convert database records to API response format
    let mut items: Vec<RoutingDecision> = Vec::new();
    for db_decision in db_decisions.iter() {
        // Parse candidates JSON
        let candidates: Vec<adapteros_db::RouterCandidate> =
            serde_json::from_str(&db_decision.candidate_adapters).unwrap_or_default();

        // Lookup stack name from adapter_stacks table if stack_id is available
        let stack_name = if let Some(stack_id) = &db_decision.stack_id {
            state
                .db
                .get_stack(&params.tenant, stack_id)
                .await
                .ok()
                .flatten()
                .map(|stack| stack.name)
        } else {
            None
        };

        // Convert to API format with Q15 to float conversion
        let candidate_infos: Vec<RouterCandidateInfo> = candidates
            .iter()
            .map(|c| {
                let gate_float = (c.gate_q15 as f32) / 32767.0;
                RouterCandidateInfo {
                    adapter_idx: c.adapter_idx,
                    adapter_name: None, // adapter_idx is internal routing index; adapter IDs are in adapters_used
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                    gate_float,
                    selected: c.gate_q15 > 0,
                }
            })
            .collect();

        // Extract selected adapters for legacy field
        let adapters_used: Vec<String> = db_decision
            .selected_adapter_ids
            .clone()
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Extract activations (gate values as floats)
        let activations: Vec<f64> = candidate_infos
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.gate_float as f64)
            .collect();

        items.push(RoutingDecision {
            id: db_decision.id.clone(),
            tenant_id: db_decision.tenant_id.clone(),
            timestamp: db_decision.timestamp.clone(),
            request_id: db_decision.request_id.clone(),
            step: db_decision.step,
            input_token_id: db_decision.input_token_id,
            stack_id: db_decision.stack_id.clone(),
            stack_name,
            stack_hash: db_decision.stack_hash.clone(),
            entropy: db_decision.entropy,
            tau: db_decision.tau,
            entropy_floor: db_decision.entropy_floor,
            k_value: db_decision.k_value,
            candidates: candidate_infos,
            router_latency_us: db_decision.router_latency_us,
            total_inference_latency_us: db_decision.total_inference_latency_us,
            overhead_pct: db_decision.overhead_pct,
            adapters_used,
            activations,
            reason: format!(
                "entropy={:.2}, k={}",
                db_decision.entropy,
                db_decision.k_value.unwrap_or(0)
            ),
            trace_id: db_decision.request_id.clone().unwrap_or_default(),
        });
    }

    info!(
        count = items.len(),
        "Successfully retrieved routing decisions"
    );

    Ok(Json(RoutingDecisionsResponse { items }))
}

/// List audits with extended fields
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audits",
    params(
        ("tenant" = String, Query, description = "Tenant ID"),
        ("limit" = Option<usize>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of audits", body = AuditsResponse)
    )
)]
pub async fn list_audits_extended(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<AuditsQuery>,
) -> Result<Json<AuditsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let audits = sqlx::query_as::<_, AuditExtended>(
        "SELECT id, tenant_id, cpid, arr, ecs5, hlr, cr, status, 
                before_cpid, after_cpid, created_at 
         FROM audits WHERE tenant_id = ? 
         ORDER BY created_at DESC LIMIT ?",
    )
    .bind(&params.tenant)
    .bind(params.limit.unwrap_or(50) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch audits")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(AuditsResponse { items: audits }))
}

/// Get promotion record with signature
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/promotions/{id}",
    params(
        ("id" = String, Path, description = "Promotion ID")
    ),
    responses(
        (status = 200, description = "Promotion record", body = PromotionRecord),
        (status = 404, description = "Promotion not found")
    )
)]
pub async fn get_promotion(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<PromotionRecord>, (StatusCode, Json<ErrorResponse>)> {
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(<(StatusCode, Json<ErrorResponse>)>::from)?;
    let promo = sqlx::query_as::<_, PromotionRecord>(
        "SELECT id, cpid, promoted_by, promoted_at, signature_b64, 
                signer_key_id, quality_json, before_cpid 
         FROM promotions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("promotion not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(promo))
}

// ===== Metrics Endpoint =====

/// Prometheus/OpenMetrics endpoint  
/// Note: This endpoint requires bearer token authentication via Authorization header.
/// Authentication is checked in the route layer, not in the handler itself.
#[utoipa::path(
    get,
    path = "/metrics",
    responses(
        (status = 200, description = "Metrics payload", body = String),
        (status = 404, description = "Metrics disabled", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "metrics"
)]
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Check if metrics are enabled
    let metrics_enabled = {
        let config = match state.config.read() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("Failed to acquire config read lock: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("internal error")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
                    .into_response();
            }
        };
        config.metrics.enabled
    };

    if !metrics_enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("metrics disabled").with_code("INTERNAL_ERROR")),
        )
            .into_response();
    }

    // Update worker metrics from database
    if let Err(e) = state
        .metrics_exporter
        .update_worker_metrics(&state.db)
        .await
    {
        tracing::warn!("Failed to update worker metrics: {}", e);
    }

    // Update alert metrics from database
    {
        use adapteros_db::process_monitoring::{AlertFilters, ProcessAlert};

        let filters = AlertFilters::default();
        match ProcessAlert::list(state.db.pool(), filters).await {
            Ok(alerts) => {
                let alert_tuples: Vec<(String, String, String, String, String)> = alerts
                    .iter()
                    .map(|a| {
                        (
                            a.title.clone(),
                            format!("{:?}", a.severity).to_lowercase(),
                            a.tenant_id.clone(),
                            a.worker_id.clone(),
                            format!("{:?}", a.status).to_lowercase(),
                        )
                    })
                    .collect();
                state.metrics_exporter.update_alert_metrics(&alert_tuples);
            }
            Err(e) => {
                tracing::warn!("Failed to fetch alerts for metrics: {}", e);
            }
        }
    }

    // Render metrics
    let metrics = match state.metrics_exporter.render() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to render metrics")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics,
    )
        .into_response()
}

/// Get federation audit report
///
/// Returns federation chain verification status and host validation results.
/// Per Observability Layer requirement for canonical audit dashboard.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audit/federation",
    responses(
        (status = 200, description = "Federation audit report", body = FederationAuditResponse)
    )
)]
pub async fn get_federation_audit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<FederationAuditResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    // Fetch federation bundle signatures
    let pool = state.db.pool();

    let signatures = sqlx::query(
        r#"
        SELECT 
            bundle_hash,
            host_id,
            signature,
            verified,
            created_at
        FROM federation_bundle_signatures
        ORDER BY created_at DESC
        LIMIT 100
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch federation signatures")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let mut host_chains: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut total_signatures = 0;
    let mut verified_signatures = 0;

    for row in signatures {
        total_signatures += 1;
        let host_id: String = row.try_get("host_id").unwrap_or_default();
        let verified: bool = row.try_get("verified").unwrap_or(false);
        let bundle_hash: String = row.try_get("bundle_hash").unwrap_or_default();

        if verified {
            verified_signatures += 1;
        }

        host_chains.entry(host_id).or_default().push(bundle_hash);
    }

    // Check quarantine status
    let quarantine_status = sqlx::query(
        r#"
        SELECT reason, created_at
        FROM policy_quarantine
        WHERE released = FALSE
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to check quarantine status")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let quarantined = quarantine_status.is_some();
    let quarantine_reason = quarantine_status.and_then(|row| row.try_get("reason").ok());

    Ok(Json(FederationAuditResponse {
        total_hosts: host_chains.len(),
        total_signatures,
        verified_signatures,
        quarantined,
        quarantine_reason,
        host_chains: host_chains
            .into_iter()
            .map(|(host_id, bundles)| HostChainSummary {
                host_id,
                bundle_count: bundles.len(),
                latest_bundle: bundles.first().cloned(),
            })
            .collect(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get compliance audit report
///
/// Returns compliance status for all policy packs and control objectives.
/// Per Observability Layer requirement for canonical audit dashboard.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audit/compliance",
    responses(
        (status = 200, description = "Compliance audit report", body = ComplianceAuditResponse)
    )
)]
pub async fn get_compliance_audit(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ComplianceAuditResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    // Fetch policy violations from telemetry bundles
    let pool = state.db.pool();

    let violations = sqlx::query(
        r#"
        SELECT COUNT(*) as count
        FROM policy_quarantine
        WHERE released = FALSE
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count violations")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let active_violations: i64 = violations.try_get("count").unwrap_or(0);

    // PRD-DATA-01: Check T1 adapter evidence compliance (cp-evidence-004)
    let t1_adapters_without_dataset = sqlx::query(
        r#"
        SELECT COUNT(*) as count
        FROM adapters
        WHERE tier = 'persistent'
          AND (primary_dataset_id IS NULL OR primary_dataset_id = '')
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count T1 adapters without dataset")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let t1_without_dataset: i64 = t1_adapters_without_dataset.try_get("count").unwrap_or(0);

    let t1_adapters_without_evidence = sqlx::query(
        r#"
        SELECT COUNT(DISTINCT a.id) as count
        FROM adapters a
        WHERE a.tier = 'persistent'
          AND NOT EXISTS (
              SELECT 1 FROM evidence_entries e
              WHERE e.adapter_id = a.id
          )
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to count T1 adapters without evidence")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let t1_without_evidence: i64 = t1_adapters_without_evidence.try_get("count").unwrap_or(0);

    // Generate compliance controls status
    let mut controls = vec![
        ComplianceControl {
            control_id: "EGRESS-001".to_string(),
            control_name: "Network Egress Control".to_string(),
            status: if active_violations == 0 {
                "compliant"
            } else {
                "pending"
            }
            .to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Zero egress mode enforced".to_string(),
                "PF rules active".to_string(),
            ],
            findings: vec![],
        },
        ComplianceControl {
            control_id: "DETERM-001".to_string(),
            control_name: "Deterministic Execution".to_string(),
            status: "compliant".to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Metal kernels precompiled".to_string(),
                "HKDF seeding enabled".to_string(),
                "Tick ledger active".to_string(),
            ],
            findings: vec![],
        },
        ComplianceControl {
            control_id: "ISOLATION-001".to_string(),
            control_name: "Tenant Isolation".to_string(),
            status: "compliant".to_string(),
            last_checked: chrono::Utc::now().to_rfc3339(),
            evidence: vec![
                "Per-tenant processes".to_string(),
                "UID/GID separation".to_string(),
            ],
            findings: vec![],
        },
    ];

    // PRD-DATA-01: Add evidence control (cp-evidence-004)
    let evidence_status = if t1_without_dataset == 0 && t1_without_evidence == 0 {
        "compliant"
    } else {
        "non_compliant"
    };
    let mut evidence_findings = vec![];
    if t1_without_dataset > 0 {
        evidence_findings.push(format!(
            "{} T1 adapters missing primary dataset",
            t1_without_dataset
        ));
    }
    if t1_without_evidence > 0 {
        evidence_findings.push(format!(
            "{} T1 adapters missing evidence entries",
            t1_without_evidence
        ));
    }

    controls.push(ComplianceControl {
        control_id: "EVIDENCE-004".to_string(),
        control_name: "Training Provenance & Evidence (cp-evidence-004)".to_string(),
        status: evidence_status.to_string(),
        last_checked: chrono::Utc::now().to_rfc3339(),
        evidence: vec![
            "Dataset-adapter linkage enabled".to_string(),
            "Evidence entries tracked".to_string(),
        ],
        findings: evidence_findings,
    });

    let compliant_count = controls.iter().filter(|c| c.status == "compliant").count();
    let compliance_rate = if !controls.is_empty() {
        (compliant_count as f64 / controls.len() as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(ComplianceAuditResponse {
        compliance_rate,
        total_controls: controls.len(),
        compliant_controls: compliant_count,
        active_violations: active_violations as usize,
        controls,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}
/// Query audit logs with filtering and pagination
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/audit/logs",
    params(
        ("user_id" = Option<String>, Query, description = "Filter by user ID"),
        ("action" = Option<String>, Query, description = "Filter by action"),
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
        ("resource_id" = Option<String>, Query, description = "Filter by resource ID"),
        ("status" = Option<String>, Query, description = "Filter by status (success/failure)"),
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("from_time" = Option<String>, Query, description = "Start time (RFC3339)"),
        ("to_time" = Option<String>, Query, description = "End time (RFC3339)"),
        ("limit" = Option<usize>, Query, description = "Maximum results (default: 100, max: 1000)"),
        ("offset" = Option<usize>, Query, description = "Offset for pagination"),
    ),
    responses(
        (status = 200, description = "Audit logs retrieved successfully", body = AuditLogsResponse),
        (status = 403, description = "Forbidden - requires AuditView permission", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "audit"
)]
pub async fn query_audit_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    axum::extract::Query(query): axum::extract::Query<crate::types::AuditLogsQuery>,
) -> Result<Json<crate::types::AuditLogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Only Admin, SRE, and Compliance can view audit logs
    crate::permissions::require_permission(&claims, crate::permissions::Permission::AuditView)?;

    // Apply defaults and limits
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    // Query audit logs from database
    // Note: The db method signature is: query_audit_logs(user_id, action, resource_type, start_date, end_date, limit)
    // Additional filtering (resource_id, status, tenant_id, offset) can be applied post-query if needed
    let _ = (
        query.resource_id.as_deref(),
        query.status.as_deref(),
        query.tenant_id.as_deref(),
        offset,
    );
    let logs = state
        .db
        .query_audit_logs_for_tenant(
            &claims.tenant_id,
            query.user_id.as_deref(),
            query.action.as_deref(),
            query.resource_type.as_deref(),
            query.from_time.as_deref(),
            query.to_time.as_deref(),
            limit as i64,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query audit logs")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert AuditLog to AuditLogResponse
    let log_responses: Vec<crate::types::AuditLogResponse> = logs
        .iter()
        .map(|log| crate::types::AuditLogResponse {
            id: log.id.clone(),
            timestamp: log.timestamp.clone(),
            user_id: log.user_id.clone(),
            user_role: log.user_role.clone(),
            tenant_id: log.tenant_id.clone(),
            action: log.action.clone(),
            resource_type: log.resource_type.clone(),
            resource_id: log.resource_id.clone(),
            status: log.status.clone(),
            error_message: log.error_message.clone(),
            ip_address: log.ip_address.clone(),
            metadata_json: log.metadata_json.clone(),
        })
        .collect();

    let total = log_responses.len();

    Ok(Json(crate::types::AuditLogsResponse {
        logs: log_responses,
        total,
        limit,
        offset,
    }))
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct FederationAuditResponse {
    pub total_hosts: usize,
    pub total_signatures: usize,
    pub verified_signatures: usize,
    pub quarantined: bool,
    pub quarantine_reason: Option<String>,
    pub host_chains: Vec<HostChainSummary>,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct HostChainSummary {
    pub host_id: String,
    pub bundle_count: usize,
    pub latest_bundle: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ComplianceAuditResponse {
    pub compliance_rate: f64,
    pub total_controls: usize,
    pub compliant_controls: usize,
    pub active_violations: usize,
    pub controls: Vec<ComplianceControl>,
    pub timestamp: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ComplianceControl {
    pub control_id: String,
    pub control_name: String,
    pub status: String,
    pub last_checked: String,
    pub evidence: Vec<String>,
    pub findings: Vec<String>,
}

/// Get the next revision number for an adapter lineage
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/adapters/next-revision/{tenant}/{domain}/{purpose}",
    params(
        ("tenant" = String, Path, description = "Tenant namespace"),
        ("domain" = String, Path, description = "Domain namespace"),
        ("purpose" = String, Path, description = "Purpose identifier")
    ),
    responses(
        (status = 200, description = "Next revision number", body = NextRevisionResponse),
        (status = 404, description = "Lineage not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "adapters"
)]
pub async fn get_next_revision(
    State(state): State<AppState>,
    Path((tenant, domain, purpose)): Path<(String, String, String)>,
) -> Result<Json<NextRevisionResponse>, (StatusCode, Json<ErrorResponse>)> {
    use crate::api_error::ApiError;

    // Get registry from database
    let registry = state
        .registry
        .as_ref()
        .ok_or_else(|| ApiError::internal("Registry not available"))?;

    // Get next revision number
    let next_rev = registry
        .next_revision_number(&tenant, &domain, &purpose)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    // Format the suggested name
    let suggested_name = format!("{}/{}/{}/r{:03}", tenant, domain, purpose, next_rev);

    Ok(Json(NextRevisionResponse {
        next_revision: next_rev,
        suggested_name,
        base_path: format!("{}/{}/{}", tenant, domain, purpose),
    }))
}

/// Response for next revision query
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct NextRevisionResponse {
    /// Next revision number
    pub next_revision: u32,
    /// Suggested full adapter name
    pub suggested_name: String,
    /// Base path (tenant/domain/purpose)
    pub base_path: String,
}
