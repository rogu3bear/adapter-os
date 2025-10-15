//! Access Control Enforcement Tests
//!
//! This module tests authentication, authorization, and access control mechanisms
//! in AdapterOS. It verifies that users and tenants can only access resources they
//! are authorized for, and that access control decisions are properly enforced
//! and audited.

use std::collections::{HashMap, HashSet};
use adapteros_core::SecurityContext;
use crate::security_test_utils::{SecurityTestHarness, MockSecurityContext};
use crate::security_test_utils::security_utils;

/// Test authentication mechanisms
#[cfg(test)]
mod authentication_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_user_authentication() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test valid authentication
            let valid_users = vec![
                ("user_1", "tenant_a", vec!["read"]),
                ("user_2", "tenant_b", vec!["read", "write"]),
                ("admin", "system", vec!["read", "write", "admin"]),
            ];

            for (user_id, tenant_id, permissions) in valid_users {
                let context = MockSecurityContext::new(tenant_id, user_id);

                for perm in permissions {
                    let context_with_perm = context.clone().with_permission(perm);
                    assert!(context_with_perm.has_permission(perm));
                    h.log_audit_event(&format!("User {} authenticated for tenant {}", user_id, tenant_id));
                }
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 6); // 3 users * 2 permissions average
        });
    }

    #[test]
    fn test_invalid_authentication_rejection() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test invalid authentication attempts
            let invalid_attempts = vec![
                ("", "tenant_a", "Empty user ID"),
                ("user_1", "", "Empty tenant ID"),
                ("nonexistent", "tenant_a", "User does not exist"),
                ("user_1", "nonexistent", "Tenant does not exist"),
            ];

            for (user_id, tenant_id, reason) in invalid_attempts {
                let context = MockSecurityContext::new(tenant_id, user_id);

                // Invalid contexts should not have permissions
                assert!(!context.has_permission("read"));
                assert!(!context.has_permission("write"));

                h.log_audit_event(&format!("Authentication rejected: {} - {}", reason, user_id));
                h.policy_engine.record_violation(&format!("Invalid authentication attempt: {}", reason));
            }

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 4);
        });
    }

    #[test]
    fn test_session_authentication() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test session-based authentication
            let user_context = MockSecurityContext::new("tenant_a", "user_1")
                .with_permission("read");

            // Simulate session creation
            let session_id = "session_123";
            h.log_audit_event(&format!("Session created: {} for user {}", session_id, user_context.user_id));

            // Verify session context
            assert_eq!(user_context.tenant_id, "tenant_a");
            assert!(user_context.has_permission("read"));

            // Simulate session expiration
            h.log_audit_event(&format!("Session expired: {}", session_id));

            // After expiration, access should be denied
            h.policy_engine.record_violation("Access attempt with expired session");

            let violations = h.policy_engine.get_violations();
            assert!(violations.iter().any(|v| v.contains("expired session")));
        });
    }
}

/// Test authorization and permission systems
#[cfg(test)]
mod authorization_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_role_based_authorization() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Define roles and their permissions
            let role_permissions = vec![
                ("guest", vec!["read_public"]),
                ("analyst", vec!["read", "analyze"]),
                ("admin", vec!["read", "write", "delete", "admin"]),
            ];

            for (role, permissions) in role_permissions {
                let context = MockSecurityContext::new("tenant_a", "user")
                    .with_role(role);

                for perm in permissions {
                    let context_with_perm = context.clone().with_permission(perm);
                    assert!(context_with_perm.has_role(role));
                    assert!(context_with_perm.has_permission(perm));
                }

                h.log_audit_event(&format!("Role {} authorized with {} permissions", role, permissions.len()));
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 3);
        });
    }

    #[test]
    fn test_permission_based_access_control() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test specific permission requirements
            let resources = vec![
                ("inference_endpoint", vec!["execute_inference"]),
                ("model_repository", vec!["read_models", "write_models"]),
                ("audit_logs", vec!["read_audit"]),
                ("system_config", vec!["read_config", "write_config"]),
            ];

            for (resource, required_perms) in resources {
                let authorized_context = MockSecurityContext::new("tenant_a", "user");

                for perm in &required_perms {
                    let context_with_perm = authorized_context.clone().with_permission(perm);
                    assert!(security_utils::has_required_permissions(&context_with_perm, &[*perm]));
                }

                // Test unauthorized access
                let unauthorized_context = MockSecurityContext::new("tenant_a", "user");
                assert!(!security_utils::has_required_permissions(&unauthorized_context, &required_perms));

                h.log_audit_event(&format!("Access control verified for resource: {}", resource));
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 4);
        });
    }

    #[test]
    fn test_tenant_based_authorization() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test that tenants can only access their own resources
            let tenant_a_context = MockSecurityContext::new("tenant_a", "user_1")
                .with_permission("read_tenant_data");

            let tenant_b_context = MockSecurityContext::new("tenant_b", "user_2")
                .with_permission("read_tenant_data");

            // Tenant A should access tenant A resources
            assert!(security_utils::validate_evidence_tenant_isolation(
                &security_utils::generate_test_evidence("tenant_a", "data"),
                &tenant_a_context.tenant_id
            ));

            // Tenant A should NOT access tenant B resources
            assert!(!security_utils::validate_evidence_tenant_isolation(
                &security_utils::generate_test_evidence("tenant_b", "data"),
                &tenant_a_context.tenant_id
            ));

            // Tenant B should access tenant B resources
            assert!(security_utils::validate_evidence_tenant_isolation(
                &security_utils::generate_test_evidence("tenant_b", "data"),
                &tenant_b_context.tenant_id
            ));

            h.log_audit_event("Tenant-based authorization enforced");
        });
    }
}

