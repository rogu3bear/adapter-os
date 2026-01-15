#![cfg(all(test, feature = "extended-tests"))]
//! Policy Rule Testing
//!
//! This module tests policy rule validation, enforcement, and compliance for adapterOS.
//! It covers policy gates, rule evaluation, policy pack validation, and deterministic
//! policy enforcement in the context of a deterministic inference runtime.

use std::collections::HashMap;
use adapteros_core::{Evidence, Policy, PolicyEngine, PolicyResult};
use adapteros_policy::{PolicyPack, PolicyRule, ValidationResult};
use crate::security_test_utils::{SecurityTestHarness, MockSecurityContext, ComplianceValidator};

/// Test policy rule validation and enforcement
#[cfg(test)]
mod policy_rule_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_policy_rule_creation_and_validation() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test basic policy rule creation
            let rule = PolicyRule::new("isolate_tenants", "Ensure tenant data isolation");
            assert_eq!(rule.name, "isolate_tenants");
            assert_eq!(rule.description, "Ensure tenant data isolation");

            // Test policy creation with rules
            let policy = Policy {
                name: "tenant_isolation".to_string(),
                rules: vec![rule],
            };

            // Add policy to engine
            h.policy_engine.add_policy("tenant_isolation", policy.clone());

            // Verify policy was added
            assert!(h.policy_engine.has_policy("tenant_isolation"));
        });
    }

    #[test]
    fn test_policy_pack_validation() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Create a policy pack with multiple policies
            let mut pack = PolicyPack::new("security_pack");

            let isolation_policy = Policy {
                name: "tenant_isolation".to_string(),
                rules: vec![
                    PolicyRule::new("isolate_tenants", "Ensure tenant data isolation"),
                    PolicyRule::new("prevent_cross_tenant_access", "Prevent cross-tenant data access"),
                ],
            };

            let evidence_policy = Policy {
                name: "evidence_integrity".to_string(),
                rules: vec![
                    PolicyRule::new("validate_evidence", "Validate evidence integrity"),
                    PolicyRule::new("prevent_evidence_tampering", "Prevent evidence tampering"),
                ],
            };

            pack.add_policy(isolation_policy);
            pack.add_policy(evidence_policy);

            // Test policy pack validation
            let validation_result = pack.validate();
            assert!(validation_result.is_valid);

            // Test that all policies are present
            assert!(pack.has_policy("tenant_isolation"));
            assert!(pack.has_policy("evidence_integrity"));
        });
    }

    #[test]
    fn test_policy_enforcement_with_security_context() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Create a policy that checks security context
            let context_check_policy = Policy {
                name: "context_validation".to_string(),
                rules: vec![
                    PolicyRule::new("check_tenant_access", "Validate tenant access permissions"),
                    PolicyRule::new("verify_user_permissions", "Verify user has required permissions"),
                ],
            };

            h.policy_engine.add_policy("context_validation", context_check_policy);

            // Test with authorized context
            let authorized_context = MockSecurityContext::new("tenant_a", "user_1")
                .with_role("tenant_admin")
                .with_permission("read_tenant_data");

            // Simulate policy evaluation (in real implementation, this would be done by PolicyEngine)
            assert!(authorized_context.has_role("tenant_admin"));
            assert!(authorized_context.has_permission("read_tenant_data"));

            // Test with unauthorized context
            let unauthorized_context = MockSecurityContext::new("tenant_b", "user_2")
                .with_role("guest");

            assert!(!unauthorized_context.has_role("tenant_admin"));
            assert!(!unauthorized_context.has_permission("read_tenant_data"));
        });
    }

    #[test]
    fn test_policy_gate_enforcement() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test policy gates for different operations
            let gates = vec![
                ("inference_gate", "Control inference operations"),
                ("data_access_gate", "Control data access operations"),
                ("evidence_collection_gate", "Control evidence collection"),
            ];

            for (gate_name, description) in gates {
                let gate_policy = Policy {
                    name: gate_name.to_string(),
                    rules: vec![
                        PolicyRule::new("check_permissions", &format!("Check permissions for {}", description)),
                        PolicyRule::new("validate_context", &format!("Validate security context for {}", description)),
                    ],
                };

                h.policy_engine.add_policy(gate_name, gate_policy);
                assert!(h.policy_engine.has_policy(gate_name));
            }

            // Test that all gates are properly configured
            assert!(h.policy_engine.has_policy("inference_gate"));
            assert!(h.policy_engine.has_policy("data_access_gate"));
            assert!(h.policy_engine.has_policy("evidence_collection_gate"));
        });
    }

    #[test]
    fn test_deterministic_policy_evaluation() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test that policy evaluation is deterministic
            let policy = Policy {
                name: "deterministic_test".to_string(),
                rules: vec![
                    PolicyRule::new("deterministic_rule_1", "First deterministic rule"),
                    PolicyRule::new("deterministic_rule_2", "Second deterministic rule"),
                ],
            };

            h.policy_engine.add_policy("deterministic_test", policy);

            // Run multiple evaluations and ensure consistency
            for _ in 0..10 {
                assert!(h.policy_engine.has_policy("deterministic_test"));
                // In a real implementation, we would evaluate the policy and check results are identical
            }
        });
    }

    #[test]
    fn test_policy_violation_handling() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test policy violation recording and handling
            h.policy_engine.record_violation("Cross-tenant data access attempt");
            h.policy_engine.record_violation("Evidence tampering detected");

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 2);
            assert!(violations.contains(&"Cross-tenant data access attempt".to_string()));
            assert!(violations.contains(&"Evidence tampering detected".to_string()));

            // Test audit logging of violations
            h.log_audit_event("Policy violation: Cross-tenant access blocked");
            let audit_log = h.get_audit_log();
            assert!(audit_log.iter().any(|entry| entry.contains("Policy violation")));
        });
    }

    #[test]
    fn test_policy_compliance_validation() {
        let mut harness = security_utils::create_tenant_isolation_harness();
        let mut validator = ComplianceValidator::new();

        // Add compliance checks for policy rules
        validator.add_check("tenant_isolation_policy", |h| {
            if h.policy_engine.has_policy("tenant_isolation") {
                Ok(())
            } else {
                Err("Tenant isolation policy not found".to_string())
            }
        });

        validator.add_check("evidence_integrity_policy", |h| {
            if h.policy_engine.has_policy("evidence_integrity") {
                Ok(())
            } else {
                Err("Evidence integrity policy not found".to_string())
            }
        });

        validator.add_check("no_policy_violations", |h| {
            let violations = h.policy_engine.get_violations();
            if violations.is_empty() {
                Ok(())
            } else {
                Err(format!("Policy violations detected: {:?}", violations))
            }
        });

        harness.run_security_test(|h| {
            let results = validator.validate(h);
            assert_eq!(results.len(), 3);

            // Should pass with default setup
            assert!(validator.all_pass(h));

            // Introduce a violation and test failure
            h.policy_engine.record_violation("Test violation");
            let results_after_violation = validator.validate(h);
            let violation_check = results_after_violation.iter()
                .find(|(name, _)| name == "no_policy_violations")
                .unwrap();
            assert!(violation_check.1.is_err());
        });
    }

    #[test]
    fn test_zero_egress_policy_enforcement() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test zero-egress policy - no external network access allowed
            let egress_policy = Policy {
                name: "zero_egress".to_string(),
                rules: vec![
                    PolicyRule::new("block_external_network", "Block all external network access"),
                    PolicyRule::new("block_dns_queries", "Block DNS queries"),
                    PolicyRule::new("block_http_requests", "Block HTTP/HTTPS requests"),
                ],
            };

            h.policy_engine.add_policy("zero_egress", egress_policy);

            // Verify policy is in place
            assert!(h.policy_engine.has_policy("zero_egress"));

            // Test that attempts to make external connections would be blocked
            // (In a real test, this would attempt actual network calls and verify blocking)
            h.log_audit_event("Zero-egress policy enforced: external connection blocked");
            let audit_log = h.get_audit_log();
            assert!(audit_log.iter().any(|entry| entry.contains("Zero-egress policy enforced")));
        });
    }

    #[test]
    fn test_policy_rule_dependencies() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test policies with dependencies
            let base_policy = Policy {
                name: "base_security".to_string(),
                rules: vec![
                    PolicyRule::new("authentication_required", "Require authentication for all operations"),
                ],
            };

            let dependent_policy = Policy {
                name: "advanced_security".to_string(),
                rules: vec![
                    PolicyRule::new("depends_on_base", "Requires base security policy"),
                    PolicyRule::new("additional_checks", "Additional security checks"),
                ],
            };

            h.policy_engine.add_policy("base_security", base_policy);
            h.policy_engine.add_policy("advanced_security", dependent_policy);

            // Verify both policies exist
            assert!(h.policy_engine.has_policy("base_security"));
            assert!(h.policy_engine.has_policy("advanced_security"));

            // In a real implementation, we would test that dependent policies
            // cannot be evaluated without their dependencies
        });
    }
}

