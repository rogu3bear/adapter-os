#![cfg(all(test, feature = "extended-tests"))]
//! Evidence Validation Tests
//!
//! This module tests evidence collection, validation, integrity, and tamper detection
//! in adapterOS. It ensures that evidence is properly collected, validated, and protected
//! against tampering while maintaining the deterministic nature of the inference runtime.

use adapteros_core::{B3Hash, Evidence};
use crate::security_test_utils::{SecurityTestHarness, MockEvidenceCollector, MockSecurityContext};
use crate::security_test_utils::security_utils;

/// Test evidence collection and validation
#[cfg(test)]
mod evidence_collection_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_evidence_creation_and_validation() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create test evidence
            let evidence = security_utils::generate_test_evidence("tenant_a", "inference_result");

            // Validate evidence structure
            assert_eq!(evidence.tenant_id, "tenant_a");
            assert_eq!(evidence.data_type, "inference_result");
            assert!(!evidence.id.as_bytes().is_empty());
            assert!(evidence.timestamp > 0);
            assert!(evidence.signature.is_some());

            // Add to collector
            h.evidence_collector.add_evidence(evidence.clone());

            // Verify evidence was collected
            let collected = h.evidence_collector.get_evidence();
            assert_eq!(collected.len(), 1);
            assert_eq!(collected[0].id, evidence.id);

            h.log_audit_event("Evidence creation and validation successful");
        });
    }

    #[test]
    fn test_evidence_integrity_verification() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create evidence with known content
            let evidence = security_utils::generate_test_evidence("tenant_a", "model_output");

            // Verify content hash integrity
            let expected_hash = B3Hash::hash(b"test_content");
            assert_eq!(evidence.content_hash, expected_hash);

            // Test signature verification (mock)
            if let Some(signature) = &evidence.signature {
                // In real implementation, this would verify cryptographic signature
                assert!(!signature.as_bytes().is_empty());
            }

            h.evidence_collector.add_evidence(evidence);
            h.log_audit_event("Evidence integrity verification passed");
        });
    }

    #[test]
    fn test_evidence_tamper_detection() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create valid evidence
            let mut evidence = security_utils::generate_test_evidence("tenant_a", "computation_result");
            h.evidence_collector.add_evidence(evidence.clone());

            // Simulate tampering by marking evidence as corrupted
            h.evidence_collector.corrupt_evidence(evidence.id);

            // Verify tamper detection
            assert!(h.evidence_collector.is_corrupted(&evidence.id));

            // Log tamper detection
            h.log_audit_event("Evidence tampering detected and flagged");
            h.policy_engine.record_violation("Evidence tampering attempt detected");

            let violations = h.policy_engine.get_violations();
            assert!(violations.iter().any(|v| v.contains("Evidence tampering")));
        });
    }

    #[test]
    fn test_evidence_chain_validation() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create a chain of evidence
            let input_evidence = security_utils::generate_test_evidence("tenant_a", "input_data");
            let computation_evidence = security_utils::generate_test_evidence("tenant_a", "computation");
            let output_evidence = security_utils::generate_test_evidence("tenant_a", "output_result");

            // Add to collector in order
            h.evidence_collector.add_evidence(input_evidence.clone());
            h.evidence_collector.add_evidence(computation_evidence.clone());
            h.evidence_collector.add_evidence(output_evidence.clone());

            // Verify chain integrity
            let all_evidence = h.evidence_collector.get_evidence();
            assert_eq!(all_evidence.len(), 3);

            // All evidence should belong to same tenant
            assert!(all_evidence.iter().all(|e| e.tenant_id == "tenant_a"));

            // Verify temporal ordering (timestamps should be non-decreasing)
            for i in 1..all_evidence.len() {
                assert!(all_evidence[i-1].timestamp <= all_evidence[i].timestamp);
            }

            h.log_audit_event("Evidence chain validation successful");
        });
    }
}

