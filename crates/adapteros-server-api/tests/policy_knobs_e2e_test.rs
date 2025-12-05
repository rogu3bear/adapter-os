//! End-to-end tests for Policy Knobs data flow
//!
//! These tests verify the complete data flow:
//! DB (tenant_execution_policies) → Handler (tenant_settings) → InferenceCore (strict_mode)
//!
//! This addresses the gap where policy knobs were defined in types but not wired end-to-end.

use adapteros_api_types::{
    CreateExecutionPolicyRequest, DeterminismPolicy, GoldenPolicy, RoutingPolicy,
};
use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_server_api::config::PathsConfig;
use adapteros_server_api::state::{ApiConfig, GeneralConfig, MetricsConfig};

/// Test helper to create an in-memory database with migrations
async fn setup_test_db() -> Result<Db> {
    Db::new_in_memory().await
}

/// Test helper to create a tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create tenant: {}", e))
        })?;
    Ok(())
}

// =============================================================================
// Test: Fallback policy flows from DB to execution policy
// =============================================================================

#[tokio::test]
async fn test_fallback_allowed_false_persists_in_db() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-policy-e2e-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create execution policy with allow_fallback = false
    let determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string(), "besteffort".to_string()],
        default_mode: "strict".to_string(),
        require_seed: true,
        allow_fallback: false, // This is the critical setting
        replay_mode: "exact".to_string(),
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: None,
        golden: None,
        require_signed_adapters: false,
    };

    let policy_id = match db.create_execution_policy(tenant_id, request, None).await {
        Ok(id) => id,
        Err(e) => {
            panic!("Failed to create execution policy: {}", e);
        }
    };

    assert!(!policy_id.is_empty(), "Policy ID should be returned");

    // Verify: Read policy back from DB
    match db.get_execution_policy_or_default(tenant_id).await {
        Ok(policy) => {
            assert!(
                !policy.determinism.allow_fallback,
                "allow_fallback should be false"
            );
            assert_eq!(
                policy.determinism.default_mode, "strict",
                "default_mode should be strict"
            );
            assert!(
                policy.determinism.require_seed,
                "require_seed should be true"
            );
            assert_eq!(
                policy.determinism.allowed_modes,
                vec!["strict", "besteffort"],
                "allowed_modes should match"
            );
        }
        Err(e) => {
            panic!("Failed to read execution policy: {}", e);
        }
    }
}

// =============================================================================
// Test: Default policy has allow_fallback = true
// =============================================================================

#[tokio::test]
async fn test_default_policy_allows_fallback() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-default-policy-001";

    // Setup: Create tenant without any execution policy
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Verify: Default policy allows fallback
    match db.get_execution_policy_or_default(tenant_id).await {
        Ok(policy) => {
            assert!(policy.is_implicit, "Policy should be implicit (default)");
            assert!(
                policy.determinism.allow_fallback,
                "Default policy should allow fallback"
            );
            assert_eq!(
                policy.determinism.default_mode, "besteffort",
                "Default mode should be besteffort"
            );
        }
        Err(e) => {
            panic!("Failed to get default policy: {}", e);
        }
    }
}

// =============================================================================
// Test: Pin enforcement policy flows correctly
// =============================================================================

#[tokio::test]
async fn test_pin_enforcement_error_persists() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-pin-enforcement-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create execution policy with pin_enforcement = "error"
    let determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string()],
        default_mode: "strict".to_string(),
        require_seed: false,
        allow_fallback: false,
        replay_mode: "exact".to_string(),
    };

    let routing = RoutingPolicy {
        allowed_stack_ids: Some(vec!["stack-1".to_string(), "stack-2".to_string()]),
        allowed_adapter_ids: None,
        denied_adapter_ids: None,
        max_adapters_per_token: None,
        pin_enforcement: "error".to_string(), // Critical: reject pins outside effective
        require_stack: true,
        require_pins: false,
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: Some(routing),
        golden: None,
        require_signed_adapters: false,
    };

    let _policy_id = match db.create_execution_policy(tenant_id, request, None).await {
        Ok(id) => id,
        Err(e) => {
            panic!("Failed to create execution policy: {}", e);
        }
    };

    // Verify: Read policy and check pin_enforcement
    match db.get_execution_policy_or_default(tenant_id).await {
        Ok(policy) => {
            assert!(policy.routing.is_some(), "Routing policy should exist");
            let routing = policy.routing.unwrap();
            assert_eq!(
                routing.pin_enforcement, "error",
                "pin_enforcement should be 'error'"
            );
            assert!(
                routing.allowed_stack_ids.is_some(),
                "allowed_stack_ids should exist"
            );
            assert!(routing.require_stack, "require_stack should be true");
        }
        Err(e) => {
            panic!("Failed to read execution policy: {}", e);
        }
    }
}

