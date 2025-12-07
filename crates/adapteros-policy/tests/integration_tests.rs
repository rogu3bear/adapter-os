//! Integration tests for all 20 policy packs
//!
//! These tests verify that all policy packs work together correctly
//! and enforce the rules as defined in .cursor/rules/global.mdc
//!
//! # Citations
//! - Policy Pack #1-20: Complete integration testing of all policy packs
//! - CLAUDE.md L142: "Policy Engine: Enforces 20 policy packs"
//! - .cursor/rules/global.mdc: Policy pack definitions and enforcement rules

use adapteros_policy::policy_packs::{
    EnforcementLevel, PolicyContext, PolicyPackConfig, PolicyPackId, PolicyPackManager,
    PolicyRequest, Priority, RequestType,
};
use adapteros_policy::ViolationSeverity;
use serde_json;

/// Test all 20 policy packs integration
#[tokio::test]
async fn test_all_policy_packs_integration() {
    let manager = PolicyPackManager::new();

    // Verify all 20 policy packs are initialized
    let configs = manager.get_all_configs();
    assert_eq!(configs.len(), 20);

    // Test each policy pack individually
    test_egress_policy_pack(&manager).await;
    test_determinism_policy_pack(&manager).await;
    test_router_policy_pack(&manager).await;
    test_evidence_policy_pack(&manager).await;
    test_refusal_policy_pack(&manager).await;
    test_numeric_units_policy_pack(&manager).await;
    test_rag_index_policy_pack(&manager).await;
    test_isolation_policy_pack(&manager).await;
    test_telemetry_policy_pack(&manager).await;
    test_retention_policy_pack(&manager).await;
    test_performance_policy_pack(&manager).await;
    test_memory_policy_pack(&manager).await;
    test_artifacts_policy_pack(&manager).await;
    test_secrets_policy_pack(&manager).await;
    test_build_release_policy_pack(&manager).await;
    test_compliance_policy_pack(&manager).await;
    test_incident_policy_pack(&manager).await;
    test_llm_output_policy_pack(&manager).await;
    test_adapter_lifecycle_policy_pack(&manager).await;
    test_full_pack_policy_pack(&manager).await;
}

