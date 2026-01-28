//! Audit logging routes
//!
//! Router configuration for audit logging endpoints.

use adapteros_server_api::state::AppState;
use axum::{routing::get, routing::post, Router};

use crate::handlers::{
    get_audit_chain, get_compliance_audit, get_federation_audit, list_audits_extended,
    query_audit_logs, verify_audit_chain, verify_receipt,
};

/// Build the audit logging router
///
/// Returns a router with the following endpoints:
/// - `GET /v1/audits` - List audits with extended information
/// - `GET /v1/audit/logs` - Query audit logs with filtering
/// - `GET /v1/audit/federation` - Get federation audit report
/// - `GET /v1/audit/compliance` - Get compliance audit report
/// - `GET /v1/audit/chain` - Get audit chain entries
/// - `GET /v1/audit/chain/verify` - Verify audit chain integrity
/// - `POST /v1/audit/receipts/verify` - Third-party receipt verification (public)
pub fn audit_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/audits", get(list_audits_extended))
        .route("/v1/audit/logs", get(query_audit_logs))
        .route("/v1/audit/federation", get(get_federation_audit))
        .route("/v1/audit/compliance", get(get_compliance_audit))
        .route("/v1/audit/chain", get(get_audit_chain))
        .route("/v1/audit/chain/verify", get(verify_audit_chain))
        .route("/v1/audit/receipts/verify", post(verify_receipt))
}
