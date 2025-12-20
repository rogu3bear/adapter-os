use crate::AosError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const TENANT_ISOLATION_ERROR_CODE: &str = "TENANT_ISOLATION_ERROR";

fn default_admin_role_names() -> Vec<String> {
    vec!["admin".to_string()]
}

fn default_admin_tenant_wildcard() -> String {
    "*".to_string()
}

fn default_true() -> bool {
    true
}

/// Tenant isolation configuration.
///
/// This structure is designed to evolve without breaking changes:
/// - `#[non_exhaustive]` prevents external exhaustive construction.
/// - New fields can be added with `#[serde(default)]` for backward compatible deserialization.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars-support", derive(schemars::JsonSchema))]
pub struct TenantIsolationConfig {
    /// When enabled, admins may access any tenant in dev mode.
    ///
    /// Intended to be wired to debug-only bypasses in higher layers.
    #[serde(default)]
    dev_mode_admin_all_tenants: bool,

    /// If true, dev-mode admin bypass is treated as an override allow that wins over denials.
    ///
    /// This is off by default in production because `dev_mode_admin_all_tenants` should be false.
    #[serde(default = "default_true")]
    dev_mode_bypass_overrides_denies: bool,

    /// Role names treated as "admin" for tenant isolation decisions.
    ///
    /// Defaults to `["admin"]` to match existing behavior.
    #[serde(default = "default_admin_role_names")]
    admin_role_names: Vec<String>,

    /// If true, `roles` (multi-role claim) is considered in addition to `role`.
    ///
    /// Defaults to false for backward compatibility with legacy single-role tokens.
    #[serde(default)]
    use_roles_claim: bool,

    /// Wildcard token that grants access to all tenants for admins (default: "*").
    #[serde(default = "default_admin_tenant_wildcard")]
    admin_tenant_wildcard: String,

    /// If true, wildcard grants are honored for admins.
    #[serde(default = "default_true")]
    allow_admin_wildcard: bool,

    /// If true, explicit tenant grants are honored for admins.
    #[serde(default = "default_true")]
    allow_admin_tenant_grants: bool,

    /// Opaque extension map for future tenant isolation configuration.
    #[serde(default)]
    extensions: BTreeMap<String, Value>,
}

impl Default for TenantIsolationConfig {
    fn default() -> Self {
        Self {
            dev_mode_admin_all_tenants: false,
            dev_mode_bypass_overrides_denies: true,
            admin_role_names: default_admin_role_names(),
            use_roles_claim: false,
            admin_tenant_wildcard: default_admin_tenant_wildcard(),
            allow_admin_wildcard: true,
            allow_admin_tenant_grants: true,
            extensions: BTreeMap::new(),
        }
    }
}

impl TenantIsolationConfig {
    pub fn dev_mode_admin_all_tenants(&self) -> bool {
        self.dev_mode_admin_all_tenants
    }

    pub fn set_dev_mode_admin_all_tenants(&mut self, enabled: bool) {
        self.dev_mode_admin_all_tenants = enabled;
    }

    pub fn dev_mode_bypass_overrides_denies(&self) -> bool {
        self.dev_mode_bypass_overrides_denies
    }

    pub fn set_dev_mode_bypass_overrides_denies(&mut self, enabled: bool) {
        self.dev_mode_bypass_overrides_denies = enabled;
    }

    pub fn admin_role_names(&self) -> &[String] {
        &self.admin_role_names
    }

    pub fn set_admin_role_names(&mut self, names: Vec<String>) {
        self.admin_role_names = names;
    }

    pub fn use_roles_claim(&self) -> bool {
        self.use_roles_claim
    }

    pub fn set_use_roles_claim(&mut self, enabled: bool) {
        self.use_roles_claim = enabled;
    }

    pub fn admin_tenant_wildcard(&self) -> &str {
        &self.admin_tenant_wildcard
    }

    pub fn set_admin_tenant_wildcard(&mut self, token: impl Into<String>) {
        self.admin_tenant_wildcard = token.into();
    }

    pub fn allow_admin_wildcard(&self) -> bool {
        self.allow_admin_wildcard
    }

