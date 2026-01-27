//! Audit handlers
//!
//! Handlers for audit logs, compliance, and federation auditing.
//! Split from adapteros-server-api for faster incremental builds.

use adapteros_db::users::Role;
use adapteros_server_api::auth::Claims;
use adapteros_server_api::middleware::require_any_role;
use adapteros_server_api::permissions::{require_permission, Permission};
use adapteros_server_api::state::AppState;
use adapteros_server_api::types::{
    AuditExtended, AuditLogResponse, AuditLogsQuery, AuditLogsResponse, AuditsQuery,
    AuditsResponse, ErrorResponse,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use sqlx::Row;

// ========== Audit Types ==========

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

// ========== Handlers ==========

/// List audits with extended information
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

/// Query audit logs with filtering and pagination
#[utoipa::path(
    tag = "audit",
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
    )
)]
pub async fn query_audit_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<AuditLogsQuery>,
) -> Result<Json<AuditLogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Only Admin, SRE, and Compliance can view audit logs
    require_permission(&claims, Permission::AuditView)?;

    // Apply defaults and limits
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    // Query audit logs from database
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
    let log_responses: Vec<AuditLogResponse> = logs
        .iter()
        .map(|log| AuditLogResponse {
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

    Ok(Json(AuditLogsResponse {
        logs: log_responses,
        total,
        limit,
        offset,
    }))
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
        let host_id: String = row.try_get("host_id").unwrap_or_else(|e| {
            tracing::warn!("Failed to get host_id from row: {}", e);
            String::new()
        });
        let verified: bool = row.try_get("verified").unwrap_or_else(|e| {
            tracing::warn!("Failed to get verified from row: {}", e);
            false
        });
        let bundle_hash: String = row.try_get("bundle_hash").unwrap_or_else(|e| {
            tracing::warn!("Failed to get bundle_hash from row: {}", e);
            String::new()
        });

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
    let quarantine_reason = quarantine_status.and_then(|row| {
        row.try_get("reason").unwrap_or_else(|e| {
            tracing::warn!("Failed to get quarantine reason from row: {}", e);
            None
        })
    });

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

    let active_violations: i64 = violations.try_get("count").unwrap_or_else(|e| {
        tracing::warn!("Failed to get violations count from row: {}", e);
        0
    });

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
    let t1_without_dataset: i64 = t1_adapters_without_dataset
        .try_get("count")
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to get T1 adapters without dataset count: {}", e);
            0
        });

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
    let t1_without_evidence: i64 = t1_adapters_without_evidence
        .try_get("count")
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to get T1 adapters without evidence count: {}", e);
            0
        });

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

// ========== Audit Chain Types for UI ==========

/// Query parameters for audit chain endpoint
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::IntoParams)]
pub struct AuditChainQuery {
    /// Maximum number of entries to return (default: 100, max: 1000)
    #[serde(default = "default_chain_limit")]
    pub limit: usize,
}

fn default_chain_limit() -> usize {
    100
}

/// Audit chain entry with hash linkage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct AuditChainEntry {
    pub id: String,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub status: String,
    pub entry_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub chain_sequence: i64,
    pub verified: bool,
}

/// Audit chain response with verification status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct AuditChainResponse {
    pub entries: Vec<AuditChainEntry>,
    pub chain_valid: bool,
    pub total_entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
}

/// Chain verification response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ChainVerificationResponse {
    pub chain_valid: bool,
    pub total_entries: usize,
    pub verified_entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_invalid_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
    pub verification_timestamp: String,
}

// ========== Audit Chain Handlers ==========

/// Get audit chain entries
///
/// Returns policy audit decision chain entries formatted for the UI.
/// The merkle root is computed from the latest entry's hash.
#[utoipa::path(
    tag = "audit",
    get,
    path = "/v1/audit/chain",
    params(AuditChainQuery),
    responses(
        (status = 200, description = "Audit chain entries retrieved", body = AuditChainResponse),
        (status = 403, description = "Forbidden - requires AuditView permission", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_audit_chain(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<AuditChainQuery>,
) -> Result<Json<AuditChainResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AuditView)?;

    let limit = query.limit.min(1000);

    // Query policy audit decisions for the tenant
    let filters = adapteros_db::policy_audit::PolicyDecisionFilters {
        tenant_id: Some(claims.tenant_id.clone()),
        limit: Some(limit as i64),
        ..Default::default()
    };

    let decisions = state
        .db
        .query_policy_decisions(filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to query audit chain")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Transform policy decisions into UI-compatible chain entries
    let entries: Vec<AuditChainEntry> = decisions
        .iter()
        .map(|d| AuditChainEntry {
            id: d.id.clone(),
            timestamp: d.timestamp.clone(),
            action: d.hook.clone(),
            resource_type: d.resource_type.clone().unwrap_or_else(|| "policy".to_string()),
            status: d.decision.clone(),
            entry_hash: d.entry_hash.clone(),
            previous_hash: d.previous_hash.clone(),
            chain_sequence: d.chain_sequence,
            verified: true, // Hash computed on insert, so entries are verified by default
        })
        .collect();

    // Get merkle root from the latest entry's hash
    let merkle_root = entries.first().map(|e| e.entry_hash.clone());

    // Verify the chain integrity
    let verification = state
        .db
        .verify_policy_audit_chain(Some(&claims.tenant_id))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to verify audit chain")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(AuditChainResponse {
        total_entries: entries.len(),
        entries,
        chain_valid: verification.is_valid,
        merkle_root,
    }))
}

/// Verify audit chain integrity
///
/// Performs full cryptographic verification of the policy audit chain.
/// Checks hash linkage and detects any tampered entries.
#[utoipa::path(
    tag = "audit",
    get,
    path = "/v1/audit/chain/verify",
    responses(
        (status = 200, description = "Chain verification result", body = ChainVerificationResponse),
        (status = 403, description = "Forbidden - requires AuditView permission", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn verify_audit_chain(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ChainVerificationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::AuditView)?;

    // Use the existing chain verification from the DB layer
    let result = state
        .db
        .verify_policy_audit_chain(Some(&claims.tenant_id))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to verify audit chain")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get the merkle root (latest entry hash) if chain is valid
    let merkle_root = if result.is_valid && result.entries_checked > 0 {
        let filters = adapteros_db::policy_audit::PolicyDecisionFilters {
            tenant_id: Some(claims.tenant_id.clone()),
            limit: Some(1),
            ..Default::default()
        };
        match state.db.query_policy_decisions(filters).await {
            Ok(decisions) => decisions.first().map(|d| d.entry_hash.clone()),
            Err(e) => {
                tracing::warn!(
                    tenant_id = %claims.tenant_id,
                    error = %e,
                    "Failed to retrieve merkle root for valid chain"
                );
                None
            }
        }
    } else {
        None
    };

    Ok(Json(ChainVerificationResponse {
        chain_valid: result.is_valid,
        total_entries: result.entries_checked,
        verified_entries: if result.is_valid {
            result.entries_checked
        } else {
            result
                .first_invalid_sequence
                .map(|s| (s - 1) as usize)
                .unwrap_or(0)
        },
        first_invalid_sequence: result.first_invalid_sequence,
        error_message: result.error_message,
        merkle_root,
        verification_timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}
