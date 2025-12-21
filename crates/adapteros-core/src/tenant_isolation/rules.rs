use super::types::{
    TenantIsolationConfig, TenantIsolationReason, TenantIsolationRequest,
    TenantIsolationRuleDecision, TenantIsolationRuleEffect,
};

pub trait TenantIsolationRule: Send + Sync {
    fn id(&self) -> &'static str;

    fn evaluate<'a>(
        &self,
        request: &TenantIsolationRequest<'a>,
        cfg: &TenantIsolationConfig,
    ) -> TenantIsolationRuleEffect;
}

#[derive(Debug, Default)]
pub struct SameTenantRule;

impl TenantIsolationRule for SameTenantRule {
    fn id(&self) -> &'static str {
        "tenant.same_tenant"
    }

    fn evaluate<'a>(
        &self,
        request: &TenantIsolationRequest<'a>,
        _cfg: &TenantIsolationConfig,
    ) -> TenantIsolationRuleEffect {
        if request.principal().tenant_id() == request.target().tenant_id() {
            return TenantIsolationRuleEffect::Allow(TenantIsolationRuleDecision::new(
                TenantIsolationReason::SameTenant,
            ));
        }
        TenantIsolationRuleEffect::Abstain
    }
}

#[derive(Debug, Default)]
pub struct DevModeAdminBypassRule;

impl TenantIsolationRule for DevModeAdminBypassRule {
    fn id(&self) -> &'static str {
        "tenant.dev_mode_admin_bypass"
    }

    fn evaluate<'a>(
        &self,
        request: &TenantIsolationRequest<'a>,
        cfg: &TenantIsolationConfig,
    ) -> TenantIsolationRuleEffect {
        if !cfg.dev_mode_admin_all_tenants() {
            return TenantIsolationRuleEffect::Abstain;
        }
        if !request.principal().is_admin(cfg) {
            return TenantIsolationRuleEffect::Abstain;
        }

        let decision = TenantIsolationRuleDecision::new(TenantIsolationReason::DevModeAdminBypass);
        if cfg.dev_mode_bypass_overrides_denies() {
            return TenantIsolationRuleEffect::AllowOverride(decision);
        }
        TenantIsolationRuleEffect::Allow(decision)
    }
}

#[derive(Debug, Default)]
pub struct AdminWildcardGrantRule;

impl TenantIsolationRule for AdminWildcardGrantRule {
    fn id(&self) -> &'static str {
        "tenant.admin_wildcard_grant"
    }

    fn evaluate<'a>(
        &self,
        request: &TenantIsolationRequest<'a>,
        cfg: &TenantIsolationConfig,
    ) -> TenantIsolationRuleEffect {
        if !cfg.allow_admin_wildcard() {
            return TenantIsolationRuleEffect::Abstain;
        }
        if !request.principal().is_admin(cfg) {
            return TenantIsolationRuleEffect::Abstain;
        }

        let wildcard = cfg.admin_tenant_wildcard();
        if request
            .principal()
            .admin_tenants()
            .iter()
            .any(|t| t == wildcard)
        {
            return TenantIsolationRuleEffect::Allow(TenantIsolationRuleDecision::new(
                TenantIsolationReason::AdminWildcardGrant,
            ));
        }

        TenantIsolationRuleEffect::Abstain
    }
}

#[derive(Debug, Default)]
pub struct AdminExplicitGrantRule;

impl TenantIsolationRule for AdminExplicitGrantRule {
    fn id(&self) -> &'static str {
        "tenant.admin_explicit_grant"
    }

    fn evaluate<'a>(
        &self,
        request: &TenantIsolationRequest<'a>,
        cfg: &TenantIsolationConfig,
    ) -> TenantIsolationRuleEffect {
        if !cfg.allow_admin_tenant_grants() {
            return TenantIsolationRuleEffect::Abstain;
        }
        if !request.principal().is_admin(cfg) {
            return TenantIsolationRuleEffect::Abstain;
        }

        if request
            .principal()
            .admin_tenants()
            .iter()
            .any(|t| t == request.target().tenant_id())
        {
            return TenantIsolationRuleEffect::Allow(TenantIsolationRuleDecision::new(
                TenantIsolationReason::AdminExplicitGrant,
            ));
        }

        TenantIsolationRuleEffect::Abstain
    }
}
