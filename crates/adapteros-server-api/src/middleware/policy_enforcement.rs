//! Policy enforcement middleware for AdapterOS
//!
//! Validates all HTTP requests against policy packs at runtime to ensure
//! compliance with security, performance, and operational policies.
//!
//! # Policy Enforcement Flow
//!
//! 1. Extract request context (tenant_id, user role, operation type)
//! 2. Construct PolicyRequest from HTTP request
//! 3. Call PolicyPackManager::validate_request()
//! 4. Block requests with Error/Critical/Blocker severity violations
//! 5. Log all violations with proper context
//! 6. Return HTTP 403 for policy violations
//!
//! # Citations
//! - CLAUDE.md L142: "Policy Engine: Enforces 20 policy packs"
//! - Policy Packs: Egress, Determinism, Router, Evidence, Refusal, etc.
//!
//! [source: crates/adapteros-server-api/src/middleware/policy_enforcement.rs]

use crate::auth::Claims;
use crate::middleware::request_id::RequestId;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_policy::policy_packs::{PolicyContext, PolicyRequest, Priority, RequestType};
use adapteros_policy::{PolicyViolation, ViolationSeverity};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Policy enforcement middleware
///
/// Validates requests against all enabled policy packs. Blocks requests
/// that violate policies with Error, Critical, or Blocker severity.
///
/// # Request Flow
///
/// 1. Extracts Claims from request extensions (set by auth_middleware)
/// 2. Extracts RequestId for correlation
/// 3. Constructs PolicyRequest with operation context
/// 4. Validates against PolicyPackManager
/// 5. Logs violations
/// 6. Blocks or allows based on violation severity
///
/// # Severity Handling
///
/// - Info: Log only, allow request
/// - Warning: Log only, allow request
/// - Error: Block request, return 403
/// - Critical: Block request, return 403
/// - Blocker: Block request, return 403
pub async fn policy_enforcement_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Extract request context
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map(|id| id.as_str().to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let claims = req.extensions().get::<Claims>().cloned();

    // Extract operation context from request
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Determine request type from path
    let request_type = determine_request_type(&path);

    // Determine operation name from method and path
    let operation = format!("{} {}", method.as_str(), path);

    // Extract tenant_id and user_id from claims
    let tenant_id = claims.as_ref().map(|c| c.tenant_id.clone());
    let user_id = claims.as_ref().map(|c| c.sub.clone());

    // Determine priority based on request type and user role
    let priority = determine_priority(&request_type, claims.as_ref());

    // Construct policy request
    let policy_request = PolicyRequest {
        request_id: request_id.clone(),
        request_type,
        tenant_id,
        user_id,
        context: PolicyContext {
            component: "api-server".to_string(),
            operation: operation.clone(),
            data: None,
            priority,
        },
        metadata: None,
    };

    debug!(
        request_id = %request_id,
        operation = %operation,
        "Validating request against policy packs"
    );

    // Validate against policy packs
    let policy_manager = Arc::clone(&state.policy_manager);

    match policy_manager.validate_request(&policy_request) {
        Ok(validation_result) => {
            if !validation_result.valid {
                // Request has policy violations
                let blocking_violations: Vec<&PolicyViolation> = validation_result
                    .violations
                    .iter()
                    .filter(|v| is_blocking_severity(&v.severity))
                    .collect();

                if !blocking_violations.is_empty() {
                    // Log all violations
                    for violation in &validation_result.violations {
                        log_violation(&request_id, &operation, violation);
                    }

                    // Return 403 with detailed violation information
                    let violation_messages: Vec<String> = blocking_violations
                        .iter()
                        .map(|v| format!("{}: {}", v.policy_pack, v.message))
                        .collect();

                    warn!(
                        request_id = %request_id,
                        operation = %operation,
                        violations = blocking_violations.len(),
                        "Request blocked by policy violations"
                    );

                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(
                            ErrorResponse::new("policy violation")
                                .with_code("POLICY_VIOLATION")
                                .with_string_details(format!(
                                    "Request violates {} policy pack(s): {}",
                                    blocking_violations.len(),
                                    violation_messages.join("; ")
                                )),
                        ),
                    ));
                }

                // Non-blocking violations (Info, Warning) - log and allow
                for violation in &validation_result.violations {
                    log_violation(&request_id, &operation, violation);
                }
            }

            // Log warnings
            for warning in &validation_result.warnings {
                debug!(
                    request_id = %request_id,
                    operation = %operation,
                    policy_pack = %warning.policy_pack,
                    message = %warning.message,
                    "Policy warning"
                );
            }

            info!(
                request_id = %request_id,
                operation = %operation,
                violations = validation_result.violations.len(),
                warnings = validation_result.warnings.len(),
                duration_ms = validation_result.duration_ms,
                "Policy validation completed"
            );

            // Request passed validation, continue to handler
            Ok(next.run(req).await)
        }
        Err(e) => {
            // Policy validation failed (system error, not policy violation)
            error!(
                request_id = %request_id,
                operation = %operation,
                error = %e,
                "Policy validation failed"
            );

            // Return 500 for policy evaluation errors
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("policy validation failed")
                        .with_code("POLICY_ERROR")
                        .with_string_details(format!("Failed to validate request: {}", e)),
                ),
            ))
        }
    }
}

