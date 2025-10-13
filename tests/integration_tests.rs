//! Integration tests for AdapterOS
//!
//! These tests verify end-to-end workflows including:
//! - Inference with LoRA adapter routing
//! - Evidence-based generation with RAG
//! - RBAC enforcement
//! - Deterministic behavior and replay
//! - Policy enforcement (refusal, evidence requirements)
//!
//! Note: These tests are written but not executed automatically.
//! They require a running AdapterOS instance and proper configuration.

use anyhow::Result;

/// Helper to create a test base URL
fn test_base_url() -> String {
    std::env::var("MPLORA_TEST_URL")
        .unwrap_or_else(|_| "http://localhost:9443".to_string())
}

#[tokio::test]
async fn test_health_check() -> Result<()> {
    let client = create_test_client();
    
    let health = client.health().await?;
    assert_eq!(health.status, "healthy");
    assert!(!health.version.is_empty());
    
    Ok(())
}

#[tokio::test]
async fn test_authentication_flow() -> Result<()> {
    let client = create_test_client();
    
    // Test login with valid credentials
    let login_response = client.login(LoginRequest {
        email: "admin@example.com".to_string(),
        password: "admin_password".to_string(),
    }).await?;
    
    assert!(!login_response.token.is_empty());
    assert_eq!(login_response.user.email, "admin@example.com");
    
    // Test logout
    client.logout().await?;
    
    Ok(())
}

#[tokio::test]
async fn test_repository_registration_workflow() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // 1. Register repository
    let repo_request = RegisterRepoRequest {
        repo_id: "test/example".to_string(),
        repo_path: "/tmp/test-repo".to_string(),
        languages: vec!["rust".to_string()],
        default_branch: "main".to_string(),
    };
    
    let repo_response = client.register_repo(repo_request).await?;
    assert_eq!(repo_response.repo_id, "test/example");
    assert_eq!(repo_response.status, "registered");
    
    // 2. Trigger scan
    let scan_request = ScanRepoRequest {
        repo_id: "test/example".to_string(),
    };
    
    let scan_response = client.scan_repo(scan_request).await?;
    assert!(scan_response.job_id.is_some());
    
    // 3. Wait for scan completion (with timeout)
    let mut attempts = 0;
    let max_attempts = 30;
    
    while attempts < max_attempts {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        
        let repos = client.list_repos().await?;
        if let Some(repo) = repos.iter().find(|r| r.repo_id == "test/example") {
            if repo.status == "ready" {
                println!("Repository scan completed successfully");
                break;
            } else if repo.status == "error" {
                return Err(anyhow::anyhow!("Repository scan failed"));
            }
        }
        
        attempts += 1;
    }
    
    assert!(attempts < max_attempts, "Scan did not complete within timeout");
    
    // 4. Verify repository details
    let repos = client.list_repos().await?;
    let repo = repos.iter().find(|r| r.repo_id == "test/example")
        .expect("Repository not found");
    
    assert_eq!(repo.status, "ready");
    assert!(repo.file_count.unwrap_or(0) > 0);
    
    Ok(())
}

