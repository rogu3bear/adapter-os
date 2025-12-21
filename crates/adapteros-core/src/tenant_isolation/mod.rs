//! Tenant isolation framework
//!
//! Provides a modular, extensible evaluation engine for tenant isolation decisions.
//! The goal is to centralize the baseline tenant isolation rules while allowing
//! future security enhancements to be added via pluggable rules without breaking
//! changes to call sites.

mod engine;
mod rules;
mod types;

pub use engine::{TenantIsolationEngine, TenantIsolationEngineBuilder};
pub use rules::{
    AdminExplicitGrantRule, AdminWildcardGrantRule, DevModeAdminBypassRule, SameTenantRule,
    TenantIsolationRule,
};
pub use types::{
    TenantIsolationAction, TenantIsolationConfig, TenantIsolationDecision, TenantIsolationReason,
    TenantIsolationRequest, TenantIsolationRuleDecision, TenantIsolationRuleEffect,
    TenantIsolationTarget, TenantIsolationVerdict, TenantIsolationViolation, TenantPrincipal,
    TENANT_ISOLATION_ERROR_CODE,
};