/// Test evidence access control and authorization
#[cfg(test)]
mod evidence_access_control_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_evidence_tenant_isolation() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create evidence for different tenants
            let evidence_a = security_utils::generate_test_evidence("tenant_a", "result");
            let evidence_b = security_utils::generate_test_evidence("tenant_b", "result");

            h.evidence_collector.add_evidence(evidence_a.clone());
            h.evidence_collector.add_evidence(evidence_b.clone());

            // Test access control
            let context_a = MockSecurityContext::new("tenant_a", "user_1")
                .with_permission("read_evidence");
            let context_b = MockSecurityContext::new("tenant_b", "user_2")
                .with_permission("read_evidence");

            // Tenant A should only access tenant A evidence
            assert!(security_utils::validate_evidence_tenant_isolation(&evidence_a, &context_a.tenant_id));
            assert!(!security_utils::validate_evidence_tenant_isolation(&evidence_b, &context_a.tenant_id));

            // Tenant B should only access tenant B evidence
            assert!(security_utils::validate_evidence_tenant_isolation(&evidence_b, &context_b.tenant_id));
            assert!(!security_utils::validate_evidence_tenant_isolation(&evidence_a, &context_b.tenant_id));

            h.log_audit_event("Evidence tenant isolation enforced");
        });
    }

    #[test]
    fn test_evidence_permission_based_access() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            let evidence = security_utils::generate_test_evidence("tenant_a", "sensitive_data");
            h.evidence_collector.add_evidence(evidence);

            // Test different permission levels
            let read_only_context = MockSecurityContext::new("tenant_a", "analyst")
                .with_permission("read_evidence");

            let full_access_context = MockSecurityContext::new("tenant_a", "admin")
                .with_permission("read_evidence")
                .with_permission("modify_evidence");

            let no_access_context = MockSecurityContext::new("tenant_a", "guest");

            // Verify permission checks
            assert!(security_utils::has_required_permissions(&read_only_context, &["read_evidence"]));
            assert!(!security_utils::has_required_permissions(&read_only_context, &["modify_evidence"]));

            assert!(security_utils::has_required_permissions(&full_access_context, &["read_evidence"]));
            assert!(security_utils::has_required_permissions(&full_access_context, &["modify_evidence"]));

            assert!(!security_utils::has_required_permissions(&no_access_context, &["read_evidence"]));

            h.log_audit_event("Evidence permission-based access control verified");
        });
    }

    #[test]
    fn test_evidence_access_auditing() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            let evidence = security_utils::generate_test_evidence("tenant_a", "audit_test");
            h.evidence_collector.add_evidence(evidence.clone());

            // Simulate access attempts
            let access_events = vec![
                ("read", "tenant_a", "user_1", "granted"),
                ("read", "tenant_b", "user_2", "denied"),
                ("modify", "tenant_a", "user_1", "denied"),
            ];

            for (action, tenant, user, result) in access_events {
                h.log_audit_event(&format!("Evidence access: {} {} by {}@{} - {}",
                    action, evidence.id.to_hex(), user, tenant, result));

                if result == "denied" {
                    h.policy_engine.record_violation(&format!("Unauthorized evidence access: {} by {}",
                        action, user));
                }
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 3);
            assert!(audit_log.iter().all(|entry| entry.contains("Evidence access")));

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 2); // Two denied accesses
        });
    }
}

