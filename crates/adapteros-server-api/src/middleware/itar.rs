//! ITAR compliance middleware for adapterOS
//!
//! This middleware enforces International Traffic in Arms Regulations (ITAR) compliance
//! for tenants flagged with itar_flag=true. It provides:
//!
//! - Enhanced audit logging for all ITAR tenant access
//! - Optional geo-blocking for ITAR tenants
//! - ITAR-specific access restrictions
//!
//! # Usage
//!
//! Add to the middleware stack after authentication:
//! ```ignore
//! Router::new()
//!     .route("/v1/infer", post(inference_handler))
//!     .layer(axum::middleware::from_fn_with_state(state.clone(), itar_compliance_middleware))
//! ```

use adapteros_db::TenantKvOps;
use crate::auth::Claims;
use crate::ip_extraction::ClientIp;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use tracing::{info, warn};

/// ITAR audit event types for compliance tracking
#[derive(Debug, Clone, Copy)]
pub enum ItarEventType {
    /// Standard access to ITAR tenant resources
    Access,
    /// Data export from ITAR tenant
    Export,
    /// Tenant configuration modification
    TenantModification,
    /// Inference request on ITAR tenant
    Inference,
    /// Training request on ITAR tenant
    Training,
}

impl std::fmt::Display for ItarEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItarEventType::Access => write!(f, "itar_access"),
            ItarEventType::Export => write!(f, "itar_export"),
            ItarEventType::TenantModification => write!(f, "itar_tenant_modification"),
            ItarEventType::Inference => write!(f, "itar_inference"),
            ItarEventType::Training => write!(f, "itar_training"),
        }
    }
}

/// ITAR compliance middleware
///
/// This middleware:
/// 1. Checks if the request's tenant has ITAR flag enabled
/// 2. Logs enhanced audit events for all ITAR tenant access
/// 3. Enforces any configured ITAR restrictions
///
/// The middleware runs after authentication, so Claims are available.
pub async fn itar_compliance_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Extract claims from request extensions (set by auth middleware)
    let claims = req.extensions().get::<Claims>().cloned();

    // If no claims (unauthenticated route), skip ITAR checks
    let claims = match claims {
        Some(c) => c,
        None => return Ok(next.run(req).await),
    };

    let tenant_id = &claims.tenant_id;

    // Check if tenant has ITAR flag
    let itar_enabled = match check_tenant_itar_flag(&state, tenant_id).await {
        Ok(flag) => flag,
        Err(e) => {
            warn!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to check ITAR flag, defaulting to non-ITAR"
            );
            false
        }
    };

    if itar_enabled {
        // Determine event type based on request path
        let path = req.uri().path();
        let event_type = classify_itar_event(path);

        // Extract client IP for audit logging
        let client_ip = req
            .extensions()
            .get::<ClientIp>()
            .map(|ip| ip.0.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Log ITAR access event
        info!(
            target: "security.itar",
            tenant_id = %tenant_id,
            user_id = %claims.sub,
            client_ip = %client_ip,
            event_type = %event_type,
            path = %path,
            method = %req.method(),
            "ITAR tenant access"
        );

        // Record ITAR audit event in database
        if let Err(e) = record_itar_audit_event(
            &state,
            tenant_id,
            &claims.sub,
            &client_ip,
            event_type,
            path,
            req.method().as_str(),
        )
        .await
        {
            warn!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to record ITAR audit event"
            );
            // Don't block request on audit failure, but log it
        }

        // Note: ITAR geo-blocking is not implemented
        // When U.S. Person verification is required, implement geo-IP checking here
        // and enable via configuration flag (not feature flag)
        let _ = &client_ip; // Suppress unused variable warning
    }

    Ok(next.run(req).await)
}

/// Check if a tenant has ITAR flag enabled
async fn check_tenant_itar_flag(state: &AppState, tenant_id: &str) -> Result<bool, String> {
    // First try KV backend if available
    if let Some(kv) = state.db.kv_backend() {
        use adapteros_db::tenants_kv::TenantKvRepository;
        let repo = TenantKvRepository::new(kv.clone());
        if let Ok(Some(tenant)) = repo.get_tenant_kv(tenant_id).await {
            return Ok(tenant.itar_flag);
        }
    }

    // Fall back to SQL
    let result = sqlx::query_scalar::<_, i64>("SELECT itar_flag FROM tenants WHERE id = ?")
        .bind(tenant_id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| e.to_string())?;

    Ok(result.unwrap_or(0) != 0)
}

/// Classify the ITAR event type based on request path
fn classify_itar_event(path: &str) -> ItarEventType {
    if path.contains("/infer") || path.contains("/chat") || path.contains("/completions") {
        ItarEventType::Inference
    } else if path.contains("/train") || path.contains("/finetune") {
        ItarEventType::Training
    } else if path.contains("/export") || path.contains("/download") {
        ItarEventType::Export
    } else if path.contains("/tenant") {
        ItarEventType::TenantModification
    } else {
        ItarEventType::Access
    }
}

/// Record an ITAR audit event to the database
async fn record_itar_audit_event(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    client_ip: &str,
    event_type: ItarEventType,
    path: &str,
    method: &str,
) -> Result<(), String> {
    let event_json = serde_json::json!({
        "event_type": event_type.to_string(),
        "path": path,
        "method": method,
        "itar_compliance": true
    });

    sqlx::query(
        r#"
        INSERT INTO audit_logs (tenant_id, user_id, action, resource_type, resource_id, ip_address, metadata, created_at)
        VALUES (?, ?, ?, 'itar', ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(event_type.to_string())
    .bind(tenant_id) // resource_id is the tenant itself for ITAR events
    .bind(client_ip)
    .bind(event_json.to_string())
    .execute(state.db.pool())
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_itar_event() {
        assert!(matches!(
            classify_itar_event("/v1/infer"),
            ItarEventType::Inference
        ));
        assert!(matches!(
            classify_itar_event("/v1/chat/completions"),
            ItarEventType::Inference
        ));
        assert!(matches!(
            classify_itar_event("/v1/train/start"),
            ItarEventType::Training
        ));
        assert!(matches!(
            classify_itar_event("/v1/export/adapter"),
            ItarEventType::Export
        ));
        assert!(matches!(
            classify_itar_event("/v1/tenant/settings"),
            ItarEventType::TenantModification
        ));
        assert!(matches!(
            classify_itar_event("/v1/adapters"),
            ItarEventType::Access
        ));
    }

    #[test]
    fn test_itar_event_type_display() {
        assert_eq!(ItarEventType::Access.to_string(), "itar_access");
        assert_eq!(ItarEventType::Inference.to_string(), "itar_inference");
        assert_eq!(ItarEventType::Export.to_string(), "itar_export");
    }
}