    pub fn set_allow_admin_wildcard(&mut self, enabled: bool) {
        self.allow_admin_wildcard = enabled;
    }

    pub fn allow_admin_tenant_grants(&self) -> bool {
        self.allow_admin_tenant_grants
    }

    pub fn set_allow_admin_tenant_grants(&mut self, enabled: bool) {
        self.allow_admin_tenant_grants = enabled;
    }

    pub fn extensions(&self) -> &BTreeMap<String, Value> {
        &self.extensions
    }

    pub fn extensions_mut(&mut self) -> &mut BTreeMap<String, Value> {
        &mut self.extensions
    }
}

/// Principal (caller) identity used for tenant isolation decisions.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct TenantPrincipal<'a> {
    tenant_id: &'a str,
    role: &'a str,
    roles: &'a [String],
    admin_tenants: &'a [String],
    subject_id: Option<&'a str>,
    subject_email: Option<&'a str>,
}

impl<'a> TenantPrincipal<'a> {
    pub fn new(tenant_id: &'a str, role: &'a str, admin_tenants: &'a [String]) -> Self {
        Self {
            tenant_id,
            role,
            roles: &[],
            admin_tenants,
            subject_id: None,
            subject_email: None,
        }
    }

    pub fn with_roles(mut self, roles: &'a [String]) -> Self {
        self.roles = roles;
        self
    }

    pub fn with_subject(
        mut self,
        subject_id: Option<&'a str>,
        subject_email: Option<&'a str>,
    ) -> Self {
        self.subject_id = subject_id;
        self.subject_email = subject_email;
        self
    }

    pub fn tenant_id(&self) -> &str {
        self.tenant_id
    }

    pub fn role(&self) -> &str {
        self.role
    }

    pub fn roles(&self) -> &[String] {
        self.roles
    }

    pub fn admin_tenants(&self) -> &[String] {
        self.admin_tenants
    }

    pub fn subject_id(&self) -> Option<&str> {
        self.subject_id
    }

    pub fn subject_email(&self) -> Option<&str> {
        self.subject_email
    }

    pub fn has_role(&self, role: &str, cfg: &TenantIsolationConfig) -> bool {
        if self.role == role {
            return true;
        }
        if !cfg.use_roles_claim() {
            return false;
        }
        self.roles.iter().any(|r| r == role)
    }

    pub fn is_admin(&self, cfg: &TenantIsolationConfig) -> bool {
        cfg.admin_role_names()
            .iter()
            .any(|admin_role| self.has_role(admin_role, cfg))
    }
}

/// Target resource identity used for tenant isolation decisions.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct TenantIsolationTarget<'a> {
    tenant_id: &'a str,
    resource_kind: Option<&'a str>,
    resource_id: Option<&'a str>,
}

impl<'a> TenantIsolationTarget<'a> {
    pub fn new(tenant_id: &'a str) -> Self {
        Self {
            tenant_id,
            resource_kind: None,
            resource_id: None,
        }
    }

    pub fn with_resource_kind(mut self, kind: Option<&'a str>) -> Self {
        self.resource_kind = kind;
        self
    }

    pub fn with_resource_id(mut self, id: Option<&'a str>) -> Self {
        self.resource_id = id;
        self
    }

    pub fn tenant_id(&self) -> &str {
        self.tenant_id
    }

    pub fn resource_kind(&self) -> Option<&str> {
        self.resource_kind
    }

    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id
    }
}

/// High-level action category for tenant isolation evaluation.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TenantIsolationAction {
    #[default]
    Unspecified,
    Read,
    Write,
    Admin,
}

/// A request evaluated by the tenant isolation engine.
#[non_exhaustive]
#[derive(Debug, Clone, Copy)]
pub struct TenantIsolationRequest<'a> {
    principal: TenantPrincipal<'a>,
    target: TenantIsolationTarget<'a>,
    action: TenantIsolationAction,
    attributes: Option<&'a Value>,
}

impl<'a> TenantIsolationRequest<'a> {
    pub fn new(principal: TenantPrincipal<'a>, target_tenant_id: &'a str) -> Self {
        Self {
            principal,
            target: TenantIsolationTarget::new(target_tenant_id),
            action: TenantIsolationAction::Unspecified,
            attributes: None,
        }
    }

