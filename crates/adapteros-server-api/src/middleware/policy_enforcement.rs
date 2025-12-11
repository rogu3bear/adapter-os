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
use adapteros_policy::hooks::{Decision, HookContext, PolicyDecision, PolicyHook};
use adapteros_policy::policy_packs::{PolicyContext, PolicyRequest, Priority, RequestType};
use adapteros_policy::{PolicyViolation, ViolationSeverity};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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
        .map(|id| id.0.clone())
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

    // #region agent log
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
    {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let log_line = format!(
            r#"{{"sessionId":"debug-session","runId":"pre-fix","hypothesisId":"H2","location":"middleware/policy_enforcement.rs:pre-validate","message":"policy request","data":{{"request_id":"{}","operation":"{}","request_type":"{:?}","priority":"{:?}","tenant_present":{},"user_present":{}}},"timestamp":{}}}"#,
            policy_request.request_id,
            operation,
            policy_request.request_type,
            policy_request.context.priority,
            policy_request.tenant_id.is_some(),
            policy_request.user_id.is_some(),
            timestamp_ms
        );
        let _ = writeln!(file, "{log_line}");
    }
    // #endregion

    debug!(
        request_id = %request_id,
        operation = %operation,
        "Validating request against policy packs"
    );

    // Validate against policy packs
    let policy_manager = Arc::clone(&state.policy_manager);

    match policy_manager.validate_request(&policy_request) {
        Ok(validation_result) => {
            let blocking_count = validation_result
                .violations
                .iter()
                .filter(|v| is_blocking_severity(&v.severity))
                .count();

            // #region agent log
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
            {
                let timestamp_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let log_line = format!(
                    r#"{{"sessionId":"debug-session","runId":"pre-fix","hypothesisId":"H2","location":"middleware/policy_enforcement.rs:post-validate","message":"policy result","data":{{"request_id":"{}","valid":{},"violations":{},"warnings":{},"blocking":{}}},"timestamp":{}}}"#,
                    policy_request.request_id,
                    validation_result.valid,
                    validation_result.violations.len(),
                    validation_result.warnings.len(),
                    blocking_count,
                    timestamp_ms
                );
                let _ = writeln!(file, "{log_line}");
            }
            // #endregion

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
            // #region agent log
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
            {
                let timestamp_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let log_line = format!(
                    r#"{{"sessionId":"debug-session","runId":"pre-fix","hypothesisId":"H3","location":"middleware/policy_enforcement.rs:error","message":"policy validation error","data":{{"request_id":"{}","operation":"{}","error":"{}"}},"timestamp":{}}}"#,
                    policy_request.request_id, operation, e, timestamp_ms
                );
                let _ = writeln!(file, "{log_line}");
            }
            // #endregion
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
    } else if path.starts_with("/v1/adapters")
        || path.starts_with("/v1/adapter-stacks")
        || path.starts_with("/v1/packages")
    {
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

/// Error type for policy hook enforcement violations (PRD-06)
#[derive(Debug)]
pub struct PolicyHookViolationError {
    pub violations: Vec<PolicyDecision>,
    pub message: String,
}

impl std::fmt::Display for PolicyHookViolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PolicyHookViolationError {}

/// Enforce policies at a specific hook for a tenant (PRD-06)
///
/// This function implements per-tenant policy enforcement at specific lifecycle hooks:
/// - OnRequestBeforeRouting: Before adapter selection
/// - OnBeforeInference: After routing, before inference execution
/// - OnAfterInference: After inference completes
///
/// # Flow
/// 1. Query tenant's enabled policies for this hook from tenant_policy_bindings
/// 2. Validate each enabled policy
/// 3. Log ALL decisions (allow/deny) to policy_audit_decisions with Merkle chain
/// 4. Return error if any policy denies
///
/// # Arguments
/// * `state` - Application state with db and policy_manager
/// * `ctx` - Hook context with tenant_id, request_id, etc.
///
/// # Returns
/// * `Ok(Vec<PolicyDecision>)` - All decisions (including allows)
/// * `Err(PolicyHookViolationError)` - If any policy denied
pub async fn enforce_at_hook(
    state: &AppState,
    ctx: &HookContext,
) -> Result<Vec<PolicyDecision>, PolicyHookViolationError> {
    let hook_name = ctx.hook.name();

    debug!(
        tenant_id = %ctx.tenant_id,
        request_id = %ctx.request_id,
        hook = %hook_name,
        resource_type = %ctx.resource_type,
        "Enforcing policies at hook"
    );

    // 1. Get tenant's active policies for this hook
    let active_policies = match state
        .db
        .get_active_policies_for_tenant(&ctx.tenant_id)
        .await
    {
        Ok(policies) => policies,
        Err(e) => {
            error!(
                tenant_id = %ctx.tenant_id,
                hook = %hook_name,
                error = %e,
                "Failed to get active policies for tenant"
            );
            // On DB error, fail open with warning (or fail closed based on policy)
            return Ok(vec![]);
        }
    };

    if active_policies.is_empty() {
        debug!(
            tenant_id = %ctx.tenant_id,
            hook = %hook_name,
            "No active policies for tenant at this hook"
        );
        return Ok(vec![]);
    }

    // 2. Filter policies that run at this hook and validate each
    let mut decisions = Vec::new();

    for policy_id in &active_policies {
        // Check if this policy runs at this hook
        let runs_at_hook = state
            .policy_manager
            .policy_runs_at_hook(policy_id, &ctx.hook);

        if !runs_at_hook {
            continue;
        }

        // Validate the policy
        let decision = match state
            .policy_manager
            .validate_policy_for_hook(policy_id, ctx)
        {
            Ok(result) => {
                if result.valid {
                    PolicyDecision {
                        policy_pack_id: policy_id.clone(),
                        hook: ctx.hook,
                        decision: Decision::Allow,
                        reason: "Policy validation passed".to_string(),
                    }
                } else {
                    PolicyDecision {
                        policy_pack_id: policy_id.clone(),
                        hook: ctx.hook,
                        decision: Decision::Deny,
                        reason: result
                            .violations
                            .first()
                            .map(|v| v.message.clone())
                            .unwrap_or_else(|| "Policy violation".to_string()),
                    }
                }
            }
            Err(e) => {
                warn!(
                    tenant_id = %ctx.tenant_id,
                    policy_id = %policy_id,
                    error = %e,
                    "Policy validation error, treating as allow"
                );
                PolicyDecision {
                    policy_pack_id: policy_id.clone(),
                    hook: ctx.hook,
                    decision: Decision::Allow,
                    reason: format!("Validation error (fail-open): {}", e),
                }
            }
        };

        decisions.push(decision);
    }

    // 3. Log ALL decisions to audit (allow AND deny)
    for decision in &decisions {
        let decision_str = match decision.decision {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
            Decision::Modify { .. } => "modify",
        };

        if let Err(e) = state
            .db
            .log_policy_decision(
                &ctx.tenant_id,
                &decision.policy_pack_id,
                hook_name,
                decision_str,
                Some(&decision.reason),
                Some(&ctx.request_id),
                ctx.user_id.as_deref(),
                Some(&ctx.resource_type),
                ctx.resource_id.as_deref(),
                None, // metadata_json
            )
            .await
        {
            error!(
                tenant_id = %ctx.tenant_id,
                policy_pack_id = %decision.policy_pack_id,
                error = %e,
                "Failed to log policy decision to audit"
            );
            // Continue despite audit failure - don't block on audit
        }
    }

    // 4. Check for any denials
    let denials: Vec<PolicyDecision> = decisions
        .iter()
        .filter(|d| matches!(d.decision, Decision::Deny))
        .cloned()
        .collect();

    if !denials.is_empty() {
        let denial_messages: Vec<String> = denials
            .iter()
            .map(|d| format!("{}: {}", d.policy_pack_id, d.reason))
            .collect();

        warn!(
            tenant_id = %ctx.tenant_id,
            request_id = %ctx.request_id,
            hook = %hook_name,
            denials = denials.len(),
            "Request blocked by policy hook enforcement"
        );

        return Err(PolicyHookViolationError {
            violations: denials,
            message: format!(
                "Policy hook {} blocked by: {}",
                hook_name,
                denial_messages.join("; ")
            ),
        });
    }

    info!(
        tenant_id = %ctx.tenant_id,
        request_id = %ctx.request_id,
        hook = %hook_name,
        policies_checked = decisions.len(),
        "Policy hook enforcement passed"
    );

    Ok(decisions)
}

/// Helper to create a HookContext from Claims and request info
pub fn create_hook_context(
    claims: &Claims,
    request_id: &str,
    hook: PolicyHook,
    resource_type: &str,
    resource_id: Option<&str>,
) -> HookContext {
    HookContext::new(
        claims.tenant_id.clone(),
        request_id.to_string(),
        hook,
        resource_type.to_string(),
    )
    .with_user_id(claims.sub.clone())
    .with_resource_id(resource_id.map(|s| s.to_string()).unwrap_or_default())
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