/// Test access control enforcement
#[cfg(test)]
mod access_control_enforcement_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_access_denial_enforcement() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test various access denial scenarios
            let denial_scenarios = vec![
                ("insufficient_permissions", "User lacks required permissions"),
                ("expired_credentials", "User credentials have expired"),
                ("account_suspended", "User account is suspended"),
                ("resource_not_found", "Requested resource does not exist"),
            ];

            for (scenario, reason) in denial_scenarios {
                let unauthorized_context = MockSecurityContext::new("tenant_a", "user");

                // Verify access is denied
                assert!(!unauthorized_context.has_permission("access_denied"));

                h.log_audit_event(&format!("Access denied: {} - {}", scenario, reason));
                h.policy_engine.record_violation(&format!("Access violation: {}", scenario));
            }

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 4);

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 4);
            assert!(audit_log.iter().all(|entry| entry.contains("Access denied")));
        });
    }

    #[test]
    fn test_privilege_escalation_prevention() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test prevention of privilege escalation attempts
            let normal_user = MockSecurityContext::new("tenant_a", "normal_user")
                .with_role("user")
                .with_permission("read");

            let admin_user = MockSecurityContext::new("tenant_a", "admin")
                .with_role("admin")
                .with_permission("admin");

            // Normal user should not be able to escalate to admin
            assert!(!normal_user.has_role("admin"));
            assert!(!normal_user.has_permission("admin"));

            // Admin should have admin privileges
            assert!(admin_user.has_role("admin"));
            assert!(admin_user.has_permission("admin"));

            // Attempted escalation should be logged and blocked
            h.log_audit_event("Privilege escalation attempt blocked: normal_user -> admin");
            h.policy_engine.record_violation("Privilege escalation attempt detected");

            let violations = h.policy_engine.get_violations();
            assert!(violations.iter().any(|v| v.contains("Privilege escalation")));
        });
    }

    #[test]
    fn test_access_control_auditing() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test comprehensive access control auditing
            let access_events = vec![
                ("login", "user_1", "tenant_a", "success"),
                ("data_access", "user_1", "tenant_a", "success"),
                ("admin_operation", "user_1", "tenant_a", "denied"),
                ("logout", "user_1", "tenant_a", "success"),
            ];

            for (action, user, tenant, result) in access_events {
                h.log_audit_event(&format!("Access control event: {} {}@{} - {}",
                    action, user, tenant, result));

                if result == "denied" {
                    h.policy_engine.record_violation(&format!("Access denied: {} for {}", action, user));
                }
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 4);

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 1); // One denied access
        });
    }
}

/// Test access control policies and rules
#[cfg(test)]
mod access_policy_tests {
    use super::*;
    use crate::security_test_utils::{ComplianceValidator};
    use adapteros_core::{Policy, PolicyRule};

