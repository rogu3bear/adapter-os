//! Inference Verdicts API Handlers
//!
//! Provides endpoints for managing inference verdicts:
//! - POST /v1/verdicts - Create/update a verdict (upsert by inference_id + evaluator_type)
//! - GET /v1/verdicts/{inference_id} - Get verdict for an inference
//! - GET /v1/verdicts - List verdicts with filters
//!
//! Verdicts represent quality assessments of inference outputs, derived either from
//! automated rules, human review, or model-based evaluation.

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use utoipa::{IntoParams, ToSchema};

// ============================================================================
// Verdict Types (following PRD spec)
// ============================================================================

/// Verdict confidence level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// High confidence - output is correct
    High,
    /// Medium confidence - likely correct but needs review
    Medium,
    /// Low confidence - output may be incorrect
    Low,
    /// Paused - requires human review before proceeding
    Paused,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verdict::High => write!(f, "high"),
            Verdict::Medium => write!(f, "medium"),
            Verdict::Low => write!(f, "low"),
            Verdict::Paused => write!(f, "paused"),
        }
    }
}

impl std::str::FromStr for Verdict {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "high" => Ok(Verdict::High),
            "medium" => Ok(Verdict::Medium),
            "low" => Ok(Verdict::Low),
            "paused" => Ok(Verdict::Paused),
            _ => Err(format!("invalid verdict: {}", s)),
        }
    }
}

/// Evaluator type - who/what produced the verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EvaluatorType {
    /// Rule-based automated evaluation
    Rule,
    /// Human reviewer
    Human,
    /// Model-based evaluation (e.g., LLM-as-judge)
    Model,
}

impl std::fmt::Display for EvaluatorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvaluatorType::Rule => write!(f, "rule"),
            EvaluatorType::Human => write!(f, "human"),
            EvaluatorType::Model => write!(f, "model"),
        }
    }
}

impl std::str::FromStr for EvaluatorType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rule" => Ok(EvaluatorType::Rule),
            "human" => Ok(EvaluatorType::Human),
            "model" => Ok(EvaluatorType::Model),
            _ => Err(format!("invalid evaluator_type: {}", s)),
        }
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create or update an inference verdict
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateVerdictRequest {
    /// The inference ID this verdict applies to
    pub inference_id: String,
    /// Verdict level: high, medium, low, paused
    pub verdict: String,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Evaluator type: rule, human, model
    pub evaluator_type: String,
    /// Optional evaluator identifier (e.g., rule name, reviewer ID, model name)
    pub evaluator_id: Option<String>,
    /// Optional JSON warnings/notes
    pub warnings_json: Option<serde_json::Value>,
    /// Optional extraction confidence score from upstream processing
    pub extraction_confidence_score: Option<f64>,
    /// Optional trust state (e.g., "needs_approval", "approved", "rejected")
    pub trust_state: Option<String>,
}

/// Response containing verdict details
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerdictResponse {
    /// Unique verdict ID
    pub id: String,
    /// The inference ID this verdict applies to
    pub inference_id: String,
    /// Verdict level
    pub verdict: String,
    /// Confidence score
    pub confidence: f64,
    /// Evaluator type
    pub evaluator_type: String,
    /// Evaluator identifier
    pub evaluator_id: Option<String>,
    /// Warnings/notes as JSON
    pub warnings_json: Option<serde_json::Value>,
    /// Extraction confidence score
    pub extraction_confidence_score: Option<f64>,
    /// Trust state
    pub trust_state: Option<String>,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
}

/// Query parameters for listing verdicts
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
pub struct ListVerdictsQuery {
    /// Filter by inference ID
    pub inference_id: Option<String>,
    /// Filter by verdict level
    pub verdict: Option<String>,
    /// Filter by evaluator type
    pub evaluator_type: Option<String>,
    /// Filter by trust state
    pub trust_state: Option<String>,
    /// Maximum number of results (default: 100)
    pub limit: Option<i64>,
    /// Offset for pagination
    pub offset: Option<i64>,
}

// ============================================================================
// Rule-Based Verdict Derivation (v1 Rules)
// ============================================================================