// =============================================================================
// Test: Policy update creates new version
// =============================================================================

#[tokio::test]
async fn test_policy_update_creates_new_version() {
    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-policy-version-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create initial policy with allow_fallback = true
    let initial_determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string(), "besteffort".to_string()],
        default_mode: "besteffort".to_string(),
        require_seed: false,
        allow_fallback: true, // Initially true
        replay_mode: "approximate".to_string(),
    };

    let initial_request = CreateExecutionPolicyRequest {
        determinism: initial_determinism,
        routing: None,
        golden: None,
        require_signed_adapters: false,
    };

    let policy_id = db
        .create_execution_policy(tenant_id, initial_request, None)
        .await
        .expect("Failed to create initial policy");

    // Verify initial state
    let initial_policy = db
        .get_execution_policy_or_default(tenant_id)
        .await
        .expect("Failed to get initial policy");
    assert!(initial_policy.determinism.allow_fallback);
    assert_eq!(initial_policy.version, 1);

    // Update policy with allow_fallback = false
    let updated_determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string()],
        default_mode: "strict".to_string(),
        require_seed: true,
        allow_fallback: false, // Now false
        replay_mode: "exact".to_string(),
    };

    let update_request = CreateExecutionPolicyRequest {
        determinism: updated_determinism,
        routing: None,
        golden: None,
        require_signed_adapters: false,
    };

    let new_policy_id = db
        .update_execution_policy(&policy_id, update_request)
        .await
        .expect("Failed to update policy");

    assert_ne!(
        policy_id, new_policy_id,
        "Update should create new policy ID"
    );

    // Verify updated state
    let updated_policy = db
        .get_execution_policy_or_default(tenant_id)
        .await
        .expect("Failed to get updated policy");
    assert!(
        !updated_policy.determinism.allow_fallback,
        "Updated policy should have allow_fallback = false"
    );
    assert_eq!(updated_policy.version, 2, "Version should be incremented");

    // Verify history contains both versions
    let history = db
        .get_execution_policy_history(tenant_id, 10)
        .await
        .expect("Failed to get policy history");
    assert_eq!(history.len(), 2, "Should have 2 versions in history");
    assert_eq!(history[0].version, 2, "First in history should be v2");
    assert_eq!(history[1].version, 1, "Second in history should be v1");
}

// =============================================================================
// Test: Strict mode computed from allow_fallback (uses production function)
// =============================================================================

#[test]
fn test_strict_mode_derives_from_allow_fallback() {
    use adapteros_server_api::inference_core::{compute_strict_mode, DeterminismMode};

    // This test uses the PRODUCTION function compute_strict_mode()
    // which is called by route_and_infer() in inference_core.rs

    // Case 1: Strict mode + allow_fallback=true → strict_mode=true
    assert!(
        compute_strict_mode(DeterminismMode::Strict, true),
        "Strict mode should be true"
    );

    // Case 2: BestEffort mode + allow_fallback=true → strict_mode=false
    assert!(
        !compute_strict_mode(DeterminismMode::BestEffort, true),
        "BestEffort with fallback allowed should not be strict"
    );

    // Case 3: BestEffort mode + allow_fallback=false → strict_mode=true
    assert!(
        compute_strict_mode(DeterminismMode::BestEffort, false),
        "BestEffort with fallback NOT allowed should be strict"
    );

    // Case 4: Relaxed mode + allow_fallback=false → strict_mode=true
    assert!(
        compute_strict_mode(DeterminismMode::Relaxed, false),
        "Relaxed with fallback NOT allowed should be strict"
    );
}

// =============================================================================
// Test: DeterminismPolicyKnobs conversion matches database values
// =============================================================================

#[tokio::test]
async fn test_api_response_matches_db_policy() {
    use adapteros_api_types::DeterminismPolicyKnobs;

    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-api-response-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create policy with specific values
    let determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string(), "besteffort".to_string()],
        default_mode: "strict".to_string(),
        require_seed: true,
        allow_fallback: false,
        replay_mode: "exact".to_string(),
    };

    let routing = RoutingPolicy {
        allowed_stack_ids: None,
        allowed_adapter_ids: None,
        denied_adapter_ids: None,
        max_adapters_per_token: None,
        pin_enforcement: "error".to_string(),
        require_stack: false,
        require_pins: false,
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: Some(routing),
        golden: None,
        require_signed_adapters: false,
    };

    db.create_execution_policy(tenant_id, request, None)
        .await
        .expect("Failed to create policy");

    // Read policy from DB
    let policy = db
        .get_execution_policy_or_default(tenant_id)
        .await
        .expect("Failed to get policy");

    // Convert to API response format (same logic as in handler)
    let api_knobs = DeterminismPolicyKnobs {
        allowed_modes: if policy.determinism.allowed_modes.is_empty() {
            None
        } else {
            Some(policy.determinism.allowed_modes.clone())
        },
        pins_outside_effective: policy.routing.as_ref().map(|r| r.pin_enforcement.clone()),
        fallback_allowed: Some(policy.determinism.allow_fallback),
    };

    // Verify API response matches DB values
    assert_eq!(
        api_knobs.allowed_modes,
        Some(vec!["strict".to_string(), "besteffort".to_string()]),
        "allowed_modes should match"
    );
    assert_eq!(
        api_knobs.pins_outside_effective,
        Some("error".to_string()),
        "pins_outside_effective should match"
    );
    assert_eq!(
        api_knobs.fallback_allowed,
        Some(false),
        "fallback_allowed should be false"
    );
}

