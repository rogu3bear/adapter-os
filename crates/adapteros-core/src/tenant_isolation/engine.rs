use super::rules::{
    AdminExplicitGrantRule, AdminWildcardGrantRule, DevModeAdminBypassRule, SameTenantRule,
    TenantIsolationRule,
};
use super::types::{
    TenantIsolationConfig, TenantIsolationReason, TenantIsolationRequest,
    TenantIsolationRuleDecision, TenantIsolationRuleEffect, TenantIsolationVerdict,
    TenantIsolationViolation,
};
use std::sync::Arc;

pub struct TenantIsolationEngine {
    config: TenantIsolationConfig,
    rules: Vec<Arc<dyn TenantIsolationRule>>,
}

impl Default for TenantIsolationEngine {
    fn default() -> Self {
        Self::new(TenantIsolationConfig::default())
    }
}

impl TenantIsolationEngine {
    pub fn new(config: TenantIsolationConfig) -> Self {
        Self::builder(config).build()
    }

    pub fn builder(config: TenantIsolationConfig) -> TenantIsolationEngineBuilder {
        let rules: Vec<Arc<dyn TenantIsolationRule>> = vec![
            Arc::new(SameTenantRule),
            Arc::new(DevModeAdminBypassRule),
            Arc::new(AdminWildcardGrantRule),
            Arc::new(AdminExplicitGrantRule),
        ];

        TenantIsolationEngineBuilder { config, rules }
    }

    pub fn config(&self) -> &TenantIsolationConfig {
        &self.config
    }

    pub fn evaluate<'a>(&self, request: &TenantIsolationRequest<'a>) -> TenantIsolationVerdict {
        let mut best_allow_override: Option<(&'static str, TenantIsolationRuleDecision)> = None;
        let mut best_deny: Option<(&'static str, TenantIsolationRuleDecision)> = None;
        let mut best_allow: Option<(&'static str, TenantIsolationRuleDecision)> = None;

        for rule in &self.rules {
            match rule.evaluate(request, &self.config) {
                TenantIsolationRuleEffect::Abstain => {}
                TenantIsolationRuleEffect::Allow(decision) => {
                    if best_allow.is_none() {
                        best_allow = Some((rule.id(), decision));
                    }
                }
                TenantIsolationRuleEffect::AllowOverride(decision) => {
                    if best_allow_override.is_none() {
                        best_allow_override = Some((rule.id(), decision));
                    }
                }
                TenantIsolationRuleEffect::Deny(decision) => {
                    if best_deny.is_none() {
                        best_deny = Some((rule.id(), decision));
                    }
                }
            }
        }

        if let Some((rule_id, decision)) = best_allow_override {
            let (reason, details) = decision.into_parts();
            return TenantIsolationVerdict::allowed(reason, Some(rule_id), details);
        }

        if let Some((rule_id, decision)) = best_deny {
            let (reason, details) = decision.into_parts();
            return TenantIsolationVerdict::denied(reason, Some(rule_id), details);
        }

        if let Some((rule_id, decision)) = best_allow {
            let (reason, details) = decision.into_parts();
            return TenantIsolationVerdict::allowed(reason, Some(rule_id), details);
        }

        TenantIsolationVerdict::denied(TenantIsolationReason::TenantMismatch, None, None)
    }

    pub fn check<'a>(&self, request: &TenantIsolationRequest<'a>) -> bool {
        self.evaluate(request).is_allowed()
    }

    pub fn validate<'a>(
        &self,
        request: &TenantIsolationRequest<'a>,
    ) -> std::result::Result<(), Box<TenantIsolationViolation>> {
        let verdict = self.evaluate(request);
        if verdict.is_allowed() {
            return Ok(());
        }

        Err(Box::new(TenantIsolationViolation::new(
            request.principal().tenant_id(),
            request.target().tenant_id(),
            Some(request.principal().role().to_string()),
            verdict.reason(),
            verdict.rule_id(),
            verdict.details().map(|s| s.to_string()),
        )))
    }

    pub fn rules(&self) -> &[Arc<dyn TenantIsolationRule>] {
        &self.rules
    }
}

pub struct TenantIsolationEngineBuilder {
    config: TenantIsolationConfig,
    rules: Vec<Arc<dyn TenantIsolationRule>>,
}

impl TenantIsolationEngineBuilder {
    pub fn with_rule(mut self, rule: impl TenantIsolationRule + 'static) -> Self {
        self.rules.push(Arc::new(rule));
        self
    }