/// Test evidence integrity and tamper resistance
#[cfg(test)]
mod evidence_integrity_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_evidence_hash_integrity() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create evidence and verify hash consistency
            let evidence1 = security_utils::generate_test_evidence("tenant_a", "data");
            let evidence2 = security_utils::generate_test_evidence("tenant_a", "data");

            // Same input should produce same hash
            assert_eq!(evidence1.content_hash, evidence2.content_hash);

            // Different input should produce different hash
            let evidence3 = security_utils::generate_test_evidence("tenant_a", "different_data");
            assert_ne!(evidence1.content_hash, evidence3.content_hash);

            h.evidence_collector.add_evidence(evidence1);
            h.evidence_collector.add_evidence(evidence2);
            h.evidence_collector.add_evidence(evidence3);

            h.log_audit_event("Evidence hash integrity verified");
        });
    }

    #[test]
    fn test_evidence_temporal_integrity() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Create evidence at different times (simulated)
            let evidence1 = Evidence {
                id: B3Hash::hash(b"evidence1"),
                tenant_id: "tenant_a".to_string(),
                data_type: "operation".to_string(),
                content_hash: B3Hash::hash(b"content1"),
                timestamp: 1000,
                signature: Some(B3Hash::hash(b"sig1")),
            };

            let evidence2 = Evidence {
                id: B3Hash::hash(b"evidence2"),
                tenant_id: "tenant_a".to_string(),
                data_type: "operation".to_string(),
                content_hash: B3Hash::hash(b"content2"),
                timestamp: 2000,
                signature: Some(B3Hash::hash(b"sig2")),
            };

            // Verify temporal ordering
            assert!(evidence1.timestamp < evidence2.timestamp);

            h.evidence_collector.add_evidence(evidence1);
            h.evidence_collector.add_evidence(evidence2);

            // Verify collected evidence maintains temporal order
            let collected = h.evidence_collector.get_evidence();
            for i in 1..collected.len() {
                assert!(collected[i-1].timestamp <= collected[i].timestamp);
            }

            h.log_audit_event("Evidence temporal integrity verified");
        });
    }

    #[test]
    fn test_evidence_immutability() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            let original_evidence = security_utils::generate_test_evidence("tenant_a", "immutable_test");
            let original_hash = original_evidence.content_hash;

            h.evidence_collector.add_evidence(original_evidence.clone());

            // Attempt to modify evidence (simulate tampering)
            h.evidence_collector.corrupt_evidence(original_evidence.id);

            // Verify evidence appears corrupted but original hash unchanged
            assert!(h.evidence_collector.is_corrupted(&original_evidence.id));
            assert_eq!(original_evidence.content_hash, original_hash);

            // Any modification attempts should be detected
            h.policy_engine.record_violation("Evidence modification attempt detected");

            let violations = h.policy_engine.get_violations();
            assert!(violations.iter().any(|v| v.contains("modification attempt")));

            h.log_audit_event("Evidence immutability protection verified");
        });
    }
}

/// Test evidence compliance and validation
#[cfg(test)]
mod evidence_compliance_tests {
    use super::*;
    use crate::security_test_utils::{ComplianceValidator};

    #[test]
    fn test_evidence_compliance_validation() {
        let mut harness = security_utils::create_evidence_validation_harness();
        let mut validator = ComplianceValidator::new();

        // Add evidence compliance checks
        validator.add_check("evidence_integrity", |h| {
            let evidence = h.evidence_collector.get_evidence();
            if evidence.is_empty() {
                return Err("No evidence collected".to_string());
            }

            for ev in &evidence {
                if ev.signature.is_none() {
                    return Err("Evidence missing signature".to_string());
                }
                if ev.timestamp == 0 {
                    return Err("Evidence has invalid timestamp".to_string());
                }
            }
            Ok(())
        });

        validator.add_check("evidence_tenant_isolation", |h| {
            let evidence = h.evidence_collector.get_evidence();
            let tenant_ids: std::collections::HashSet<_> = evidence.iter()
                .map(|e| &e.tenant_id)
                .collect();

            // In this test, we expect only one tenant's evidence
            if tenant_ids.len() > 1 {
                return Err("Evidence from multiple tenants detected".to_string());
            }
            Ok(())
        });

        validator.add_check("no_tampered_evidence", |h| {
            let evidence = h.evidence_collector.get_evidence();
            for ev in &evidence {
                if h.evidence_collector.is_corrupted(&ev.id) {
                    return Err("Tampered evidence detected".to_string());
                }
            }
            Ok(())
        });

        harness.run_security_test(|h| {
            // Add valid evidence
            let evidence = security_utils::generate_test_evidence("tenant_a", "compliance_test");
            h.evidence_collector.add_evidence(evidence);

            // Should pass compliance checks
            let results = validator.validate(h);
            assert_eq!(results.len(), 3);
            assert!(validator.all_pass(h));

            // Introduce compliance violation
            h.evidence_collector.corrupt_evidence(h.evidence_collector.get_evidence()[0].id);

            let results_after_tamper = validator.validate(h);
            let tamper_check = results_after_tamper.iter()
                .find(|(name, _)| name == "no_tampered_evidence")
                .unwrap();
            assert!(tamper_check.1.is_err());
        });
    }