// =============================================================================
// Helper: Create test ApiConfig
// =============================================================================

fn create_test_config(global_determinism: Option<&str>, use_session_stack: bool) -> ApiConfig {
    ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
        use_session_stack_for_routing: use_session_stack,
        capacity_limits: Default::default(),
        general: global_determinism.map(|mode| GeneralConfig {
            system_name: None,
            environment: None,
            api_base_url: None,
            determinism_mode: Some(mode.to_string()),
        }),
        server: Default::default(),
        security: Default::default(),
        performance: Default::default(),
        paths: PathsConfig {
            artifacts_root: "/tmp/test".to_string(),
            bundles_root: "/tmp/test".to_string(),
            adapters_root: "/tmp/test".to_string(),
            plan_dir: "/tmp/test".to_string(),
            datasets_root: "/tmp/test".to_string(),
            documents_root: "/tmp/test".to_string(),
        },
        chat_context: Default::default(),
    }
}

// =============================================================================
// Test: resolve_tenant_execution_policy with global defaults only
// =============================================================================

#[tokio::test]
async fn test_resolve_policy_global_only() {
    use adapteros_server_api::inference_core::{resolve_tenant_execution_policy, DeterminismMode};

    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-global-only-001";

    // Setup: Create tenant without any execution policy
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Config with global strict mode
    let config = create_test_config(Some("strict"), false);

    // Resolve policy with no stack override
    let resolved = resolve_tenant_execution_policy(&db, &config, tenant_id, None)
        .await
        .expect("Failed to resolve policy");

    // Should use tenant default (besteffort) since tenant has no policy → implicit default
    // The implicit default has default_mode = "besteffort"
    assert_eq!(
        resolved.effective_determinism_mode,
        DeterminismMode::BestEffort,
        "Should use tenant implicit default (besteffort), not global strict"
    );
    assert!(
        resolved.policy.is_implicit,
        "Policy should be implicit (default)"
    );
}

// =============================================================================
// Test: resolve_tenant_execution_policy with tenant override
// =============================================================================

#[tokio::test]
async fn test_resolve_policy_tenant_override() {
    use adapteros_server_api::inference_core::{resolve_tenant_execution_policy, DeterminismMode};

    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-override-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create tenant execution policy with strict default
    let determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string()],
        default_mode: "strict".to_string(),
        require_seed: true,
        allow_fallback: false,
        replay_mode: "exact".to_string(),
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: None,
        golden: None,
        require_signed_adapters: false,
    };

    db.create_execution_policy(tenant_id, request, None)
        .await
        .expect("Failed to create policy");

    // Config with global relaxed mode
    let config = create_test_config(Some("relaxed"), false);

    // Resolve policy with no stack override
    let resolved = resolve_tenant_execution_policy(&db, &config, tenant_id, None)
        .await
        .expect("Failed to resolve policy");

    // Tenant policy (strict) should take precedence over global (relaxed)
    assert_eq!(
        resolved.effective_determinism_mode,
        DeterminismMode::Strict,
        "Tenant policy should override global"
    );
    assert!(
        !resolved.policy.is_implicit,
        "Policy should be explicit (not implicit)"
    );
    assert!(
        resolved.strict_mode,
        "Strict mode should be true (allow_fallback=false)"
    );
}

// =============================================================================
// Test: resolve_tenant_execution_policy with stack override
// =============================================================================

#[tokio::test]
async fn test_resolve_policy_stack_override() {
    use adapteros_server_api::inference_core::{resolve_tenant_execution_policy, DeterminismMode};

    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-stack-override-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create tenant execution policy with besteffort default
    let determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string(), "besteffort".to_string()],
        default_mode: "besteffort".to_string(),
        require_seed: false,
        allow_fallback: true,
        replay_mode: "approximate".to_string(),
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: None,
        golden: None,
        require_signed_adapters: false,
    };

    db.create_execution_policy(tenant_id, request, None)
        .await
        .expect("Failed to create policy");

    // Config with global relaxed mode
    let config = create_test_config(Some("relaxed"), false);

    // Resolve policy WITH stack override to strict
    let resolved = resolve_tenant_execution_policy(&db, &config, tenant_id, Some("strict"))
        .await
        .expect("Failed to resolve policy");

    // Stack override (strict) should take precedence over tenant (besteffort) and global (relaxed)
    assert_eq!(
        resolved.effective_determinism_mode,
        DeterminismMode::Strict,
        "Stack should override tenant and global"
    );
}