    pub fn with_rule_arc(mut self, rule: Arc<dyn TenantIsolationRule>) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn with_rule_first(mut self, rule: impl TenantIsolationRule + 'static) -> Self {
        self.rules.insert(0, Arc::new(rule));
        self
    }

    pub fn build(self) -> TenantIsolationEngine {
        TenantIsolationEngine {
            config: self.config,
            rules: self.rules,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tenant_isolation::{
        TenantIsolationDecision, TenantIsolationRuleEffect, TenantPrincipal,
    };

    #[test]
    fn allows_same_tenant() {
        let engine = TenantIsolationEngine::default();
        let admin_tenants: Vec<String> = vec![];
        let principal = TenantPrincipal::new("t1", "user", &admin_tenants);
        let req = TenantIsolationRequest::new(principal, "t1");
        let verdict = engine.evaluate(&req);
        assert!(verdict.is_allowed());
        assert_eq!(verdict.reason(), TenantIsolationReason::SameTenant);
        assert_eq!(verdict.decision(), TenantIsolationDecision::Allow);
    }

    #[test]
    fn denies_tenant_mismatch_for_non_admin() {
        let engine = TenantIsolationEngine::default();
        let admin_tenants: Vec<String> = vec![];
        let principal = TenantPrincipal::new("t1", "user", &admin_tenants);
        let req = TenantIsolationRequest::new(principal, "t2");
        let verdict = engine.evaluate(&req);
        assert!(!verdict.is_allowed());
        assert_eq!(verdict.reason(), TenantIsolationReason::TenantMismatch);
    }

    #[test]
    fn allows_admin_with_explicit_grant() {
        let engine = TenantIsolationEngine::default();
        let admin_tenants: Vec<String> = vec!["t2".to_string()];
        let principal = TenantPrincipal::new("t1", "admin", &admin_tenants);
        let req = TenantIsolationRequest::new(principal, "t2");
        let verdict = engine.evaluate(&req);
        assert!(verdict.is_allowed());
        assert_eq!(verdict.reason(), TenantIsolationReason::AdminExplicitGrant);
    }

    #[test]
    fn allows_admin_with_wildcard_grant() {
        let engine = TenantIsolationEngine::default();
        let admin_tenants: Vec<String> = vec!["*".to_string()];
        let principal = TenantPrincipal::new("t1", "admin", &admin_tenants);
        let req = TenantIsolationRequest::new(principal, "t2");
        let verdict = engine.evaluate(&req);
        assert!(verdict.is_allowed());
        assert_eq!(verdict.reason(), TenantIsolationReason::AdminWildcardGrant);
    }

    #[test]
    fn allows_dev_mode_admin_bypass() {
        let mut cfg = TenantIsolationConfig::default();
        cfg.set_dev_mode_admin_all_tenants(true);
        let engine = TenantIsolationEngine::new(cfg);

        let admin_tenants: Vec<String> = vec![];
        let principal = TenantPrincipal::new("t1", "admin", &admin_tenants);
        let req = TenantIsolationRequest::new(principal, "t2");
        let verdict = engine.evaluate(&req);
        assert!(verdict.is_allowed());
        assert_eq!(verdict.reason(), TenantIsolationReason::DevModeAdminBypass);
    }

    #[test]
    fn extension_deny_overrides_allow() {
        #[derive(Debug, Default)]
        struct DenyTenantRule;

        impl TenantIsolationRule for DenyTenantRule {
            fn id(&self) -> &'static str {
                "test.deny_tenant"
            }

            fn evaluate<'a>(
                &self,
                request: &TenantIsolationRequest<'a>,
                _cfg: &TenantIsolationConfig,
            ) -> TenantIsolationRuleEffect {
                if request.target().tenant_id() == "t1" {
                    return TenantIsolationRuleEffect::Deny(TenantIsolationRuleDecision::new(
                        TenantIsolationReason::DeniedByRule,
                    ));
                }
                TenantIsolationRuleEffect::Abstain
            }
        }

        let engine = TenantIsolationEngine::builder(TenantIsolationConfig::default())
            .with_rule(DenyTenantRule::default())
            .build();

        let admin_tenants: Vec<String> = vec![];
        let principal = TenantPrincipal::new("t1", "user", &admin_tenants);
        let req = TenantIsolationRequest::new(principal, "t1");
        let verdict = engine.evaluate(&req);
        assert!(!verdict.is_allowed());
        assert_eq!(verdict.reason(), TenantIsolationReason::DeniedByRule);
    }
}