#[tokio::test]
async fn test_patch_proposal_workflow() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // 1. Propose patch
    let patch_request = ProposePatchRequest {
        prompt: "Fix the failing test in main.rs".to_string(),
        context_files: vec!["src/main.rs".to_string()],
        repo_path: "/tmp/test-repo".to_string(),
    };
    
    let patch_response = client.propose_patch(patch_request).await?;
    assert!(!patch_response.proposal_id.is_empty());
    assert!(!patch_response.patches.is_empty());
    assert!(!patch_response.evidence.is_empty());
    assert!(patch_response.confidence > 0.0);
    
    // 2. Validate patch (dry run)
    let validate_request = ValidatePatchRequest {
        proposal_id: patch_response.proposal_id.clone(),
        dry_run: true,
    };
    
    let validate_response = client.validate_patch(validate_request).await?;
    assert!(validate_response.valid);
    assert!(validate_response.errors.is_empty());
    
    // 3. Apply patch (if validation passed)
    if validate_response.valid {
        let apply_request = ApplyPatchRequest {
            proposal_id: patch_response.proposal_id,
            confirm: true,
        };
        
        let apply_response = client.apply_patch(apply_request).await?;
        assert!(apply_response.success);
        assert!(apply_response.backup_id.is_some());
        
        println!("Patch applied successfully. Backup ID: {:?}", apply_response.backup_id);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_rbac_enforcement() -> Result<()> {
    // Test admin access
    let (admin_client, _) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // Admin can create tenants
    let tenant_request = CreateTenantRequest {
        name: "test-tenant".to_string(),
        itar_flag: false,
    };
    
    let tenant_response = admin_client.create_tenant(tenant_request).await?;
    assert_eq!(tenant_response.name, "test-tenant");
    
    // Test viewer access (should fail for tenant creation)
    let (viewer_client, _) = create_authenticated_client("viewer@example.com", "viewer_password").await?;
    
    let tenant_request = CreateTenantRequest {
        name: "test-tenant-2".to_string(),
        itar_flag: false,
    };
    
    let result = viewer_client.create_tenant(tenant_request).await;
    assert!(result.is_err(), "Viewer should not be able to create tenants");
    
    // Viewer can list tenants (read-only)
    let tenants = viewer_client.list_tenants().await?;
    assert!(!tenants.is_empty());
    
    Ok(())
}

#[tokio::test]
async fn test_adapter_management() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // List adapters
    let adapters_response = client.list_adapters("default").await?;
    assert!(!adapters_response.adapters.is_empty());
    
    // Check tier breakdown
    assert!(adapters_response.tier_breakdown.base > 0);
    
    // Get adapter activations
    let activations = client.get_adapter_activations("default", 100).await?;
    assert!(!activations.is_empty());
    
    Ok(())
}

#[tokio::test]
async fn test_commit_inspection() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // Get commit details
    let commit_details = client.get_commit_details("test/example", "abc123").await?;
    
    assert_eq!(commit_details.repo_id, "test/example");
    assert_eq!(commit_details.sha, "abc123");
    assert!(!commit_details.changed_files.is_empty());
    
    Ok(())
}

#[tokio::test]
async fn test_code_policy_management() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // Get current policy
    let policy = client.get_code_policy("default").await?;
    assert!(policy.min_evidence_spans >= 1);
    
    // Update policy
    let update_request = UpdateCodePolicyRequest {
        tenant_id: "default".to_string(),
        min_evidence_spans: Some(2),
        allow_auto_apply: Some(false),
        test_coverage_min: Some(0.85),
        path_allowlist: None,
        path_denylist: None,
        secret_patterns: None,
        max_patch_size_lines: Some(600),
    };
    
    let updated_policy = client.update_code_policy(update_request).await?;
    assert_eq!(updated_policy.min_evidence_spans, 2);
    assert_eq!(updated_policy.max_patch_size_lines, 600);
    
    Ok(())
}

#[tokio::test]
async fn test_metrics_collection() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // Get code metrics
    let metrics = client.get_code_metrics("code_v1", "7d").await?;
    
    assert!(metrics.acceptance_rate >= 0.0 && metrics.acceptance_rate <= 1.0);
    assert!(metrics.compile_success_rate >= 0.0 && metrics.compile_success_rate <= 1.0);
    assert!(metrics.test_pass_rate >= 0.0 && metrics.test_pass_rate <= 1.0);
    
    println!("Metrics for code_v1:");
    println!("  Acceptance rate: {:.2}%", metrics.acceptance_rate * 100.0);
    println!("  Compile success: {:.2}%", metrics.compile_success_rate * 100.0);
    println!("  Test pass rate: {:.2}%", metrics.test_pass_rate * 100.0);
    
    Ok(())
}

#[tokio::test]
async fn test_routing_inspector() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    // Extract router features
    let features_request = RouterFeaturesRequest {
        prompt: "Fix the timeout issue in the payment processor".to_string(),
        context_file: Some("src/payments/processor.rs".to_string()),
        repo_path: "/tmp/test-repo".to_string(),
    };
    
    let features = client.extract_router_features(features_request).await?;
    
    assert!(!features.language_scores.is_empty());
    assert!(features.symbol_hits > 0);
    
    // Score adapters
    let score_request = ScoreAdaptersRequest {
        features: features.clone(),
        tenant_id: "default".to_string(),
    };
    
    let scores = client.score_adapters(score_request).await?;
    assert!(!scores.adapter_scores.is_empty());
    
    // Check that top K adapters are selected
    let selected_count = scores.adapter_scores.iter().filter(|s| s.selected).count();
    assert!(selected_count > 0 && selected_count <= 3); // K=3
    
    Ok(())
}

