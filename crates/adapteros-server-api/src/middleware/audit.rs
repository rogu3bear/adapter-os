//! Automatic audit logging middleware for adapterOS API handlers
//!
//! This middleware automatically logs audit events based on handler outcomes,
//! removing the need for manual `audit_helper::log_*` calls in handlers.
//!
//! # Usage
//!
//! Configure routes with audit config:
//! ```ignore
//! use crate::middleware::audit::{AuditConfig, audit_layer};
//!
//! Router::new()
//!     .route("/adapters", post(register_adapter))
//!     .layer(audit_layer(AuditConfig::new("adapter.register", "adapter")))
//! ```
//!
//! Or use the route extension helper:
//! ```ignore
//! .route("/adapters", post(register_adapter).route_layer(Extension(
//!     AuditConfig::new("adapter.register", "adapter")
//! )))
//! ```

use crate::audit_helper;
use crate::middleware::context::RequestContext;
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// Configuration for automatic audit logging on a route
///
/// Attach this to routes that should be automatically audited.
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// The action being performed (e.g., "adapter.register", "training.start")
    pub action: &'static str,

    /// The type of resource being acted upon (e.g., "adapter", "policy")
    pub resource_type: &'static str,

    /// Whether to audit successful requests (default: true)
    pub audit_success: bool,

    /// Whether to audit failed requests (default: true)
    pub audit_failure: bool,

    /// Optional: extract resource ID from request path
    /// If set, extracts the segment at this index from the path
    /// e.g., path_segment_index = 2 for "/v1/adapters/{id}" extracts "{id}"
    pub path_segment_index: Option<usize>,
}

impl AuditConfig {
    /// Create a new audit config with the given action and resource type
    pub fn new(action: &'static str, resource_type: &'static str) -> Self {
        Self {
            action,
            resource_type,
            audit_success: true,
            audit_failure: true,
            path_segment_index: None,
        }
    }

    /// Set the path segment index for extracting resource ID
    pub fn with_resource_id_from_path(mut self, index: usize) -> Self {
        self.path_segment_index = Some(index);
        self
    }

    /// Only audit successful requests
    pub fn success_only(mut self) -> Self {
        self.audit_success = true;
        self.audit_failure = false;
        self
    }

    /// Only audit failed requests
    pub fn failure_only(mut self) -> Self {
        self.audit_success = false;
        self.audit_failure = true;
        self
    }

    /// Extract resource ID from path based on configured index
    fn extract_resource_id(&self, path: &str) -> Option<String> {
        self.path_segment_index.and_then(|index| {
            path.split('/')
                .filter(|s| !s.is_empty())
                .nth(index)
                .map(|s| s.to_string())
        })
    }
}