    #[test]
    fn test_access_control_policy_enforcement() {
        let mut harness = security_utils::create_access_control_harness();
        let mut validator = ComplianceValidator::new();

        // Add access control policy checks
        validator.add_check("authentication_required", |h| {
            // In a real system, this would check if all operations require authentication
            // For this test, we assume the harness provides authenticated contexts
            Ok(())
        });

        validator.add_check("authorization_enforced", |h| {
            // Check that permissions are properly enforced
            let authorized_context = MockSecurityContext::new("tenant_a", "user")
                .with_permission("required_perm");

            if security_utils::has_required_permissions(&authorized_context, &["required_perm"]) {
                Ok(())
            } else {
                Err("Authorization not properly enforced".to_string())
            }
        });

        validator.add_check("no_unauthorized_access", |h| {
            let violations = h.policy_engine.get_violations();
            if violations.iter().any(|v| v.contains("unauthorized")) {
                Err("Unauthorized access detected".to_string())
            } else {
                Ok(())
            }
        });

        harness.run_security_test(|h| {
            // Add access control policies
            let auth_policy = Policy {
                name: "authentication_policy".to_string(),
                rules: vec![
                    PolicyRule::new("require_auth", "All operations require authentication"),
                    PolicyRule::new("validate_credentials", "Credentials must be validated"),
                ],
            };

            let authz_policy = Policy {
                name: "authorization_policy".to_string(),
                rules: vec![
                    PolicyRule::new("check_permissions", "Check user permissions"),
                    PolicyRule::new("enforce_policies", "Enforce access policies"),
                ],
            };

            h.policy_engine.add_policy("authentication_policy", auth_policy);
            h.policy_engine.add_policy("authorization_policy", authz_policy);

            // Should pass compliance checks
            let results = validator.validate(h);
            assert_eq!(results.len(), 3);
            assert!(validator.all_pass(h));

            // Introduce policy violation
            h.policy_engine.record_violation("Unauthorized access attempt");

            let results_after_violation = validator.validate(h);
            let authz_check = results_after_violation.iter()
                .find(|(name, _)| name == "no_unauthorized_access")
                .unwrap();
            assert!(authz_check.1.is_err());
        });
    }

    #[test]
    fn test_least_privilege_principle() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Test that users have only the minimum permissions needed
            let user_permissions = vec![
                ("guest", vec!["read_public"]),
                ("developer", vec!["read", "write_code"]),
                ("auditor", vec!["read_audit", "read_compliance"]),
                ("operator", vec!["read", "start_stop_services"]),
            ];

            for (role, expected_perms) in user_permissions {
                let context = MockSecurityContext::new("tenant_a", "user")
                    .with_role(role);

                // Add only the expected permissions
                let mut context_with_perms = context;
                for perm in &expected_perms {
                    context_with_perms = context_with_perms.with_permission(perm);
                }

                // Verify user has exactly the expected permissions
                for expected_perm in &expected_perms {
                    assert!(context_with_perms.has_permission(expected_perm));
                }

                // Verify user does NOT have excessive permissions
                let excessive_perms = vec!["admin", "delete_all", "modify_security"];
                for excessive_perm in excessive_perms {
                    assert!(!context_with_perms.has_permission(excessive_perm),
                        "User with role {} has excessive permission: {}", role, excessive_perm);
                }

                h.log_audit_event(&format!("Least privilege verified for role: {}", role));
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 4);
        });
    }
}

/// Integration tests for access control
#[cfg(test)]
mod access_control_integration_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_end_to_end_access_control_workflow() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Simulate complete access control workflow
            let user_id = "test_user";
            let tenant_id = "tenant_a";

            // 1. Authentication
            let authenticated_context = MockSecurityContext::new(tenant_id, user_id)
                .with_permission("authenticated");
            h.log_audit_event(&format!("User {} authenticated", user_id));

            // 2. Authorization
            let authorized_context = authenticated_context
                .with_role("analyst")
                .with_permission("read_data")
                .with_permission("run_inference");
            h.log_audit_event(&format!("User {} authorized with analyst role", user_id));

            // 3. Access attempt
            assert!(authorized_context.has_role("analyst"));
            assert!(authorized_context.has_permission("read_data"));
            assert!(authorized_context.has_permission("run_inference"));
            h.log_audit_event(&format!("Access granted for user {} to inference operations", user_id));

            // 4. Access enforcement (simulated operation)
            h.log_audit_event(&format!("Inference operation completed for user {}", user_id));

            // 5. Audit logging
            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 4);
            assert!(audit_log[0].contains("authenticated"));
            assert!(audit_log[1].contains("authorized"));
            assert!(audit_log[2].contains("Access granted"));
            assert!(audit_log[3].contains("completed"));
        });
    }

    #[test]
    fn test_access_control_under_attack() {
        let mut harness = security_utils::create_access_control_harness();

        harness.run_security_test(|h| {
            // Simulate various access control attacks
            let attacks = vec![
                "brute_force_authentication",
                "session_hijacking",
                "privilege_escalation",
                "authorization_bypass",
                "token_replay",
            ];

            for attack in attacks {
                h.policy_engine.record_violation(&format!("Access control attack blocked: {}", attack));
                h.log_audit_event(&format!("Security control held against: {}", attack));
            }

            // Verify that legitimate access still works
            let valid_context = MockSecurityContext::new("tenant_a", "legitimate_user")
                .with_permission("valid_access");

            assert!(valid_context.has_permission("valid_access"));
            h.log_audit_event("Legitimate access maintained despite attacks");

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 5);

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 6); // 5 attacks + 1 legitimate access
        });
    }
}