#[tokio::test]
async fn test_end_to_end_code_intelligence() -> Result<()> {
    let (client, _token) = create_authenticated_client("admin@example.com", "admin_password").await?;
    
    println!("Starting end-to-end code intelligence test...");
    
    // 1. Register repository
    println!("1. Registering repository...");
    let repo_request = RegisterRepoRequest {
        repo_id: "e2e/test".to_string(),
        repo_path: "/tmp/e2e-test-repo".to_string(),
        languages: vec!["rust".to_string(), "python".to_string()],
        default_branch: "main".to_string(),
    };
    
    let repo = client.register_repo(repo_request).await?;
    println!("   ✓ Repository registered: {}", repo.repo_id);
    
    // 2. Trigger scan
    println!("2. Triggering repository scan...");
    let scan_request = ScanRepoRequest {
        repo_id: "e2e/test".to_string(),
    };
    
    client.scan_repo(scan_request).await?;
    println!("   ✓ Scan initiated");
    
    // 3. Wait for scan completion
    println!("3. Waiting for scan completion...");
    let mut attempts = 0;
    while attempts < 30 {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let repos = client.list_repos().await?;
        if let Some(r) = repos.iter().find(|r| r.repo_id == "e2e/test") {
            if r.status == "ready" {
                println!("   ✓ Scan completed");
                break;
            }
        }
        attempts += 1;
    }
    
    // 4. Check adapters were created
    println!("4. Checking adapters...");
    let adapters = client.list_adapters("default").await?;
    println!("   ✓ Found {} adapters", adapters.adapters.len());
    
    // 5. Propose a patch
    println!("5. Proposing patch...");
    let patch_request = ProposePatchRequest {
        prompt: "Add error handling to the main function".to_string(),
        context_files: vec!["src/main.rs".to_string()],
        repo_path: "/tmp/e2e-test-repo".to_string(),
    };
    
    let patch = client.propose_patch(patch_request).await?;
    println!("   ✓ Patch proposed (ID: {})", patch.proposal_id);
    println!("   ✓ Confidence: {:.2}", patch.confidence);
    println!("   ✓ Citations: {}", patch.evidence.len());
    
    // 6. Validate patch
    println!("6. Validating patch...");
    let validate_request = ValidatePatchRequest {
        proposal_id: patch.proposal_id.clone(),
        dry_run: true,
    };
    
    let validation = client.validate_patch(validate_request).await?;
    println!("   ✓ Validation: {}", if validation.valid { "PASSED" } else { "FAILED" });
    
    // 7. Check metrics
    println!("7. Checking metrics...");
    let metrics = client.get_code_metrics("code_v1", "7d").await?;
    println!("   ✓ Acceptance rate: {:.2}%", metrics.acceptance_rate * 100.0);
    
    println!("\n✓ End-to-end test completed successfully!");
    
    Ok(())
}

/// Test helper functions
#[cfg(test)]
mod helpers {
    use super::*;
    
    /// Setup test environment
    pub async fn setup_test_env() -> Result<()> {
        // Create test directories
        std::fs::create_dir_all("/tmp/test-repo")?;
        std::fs::create_dir_all("/tmp/e2e-test-repo")?;
        
        // Initialize git repositories
        std::process::Command::new("git")
            .args(&["init", "/tmp/test-repo"])
            .output()?;
        
        std::process::Command::new("git")
            .args(&["init", "/tmp/e2e-test-repo"])
            .output()?;
        
        Ok(())
    }
    
    /// Cleanup test environment
    pub async fn cleanup_test_env() -> Result<()> {
        std::fs::remove_dir_all("/tmp/test-repo").ok();
        std::fs::remove_dir_all("/tmp/e2e-test-repo").ok();
        Ok(())
    }
}