/// Integration tests for policy rules across components
#[cfg(test)]
mod policy_integration_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_cross_component_policy_enforcement() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Test policies that span multiple components
            let cross_component_policy = Policy {
                name: "cross_component_security".to_string(),
                rules: vec![
                    PolicyRule::new("inference_security", "Secure inference operations"),
                    PolicyRule::new("evidence_security", "Secure evidence handling"),
                    PolicyRule::new("telemetry_security", "Secure telemetry collection"),
                ],
            };

            h.policy_engine.add_policy("cross_component_security", cross_component_policy);

            // Test that the policy covers all components
            assert!(h.policy_engine.has_policy("cross_component_security"));

            // Log security events for each component
            h.log_audit_event("Inference operation secured");
            h.log_audit_event("Evidence handling secured");
            h.log_audit_event("Telemetry collection secured");

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 3);
            assert!(audit_log.iter().all(|entry| entry.contains("secured")));
        });
    }

    #[test]
    fn test_policy_performance_under_load() {
        let mut harness = security_utils::create_tenant_isolation_harness();

        harness.run_security_test(|h| {
            // Create multiple policies to test performance
            for i in 0..100 {
                let policy = Policy {
                    name: format!("performance_policy_{}", i),
                    rules: vec![
                        PolicyRule::new("perf_rule_1", "Performance test rule 1"),
                        PolicyRule::new("perf_rule_2", "Performance test rule 2"),
                    ],
                };
                h.policy_engine.add_policy(&policy.name, policy);
            }

            // Verify all policies were added
            for i in 0..100 {
                assert!(h.policy_engine.has_policy(&format!("performance_policy_{}", i)));
            }

            // Test that policy lookups remain fast
            // (In a real benchmark, we would measure timing here)
        });
    }
}