/// Test Egress Policy Pack (#1)
async fn test_egress_policy_pack(manager: &PolicyPackManager) {
    // Test TCP connection attempt (should be blocked)
    let request = PolicyRequest {
        request_id: "test-egress-tcp".to_string(),
        request_type: RequestType::NetworkOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "test-component".to_string(),
            operation: "network_connection".to_string(),
            data: Some(serde_json::json!({
                "protocol": "tcp",
                "port": 8080
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let tcp_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Egress Ruleset"
            && v.message.contains("TCP/UDP connections are not allowed")
    });
    assert!(tcp_violation.is_some());
    assert_eq!(tcp_violation.unwrap().severity, ViolationSeverity::Critical);

    // Test DNS resolution attempt (should be blocked)
    let dns_request = PolicyRequest {
        request_id: "test-egress-dns".to_string(),
        request_type: RequestType::NetworkOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "test-component".to_string(),
            operation: "dns_resolution".to_string(),
            data: Some(serde_json::json!({
                "hostname": "example.com"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let dns_result = manager.validate_request(&dns_request).unwrap();
    assert!(!dns_result.valid);
    assert!(!dns_result.violations.is_empty());

    let dns_violation = dns_result.violations.iter().find(|v| {
        v.policy_pack == "Egress Ruleset"
            && v.message
                .contains("DNS resolution requests are not allowed")
    });
    assert!(dns_violation.is_some());
    assert_eq!(dns_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test Determinism Policy Pack (#2)
async fn test_determinism_policy_pack(manager: &PolicyPackManager) {
    // Test runtime kernel compilation (should be blocked)
    let request = PolicyRequest {
        request_id: "test-determinism-kernel".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "kernel-compiler".to_string(),
            operation: "kernel_compile".to_string(),
            data: Some(serde_json::json!({
                "kernel_source": "attention.metal"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let kernel_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Determinism Ruleset"
            && v.message
                .contains("Runtime kernel compilation is not allowed")
    });
    assert!(kernel_violation.is_some());
    assert_eq!(kernel_violation.unwrap().severity, ViolationSeverity::High);

    // Test non-HKDF RNG usage (should be blocked)
    let rng_request = PolicyRequest {
        request_id: "test-determinism-rng".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "rng-generator".to_string(),
            operation: "generate_random".to_string(),
            data: Some(serde_json::json!({
                "rng_type": "system_random"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let rng_result = manager.validate_request(&rng_request).unwrap();
    assert!(!rng_result.valid);
    assert!(!rng_result.violations.is_empty());

    let rng_violation = rng_result.violations.iter().find(|v| {
        v.policy_pack == "Determinism Ruleset" && v.message.contains("Non-HKDF RNG usage detected")
    });
    assert!(rng_violation.is_some());
    assert_eq!(rng_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test Router Policy Pack (#3)
async fn test_router_policy_pack(manager: &PolicyPackManager) {
    // Test K-sparse value exceeding maximum (should be blocked)
    let request = PolicyRequest {
        request_id: "test-router-k-sparse".to_string(),
        request_type: RequestType::AdapterOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "router".to_string(),
            operation: "configure_k_sparse".to_string(),
            data: Some(serde_json::json!({
                "k_sparse": 5
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let k_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Router Ruleset" && v.message.contains("K-sparse value exceeds maximum")
    });
    assert!(k_violation.is_some());
    assert_eq!(k_violation.unwrap().severity, ViolationSeverity::High);

    // Test non-Q15 gate quantization (should be blocked)
    let gate_request = PolicyRequest {
        request_id: "test-router-gate-quant".to_string(),
        request_type: RequestType::AdapterOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "router".to_string(),
            operation: "configure_gate_quantization".to_string(),
            data: Some(serde_json::json!({
                "gate_quant": "fp32"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let gate_result = manager.validate_request(&gate_request).unwrap();
    assert!(!gate_result.valid);
    assert!(!gate_result.violations.is_empty());

    let gate_violation = gate_result.violations.iter().find(|v| {
        v.policy_pack == "Router Ruleset" && v.message.contains("Gate quantization must be Q15")
    });
    assert!(gate_violation.is_some());
    assert_eq!(gate_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test Evidence Policy Pack (#4)
async fn test_evidence_policy_pack(manager: &PolicyPackManager) {
    // Test inference without evidence spans (should be blocked)
    let request = PolicyRequest {
        request_id: "test-evidence-no-spans".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "inference-engine".to_string(),
            operation: "generate_response".to_string(),
            data: Some(serde_json::json!({
                "evidence_spans": []
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let evidence_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Evidence Ruleset"
            && v.message
                .contains("Evidence spans are required for inference")
    });
    assert!(evidence_violation.is_some());
    assert_eq!(
        evidence_violation.unwrap().severity,
        ViolationSeverity::High
    );
}

/// Test Refusal Policy Pack (#5)
async fn test_refusal_policy_pack(manager: &PolicyPackManager) {
    // Test low confidence response (should trigger warning)
    let request = PolicyRequest {
        request_id: "test-refusal-low-confidence".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "inference-engine".to_string(),
            operation: "generate_response".to_string(),
            data: Some(serde_json::json!({
                "confidence": 0.3
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    // Refusal policy pack emits Warning violations which don't block at Error enforcement level
    assert!(result.valid);
    assert!(!result.violations.is_empty());

    let refusal_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Refusal Ruleset"
            && v.message
                .contains("Low confidence response should be refused")
    });
    assert!(refusal_violation.is_some());
    assert_eq!(
        refusal_violation.unwrap().severity,
        ViolationSeverity::Medium
    );
}

/// Test Numeric Units Policy Pack (#6)
async fn test_numeric_units_policy_pack(manager: &PolicyPackManager) {
    // Test numeric value without units (should be blocked)
    let request = PolicyRequest {
        request_id: "test-numeric-no-units".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "inference-engine".to_string(),
            operation: "generate_response".to_string(),
            data: Some(serde_json::json!({
                "numeric_values": [
                    {"value": 100, "unit": null}
                ]
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let units_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Numeric & Units Ruleset"
            && v.message.contains("Units are required for numeric values")
    });
    assert!(units_violation.is_some());
    assert_eq!(units_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test RAG Index Policy Pack (#7)
async fn test_rag_index_policy_pack(manager: &PolicyPackManager) {
    // Test cross-tenant access (should be blocked)
    let request = PolicyRequest {
        request_id: "test-rag-cross-tenant".to_string(),
        request_type: RequestType::DatabaseOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "rag-index".to_string(),
            operation: "search_documents".to_string(),
            data: Some(serde_json::json!({
                "tenant_id": "test-tenant",
                "cross_tenant_access": true
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let rag_violation = result.violations.iter().find(|v| {
        v.policy_pack == "RAG Index Ruleset" && v.message.contains("Cross-tenant access detected")
    });
    assert!(rag_violation.is_some());
    assert_eq!(rag_violation.unwrap().severity, ViolationSeverity::Critical);
}

/// Test Isolation Policy Pack (#8)
async fn test_isolation_policy_pack(manager: &PolicyPackManager) {
    // Test shared memory usage (should be blocked)
    let request = PolicyRequest {
        request_id: "test-isolation-shared-memory".to_string(),
        request_type: RequestType::MemoryOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "memory-manager".to_string(),
            operation: "allocate_memory".to_string(),
            data: Some(serde_json::json!({
                "use_shared_memory": true
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let isolation_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Isolation Ruleset"
            && v.message.contains("Shared memory usage is forbidden")
    });
    assert!(isolation_violation.is_some());
    assert_eq!(
        isolation_violation.unwrap().severity,
        ViolationSeverity::High
    );
}

/// Test Telemetry Policy Pack (#9)
async fn test_telemetry_policy_pack(manager: &PolicyPackManager) {
    // Test excessive sampling rate (should trigger warning)
    let request = PolicyRequest {
        request_id: "test-telemetry-sampling".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "telemetry-collector".to_string(),
            operation: "configure_sampling".to_string(),
            data: Some(serde_json::json!({
                "sampling_rate": 1.5
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    // Telemetry policy pack emits Warning violations which don't block at Error enforcement level
    assert!(result.valid);
    assert!(!result.violations.is_empty());

    let telemetry_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Telemetry Ruleset" && v.message.contains("Sampling rate exceeds maximum")
    });
    assert!(telemetry_violation.is_some());
    assert_eq!(
        telemetry_violation.unwrap().severity,
        ViolationSeverity::Medium
    );
}

/// Test Retention Policy Pack (#10)
async fn test_retention_policy_pack(manager: &PolicyPackManager) {
    // Test bundle count exceeding retention limit (should trigger warning)
    let request = PolicyRequest {
        request_id: "test-retention-bundle-count".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "bundle-manager".to_string(),
            operation: "check_retention".to_string(),
            data: Some(serde_json::json!({
                "bundle_count": 15
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(result.valid); // Should be valid but with warnings
    assert!(!result.warnings.is_empty());

    let retention_warning = result.warnings.iter().find(|w| {
        w.policy_pack == "Retention Ruleset"
            && w.message.contains("Bundle count exceeds retention limit")
    });
    assert!(retention_warning.is_some());
}

/// Test Performance Policy Pack (#11)
async fn test_performance_policy_pack(manager: &PolicyPackManager) {
    // Test latency exceeding p95 budget (should be blocked)
    let request = PolicyRequest {
        request_id: "test-performance-latency".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "inference-engine".to_string(),
            operation: "generate_response".to_string(),
            data: Some(serde_json::json!({
                "latency_p95_ms": 30.0
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let performance_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Performance Ruleset" && v.message.contains("Latency exceeds p95 budget")
    });
    assert!(performance_violation.is_some());
    assert_eq!(
        performance_violation.unwrap().severity,
        ViolationSeverity::High
    );
}

/// Test Memory Policy Pack (#12)
async fn test_memory_policy_pack(manager: &PolicyPackManager) {
    // Test memory headroom below minimum threshold (should be blocked)
    let request = PolicyRequest {
        request_id: "test-memory-headroom".to_string(),
        request_type: RequestType::MemoryOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "memory-manager".to_string(),
            operation: "check_headroom".to_string(),
            data: Some(serde_json::json!({
                "headroom_pct": 10.0
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let memory_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Memory Ruleset"
            && v.message
                .contains("Memory headroom below minimum threshold")
    });
    assert!(memory_violation.is_some());
    assert_eq!(memory_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test Artifacts Policy Pack (#13)
async fn test_artifacts_policy_pack(manager: &PolicyPackManager) {
    // Test artifact without signature (should be blocked)
    let request = PolicyRequest {
        request_id: "test-artifacts-no-signature".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "artifact-manager".to_string(),
            operation: "import_artifact".to_string(),
            data: Some(serde_json::json!({
                "artifact": {
                    "signature": null,
                    "sbom": "present"
                }
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let artifacts_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Artifacts Ruleset" && v.message.contains("Artifact signature is required")
    });
    assert!(artifacts_violation.is_some());
    assert_eq!(
        artifacts_violation.unwrap().severity,
        ViolationSeverity::Critical
    );
}

/// Test Secrets Policy Pack (#14)
async fn test_secrets_policy_pack(manager: &PolicyPackManager) {
    // Test plaintext secrets (should be blocked)
    let request = PolicyRequest {
        request_id: "test-secrets-plaintext".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "secrets-manager".to_string(),
            operation: "store_secret".to_string(),
            data: Some(serde_json::json!({
                "secrets": [
                    {"name": "api_key", "plaintext": true, "value": "secret123"}
                ]
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let secrets_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Secrets Ruleset"
            && v.message.contains("Plaintext secrets are not allowed")
    });
    assert!(secrets_violation.is_some());
    assert_eq!(
        secrets_violation.unwrap().severity,
        ViolationSeverity::Critical
    );
}

/// Test Build Release Policy Pack (#15)
async fn test_build_release_policy_pack(manager: &PolicyPackManager) {
    // Test replay with non-zero diff (should be blocked)
    let request = PolicyRequest {
        request_id: "test-build-release-replay".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "build-system".to_string(),
            operation: "verify_determinism".to_string(),
            data: Some(serde_json::json!({
                "replay_diff": 0.1
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let build_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Build & Release Ruleset"
            && v.message.contains("Replay shows non-zero diff")
    });
    assert!(build_violation.is_some());
    assert_eq!(
        build_violation.unwrap().severity,
        ViolationSeverity::Critical
    );
}

/// Test Compliance Policy Pack (#16)
async fn test_compliance_policy_pack(manager: &PolicyPackManager) {
    // Test compliance without evidence links (should be blocked)
    let request = PolicyRequest {
        request_id: "test-compliance-no-evidence".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "compliance-checker".to_string(),
            operation: "verify_compliance".to_string(),
            data: Some(serde_json::json!({
                "compliance": {
                    "evidence_links": null
                }
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let compliance_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Compliance Ruleset"
            && v.message.contains("Compliance evidence links are required")
    });
    assert!(compliance_violation.is_some());
    assert_eq!(
        compliance_violation.unwrap().severity,
        ViolationSeverity::High
    );
}

/// Test Incident Policy Pack (#17)
async fn test_incident_policy_pack(manager: &PolicyPackManager) {
    // Test incident without response procedures (should be blocked)
    let request = PolicyRequest {
        request_id: "test-incident-no-procedures".to_string(),
        request_type: RequestType::SystemOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "incident-manager".to_string(),
            operation: "handle_incident".to_string(),
            data: Some(serde_json::json!({
                "incident_type": "memory_pressure",
                "procedures": null
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let incident_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Incident Ruleset"
            && v.message
                .contains("Incident response procedures are required")
    });
    assert!(incident_violation.is_some());
    assert_eq!(
        incident_violation.unwrap().severity,
        ViolationSeverity::High
    );
}

/// Test LLM Output Policy Pack (#18)
async fn test_llm_output_policy_pack(manager: &PolicyPackManager) {
    // Test non-JSON output format (should be blocked)
    let request = PolicyRequest {
        request_id: "test-llm-output-format".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "inference-engine".to_string(),
            operation: "generate_response".to_string(),
            data: Some(serde_json::json!({
                "output_format": "text"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let output_violation = result.violations.iter().find(|v| {
        v.policy_pack == "LLM Output Ruleset" && v.message.contains("Output format must be JSON")
    });
    assert!(output_violation.is_some());
    assert_eq!(output_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test Adapter Lifecycle Policy Pack (#19)
async fn test_adapter_lifecycle_policy_pack(manager: &PolicyPackManager) {
    // Test adapter activation below minimum threshold (should trigger warning)
    let request = PolicyRequest {
        request_id: "test-adapter-lifecycle-activation".to_string(),
        request_type: RequestType::AdapterOperation,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "adapter-manager".to_string(),
            operation: "check_activation".to_string(),
            data: Some(serde_json::json!({
                "activation_pct": 1.0
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(result.valid); // Should be valid but with warnings
    assert!(!result.warnings.is_empty());

    let lifecycle_warning = result.warnings.iter().find(|w| {
        w.policy_pack == "Adapter Lifecycle Ruleset"
            && w.message
                .contains("Adapter activation below minimum threshold")
    });
    assert!(lifecycle_warning.is_some());
}

/// Test Full Pack Policy Pack (#20)
async fn test_full_pack_policy_pack(manager: &PolicyPackManager) {
    // Test invalid policy schema version (should be blocked)
    let request = PolicyRequest {
        request_id: "test-full-pack-schema".to_string(),
        request_type: RequestType::PolicyUpdate,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "policy-manager".to_string(),
            operation: "update_policy".to_string(),
            data: Some(serde_json::json!({
                "schema": "adapteros.policy.v2"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&request).unwrap();
    assert!(!result.valid);
    assert!(!result.violations.is_empty());

    let schema_violation = result.violations.iter().find(|v| {
        v.policy_pack == "Full Pack Example" && v.message.contains("Invalid policy schema version")
    });
    assert!(schema_violation.is_some());
    assert_eq!(schema_violation.unwrap().severity, ViolationSeverity::High);
}

/// Test policy pack configuration management
#[tokio::test]
async fn test_policy_pack_configuration() {
    let mut manager = PolicyPackManager::new();

    // Test getting policy pack configuration
    let egress_config = manager.get_pack_config(&PolicyPackId::Egress);
    assert!(egress_config.is_some());
    assert!(egress_config.unwrap().enabled);

    // Test updating policy pack configuration
    let new_config = PolicyPackConfig {
        id: PolicyPackId::Egress,
        config: serde_json::json!({"mode": "deny_all"}),
        enabled: false,
        enforcement_level: EnforcementLevel::Warning,
        last_updated: chrono::Utc::now(),
    };

    manager
        .update_pack_config(PolicyPackId::Egress, new_config)
        .unwrap();

    let updated_config = manager.get_pack_config(&PolicyPackId::Egress);
    assert!(updated_config.is_some());
    assert!(!updated_config.unwrap().enabled);

    // Test enabling/disabling policy packs
    manager
        .set_pack_enabled(PolicyPackId::Egress, true)
        .unwrap();
    let enabled_config = manager.get_pack_config(&PolicyPackId::Egress);
    assert!(enabled_config.is_some());
    assert!(enabled_config.unwrap().enabled);
}

/// Test policy pack validation performance
#[tokio::test]
async fn test_policy_pack_validation_performance() {
    let manager = PolicyPackManager::new();

    let request = PolicyRequest {
        request_id: "test-performance".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "test-component".to_string(),
            operation: "test-operation".to_string(),
            data: Some(serde_json::json!({
                "confidence": 0.8,
                "evidence_spans": [{"id": "span1", "content": "test"}],
                "output_format": "json"
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let start_time = std::time::Instant::now();
    let result = manager.validate_request(&request).unwrap();
    let duration = start_time.elapsed();

    // Policy validation should complete within reasonable time
    assert!(duration.as_millis() < 100);
    assert!(result.duration_ms < 100);

    // Should be valid with no violations
    assert!(result.valid);
    assert!(result.violations.is_empty());
}

/// Test comprehensive policy pack integration
#[tokio::test]
async fn test_comprehensive_policy_integration() {
    let manager = PolicyPackManager::new();

    // Test a complex request that triggers multiple policy packs
    let complex_request = PolicyRequest {
        request_id: "test-comprehensive".to_string(),
        request_type: RequestType::Inference,
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
        context: PolicyContext {
            component: "inference-engine".to_string(),
            operation: "generate_response".to_string(),
            data: Some(serde_json::json!({
                "protocol": "tcp",
                "confidence": 0.3,
                "evidence_spans": [],
                "numeric_values": [{"value": 100, "unit": null}],
                "output_format": "text",
                "latency_p95_ms": 30.0,
                "headroom_pct": 10.0
            })),
            priority: Priority::Normal,
        },
        metadata: None,
    };

    let result = manager.validate_request(&complex_request).unwrap();

    // Should have multiple violations from different policy packs
    assert!(!result.valid);
    assert!(result.violations.len() > 5);

    // Check that multiple policy packs are involved
    let policy_packs: std::collections::HashSet<String> = result
        .violations
        .iter()
        .map(|v| v.policy_pack.clone())
        .collect();

    assert!(policy_packs.len() > 5);
    assert!(policy_packs.contains("Egress Ruleset"));
    assert!(policy_packs.contains("Evidence Ruleset"));
    assert!(policy_packs.contains("Refusal Ruleset"));
    assert!(policy_packs.contains("Numeric & Units Ruleset"));
    assert!(policy_packs.contains("LLM Output Ruleset"));
    assert!(policy_packs.contains("Performance Ruleset"));
    assert!(policy_packs.contains("Memory Ruleset"));
}

/// Test policy pack enforcement levels
#[tokio::test]
async fn test_policy_pack_enforcement_levels() {
    let manager = PolicyPackManager::new();

    // Test with different enforcement levels
    let configs = manager.get_all_configs();

    for (pack_id, config) in configs {
        match config.enforcement_level {
            EnforcementLevel::Info => {
                // Info level should not block operations
                let request = create_test_request(pack_id);
                let result = manager.validate_request(&request).unwrap();
                // Info level violations should not make the request invalid
                assert!(
                    result.valid
                        || result
                            .violations
                            .iter()
                            .all(|v| matches!(v.severity, ViolationSeverity::Low))
                );
            }
            EnforcementLevel::Warning => {
                // Warning level should allow operations but flag issues
                let request = create_test_request(pack_id);
                let result = manager.validate_request(&request).unwrap();
                // Warning level should be valid or have only warning violations
                assert!(
                    result.valid
                        || result
                            .violations
                            .iter()
                            .all(|v| matches!(v.severity, ViolationSeverity::Medium))
                );
            }
            EnforcementLevel::Error => {
                // Error level should block operations when violations occur
                let request = create_test_request(pack_id);
                let result = manager.validate_request(&request).unwrap();
                // Error level should block operations if Error-severity violations are present
                let has_error_violations = result
                    .violations
                    .iter()
                    .any(|v| matches!(v.severity, ViolationSeverity::High));
                let has_critical_violations = result
                    .violations
                    .iter()
                    .any(|v| matches!(v.severity, ViolationSeverity::Critical));
                let has_blocker_violations = result
                    .violations
                    .iter()
                    .any(|v| matches!(v.severity, ViolationSeverity::Critical));

                if has_error_violations || has_critical_violations || has_blocker_violations {
                    assert!(!result.valid);
                } else {
                    // If no blocking violations, request should be valid (even with Warning violations)
                    // Note: Refusal pack emits Warning violations but should still be valid
                    assert!(result.valid);
                }
            }
            EnforcementLevel::Critical => {
                // Critical level should block operations with critical violations
                let request = create_test_request(pack_id);
                let result = manager.validate_request(&request).unwrap();
                // Critical level should block operations
                assert!(!result.valid);
                assert!(result
                    .violations
                    .iter()
                    .any(|v| matches!(v.severity, ViolationSeverity::Critical)));
            }
        }
    }
}

/// Helper function to create test requests for different policy packs
fn create_test_request(pack_id: &PolicyPackId) -> PolicyRequest {
    match pack_id {
        PolicyPackId::Egress => PolicyRequest {
            request_id: "test-egress".to_string(),
            request_type: RequestType::NetworkOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "test-component".to_string(),
                operation: "network_connection".to_string(),
                data: Some(serde_json::json!({"protocol": "tcp"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Evidence => PolicyRequest {
            request_id: "test-evidence".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "inference-engine".to_string(),
                operation: "generate_response".to_string(),
                data: Some(serde_json::json!({"evidence_spans": []})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Determinism => PolicyRequest {
            request_id: "test-determinism".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "kernel-compiler".to_string(),
                operation: "kernel_compile".to_string(),
                data: Some(serde_json::json!({"kernel_source": "attention.metal"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Router => PolicyRequest {
            request_id: "test-router".to_string(),
            request_type: RequestType::AdapterOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "router".to_string(),
                operation: "configure_k_sparse".to_string(),
                data: Some(serde_json::json!({"k_sparse": 5})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Refusal => PolicyRequest {
            request_id: "test-refusal".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "inference-engine".to_string(),
                operation: "generate_response".to_string(),
                data: Some(serde_json::json!({"confidence": 0.3})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::NumericUnits => PolicyRequest {
            request_id: "test-numeric-units".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "inference-engine".to_string(),
                operation: "generate_response".to_string(),
                data: Some(serde_json::json!({"numeric_values": [{"value": 100, "unit": null}]})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::LlmOutput => PolicyRequest {
            request_id: "test-llm-output".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "inference-engine".to_string(),
                operation: "generate_response".to_string(),
                data: Some(serde_json::json!({"output_format": "text"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Performance => PolicyRequest {
            request_id: "test-performance".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "inference-engine".to_string(),
                operation: "generate_response".to_string(),
                data: Some(serde_json::json!({"latency_p95_ms": 30.0})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Memory => PolicyRequest {
            request_id: "test-memory".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "inference-engine".to_string(),
                operation: "generate_response".to_string(),
                data: Some(serde_json::json!({"headroom_pct": 10.0})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::RagIndex => PolicyRequest {
            request_id: "test-rag-index".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "rag-system".to_string(),
                operation: "cross_tenant_access".to_string(),
                data: Some(serde_json::json!({"target_tenant": "other-tenant"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Isolation => PolicyRequest {
            request_id: "test-isolation".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "isolation-manager".to_string(),
                operation: "shared_memory_access".to_string(),
                data: Some(serde_json::json!({"memory_type": "shared"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Artifacts => PolicyRequest {
            request_id: "test-artifacts".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "artifact-manager".to_string(),
                operation: "load_unsigned_artifact".to_string(),
                data: Some(serde_json::json!({"signature": null})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Secrets => PolicyRequest {
            request_id: "test-secrets".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "secret-manager".to_string(),
                operation: "store_plaintext_secret".to_string(),
                data: Some(serde_json::json!({"secret": "plaintext-value"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::BuildRelease => PolicyRequest {
            request_id: "test-build-release".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "build-system".to_string(),
                operation: "promote_without_tests".to_string(),
                data: Some(serde_json::json!({"tests_passed": false})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Compliance => PolicyRequest {
            request_id: "test-compliance".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "compliance-checker".to_string(),
                operation: "promote_without_evidence".to_string(),
                data: Some(serde_json::json!({"evidence_links": []})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::Incident => PolicyRequest {
            request_id: "test-incident".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "incident-manager".to_string(),
                operation: "memory_pressure_response".to_string(),
                data: Some(serde_json::json!({"response_action": "invalid_action"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::AdapterLifecycle => PolicyRequest {
            request_id: "test-adapter-lifecycle".to_string(),
            request_type: RequestType::AdapterOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "adapter-manager".to_string(),
                operation: "load_unregistered_adapter".to_string(),
                data: Some(serde_json::json!({"adapter_hash": "unregistered-hash"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        PolicyPackId::FullPack => PolicyRequest {
            request_id: "test-full-pack".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "policy-manager".to_string(),
                operation: "validate_invalid_schema".to_string(),
                data: Some(serde_json::json!({"schema_version": "invalid"})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
        _ => PolicyRequest {
            request_id: "test-generic".to_string(),
            request_type: RequestType::SystemOperation,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "test-component".to_string(),
                operation: "test-operation".to_string(),
                data: Some(serde_json::json!({})),
                priority: Priority::Normal,
            },
            metadata: None,
        },
    }
}