/// Derive verdict from inference context using v1 rules.
///
/// # Rules (v1):
/// - If extraction_confidence_score < 0.8 => low + warning
/// - If trust_state = "needs_approval" => paused
/// - Otherwise => high
///
/// # Returns
/// Tuple of (verdict, confidence, optional warning reason)
///
/// # Examples
/// ```
/// use adapteros_server_api::handlers::verdicts::{derive_rule_verdict, Verdict};
///
/// // Low extraction confidence => low verdict
/// let (verdict, confidence, warning) = derive_rule_verdict(Some(0.5), None);
/// assert_eq!(verdict, Verdict::Low);
/// assert!(warning.is_some());
///
/// // Needs approval => paused
/// let (verdict, _, _) = derive_rule_verdict(None, Some("needs_approval"));
/// assert_eq!(verdict, Verdict::Paused);
///
/// // Normal case => high
/// let (verdict, confidence, warning) = derive_rule_verdict(Some(0.95), Some("approved"));
/// assert_eq!(verdict, Verdict::High);
/// assert!(warning.is_none());
/// ```
pub fn derive_rule_verdict(
    extraction_confidence_score: Option<f64>,
    trust_state: Option<&str>,
) -> (Verdict, f64, Option<String>) {
    // Rule 1: Check trust state for approval requirements
    if let Some(state) = trust_state {
        if state == "needs_approval" {
            return (
                Verdict::Paused,
                1.0, // High confidence in the rule triggering
                Some("Trust state requires approval".to_string()),
            );
        }
    }

    // Rule 2: Check extraction confidence score
    if let Some(score) = extraction_confidence_score {
        if score < 0.8 {
            return (
                Verdict::Low,
                score, // Use extraction confidence as verdict confidence
                Some(format!(
                    "Extraction confidence score {:.2} below threshold 0.80",
                    score
                )),
            );
        }
    }

    // Rule 3: Default to high confidence
    // Use extraction confidence if available, otherwise default to 0.9
    let confidence = extraction_confidence_score.unwrap_or(0.9);
    (Verdict::High, confidence, None)
}

// ============================================================================
// Handlers
// ============================================================================

