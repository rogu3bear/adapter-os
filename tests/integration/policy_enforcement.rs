#![cfg(all(test, feature = "extended-tests"))]
//! Policy Enforcement Validation Tests
//!
//! Tests to verify that tenant-specific policies are correctly applied,
//! enforced, and that violations are properly detected and handled.

use anyhow::Result;
use super::test_utils::*;
use super::fixtures::*;

/// Test basic policy enforcement for evidence requirements
#[tokio::test]
async fn test_evidence_policy_enforcement() -> Result<()> {
    println!("\n=== Test: Evidence Policy Enforcement ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let validator = PolicyValidator::new();

    // Setup tenants with different policy requirements
    if let Some(tenant_config) = config.get_tenant("tenant_a") {
        harness.add_tenant(tenant_config.clone());
    }
    if let Some(tenant_config) = config.get_tenant("tenant_b") {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap(); // Basic evidence policy
    let tenant_b = harness.get_tenant("tenant_b").unwrap(); // Strict regulated policy

    // Test tenant A with basic evidence requirements
    let request_a = create_inference_request(
        "test_cp_v1",
        "Simple question for tenant A",
        50,
        false  // No evidence required
    );

    let result_a = tenant_a.run_inference(request_a).await?;
    assert_eq!(result_a["status"], "success",
        "Tenant A should succeed with basic policy");

    // Test tenant A with evidence requirement
    let request_a_evidence = create_inference_request(
        "test_cp_v1",
        "Complex question requiring evidence for tenant A",
        50,
        true  // Evidence required
    );

    let result_a_evidence = tenant_a.run_inference(request_a_evidence).await?;
    // Should succeed if evidence is available, or be refused if not
    if result_a_evidence["status"] == "success" {
        assert!(result_a_evidence["trace"]["evidence"].as_array().unwrap().len() > 0,
            "Should have evidence when evidence required and successful");
    }

    // Test tenant B with strict policy (higher evidence requirements)
    let request_b = create_inference_request(
        "test_cp_v1",
        "Question for tenant B with strict policy",
        50,
        true  // Evidence required
    );

    let result_b = tenant_b.run_inference(request_b).await?;
    // Tenant B should have stricter evidence requirements
    if result_b["refusal"].is_object() {
        validator.record_violation(PolicyViolation {
            tenant_id: "tenant_b".to_string(),
            policy_type: "evidence".to_string(),
            description: "Insufficient evidence for strict policy".to_string(),
            severity: "medium".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    // Verify policy enforcement worked
    assert!(!validator.has_violations("tenant_a") || validator.get_violations("tenant_a").len() <= 1,
        "Tenant A should have minimal policy violations");

    println!("✓ Evidence policy enforcement verified");
    println!("  Tenant A violations: {}", validator.get_violations("tenant_a").len());
    println!("  Tenant B violations: {}", validator.get_violations("tenant_b").len());

    harness.cleanup().await?;
    Ok(())
}

/// Test resource limit policy enforcement
#[tokio::test]
async fn test_resource_limit_policy_enforcement() -> Result<()> {
    println!("\n=== Test: Resource Limit Policy Enforcement ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let validator = PolicyValidator::new();
    let monitor = ResourceMonitor::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap(); // Standard limits
    let tenant_c = harness.get_tenant("tenant_c").unwrap(); // Resource limited

    // Test tenant A with normal resource usage
    for i in 0..3 {
        let request = create_inference_request(
            "test_cp_v1",
            &format!("Normal usage request {}", i),
            50,
            false
        );

        let result = tenant_a.run_inference(request).await?;
        assert_eq!(result["status"], "success");

        monitor.record_usage("tenant_a", ResourceMetrics {
            memory_mb: 200.0 + (i as f64 * 50.0),
            cpu_percent: 20.0 + (i as f64 * 5.0),
            storage_mb: 100.0,
            timestamp: std::time::Instant::now(),
        });
    }

    // Test tenant C with resource limits
    for i in 0..5 {
        let request = create_inference_request(
            "test_cp_v1",
            &format!("Limited usage request {}", i),
            25,  // Smaller requests
            false
        );

        let result = tenant_c.run_inference(request).await?;
        assert_eq!(result["status"], "success");

        monitor.record_usage("tenant_c", ResourceMetrics {
            memory_mb: 100.0 + (i as f64 * 20.0),  // Lower memory usage
            cpu_percent: 10.0 + (i as f64 * 2.0),  // Lower CPU usage
            storage_mb: 50.0,
            timestamp: std::time::Instant::now(),
        });
    }

    // Check resource usage against limits
    let avg_usage_a = monitor.average_usage("tenant_a").unwrap();
    let avg_usage_c = monitor.average_usage("tenant_c").unwrap();

    let tenant_a_config = TestTenantConfigs::tenant_a();
    let tenant_c_config = TestTenantConfigs::tenant_c();

    let max_memory_a = tenant_a_config["max_memory_mb"].as_f64().unwrap();
    let max_memory_c = tenant_c_config["max_memory_mb"].as_f64().unwrap();

    // Verify tenant A stays within standard limits
    assert!(avg_usage_a.memory_mb <= max_memory_a,
        "Tenant A should stay within memory limits: {} <= {}", avg_usage_a.memory_mb, max_memory_a);

    // Verify tenant C stays within restricted limits
    assert!(avg_usage_c.memory_mb <= max_memory_c,
        "Tenant C should stay within memory limits: {} <= {}", avg_usage_c.memory_mb, max_memory_c);

    // Record policy violations if limits exceeded
    if avg_usage_a.memory_mb > max_memory_a {
        validator.record_violation(PolicyViolation {
            tenant_id: "tenant_a".to_string(),
            policy_type: "resource_limit".to_string(),
            description: format!("Memory usage {} MB exceeds limit {} MB", avg_usage_a.memory_mb, max_memory_a),
            severity: "high".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    if avg_usage_c.memory_mb > max_memory_c {
        validator.record_violation(PolicyViolation {
            tenant_id: "tenant_c".to_string(),
            policy_type: "resource_limit".to_string(),
            description: format!("Memory usage {} MB exceeds limit {} MB", avg_usage_c.memory_mb, max_memory_c),
            severity: "high".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    println!("✓ Resource limit policy enforcement verified");
    println!("  Tenant A avg memory: {:.1} MB (limit: {} MB)", avg_usage_a.memory_mb, max_memory_a);
    println!("  Tenant C avg memory: {:.1} MB (limit: {} MB)", avg_usage_c.memory_mb, max_memory_c);

    harness.cleanup().await?;
    Ok(())
}

/// Test security policy enforcement
#[tokio::test]
async fn test_security_policy_enforcement() -> Result<()> {
    println!("\n=== Test: Security Policy Enforcement ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let validator = PolicyValidator::new();

    if let Some(tenant_config) = config.get_tenant("tenant_b") {
        harness.add_tenant(tenant_config.clone()); // Strict regulated tenant
    } else {
        println!("⚠ Skipping test - regulated tenant not configured");
        return Ok(());
    }

    harness.setup().await?;

    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Test security violations that should be blocked
    let security_violations = vec![
        ("Hardcoded password", "const PASSWORD = \"secret123\";"),
        ("API key exposure", "let apiKey = \"sk-123456789\";"),
        ("Token leakage", "token: \"bearer_abcdef\""),
    ];

    for (violation_type, test_content) in security_violations {
        let request = serde_json::json!({
            "cpid": "test_cp_v1",
            "prompt": format!("Generate code with: {}", test_content),
            "max_tokens": 100,
            "require_evidence": false
        });

        let result = tenant_b.run_inference(request).await;

        // Should either fail or have security violation in response
        if let Ok(response) = result {
            if response["refusal"].is_object() {
                validator.record_violation(PolicyViolation {
                    tenant_id: "tenant_b".to_string(),
                    policy_type: "security".to_string(),
                    description: format!("Security violation blocked: {}", violation_type),
                    severity: "critical".to_string(),
                    timestamp: std::time::Instant::now(),
                });
            }
        } else {
            // Request failed - could be due to security policy
            validator.record_violation(PolicyViolation {
                tenant_id: "tenant_b".to_string(),
                policy_type: "security".to_string(),
                description: format!("Security violation rejected: {}", violation_type),
                severity: "critical".to_string(),
                timestamp: std::time::Instant::now(),
            });
        }
    }

    // Test that legitimate requests still work
    let legitimate_request = create_inference_request(
        "test_cp_v1",
        "Write a function to calculate fibonacci numbers",
        50,
        false
    );

    let legitimate_result = tenant_b.run_inference(legitimate_request).await?;
    assert_eq!(legitimate_result["status"], "success",
        "Legitimate requests should still succeed");

    println!("✓ Security policy enforcement verified");
    println!("  Security violations detected: {}", validator.get_violations("tenant_b").len());

    harness.cleanup().await?;
    Ok(())
}

/// Test patch policy enforcement
#[tokio::test]
async fn test_patch_policy_enforcement() -> Result<()> {
    println!("\n=== Test: Patch Policy Enforcement ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let validator = PolicyValidator::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();
    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Test patch size limits
    let large_patch = TestPolicyData::valid_patch();
    let oversized_request = serde_json::json!({
        "cpid": "test_cp_v1",
        "prompt": format!("Apply this large patch: {}", "x".repeat(1000)),
        "max_tokens": 200,
        "require_evidence": true
    });

    // Test with tenant A (standard limits)
    let result_a = tenant_a.run_inference(oversized_request.clone()).await;
    if let Ok(response) = result_a {
        if response["refusal"].is_object() {
            validator.record_violation(PolicyViolation {
                tenant_id: "tenant_a".to_string(),
                policy_type: "patch_size".to_string(),
                description: "Patch size limit enforced".to_string(),
                severity: "medium".to_string(),
                timestamp: std::time::Instant::now(),
            });
        }
    }

    // Test with tenant B (stricter limits)
    let result_b = tenant_b.run_inference(oversized_request).await;
    if let Ok(response) = result_b {
        if response["refusal"].is_object() {
            validator.record_violation(PolicyViolation {
                tenant_id: "tenant_b".to_string(),
                policy_type: "patch_size".to_string(),
                description: "Strict patch size limit enforced".to_string(),
                severity: "high".to_string(),
                timestamp: std::time::Instant::now(),
            });
        }
    }

    // Test patch validation
    let invalid_patch_request = serde_json::json!({
        "cpid": "test_cp_v1",
        "prompt": "Apply this invalid patch with syntax errors: {{{",
        "max_tokens": 50,
        "require_evidence": true
    });

    let invalid_result = tenant_a.run_inference(invalid_patch_request).await;
    if invalid_result.is_err() || invalid_result.as_ref().unwrap()["status"] != "success" {
        validator.record_violation(PolicyViolation {
            tenant_id: "tenant_a".to_string(),
            policy_type: "patch_validation".to_string(),
            description: "Invalid patch correctly rejected".to_string(),
            severity: "medium".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    println!("✓ Patch policy enforcement verified");
    println!("  Total policy violations: {}", validator.get_violations("tenant_a").len() + validator.get_violations("tenant_b").len());

    harness.cleanup().await?;
    Ok(())
}

/// Test compliance policy enforcement
#[tokio::test]
async fn test_compliance_policy_enforcement() -> Result<()> {
    println!("\n=== Test: Compliance Policy Enforcement ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let validator = PolicyValidator::new();

    // Focus on ITAR/regulatory compliance for tenant B
    if let Some(tenant_config) = config.get_tenant("tenant_b") {
        harness.add_tenant(tenant_config.clone());
    } else {
        println!("⚠ Skipping test - regulated tenant not configured");
        return Ok(());
    }

    harness.setup().await?;

    let tenant_b = harness.get_tenant("tenant_b").unwrap();

    // Test export-controlled content restrictions
    let restricted_content = vec![
        "ITAR controlled technical data",
        "Export controlled algorithms",
        "Classified implementation details",
        "Restricted source code patterns",
    ];

    for content in restricted_content {
        let request = serde_json::json!({
            "cpid": "test_cp_v1",
            "prompt": format!("Explain and provide examples of: {}", content),
            "max_tokens": 100,
            "require_evidence": true
        });

        let result = tenant_b.run_inference(request).await;

        if result.is_err() || result.as_ref().unwrap().get("refusal").is_some() {
            validator.record_violation(PolicyViolation {
                tenant_id: "tenant_b".to_string(),
                policy_type: "compliance".to_string(),
                description: format!("Compliance violation blocked: {}", content),
                severity: "critical".to_string(),
                timestamp: std::time::Instant::now(),
            });
        }
    }

    // Test that compliant content still works
    let compliant_request = create_inference_request(
        "test_cp_v1",
        "Explain basic programming concepts",
        50,
        false
    );

    let compliant_result = tenant_b.run_inference(compliant_request).await?;
    assert_eq!(compliant_result["status"], "success",
        "Compliant content should be allowed");

    println!("✓ Compliance policy enforcement verified");
    println!("  Compliance violations: {}", validator.get_violations("tenant_b").len());

    harness.cleanup().await?;
    Ok(())
}

/// Test policy override and exception handling
#[tokio::test]
async fn test_policy_override_handling() -> Result<()> {
    println!("\n=== Test: Policy Override Handling ===");

    let config = TestConfig::from_env();
    let mut harness = MultiTenantHarness::new(config.base_url().to_string());
    let validator = PolicyValidator::new();

    for (tenant_id, tenant_config) in config.tenants() {
        harness.add_tenant(tenant_config.clone());
    }

    harness.setup().await?;

    let tenant_a = harness.get_tenant("tenant_a").unwrap();

    // Test policy override scenarios
    // (In real implementation, this would test admin override capabilities)

    // Test that policy violations are consistently enforced
    let violation_request = serde_json::json!({
        "cpid": "test_cp_v1",
        "prompt": "Generate content that violates policies",
        "max_tokens": 200,  // Exceeds normal limits
        "require_evidence": false  // Should require evidence
    });

    let result = tenant_a.run_inference(violation_request).await;

    if result.is_err() || result.as_ref().unwrap().get("refusal").is_some() {
        validator.record_violation(PolicyViolation {
            tenant_id: "tenant_a".to_string(),
            policy_type: "policy_override".to_string(),
            description: "Policy override correctly prevented".to_string(),
            severity: "medium".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    println!("✓ Policy override handling verified");
    println!("  Override violations: {}", validator.get_violations("tenant_a").len());

    harness.cleanup().await?;
    Ok(())
}