/// Middleware that automatically logs audit events based on handler outcome
///
/// This middleware:
/// 1. Checks for AuditConfig in request extensions
/// 2. Extracts RequestContext for user info
/// 3. Runs the handler
/// 4. Logs success or failure based on response status
///
/// Routes without AuditConfig are not audited.
pub async fn audit_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Extract audit config (if present)
    let audit_config = req.extensions().get::<AuditConfig>().cloned();

    // Extract request context
    let ctx = req.extensions().get::<Arc<RequestContext>>().cloned();

    // Extract request info before running handler
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Run the handler
    let response = next.run(req).await;

    // Only audit if config is present and we have a context with claims
    if let (Some(config), Some(ctx)) = (audit_config, ctx) {
        // Only audit authenticated requests
        if let Some(claims) = &ctx.claims {
            let principal_id = ctx
                .principal()
                .map(|p| p.principal_id.clone())
                .unwrap_or_else(|| claims.sub.clone());
            let principal_type = ctx
                .principal()
                .map(|p| format!("{:?}", p.principal_type))
                .unwrap_or_else(|| "unknown".to_string());
            let auth_mode = ctx
                .principal()
                .map(|p| format!("{:?}", p.auth_mode))
                .unwrap_or_else(|| "unknown".to_string());
            let status = response.status();
            let resource_id = config.extract_resource_id(&path);

            // Log based on response status
            if status.is_success() && config.audit_success {
                if let Err(e) = audit_helper::log_success(
                    &state.db,
                    claims,
                    config.action,
                    config.resource_type,
                    resource_id.as_deref(),
                    Some(ctx.client_ip.as_str()),
                )
                .await
                {
                    warn!(
                        error = %e,
                        action = config.action,
                        "Failed to log audit success"
                    );
                } else {
                    info!(
                        action = config.action,
                        resource_type = config.resource_type,
                        resource_id = ?resource_id,
                        method = %method,
                        path = %path,
                        status = %status.as_u16(),
                        user_id = %principal_id,
                        principal_type = %principal_type,
                        auth_mode = %auth_mode,
                        "Audit: operation succeeded"
                    );
                }
            } else if (status.is_client_error() || status.is_server_error()) && config.audit_failure
            {
                let error_reason = format!("HTTP {}", status.as_u16());
                if let Err(e) = audit_helper::log_failure(
                    &state.db,
                    claims,
                    config.action,
                    config.resource_type,
                    resource_id.as_deref(),
                    &error_reason,
                    Some(ctx.client_ip.as_str()),
                )
                .await
                {
                    warn!(
                        error = %e,
                        action = config.action,
                        "Failed to log audit failure"
                    );
                } else {
                    info!(
                        action = config.action,
                        resource_type = config.resource_type,
                        resource_id = ?resource_id,
                        method = %method,
                        path = %path,
                        status = %status.as_u16(),
                        user_id = %principal_id,
                        principal_type = %principal_type,
                        auth_mode = %auth_mode,
                        "Audit: operation failed"
                    );
                }
            }
        } else {
            // Unauthenticated request on an audited route — no claims available
            // so we cannot persist to the audit DB. Emit a structured tracing
            // event so log aggregation pipelines can still capture these.
            let status = response.status();
            if (status.is_client_error() || status.is_server_error()) && config.audit_failure {
                warn!(
                    target: "audit.unauth",
                    action = config.action,
                    resource_type = config.resource_type,
                    method = %method,
                    path = %path,
                    status = %status.as_u16(),
                    request_id = %ctx.request_id,
                    client_ip = %ctx.client_ip,
                    "Audit: unauthenticated request failed"
                );
            }
        }
    }

    response
}

/// Immutable startup audit record for pre-request boot events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupAuditRecord {
    pub timestamp: String,
    pub phase: String,
    pub event: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Append startup audit record to immutable JSONL trail under `var/run/`.
///
/// This helper is intentionally file-based so startup failures that occur
/// before full DB readiness still leave an auditable trail for operators.
pub fn append_startup_audit_record(record: &StartupAuditRecord) {
    let path = adapteros_core::rebase_var_path("var/run/startup_audit.jsonl");
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            warn!(
                error = %error,
                path = %parent.display(),
                "Failed to create startup audit directory"
            );
            return;
        }
    }

    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) => {
            warn!(
                error = %error,
                path = %path.display(),
                "Failed to open startup audit trail"
            );
            return;
        }
    };

    use std::io::Write;
    match serde_json::to_string(record) {
        Ok(line) => {
            if let Err(error) = writeln!(file, "{}", line) {
                warn!(
                    error = %error,
                    path = %path.display(),
                    "Failed to append startup audit record"
                );
            }
        }
        Err(error) => warn!(error = %error, "Failed to serialize startup audit record"),
    }
}

/// Convenience constructor that appends a startup audit record immediately.
pub fn record_startup_audit_event(
    phase: impl Into<String>,
    event: impl Into<String>,
    message: impl Into<String>,
    code: Option<String>,
) {
    append_startup_audit_record(&StartupAuditRecord {
        timestamp: Utc::now().to_rfc3339(),
        phase: phase.into(),
        event: event.into(),
        message: message.into(),
        code,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_config_extract_resource_id() {
        let config = AuditConfig::new("adapter.get", "adapter").with_resource_id_from_path(2);
        assert_eq!(
            config.extract_resource_id("/v1/adapters/abc-123"),
            Some("abc-123".to_string())
        );
        assert_eq!(config.extract_resource_id("/v1/adapters"), None);
    }

    #[test]
    fn test_audit_config_builder() {
        let config = AuditConfig::new("test.action", "test")
            .success_only()
            .with_resource_id_from_path(1);
        assert!(config.audit_success);
        assert!(!config.audit_failure);
        assert_eq!(config.path_segment_index, Some(1));
    }
}