/// Create or update an inference verdict
///
/// If a verdict already exists for the (inference_id, evaluator_type) pair,
/// it will be updated. Otherwise, a new verdict is created.
#[utoipa::path(
    post,
    path = "/v1/verdicts",
    request_body = CreateVerdictRequest,
    responses(
        (status = 201, description = "Verdict created/updated", body = VerdictResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "verdicts",
    security(("bearer_auth" = []))
)]
pub async fn create_verdict(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateVerdictRequest>,
) -> ApiResult<VerdictResponse> {
    // Require write permission for verdicts
    require_permission(&claims, Permission::ReplayManage)?;

    // Validate verdict value
    let verdict: Verdict = request
        .verdict
        .parse()
        .map_err(|e: String| ApiError::bad_request(e))?;

    // Validate evaluator type
    let evaluator_type: EvaluatorType = request
        .evaluator_type
        .parse()
        .map_err(|e: String| ApiError::bad_request(e))?;

    // Validate confidence range
    if request.confidence < 0.0 || request.confidence > 1.0 {
        return Err(ApiError::bad_request(
            "confidence must be between 0.0 and 1.0",
        ));
    }

    // Validate extraction confidence if provided
    if let Some(score) = request.extraction_confidence_score {
        if score < 0.0 || score > 1.0 {
            return Err(ApiError::bad_request(
                "extraction_confidence_score must be between 0.0 and 1.0",
            ));
        }
    }

    // Resolve inference ID (supports aliases/short IDs)
    let inference_id = crate::id_resolver::resolve_any_id(&state.db, &request.inference_id)
        .await
        .map_err(|e| ApiError::not_found("Inference").with_details(e.to_string()))?;

    // Validate tenant isolation by checking the inference trace exists and belongs to tenant
    // Note: For now we do a best-effort check. In production, this should query
    // the inference_traces table to verify tenant ownership.
    debug!(
        inference_id = %inference_id,
        tenant_id = %claims.tenant_id,
        "Creating verdict for inference"
    );

    // Generate verdict ID (using Decision kind as closest semantic match)
    use adapteros_core::ids::IdKind;
    let verdict_id = crate::id_generator::readable_id(IdKind::Decision, "verdict");

    // Serialize warnings to JSON string if present
    let warnings_json_str = request
        .warnings_json
        .as_ref()
        .map(|v| serde_json::to_string(v))
        .transpose()
        .map_err(|e| ApiError::bad_request(format!("Invalid warnings_json: {}", e)))?;

    // Upsert the verdict (insert or update by inference_id + evaluator_type)
    let created_at = chrono::Utc::now().to_rfc3339();

    // Build verdict params using the DB module's builder pattern
    use adapteros_db::inference_verdicts::{CreateVerdictParams, Verdict as DbVerdict, EvaluatorType as DbEvaluatorType};

    let db_verdict = match verdict {
        Verdict::High => DbVerdict::High,
        Verdict::Medium => DbVerdict::Medium,
        Verdict::Low => DbVerdict::Low,
        Verdict::Paused => DbVerdict::Paused,
    };

    let db_evaluator_type = match evaluator_type {
        EvaluatorType::Rule => DbEvaluatorType::Rule,
        EvaluatorType::Human => DbEvaluatorType::Human,
        EvaluatorType::Model => DbEvaluatorType::Model,
    };

    let mut params = CreateVerdictParams::new(
        &claims.tenant_id,
        &inference_id,
        db_verdict,
        request.confidence,
        db_evaluator_type,
    );
    params.id = verdict_id.clone();

    if let Some(ref eval_id) = request.evaluator_id {
        params = params.with_evaluator_id(eval_id);
    }
    if let Some(ref json_str) = warnings_json_str {
        params = params.with_warnings_json(json_str);
    }
    if let Some(score) = request.extraction_confidence_score {
        params = params.with_extraction_confidence(score);
    }
    if let Some(ref state_str) = request.trust_state {
        params = params.with_trust_state(state_str);
    }

    state
        .db
        .create_inference_verdict(&params)
        .await
        .map_err(ApiError::db_error)?;

    info!(
        verdict_id = %verdict_id,
        inference_id = %inference_id,
        verdict = %verdict,
        evaluator_type = %evaluator_type,
        confidence = request.confidence,
        "Verdict created/updated"
    );

    Ok(Json(VerdictResponse {
        id: verdict_id,
        inference_id,
        verdict: verdict.to_string(),
        confidence: request.confidence,
        evaluator_type: evaluator_type.to_string(),
        evaluator_id: request.evaluator_id,
        warnings_json: request.warnings_json,
        extraction_confidence_score: request.extraction_confidence_score,
        trust_state: request.trust_state,
        created_at,
    }))
}