/// Determine request type from URL path
fn determine_request_type(path: &str) -> RequestType {
    if path.starts_with("/v1/infer") || path.starts_with("/v1/streaming/infer") {
        RequestType::Inference
    } else if path.starts_with("/v1/adapters") || path.starts_with("/v1/adapter-stacks") {
        RequestType::AdapterOperation
    } else if path.starts_with("/v1/training") || path.starts_with("/v1/datasets") {
        RequestType::TrainingOperation
    } else if path.starts_with("/v1/policies") {
        RequestType::PolicyUpdate
    } else if path.starts_with("/v1/system") || path.starts_with("/v1/metrics") {
        RequestType::SystemOperation
    } else if path.starts_with("/v1/users") || path.starts_with("/v1/tenants") {
        RequestType::UserOperation
    } else if path.starts_with("/v1/documents") || path.starts_with("/v1/collections") {
        RequestType::FileOperation
    } else {
        RequestType::SystemOperation
    }
}

/// Determine request priority based on request type and user role
fn determine_priority(request_type: &RequestType, claims: Option<&Claims>) -> Priority {
    match request_type {
        RequestType::Inference => Priority::High,
        RequestType::TrainingOperation => Priority::Normal,
        RequestType::PolicyUpdate => {
            // Admin policy updates are critical
            if let Some(claims) = claims {
                if claims.role == "admin" {
                    Priority::Critical
                } else {
                    Priority::High
                }
            } else {
                Priority::High
            }
        }
        RequestType::SystemOperation => Priority::High,
        _ => Priority::Normal,
    }
}

/// Check if a violation severity should block the request
fn is_blocking_severity(severity: &ViolationSeverity) -> bool {
    matches!(
        severity,
        ViolationSeverity::High | ViolationSeverity::Critical
    )
}

/// Log a policy violation with appropriate severity
fn log_violation(request_id: &str, operation: &str, violation: &PolicyViolation) {
    match violation.severity {
        ViolationSeverity::Low => {
            info!(
                request_id = %request_id,
                operation = %operation,
                policy_pack = %violation.policy_pack,
                violation_id = %violation.violation_id,
                message = %violation.message,
                "Policy violation (Low)"
            );
        }
        ViolationSeverity::Medium => {
            warn!(
                request_id = %request_id,
                operation = %operation,
                policy_pack = %violation.policy_pack,
                violation_id = %violation.violation_id,
                message = %violation.message,
                "Policy violation (Medium)"
            );
        }
        ViolationSeverity::High => {
            error!(
                request_id = %request_id,
                operation = %operation,
                policy_pack = %violation.policy_pack,
                violation_id = %violation.violation_id,
                message = %violation.message,
                remediation = ?violation.remediation,
                "Policy violation (High)"
            );
        }
        ViolationSeverity::Critical => {
            error!(
                request_id = %request_id,
                operation = %operation,
                policy_pack = %violation.policy_pack,
                violation_id = %violation.violation_id,
                message = %violation.message,
                remediation = ?violation.remediation,
                "Policy violation (Critical)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_request_type() {
        assert!(matches!(
            determine_request_type("/v1/infer"),
            RequestType::Inference
        ));
        assert!(matches!(
            determine_request_type("/v1/adapters/list"),
            RequestType::AdapterOperation
        ));
        assert!(matches!(
            determine_request_type("/v1/training/jobs"),
            RequestType::TrainingOperation
        ));
        assert!(matches!(
            determine_request_type("/v1/policies/list"),
            RequestType::PolicyUpdate
        ));
    }

    #[test]
    fn test_is_blocking_severity() {
        assert!(!is_blocking_severity(&ViolationSeverity::Low));
        assert!(!is_blocking_severity(&ViolationSeverity::Medium));
        assert!(is_blocking_severity(&ViolationSeverity::High));
        assert!(is_blocking_severity(&ViolationSeverity::Critical));
    }

    #[test]
    fn test_determine_priority() {
        assert!(matches!(
            determine_priority(&RequestType::Inference, None),
            Priority::High
        ));
        assert!(matches!(
            determine_priority(&RequestType::TrainingOperation, None),
            Priority::Normal
        ));
    }
}
