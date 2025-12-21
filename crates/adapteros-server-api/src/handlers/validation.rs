use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::{AdapterName, StackName};
use adapteros_policy::{AdapterNameValidation, NamingConfig, NamingPolicy, StackNameValidation};
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

/// Request to validate an adapter name
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ValidateAdapterNameRequest {
    /// Adapter name to validate (format: {tenant}/{domain}/{purpose}/{revision})
    pub name: String,
    /// Tenant ID making the request
    pub tenant_id: String,
    /// Parent adapter name (if forking/extending)
    pub parent_name: Option<String>,
    /// Latest revision number in lineage (if extending)
    pub latest_revision: Option<u32>,
}

/// Response for adapter name validation
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ValidateAdapterNameResponse {
    /// Whether the name is valid
    pub valid: bool,
    /// List of validation violations (empty if valid)
    pub violations: Vec<NameViolationResponse>,
    /// Parsed adapter name components (if valid)
    pub parsed: Option<ParsedAdapterName>,
}

/// Parsed adapter name components
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ParsedAdapterName {
    pub tenant: String,
    pub domain: String,
    pub purpose: String,
    pub revision: String,
    pub revision_number: u32,
    pub base_path: String,
    pub display_name: String,
}

/// Name violation details
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NameViolationResponse {
    /// Violation type
    pub violation_type: String,
    /// Component that violated policy
    pub component: String,
    /// Detailed reason
    pub reason: String,
    /// Suggested fix
    pub suggestion: Option<String>,
}

/// Request to validate a stack name
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ValidateStackNameRequest {
    /// Stack name to validate (format: stack.{namespace}[.{identifier}])
    pub name: String,
    /// Tenant ID making the request
    pub tenant_id: String,
}

/// Response for stack name validation
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ValidateStackNameResponse {
    /// Whether the name is valid
    pub valid: bool,
    /// List of validation violations (empty if valid)
    pub violations: Vec<NameViolationResponse>,
    /// Parsed stack name components (if valid)
    pub parsed: Option<ParsedStackName>,
}

/// Parsed stack name components
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ParsedStackName {
    pub namespace: String,
    pub identifier: Option<String>,
    pub full_name: String,
}

/// Validate an adapter name
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/adapters/validate-name",
    request_body = ValidateAdapterNameRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateAdapterNameResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "adapters"
)]
pub async fn validate_adapter_name(
    State(state): State<AppState>,
    Json(req): Json<ValidateAdapterNameRequest>,
) -> Result<Json<ValidateAdapterNameResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Create naming policy with default config
    let policy = NamingPolicy::new(NamingConfig::default());

    // Create validation request
    let validation_req = AdapterNameValidation {
        name: req.name.clone(),
        tenant_id: req.tenant_id.clone(),
        parent_name: req.parent_name.clone(),
        latest_revision: req.latest_revision,
    };

    // Analyze the name for violations
    let violations = policy.analyze_adapter_name(&validation_req);

    // Try to parse the name if no violations
    let parsed = if violations.is_empty() {
        AdapterName::parse(&req.name)
            .ok()
            .map(|name| ParsedAdapterName {
                tenant: name.tenant().to_string(),
                domain: name.domain().to_string(),
                purpose: name.purpose().to_string(),
                revision: name.revision().to_string(),
                revision_number: name.revision_number().unwrap_or(0),
                base_path: name.base_path().to_string(),
                display_name: name.display_name().to_string(),
            })
    } else {
        None
    };

    // Convert violations to response format
    let violation_responses: Vec<_> = violations
        .iter()
        .map(|v| NameViolationResponse {
            violation_type: format!("{:?}", v.violation_type),
            component: v.component.clone(),
            reason: v.reason.clone(),
            suggestion: v.suggestion.clone(),
        })
        .collect();

    Ok(Json(ValidateAdapterNameResponse {
        valid: violation_responses.is_empty(),
        violations: violation_responses,
        parsed,
    }))
}

/// Validate a stack name
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/stacks/validate-name",
    request_body = ValidateStackNameRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateStackNameResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "stacks"
)]
pub async fn validate_stack_name(
    State(state): State<AppState>,
    Json(req): Json<ValidateStackNameRequest>,
) -> Result<Json<ValidateStackNameResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Create naming policy with default config
    let policy = NamingPolicy::new(NamingConfig::default());

    // Create validation request
    let validation_req = StackNameValidation {
        name: req.name.clone(),
        tenant_id: req.tenant_id.clone(),
    };

    // Validate the stack name
    let result = policy.validate_stack_name(&validation_req);

    // Try to parse the name
    let parsed = StackName::parse(&req.name)
        .ok()
        .map(|name| ParsedStackName {
            namespace: name.namespace().to_string(),
            identifier: name.identifier().map(|s| s.to_string()),
            full_name: name.to_string(),
        });

    // Convert error to violation if validation failed
    let violations = if let Err(e) = result {
        vec![NameViolationResponse {
            violation_type: "ValidationError".to_string(),
            component: req.name.clone(),
            reason: e.to_string(),
            suggestion: None,
        }]
    } else {
        vec![]
    };

    let valid = violations.is_empty();
    let parsed_result = if valid { parsed } else { None };

    Ok(Json(ValidateStackNameResponse {
        valid,
        violations,
        parsed: parsed_result,
    }))
}