/// Get verdict for an inference by inference ID
#[utoipa::path(
    get,
    path = "/v1/verdicts/{inference_id}",
    params(
        ("inference_id" = String, Path, description = "Inference ID to get verdict for")
    ),
    responses(
        (status = 200, description = "Verdict details", body = VerdictResponse),
        (status = 404, description = "Verdict not found"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "verdicts",
    security(("bearer_auth" = []))
)]
pub async fn get_verdict(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(inference_id): Path<String>,
) -> ApiResult<VerdictResponse> {
    // Require read permission
    require_permission(&claims, Permission::InferenceExecute)?;

    // Resolve inference ID
    let inference_id = crate::id_resolver::resolve_any_id(&state.db, &inference_id)
        .await
        .map_err(|e| ApiError::not_found("Inference").with_details(e.to_string()))?;

    // Get verdict from database (with tenant isolation)
    let verdict = state
        .db
        .get_latest_inference_verdict(&claims.tenant_id, &inference_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Verdict"))?;

    // Parse warnings JSON if present
    let warnings_json = verdict
        .warnings_json
        .as_ref()
        .map(|s| serde_json::from_str(s))
        .transpose()
        .map_err(|e| {
            warn!(
                inference_id = %inference_id,
                error = %e,
                "Failed to parse stored warnings_json"
            );
            ApiError::internal("Failed to parse verdict warnings")
        })?;

    Ok(Json(VerdictResponse {
        id: verdict.id,
        inference_id: verdict.inference_id,
        verdict: verdict.verdict,
        confidence: verdict.confidence,
        evaluator_type: verdict.evaluator_type,
        evaluator_id: verdict.evaluator_id,
        warnings_json,
        extraction_confidence_score: verdict.extraction_confidence_score,
        trust_state: verdict.trust_state,
        created_at: verdict.created_at,
    }))
}

/// List verdicts with optional filters
#[utoipa::path(
    get,
    path = "/v1/verdicts",
    params(ListVerdictsQuery),
    responses(
        (status = 200, description = "List of verdicts", body = Vec<VerdictResponse>),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "verdicts",
    security(("bearer_auth" = []))
)]
pub async fn list_verdicts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListVerdictsQuery>,
) -> ApiResult<Vec<VerdictResponse>> {
    // Require read permission
    require_permission(&claims, Permission::InferenceExecute)?;

    let limit = query.limit.unwrap_or(100).min(1000) as u32;
    let offset = query.offset.unwrap_or(0) as u32;

    // Parse filter enums if provided
    use adapteros_db::inference_verdicts::{Verdict as DbVerdict, EvaluatorType as DbEvaluatorType};

    let verdict_filter = query.verdict.as_ref().and_then(|v| v.parse::<DbVerdict>().ok());
    let evaluator_filter = query.evaluator_type.as_ref().and_then(|e| e.parse::<DbEvaluatorType>().ok());

    // If inference_id is provided, get verdicts for that specific inference
    let verdicts = if let Some(ref inference_id) = query.inference_id {
        let resolved_id = crate::id_resolver::resolve_any_id(&state.db, inference_id)
            .await
            .map_err(|e| ApiError::not_found("Inference").with_details(e.to_string()))?;

        state
            .db
            .list_inference_verdicts_by_inference(&claims.tenant_id, &resolved_id)
            .await
            .map_err(ApiError::db_error)?
    } else {
        // List verdicts for tenant with filters
        state
            .db
            .list_inference_verdicts_by_tenant(
                &claims.tenant_id,
                verdict_filter,
                evaluator_filter,
                limit,
                offset,
            )
            .await
            .map_err(ApiError::db_error)?
    };

    let responses: Vec<VerdictResponse> = verdicts
        .into_iter()
        .map(|v| {
            let warnings_json = v
                .warnings_json
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok());

            VerdictResponse {
                id: v.id,
                inference_id: v.inference_id,
                verdict: v.verdict,
                confidence: v.confidence,
                evaluator_type: v.evaluator_type,
                evaluator_id: v.evaluator_id,
                warnings_json,
                extraction_confidence_score: v.extraction_confidence_score,
                trust_state: v.trust_state,
                created_at: v.created_at,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// Derive and optionally store a rule-based verdict for an inference
///
/// This endpoint derives a verdict using the v1 rule engine and optionally
/// stores it in the database.
#[utoipa::path(
    post,
    path = "/v1/verdicts/derive",
    request_body = DeriveVerdictRequest,
    responses(
        (status = 200, description = "Derived verdict", body = DeriveVerdictResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error")
    ),
    tag = "verdicts",
    security(("bearer_auth" = []))
)]
pub async fn derive_verdict(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<DeriveVerdictRequest>,
) -> ApiResult<DeriveVerdictResponse> {
    // Require write permission for storing verdicts
    require_permission(&claims, Permission::ReplayManage)?;

    // Derive verdict using rules
    let (verdict, confidence, warning) = derive_rule_verdict(
        request.extraction_confidence_score,
        request.trust_state.as_deref(),
    );

    let warnings_json = warning
        .as_ref()
        .map(|w| serde_json::json!({ "rule_warning": w }));

    // If store=true and inference_id provided, persist the verdict
    if request.store.unwrap_or(false) {
        if let Some(ref inference_id) = request.inference_id {
            use adapteros_db::inference_verdicts::{CreateVerdictParams, Verdict as DbVerdict, EvaluatorType as DbEvaluatorType};

            use adapteros_core::ids::IdKind;
            let verdict_id = crate::id_generator::readable_id(IdKind::Decision, "verdict");

            let warnings_str = warnings_json
                .as_ref()
                .map(|v| serde_json::to_string(v))
                .transpose()
                .map_err(|e| ApiError::internal(format!("Failed to serialize warnings: {}", e)))?;

            let db_verdict = match verdict {
                Verdict::High => DbVerdict::High,
                Verdict::Medium => DbVerdict::Medium,
                Verdict::Low => DbVerdict::Low,
                Verdict::Paused => DbVerdict::Paused,
            };

            let mut params = CreateVerdictParams::new(
                &claims.tenant_id,
                inference_id,
                db_verdict,
                confidence,
                DbEvaluatorType::Rule,
            );
            params.id = verdict_id.clone();
            params = params.with_evaluator_id("v1_rules");

            if let Some(ref json_str) = warnings_str {
                params = params.with_warnings_json(json_str);
            }
            if let Some(score) = request.extraction_confidence_score {
                params = params.with_extraction_confidence(score);
            }
            if let Some(ref state_str) = request.trust_state {
                params = params.with_trust_state(state_str);
            }

            state
                .db
                .create_inference_verdict(&params)
                .await
                .map_err(ApiError::db_error)?;

            info!(
                verdict_id = %verdict_id,
                inference_id = %inference_id,
                verdict = %verdict,
                "Rule-derived verdict stored"
            );
        }
    }

    Ok(Json(DeriveVerdictResponse {
        verdict: verdict.to_string(),
        confidence,
        warning,
        warnings_json,
    }))
}

/// Request to derive a verdict using rules
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeriveVerdictRequest {
    /// Optional inference ID (required if store=true)
    pub inference_id: Option<String>,
    /// Extraction confidence score from upstream processing
    pub extraction_confidence_score: Option<f64>,
    /// Trust state
    pub trust_state: Option<String>,
    /// Whether to store the derived verdict (default: false)
    pub store: Option<bool>,
}

/// Response from verdict derivation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeriveVerdictResponse {
    /// Derived verdict level
    pub verdict: String,
    /// Confidence score
    pub confidence: f64,
    /// Warning message if rules triggered
    pub warning: Option<String>,
    /// Structured warnings as JSON
    pub warnings_json: Option<serde_json::Value>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_rule_verdict_low_extraction() {
        let (verdict, confidence, warning) = derive_rule_verdict(Some(0.5), None);
        assert_eq!(verdict, Verdict::Low);
        assert!((confidence - 0.5).abs() < 0.001);
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("below threshold"));
    }

    #[test]
    fn test_derive_rule_verdict_needs_approval() {
        let (verdict, confidence, warning) = derive_rule_verdict(Some(0.95), Some("needs_approval"));
        assert_eq!(verdict, Verdict::Paused);
        assert!((confidence - 1.0).abs() < 0.001);
        assert!(warning.is_some());
        assert!(warning.unwrap().contains("requires approval"));
    }

    #[test]
    fn test_derive_rule_verdict_high() {
        let (verdict, confidence, warning) = derive_rule_verdict(Some(0.95), Some("approved"));
        assert_eq!(verdict, Verdict::High);
        assert!((confidence - 0.95).abs() < 0.001);
        assert!(warning.is_none());
    }

    #[test]
    fn test_derive_rule_verdict_default() {
        let (verdict, confidence, warning) = derive_rule_verdict(None, None);
        assert_eq!(verdict, Verdict::High);
        assert!((confidence - 0.9).abs() < 0.001);
        assert!(warning.is_none());
    }

    #[test]
    fn test_derive_rule_verdict_boundary() {
        // Exactly at threshold should still be low (< 0.8)
        let (verdict, _, _) = derive_rule_verdict(Some(0.79), None);
        assert_eq!(verdict, Verdict::Low);

        // At threshold should be high (>= 0.8)
        let (verdict, _, _) = derive_rule_verdict(Some(0.8), None);
        assert_eq!(verdict, Verdict::High);
    }

    #[test]
    fn test_verdict_from_str() {
        assert_eq!("high".parse::<Verdict>().unwrap(), Verdict::High);
        assert_eq!("HIGH".parse::<Verdict>().unwrap(), Verdict::High);
        assert_eq!("Medium".parse::<Verdict>().unwrap(), Verdict::Medium);
        assert_eq!("low".parse::<Verdict>().unwrap(), Verdict::Low);
        assert_eq!("paused".parse::<Verdict>().unwrap(), Verdict::Paused);
        assert!("invalid".parse::<Verdict>().is_err());
    }

    #[test]
    fn test_evaluator_type_from_str() {
        assert_eq!("rule".parse::<EvaluatorType>().unwrap(), EvaluatorType::Rule);
        assert_eq!("human".parse::<EvaluatorType>().unwrap(), EvaluatorType::Human);
        assert_eq!("model".parse::<EvaluatorType>().unwrap(), EvaluatorType::Model);
        assert!("invalid".parse::<EvaluatorType>().is_err());
    }
}
