//! Audit logging endpoints for adapteros server
//!
//! This crate provides audit logging endpoints for the AdapterOS control plane.
//! Split from adapteros-server-api for faster incremental builds.
//!
//! Handlers use the spoke pattern: they depend on adapteros-server-api for
//! shared types (AppState, Claims, ErrorResponse) while keeping handler logic
//! in this crate.

pub mod handlers;
pub mod routes;

pub use handlers::{
    get_audit_chain, get_compliance_audit, get_federation_audit, list_audits_extended,
    query_audit_logs, verify_audit_chain, AuditChainEntry, AuditChainQuery, AuditChainResponse,
    ChainVerificationResponse, ComplianceAuditResponse, ComplianceControl,
    FederationAuditResponse, HostChainSummary,
};
pub use routes::audit_routes;