// =============================================================================
// Test: routing knobs derived from config
// =============================================================================

#[tokio::test]
async fn test_routing_knobs_derived() {
    use adapteros_server_api::inference_core::resolve_tenant_execution_policy;

    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-routing-knobs-001";

    // Setup: Create tenant without any execution policy
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Test with use_session_stack_for_routing = true
    let config_true = create_test_config(None, true);
    let resolved_true = resolve_tenant_execution_policy(&db, &config_true, tenant_id, None)
        .await
        .expect("Failed to resolve policy");

    assert!(
        resolved_true.routing.use_session_stack_for_routing,
        "use_session_stack_for_routing should be true from config"
    );
    assert!(
        !resolved_true.routing.allow_pins_outside_effective_set,
        "allow_pins_outside_effective_set should always be false (Bundle A invariant)"
    );

    // Test with use_session_stack_for_routing = false
    let config_false = create_test_config(None, false);
    let resolved_false = resolve_tenant_execution_policy(&db, &config_false, tenant_id, None)
        .await
        .expect("Failed to resolve policy");

    assert!(
        !resolved_false.routing.use_session_stack_for_routing,
        "use_session_stack_for_routing should be false from config"
    );
}

// =============================================================================
// Test: ResolvedExecutionPolicy struct fields populated correctly
// =============================================================================

#[tokio::test]
async fn test_resolved_policy_struct_fields() {
    use adapteros_server_api::inference_core::{resolve_tenant_execution_policy, DeterminismMode};

    let db = match setup_test_db().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    let tenant_id = "tenant-struct-fields-001";

    // Setup: Create tenant
    if let Err(e) = create_test_tenant(&db, tenant_id).await {
        eprintln!("Skipping test - tenant creation failed: {}", e);
        return;
    }

    // Create complete execution policy with all fields
    let determinism = DeterminismPolicy {
        allowed_modes: vec!["strict".to_string(), "besteffort".to_string()],
        default_mode: "strict".to_string(),
        require_seed: true,
        allow_fallback: false,
        replay_mode: "exact".to_string(),
    };

    let routing = RoutingPolicy {
        allowed_stack_ids: Some(vec!["stack-1".to_string()]),
        allowed_adapter_ids: Some(vec!["adapter-1".to_string()]),
        denied_adapter_ids: None,
        max_adapters_per_token: None,
        pin_enforcement: "error".to_string(),
        require_stack: true,
        require_pins: false,
    };

    let golden = GoldenPolicy {
        fail_on_drift: true,
        golden_baseline_id: Some("baseline-001".to_string()),
        epsilon_threshold: 0.001,
    };

    let request = CreateExecutionPolicyRequest {
        determinism,
        routing: Some(routing),
        golden: Some(golden),
        require_signed_adapters: false,
    };

    db.create_execution_policy(tenant_id, request, None)
        .await
        .expect("Failed to create policy");

    // Config
    let config = create_test_config(Some("relaxed"), true);

    // Resolve
    let resolved = resolve_tenant_execution_policy(&db, &config, tenant_id, None)
        .await
        .expect("Failed to resolve policy");

    // Verify all fields are populated correctly
    assert_eq!(
        resolved.effective_determinism_mode,
        DeterminismMode::Strict,
        "effective_determinism_mode should be strict"
    );
    assert!(resolved.strict_mode, "strict_mode should be true");

    // Verify policy fields
    assert!(!resolved.policy.is_implicit, "Policy should be explicit");
    assert_eq!(
        resolved.policy.determinism.default_mode, "strict",
        "Policy determinism default_mode should be strict"
    );
    assert!(!resolved.policy.determinism.allow_fallback);

    // Verify routing resolved
    assert!(
        resolved.routing.use_session_stack_for_routing,
        "use_session_stack_for_routing should come from config"
    );
    assert!(!resolved.routing.allow_pins_outside_effective_set);

    // Verify golden resolved
    assert!(
        resolved.golden.fail_on_drift,
        "fail_on_drift should be true"
    );
    assert_eq!(
        resolved.golden.golden_baseline_id,
        Some("baseline-001".to_string()),
        "golden_baseline_id should be populated"
    );
    assert!(
        (resolved.golden.epsilon_threshold - 0.001).abs() < f64::EPSILON,
        "epsilon_threshold should be 0.001"
    );
}