    pub fn with_target(mut self, target: TenantIsolationTarget<'a>) -> Self {
        self.target = target;
        self
    }

    pub fn with_action(mut self, action: TenantIsolationAction) -> Self {
        self.action = action;
        self
    }

    pub fn with_attributes(mut self, attributes: Option<&'a Value>) -> Self {
        self.attributes = attributes;
        self
    }

    pub fn principal(&self) -> &TenantPrincipal<'a> {
        &self.principal
    }

    pub fn target(&self) -> &TenantIsolationTarget<'a> {
        &self.target
    }

    pub fn action(&self) -> TenantIsolationAction {
        self.action
    }

    pub fn attributes(&self) -> Option<&Value> {
        self.attributes
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantIsolationDecision {
    Allow,
    Deny,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantIsolationReason {
    SameTenant,
    DevModeAdminBypass,
    AdminWildcardGrant,
    AdminExplicitGrant,
    TenantMismatch,
    DeniedByRule,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantIsolationRuleDecision {
    reason: TenantIsolationReason,
    details: Option<String>,
}

impl TenantIsolationRuleDecision {
    pub fn new(reason: TenantIsolationReason) -> Self {
        Self {
            reason,
            details: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn into_parts(self) -> (TenantIsolationReason, Option<String>) {
        (self.reason, self.details)
    }

    pub fn reason(&self) -> TenantIsolationReason {
        self.reason
    }

    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantIsolationRuleEffect {
    Abstain,
    Allow(TenantIsolationRuleDecision),
    AllowOverride(TenantIsolationRuleDecision),
    Deny(TenantIsolationRuleDecision),
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantIsolationVerdict {
    decision: TenantIsolationDecision,
    reason: TenantIsolationReason,
    rule_id: Option<&'static str>,
    details: Option<String>,
}

impl TenantIsolationVerdict {
    pub fn allowed(
        reason: TenantIsolationReason,
        rule_id: Option<&'static str>,
        details: Option<String>,
    ) -> Self {
        Self {
            decision: TenantIsolationDecision::Allow,
            reason,
            rule_id,
            details,
        }
    }

    pub fn denied(
        reason: TenantIsolationReason,
        rule_id: Option<&'static str>,
        details: Option<String>,
    ) -> Self {
        Self {
            decision: TenantIsolationDecision::Deny,
            reason,
            rule_id,
            details,
        }
    }

    pub fn decision(&self) -> TenantIsolationDecision {
        self.decision
    }

    pub fn reason(&self) -> TenantIsolationReason {
        self.reason
    }

    pub fn rule_id(&self) -> Option<&'static str> {
        self.rule_id
    }

    pub fn details(&self) -> Option<&str> {
        self.details.as_deref()
    }

    pub fn is_allowed(&self) -> bool {
        matches!(self.decision, TenantIsolationDecision::Allow)
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{message}")]
pub struct TenantIsolationViolation {
    message: String,
    pub principal_tenant_id: String,
    pub resource_tenant_id: String,
    pub principal_role: Option<String>,
    pub reason: TenantIsolationReason,
    pub rule_id: Option<&'static str>,
    pub details: Option<String>,
}

impl TenantIsolationViolation {
    pub fn new(
        principal_tenant_id: impl Into<String>,
        resource_tenant_id: impl Into<String>,
        principal_role: Option<String>,
        reason: TenantIsolationReason,
        rule_id: Option<&'static str>,
        details: Option<String>,
    ) -> Self {
        let principal_tenant_id = principal_tenant_id.into();
        let resource_tenant_id = resource_tenant_id.into();

        let message = format!(
            "Tenant isolation violation: user tenant '{}' cannot access resource tenant '{}'",
            principal_tenant_id, resource_tenant_id
        );

        Self {
            message,
            principal_tenant_id,
            resource_tenant_id,
            principal_role,
            reason,
            rule_id,
            details,
        }
    }

    pub fn code(&self) -> &'static str {
        TENANT_ISOLATION_ERROR_CODE
    }
}

impl From<TenantIsolationViolation> for AosError {
    fn from(v: TenantIsolationViolation) -> Self {
        AosError::Authz(v.to_string())
    }
}