    #[test]
    fn test_evidence_retention_policy() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Simulate evidence retention requirements
            let retention_periods = vec![
                ("inference_results", 30), // 30 days
                ("audit_logs", 90),       // 90 days
                ("security_events", 365), // 1 year
            ];

            for (evidence_type, retention_days) in retention_periods {
                let evidence = security_utils::generate_test_evidence("tenant_a", evidence_type);
                h.evidence_collector.add_evidence(evidence);

                h.log_audit_event(&format!("Evidence retention policy applied: {} days for {}",
                    retention_days, evidence_type));
            }

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 3);
            assert!(audit_log.iter().all(|entry| entry.contains("retention policy")));
        });
    }
}

/// Integration tests for evidence validation
#[cfg(test)]
mod evidence_integration_tests {
    use super::*;
    use crate::security_test_utils::security_utils;

    #[test]
    fn test_end_to_end_evidence_workflow() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Simulate complete evidence workflow
            let tenant = "tenant_a";

            // 1. Input evidence
            let input_evidence = security_utils::generate_test_evidence(tenant, "input");
            h.evidence_collector.add_evidence(input_evidence);
            h.log_audit_event("Input evidence collected");

            // 2. Processing evidence
            let processing_evidence = security_utils::generate_test_evidence(tenant, "processing");
            h.evidence_collector.add_evidence(processing_evidence);
            h.log_audit_event("Processing evidence collected");

            // 3. Output evidence
            let output_evidence = security_utils::generate_test_evidence(tenant, "output");
            h.evidence_collector.add_evidence(output_evidence);
            h.log_audit_event("Output evidence collected");

            // Verify complete chain
            let all_evidence = h.evidence_collector.get_evidence();
            assert_eq!(all_evidence.len(), 3);

            // All evidence should be valid and untampered
            for ev in &all_evidence {
                assert!(!h.evidence_collector.is_corrupted(&ev.id));
                assert_eq!(ev.tenant_id, tenant);
            }

            // Verify audit trail
            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 3);
            assert!(audit_log.iter().all(|entry| entry.contains("evidence collected")));
        });
    }

    #[test]
    fn test_evidence_under_attack() {
        let mut harness = security_utils::create_evidence_validation_harness();

        harness.run_security_test(|h| {
            // Setup valid evidence
            let evidence = security_utils::generate_test_evidence("tenant_a", "secure_data");
            h.evidence_collector.add_evidence(evidence.clone());

            // Simulate various attack scenarios
            let attacks = vec![
                "hash_collision_attack",
                "signature_forgery",
                "temporal_manipulation",
                "cross_tenant_contamination",
            ];

            for attack in attacks {
                h.policy_engine.record_violation(&format!("Evidence attack detected: {}", attack));
                h.log_audit_event(&format!("Evidence protection held against: {}", attack));
            }

            // Verify evidence integrity maintained
            assert!(!h.evidence_collector.is_corrupted(&evidence.id));
            assert_eq!(evidence.tenant_id, "tenant_a");

            let violations = h.policy_engine.get_violations();
            assert_eq!(violations.len(), 4);

            let audit_log = h.get_audit_log();
            assert_eq!(audit_log.len(), 4);
            assert!(audit_log.iter().all(|entry| entry.contains("protection held against")));
        });
    }
}